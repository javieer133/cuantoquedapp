//! Menu bar / system tray icon: lifecycle, menu and refresh. The pieces live in submodules:
//! `mac_bar` (title with badges), `windows_tray` (drawn icon and tooltip), `settings_menu`
//! (submenus "Show" and "Alert above"), `autostart_entry` ("start at login").
mod autostart_entry;
#[cfg(target_os = "macos")]
mod mac_bar;
mod settings_menu;
// Compiled on every platform so it is type-checked on the Mac too; only used on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
mod windows_tray;

use std::{
    path::PathBuf,
    sync::Arc,
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};

use chrono::{Local, Utc};
use rust_i18n::t;
use tao::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

use crate::{
    debug_log::debug_log,
    i18n::{self, Lang},
    monitor::{self, Report},
    panel::{Panel, PanelAction},
    panel_view::{panel_view_with, settings_view},
    presentation,
    reader::{Reader, Supervisor},
    reset_watch::{self, ResetWatch},
    settings::{self, Choice, Settings, apply_visibility},
    sources::{SharedTools, Sources, Tools},
};
use autostart_entry::AutostartEntry;
use settings_menu::SettingsMenu;

/// What the reading thread delivered since the last poll.
#[derive(Debug)]
enum ReaderUpdate {
    Report(Report),
    /// The thread is gone and nothing more will arrive.
    Lost,
    Nothing,
}

/// A report already queued is delivered before `Lost`: the last data is never dropped.
fn poll_reader(receiver: &Receiver<Report>) -> ReaderUpdate {
    match receiver.try_recv() {
        Ok(report) => ReaderUpdate::Report(report),
        Err(TryRecvError::Disconnected) => ReaderUpdate::Lost,
        Err(TryRecvError::Empty) => ReaderUpdate::Nothing,
    }
}

/// On Mac the app lives only in the menu bar: no Dock icon.
#[cfg(target_os = "macos")]
fn create_event_loop() -> EventLoop<()> {
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    let mut event_loop = EventLoop::new();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    event_loop
}

#[cfg(not(target_os = "macos"))]
fn create_event_loop() -> EventLoop<()> {
    EventLoop::new()
}

/// Keeps exactly one disabled menu item per line of `text`, at the top of `menu`.
/// A multi-line single item renders poorly on some systems; separate items do not.
fn sync_detail(menu: &Menu, items: &mut Vec<MenuItem>, text: &str) {
    let lines: Vec<&str> = text.lines().collect();
    for (position, line) in lines.iter().enumerate() {
        if let Some(item) = items.get(position) {
            item.set_text(line);
            continue;
        }
        let item = MenuItem::new(line, false, None);
        if menu.insert(&item, position).is_err() {
            break;
        }
        items.push(item);
    }
    while items.len() > lines.len() {
        if let Some(item) = items.pop() {
            // Removing is best effort: a stale line is better than a crash.
            let _ = menu.remove(&item);
        }
    }
}

