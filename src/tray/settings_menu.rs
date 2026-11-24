//! Submenus "Show" and "Alert above" of the right-click menu. All decisions live in
//! `settings`; this only draws the state (`sync`), names the items in the current language
//! (`relabel`) and maps a clicked item back to a `Choice`.
use rust_i18n::t;
use tray_icon::menu::{CheckMenuItem, MenuId, PredefinedMenuItem, Submenu};

use crate::{
    i18n::Lang,
    presentation::limit_name,
    settings::{Alert, BarWindow, Choice, Settings},
};

pub(super) struct SettingsMenu {
    pub(super) show: Submenu,
    pub(super) alert: Submenu,
    pub(super) five: CheckMenuItem,
    pub(super) week: CheckMenuItem,
    pub(super) codex: CheckMenuItem,
    pub(super) claude: CheckMenuItem,
    pub(super) bar_five: CheckMenuItem,
    pub(super) bar_week: CheckMenuItem,
    pub(super) alerts: [(CheckMenuItem, Alert); 4],
}

impl SettingsMenu {
    pub(super) fn new(lang: Lang) -> Option<Self> {
        let check = || CheckMenuItem::new("", true, false, None);
        let menu = SettingsMenu {
            show: Submenu::new("", true),
            alert: Submenu::new("", true),
            five: check(),
            week: check(),
            codex: check(),
            claude: check(),
            bar_five: check(),
            bar_week: check(),
            alerts: [
                (check(), Alert::Off),
                (check(), Alert::At50),
                (check(), Alert::At75),
                (check(), Alert::At90),
            ],
        };
        menu.relabel(lang);
        let separator = || PredefinedMenuItem::separator();
        menu.show
            .append_items(&[
                &menu.five,
                &menu.week,
                &separator(),
                &menu.codex,
                &menu.claude,
                &separator(),
                &menu.bar_five,
                &menu.bar_week,
            ])
            .ok()?;
        for (item, _) in &menu.alerts {
            menu.alert.append(item).ok()?;
        }
        Some(menu)
    }

    /// Names every item in `lang`.
    pub(super) fn relabel(&self, lang: Lang) {
        let locale = lang.code();
        self.show.set_text(t!("settings.show", locale = locale));
        self.alert.set_text(t!("settings.alert", locale = locale));
        self.five
            .set_text(t!("settings.five_hour_window", locale = locale));
        self.week
            .set_text(t!("settings.week_window", locale = locale));
        self.codex.set_text(limit_name("codex"));
        self.claude.set_text(limit_name("claude"));
        self.bar_five
            .set_text(t!("menu.bar_five_hours", locale = locale));
        self.bar_week.set_text(t!("menu.bar_week", locale = locale));
        for (item, alert) in &self.alerts {
            match alert.threshold() {
                Some(threshold) => item.set_text(format!("{threshold} %")),
                None => item.set_text(t!("menu.alert_off", locale = locale)),
            }
        }
    }

    /// The choice behind a clicked item, if it is one of ours.
    pub(super) fn choice_for(&self, id: &MenuId) -> Option<Choice> {
        let known = [
            (&self.five, Choice::ToggleFiveHour),
            (&self.week, Choice::ToggleWeek),
            (&self.codex, Choice::ToggleCodex),
            (&self.claude, Choice::ToggleClaude),
            (&self.bar_five, Choice::Bar(BarWindow::FiveHour)),
            (&self.bar_week, Choice::Bar(BarWindow::Week)),
        ];
        known
            .iter()
            .find(|(item, _)| item.id() == id)
            .map(|(_, choice)| *choice)
            .or_else(|| {
                self.alerts
                    .iter()
                    .find(|(item, _)| item.id() == id)
                    .map(|(_, alert)| Choice::Alert(*alert))
            })
    }

    /// Shows what the settings say (the menu toggles a check by itself on click, which may
    /// not match a refused change such as hiding the last window).
    pub(super) fn sync(&self, settings: &Settings) {
        self.five.set_checked(settings.show_five_hour);
        self.five.set_enabled(settings.can_toggle_five_hour());
        self.week.set_checked(settings.show_week);
        self.week.set_enabled(settings.can_toggle_week());
        self.codex.set_checked(settings.show_codex);
        self.codex.set_enabled(settings.can_toggle_codex());
        self.claude.set_checked(settings.show_claude);
        self.claude.set_enabled(settings.can_toggle_claude());
        let bar = settings.effective_bar_window();
        self.bar_five.set_checked(bar == BarWindow::FiveHour);
        self.bar_week.set_checked(bar == BarWindow::Week);
        self.bar_five.set_enabled(settings.can_choose_bar_window());
        self.bar_week.set_enabled(settings.can_choose_bar_window());
        for (item, alert) in &self.alerts {
            item.set_checked(*alert == settings.alert);
        }
    }
}