/// Shows the icon and blocks until the user chooses "Quit".
/// With `live`, each reading also asks Codex and Claude live and merges them with the files.
pub fn run(folder: PathBuf, live: bool) {
    let event_loop = create_event_loop();
    let settings_path = settings::default_path();
    if let (Some(old), Some(new)) = (settings::legacy_path(), &settings_path) {
        crate::legacy::adopt_file(&old, new);
    }
    let mut settings = settings_path
        .as_deref()
        .map(settings::load)
        .unwrap_or_default();
    // Read once: a change of the system language shows after a restart; the setting, at once.
    let (forced_lang, system_lang) = (i18n::env_override(), i18n::system());
    let lang_for =
        move |settings: &Settings| i18n::pick(forced_lang, settings.language, system_lang);
    let mut lang = lang_for(&settings);
    // Hidden tools are not asked live; every reader (also a replacement) sees the current value.
    let tools = Arc::new(SharedTools::new(wanted_tools(&settings)));
    let reader_tools = Arc::clone(&tools);
    let make_reader = move || {
        Reader::spawn(
            folder.clone(),
            live.then(Sources::from_environment),
            Arc::clone(&reader_tools),
        )
    };
    let mut reader = match make_reader() {
        Ok(reader) => reader,
        Err(error) => {
            eprintln!("Could not start reading Codex ({:?})", error.kind());
            return;
        }
    };
    let mut supervisor = Supervisor::new(Instant::now());

    let menu = Menu::new();
    let placeholder = MenuItem::new(t!("menu.reading", locale = lang.code()), false, None);
    let refresh_item = MenuItem::new("", true, None);
    let quit = MenuItem::new("", true, None);
    let autostart_entry = AutostartEntry::new(lang);
    let settings_menu = SettingsMenu::new(lang);
    relabel_menu(&refresh_item, &quit, lang);
    let built = menu.append_items(&[&placeholder, &PredefinedMenuItem::separator()]);
    // Without the submenus the app still works with the saved settings.
    let built = built.and_then(|()| match &settings_menu {
        Some(entries) => menu.append_items(&[
            &entries.show,
            &entries.alert,
            &PredefinedMenuItem::separator(),
        ]),
        None => Ok(()),
    });
    if built
        .and_then(|()| menu.append_items(&[&refresh_item, &autostart_entry.item, &quit]))
        .is_err()
    {
        eprintln!("Could not create the menu");
        return;
    }
    if let Some(entries) = &settings_menu {
        entries.sync(&settings);
    }
    // One disabled item per line of the detail, always above the separator.
    let mut detail_items = vec![placeholder];
    #[cfg(windows)]
    let mut windows_shown = windows_tray::Shown::default();
    #[cfg(target_os = "macos")]
    let mut mac_shown = mac_bar::Shown::default();
    let mut tray = None;
    let mut panel: Option<Panel> = None;
    let mut login_item = autostart_entry.login_item();
    let mut report = None;
    let mut settings_open = false;
    let mut reader_lost = false;
    let mut next_refresh = Instant::now();
    let mut reset_watch = ResetWatch::default();
    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        if let Event::NewEvents(StartCause::Init) = event {
            // Left click opens the panel; the menu (autostart, quit) moves to the right click.
            // Without a panel the menu keeps answering the left click, as before.
            panel = Panel::new(target);
            debug_log(&format!("panel created: {}", panel.is_some()));
            if panel.is_some() {
                detail_items[0].set_text(t!("menu.left_click", locale = lang.code()));
            }
            let builder = TrayIconBuilder::new()
                .with_title("Codex ?%")
                .with_menu(Box::new(menu.clone()))
                .with_menu_on_left_click(panel.is_none());
            // Windows cannot show the title: start with a "?" icon until data arrives.
            #[cfg(windows)]
            let builder = match windows_tray::icon(crate::tray_badge::BadgeState::Unknown) {
                Some(icon) => builder
                    .with_icon(icon)
                    .with_tooltip(crate::tray_badge::tooltip_reading(lang)),
                None => builder,
            };
            match builder.build() {
                Ok(fresh) => tray = Some(fresh),
                Err(_) => {
                    eprintln!("Could not create the tray icon");
                    *control_flow = ControlFlow::Exit;
                }
            }
        }

        let tick = Instant::now();
        let update = poll_reader(&reader.reports);
        let reader_gone = matches!(update, ReaderUpdate::Lost);
        if let ReaderUpdate::Report(fresh) = update {
            report = Some(fresh);
            reader_lost = false;
            supervisor.on_report(tick);
            next_refresh = tick;
        }
        // A reader that died or stopped delivering is replaced (with a pause between tries).
        if supervisor.should_restart(tick, reader_gone) {
            debug_log(&format!("reader replaced (gone: {reader_gone})"));
            supervisor.restarted(tick);
            match make_reader() {
                Ok(fresh) => reader = fresh,
                Err(_) => reader_gone_report(&mut report, &mut reader_lost, &mut next_refresh),
            }
        } else if reader_gone && !reader_lost {
            reader_gone_report(&mut report, &mut reader_lost, &mut next_refresh);
        }

        // A shown window just reset: its data is from the old window, so ask for fresh data
        // now (forced, like "Refresh now") instead of at the next round.
        if !reader_lost
            && let Some(current) = &report
            && reset_watch.should_refresh(
                reset_watch::passed_reset(current, &settings, Utc::now()),
                Instant::now(),
            )
        {
            debug_log("window reset: refreshing now");
            let _ = reader.refresh_now();
        }

        // A new language from the settings renames the menu at once.
        let wanted = lang_for(&settings);
        if wanted != lang {
            lang = wanted;
            relabel_menu(&refresh_item, &quit, lang);
            autostart_entry.relabel(lang);
            if let Some(entries) = &settings_menu {
                entries.relabel(lang);
            }
            if panel.is_some()
                && let Some(item) = detail_items.first()
            {
                item.set_text(t!("menu.left_click", locale = lang.code()));
            }
            next_refresh = Instant::now();
        }

        // Recomputing also lets an expired reset be flagged between readings.
        if Instant::now() >= next_refresh
            && let (Some(tray), Some(report)) = (&tray, &report)
        {
            next_refresh = Instant::now() + Duration::from_secs(1);
            let now = Utc::now();
            #[cfg(target_os = "macos")]
            mac_bar::refresh(
                tray,
                &mut mac_shown,
                report,
                reader_lost,
                now,
                &settings,
                lang,
            );
            #[cfg(not(target_os = "macos"))]
            {
                let title = if reader_lost {
                    presentation::title_reader_lost(lang)
                } else {
                    presentation::title(report, now, lang)
                };
                tray.set_title(Some(title));
            }
            #[cfg(windows)]
            windows_tray::refresh(tray, &mut windows_shown, report, now, &settings, lang);
            match &mut panel {
                Some(panel) => {
                    // A hidden panel opens again on the limits, not on the settings.
                    if settings_open && !panel.is_visible() {
                        settings_open = false;
                    }
                    panel.set_view(&if settings_open {
                        settings_view(&settings, login_item, lang)
                    } else {
                        panel_view_with(
                            report,
                            now,
                            &Local,
                            login_item,
                            supervisor.is_refreshing(),
                            &settings,
                            lang,
                        )
                    })
                }
                None => sync_detail(
                    &menu,
                    &mut detail_items,
                    &presentation::format_report(
                        &apply_visibility(report, &settings),
                        now,
                        &Local,
                        lang,
                    ),
                ),
            }
        }
        if let Event::WindowEvent {
            window_id,
            event: WindowEvent::Focused(false),
            ..
        } = &event
            && let Some(panel) = &mut panel
            && *window_id == panel.window_id()
        {
            panel.focus_lost();
        }
        let mut refresh_requested = false;
        if let Some(panel) = &mut panel {
            for action in panel.actions() {
                match action {
                    PanelAction::Quit => *control_flow = ControlFlow::Exit,
                    PanelAction::Refresh => refresh_requested = true,
                    PanelAction::ToggleAutostart => {
                        autostart_entry.toggle();
                        login_item = autostart_entry.login_item();
                    }
                    PanelAction::OpenSettings => settings_open = true,
                    PanelAction::CloseSettings => settings_open = false,
                    PanelAction::Choose(choice) => choose(
                        &mut settings,
                        choice,
                        settings_path.as_deref(),
                        settings_menu.as_ref(),
                    ),
                }
                // Redraw now instead of at the next second.
                next_refresh = Instant::now();
            }
        }
        while let Ok(tray_event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = &tray_event
            {
                debug_log(&format!("tray click: {button:?} {button_state:?}"));
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = tray_event
                && let Some(panel) = &mut panel
            {
                panel.toggle(rect);
            }
        }
        while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            if menu_event.id() == quit.id() {
                *control_flow = ControlFlow::Exit;
            } else if menu_event.id() == refresh_item.id() {
                refresh_requested = true;
            } else if menu_event.id() == autostart_entry.item.id() {
                autostart_entry.toggle();
                login_item = autostart_entry.login_item();
                next_refresh = Instant::now();
            } else if let Some(choice) = settings_menu
                .as_ref()
                .and_then(|entries| entries.choice_for(menu_event.id()))
            {
                choose(
                    &mut settings,
                    choice,
                    settings_path.as_deref(),
                    settings_menu.as_ref(),
                );
                next_refresh = Instant::now();
            }
        }
        // A tool shown again is asked at once instead of at the next round.
        let previous = tools.set(wanted_tools(&settings));
        let current = tools.get();
        if (current.codex && !previous.codex) || (current.claude && !previous.claude) {
            let _ = reader.refresh_now();
        }
        if refresh_requested && supervisor.request_manual_refresh(Instant::now()) {
            // If the reader is gone, the replacement above takes care of it.
            let _ = reader.refresh_now();
            // Show "Refreshing…" right away.
            next_refresh = Instant::now();
        }
    });
}

/// The fixed items of the right-click menu, named in `lang`.
fn relabel_menu(refresh: &MenuItem, quit: &MenuItem, lang: Lang) {
    refresh.set_text(t!("menu.refresh", locale = lang.code()));
    quit.set_text(t!("menu.quit", locale = lang.code()));
}

fn wanted_tools(settings: &Settings) -> Tools {
    Tools {
        codex: settings.show_codex,
        claude: settings.show_claude,
    }
}

/// A choice from the panel or the right-click menu: apply, save, and show it in both.
fn choose(
    settings: &mut Settings,
    choice: Choice,
    path: Option<&std::path::Path>,
    menu: Option<&SettingsMenu>,
) {
    *settings = settings.apply(choice);
    if let Some(path) = path
        && settings::save(path, settings).is_err()
    {
        // The choice still applies until the app closes.
        eprintln!("Could not save the settings");
    }
    if let Some(menu) = menu {
        menu.sync(settings);
    }
}

/// The reader could not be (re)started: show the "reader lost" report instead of stale data.
fn reader_gone_report(
    report: &mut Option<Report>,
    reader_lost: &mut bool,
    next_refresh: &mut Instant,
) {
    *reader_lost = true;
    *report = Some(monitor::reader_lost_report());
    *next_refresh = Instant::now();
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::sync_channel;

    use super::*;

    #[test]
    fn empty_channel_means_nothing_new() {
        let (_sender, receiver) = sync_channel::<Report>(1);
        assert!(matches!(poll_reader(&receiver), ReaderUpdate::Nothing));
    }

    #[test]
    fn a_sent_report_is_delivered() {
        let (sender, receiver) = sync_channel(1);
        sender.send(monitor::reader_lost_report()).unwrap();
        assert!(matches!(poll_reader(&receiver), ReaderUpdate::Report(_)));
    }

    #[test]
    fn a_dropped_sender_means_the_reader_is_lost() {
        let (sender, receiver) = sync_channel::<Report>(1);
        drop(sender);
        assert!(matches!(poll_reader(&receiver), ReaderUpdate::Lost));
        // Stays lost on every later poll.
        assert!(matches!(poll_reader(&receiver), ReaderUpdate::Lost));
    }

    #[test]
    fn the_last_report_is_not_dropped_when_the_reader_dies() {
        let (sender, receiver) = sync_channel(1);
        sender.send(monitor::reader_lost_report()).unwrap();
        drop(sender);
        assert!(matches!(poll_reader(&receiver), ReaderUpdate::Report(_)));
        assert!(matches!(poll_reader(&receiver), ReaderUpdate::Lost));
    }

    #[test]
    fn a_panicking_reader_thread_is_detected() {
        let (sender, receiver) = sync_channel::<Report>(1);
        let handle = std::thread::spawn(move || {
            let _keep = sender;
            panic!("simulated failure in the reading thread");
        });
        assert!(handle.join().is_err());
        assert!(matches!(poll_reader(&receiver), ReaderUpdate::Lost));
    }
}
