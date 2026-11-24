//! Content of the floating panel: a self-contained HTML page built from a `Report`.
//!
//! Pure code (no window, no clock): the caller passes the time and the time zone, so the
//! output is deterministic. All text that comes from outside is escaped.
use chrono::{DateTime, TimeZone, Utc};
use rust_i18n::t;

use crate::{
    codex::{Credits, Window},
    i18n::{Lang, Language},
    monitor::Report,
    presentation::{
        REFRESHING_MARK, age, has_reset, limit_name, reset_parts, reset_text, window_name,
    },
    settings::{Alert, BarWindow, Choice, Settings, Theme, apply_visibility},
};

/// Panel width in logical pixels (fixed; the height depends on the content).
pub const WIDTH: u32 = 340;

/// State of the "start at login" option shown in the panel footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItem {
    /// No footer at all.
    Hidden,
    /// Shown disabled: the app is not installed, so there is nothing to register.
    Unavailable,
    Off,
    On,
}

pub struct PanelView {
    pub html: String,
    /// Logical pixels needed to show the whole content without scrolling.
    pub height: u32,
}

/// The page is not a website: the browser's own menu (Reload, Back, Inspect…) and its reload
/// keys would leave it blank, because there is nothing to reload from. They are switched off.
/// Escape closes the panel, and once loaded the page reports its real height (`h:<px>`), so
/// the window fits the content even where the estimate below is off.
const GUARD_SCRIPT: &str = "addEventListener('contextmenu',e=>e.preventDefault());addEventListener('keydown',e=>{const k=e.key.toLowerCase();if(k==='escape'){e.preventDefault();window.ipc.postMessage('close');return;}if(k==='f5'||((e.ctrlKey||e.metaKey)&&(k==='r'||k==='p'||k==='s'||k==='u'))||(e.altKey&&(k==='arrowleft'||k==='arrowright')))e.preventDefault();});addEventListener('load',()=>window.ipc.postMessage('h:'+Math.ceil(document.body.getBoundingClientRect().height)));";

/// Heights the page may ask for (logical px); anything else is ignored.
pub const MIN_HEIGHT: u32 = 60;
pub const MAX_HEIGHT: u32 = 900;

/// The height in a `h:<px>` message from the page, if it is one and it is sensible.
pub fn parse_height(message: &str) -> Option<u32> {
    let height = message.strip_prefix("h:")?.parse::<u32>().ok()?;
    (MIN_HEIGHT..=MAX_HEIGHT)
        .contains(&height)
        .then_some(height)
}

/// No network and no external resources: only the page's own inline style and button handlers.
const CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'";

/// Dark colors, used by the system's dark mode (theme "Auto") and by the "Dark" theme.
macro_rules! dark_colors {
    () => {
        "color-scheme:dark;--bg:#262628;--fg:#f5f5f7;--muted:#9a9aa0;--track:#3d3d40;--line:#3a3a3d;--off:#55555a;--hover:#4a4a4e;--card:#2e2e31;--seg-on:#5a5a5f;--accent:#0a84ff"
    };
}

const STYLE: &str = concat!(
    r#"
:root{color-scheme:light;--bg:#fff;--fg:#1d1d1f;--muted:#6e6e73;--track:#e8e8ed;--line:#e5e5ea;--off:#d1d1d6;--hover:#dcdce1;--card:#f2f2f5;--seg-on:#fff;--accent:#007aff;--ok:#2fa06b;--warn:#e8a317;--bad:#e5484d}
@media(prefers-color-scheme:dark){:root:not([data-theme=light]){"#,
    dark_colors!(),
    r#"}}
:root[data-theme=dark]{"#,
    dark_colors!(),
    r#"}
*{box-sizing:border-box}
body{margin:0;padding:14px 16px;background:var(--bg);color:var(--fg);font:13px/1.35 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;user-select:none;-webkit-user-select:none;cursor:default;overflow:hidden}
.limit+.limit{margin-top:14px;padding-top:12px;border-top:1px solid var(--line)}
.head{display:flex;justify-content:space-between;align-items:baseline;margin-bottom:8px}
.name{font-weight:600;font-size:14px;display:inline-flex;align-items:center}
.ico{width:18px;height:18px;margin-right:8px;flex:none}.plan{color:var(--muted);font-weight:400;margin-left:4px}
.age{color:var(--muted);font-size:11px}
.win{margin-top:9px}
.row{display:flex;justify-content:space-between;align-items:baseline}
.label{color:var(--muted)}.pct{font-weight:600;font-size:15px;font-variant-numeric:tabular-nums}
.track{height:7px;border-radius:4px;background:var(--track);margin:5px 0 4px;overflow:hidden}
.fill{height:100%;border-radius:4px}
.ok{background:var(--ok)}.warn{background:var(--warn)}.bad{background:var(--bad)}
.reset,.extra{color:var(--muted);font-size:11px}
.reset b{color:var(--fg);font-weight:600}
.pct.alert{color:var(--bad)}
.refreshing{color:var(--warn);font-weight:600}
.alert-msg{color:var(--bad);font-size:11px;font-weight:600;margin-top:2px}
.extra{margin-top:8px}
.notice{margin-top:12px;color:var(--warn);font-size:11px}
.empty{color:var(--muted)}
.foot{margin-top:14px;padding-top:6px;border-top:1px solid var(--line);display:flex;flex-direction:column}
button{all:unset;box-sizing:border-box;display:flex;justify-content:space-between;align-items:center;padding:6px 6px;margin:0 -6px;border-radius:6px;cursor:pointer}
button:hover:not([disabled]){background:var(--track)}
button[disabled]{color:var(--muted);cursor:default}
.check{color:var(--ok);font-weight:700}.chev{color:var(--muted)}
.top{display:flex;align-items:center;justify-content:space-between}
.top b{font-size:15px}.top button{color:var(--accent);font-weight:600}
.group{margin-top:14px}
.note{color:var(--muted);font-size:11px;margin:4px 0 0 2px}
.ver{margin-top:14px;text-align:center;color:var(--muted);font-size:11px}
.gh{color:var(--muted);font-size:11px;font-weight:600;margin:0 0 6px 2px}
.card{display:flex;flex-direction:column;padding:0 12px;border-radius:9px;background:var(--card)}
.card button{margin:0;padding:7px 0;border-radius:0}
.card button+button{border-top:1px solid var(--line)}
.card button:hover:not([disabled]){background:none}
.sw{position:relative;width:32px;height:18px;border-radius:9px;background:var(--off);flex:none;transition:background .15s}
.sw::after{content:"";position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:50%;background:#fff;box-shadow:0 1px 2px rgba(0,0,0,.3)}
.sw.on{background:var(--ok)}.sw.on::after{left:16px}
button[disabled] .sw{opacity:.45}
.seg{display:flex;gap:2px;padding:2px;border-radius:8px;background:var(--track)}
.seg button{flex:1;justify-content:center;margin:0;padding:4px 0;border-radius:6px;font-size:12px}
.seg button:hover:not([disabled]):not(.on){background:var(--hover)}
.seg button.on{background:var(--seg-on);color:var(--fg);font-weight:600;box-shadow:0 1px 2px rgba(0,0,0,.18)}
.seg button[disabled]:not(.on){color:var(--muted)}
"#
);

// Layout estimates in px, tied to STYLE.
const BASE_HEIGHT: u32 = 28;
const LIMIT_HEAD: u32 = 30;
const LIMIT_GAP: u32 = 27;
const WINDOW_HEIGHT: u32 = 54;
const ALERT_HEIGHT: u32 = 16;
const EXTRA_HEIGHT: u32 = 22;
const NOTICE_HEIGHT: u32 = 28;
const EMPTY_HEIGHT: u32 = 20;
const FOOT_PADDING: u32 = 21;
const BUTTON_HEIGHT: u32 = 30;
const FOOT_BUTTONS: u32 = 3;
// Settings view.
const TOP_HEIGHT: u32 = 30;
const GROUP_HEAD: u32 = 35;
const ROW_HEIGHT: u32 = 32;
const ROW_BORDER: u32 = 1;
const SEGMENTED_HEIGHT: u32 = 29;
const NOTE_HEIGHT: u32 = 19;
const VERSION_HEIGHT: u32 = 29;

/// Shown at the bottom of the settings view; taken from `Cargo.toml` at build time.
pub const VERSION_TEXT: &str = concat!("CuantoQuedapp ", env!("CARGO_PKG_VERSION"));

/// Small badges of our own design (not the vendors' logos): a terminal prompt for Codex and
/// an eight-point star for Claude. Unknown limits get no icon.
fn icon_svg(limit_id: &str) -> &'static str {
    match limit_id {
        "codex" => {
            r##"<svg class="ico" viewBox="0 0 20 20" aria-hidden="true"><rect width="20" height="20" rx="5" fill="#4c5fd5"/><path d="M5.5 7l3.5 3-3.5 3M11 13.5h3.5" fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"##
        }
        "claude" => {
            r##"<svg class="ico" viewBox="0 0 20 20" aria-hidden="true"><rect width="20" height="20" rx="5" fill="#d9774f"/><path d="M10 4.5v11M4.5 10h11M6.1 6.1l7.8 7.8M13.9 6.1l-7.8 7.8" fill="none" stroke="#fff" stroke-width="1.7" stroke-linecap="round"/></svg>"##
        }
        _ => "",
    }
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            other => other.to_string(),
        })
        .collect()
}

fn level(percent: f64) -> &'static str {
    if percent >= 85.0 {
        "bad"
    } else if percent >= 60.0 {
        "warn"
    } else {
        "ok"
    }
}

fn window_html<T: TimeZone>(
    window: &Window,
    now: DateTime<Utc>,
    zone: &T,
    settings: &Settings,
    lang: Lang,
) -> (String, bool) {
    let locale = lang.code();
    if has_reset(window, now) {
        // The old percentage means nothing after a reset: an empty bar until fresh data comes.
        let html = format!(
            r#"<div class="win"><div class="row"><span class="label">{}</span><span class="pct refreshing">{REFRESHING_MARK}</span></div><div class="track"></div><div class="reset refreshing">{}</div></div>"#,
            escape(&window_name(window.window_minutes, lang)),
            escape(&t!("reset.refreshing", locale = locale)),
        );
        return (html, false);
    }
    let percent = window.used_percent.clamp(0.0, 100.0);
    let alert = settings.alert.is_triggered(window, now);
    let reset = match reset_parts(window, now, zone, lang) {
        // The translation holds the `<b>` markup; only the values are escaped.
        Some((remaining, when)) => t!(
            "reset.html",
            locale = locale,
            remaining = escape(&remaining),
            when = escape(&when)
        )
        .into(),
        None => escape(&reset_text(window, now, zone, lang)),
    };
    let alert_line = match (alert, settings.alert.threshold()) {
        (true, Some(threshold)) => format!(
            r#"<div class="alert-msg">{}</div>"#,
            escape(&t!("panel.alert", locale = locale, threshold = threshold))
        ),
        _ => String::new(),
    };
    let html = format!(
        r#"<div class="win"><div class="row"><span class="label">{}</span><span class="pct{}">{:.0}%</span></div><div class="track"><div class="fill {}" style="width:{:.0}%"></div></div><div class="reset">{}</div>{}</div>"#,
        escape(&window_name(window.window_minutes, lang)),
        if alert { " alert" } else { "" },
        percent,
        level(percent),
        percent,
        reset,
        alert_line,
    );
    (html, alert)
}

/// Extra credits are mentioned only when there is something to say.
fn credits_text(credits: &Credits, lang: Lang) -> Option<String> {
    let locale = lang.code();
    let extra = if credits.unlimited {
        t!("credits.unlimited", locale = locale)
    } else if credits.has_credits {
        t!(
            "credits.balance",
            locale = locale,
            balance = &credits.balance
        )
    } else {
        return None;
    };
    Some(t!("credits.line", locale = locale, extra = extra).into())
}

/// The panel without footer (content only).
pub fn panel_view<T: TimeZone>(
    report: &Report,
    now: DateTime<Utc>,
    zone: &T,
    lang: Lang,
) -> PanelView {
    let settings = Settings {
        alert: Alert::Off,
        ..Settings::default()
    };
    panel_view_with(report, now, zone, LoginItem::Hidden, false, &settings, lang)
}

/// Footer of the limits view; `LoginItem::Hidden` means no footer at all.
fn footer_html(login: LoginItem, refreshing: bool, lang: Lang) -> Option<String> {
    if login == LoginItem::Hidden {
        return None;
    }
    let locale = lang.code();
    let refresh = if refreshing {
        format!(
            r#"<button disabled>{}</button>"#,
            t!("panel.refreshing", locale = locale)
        )
    } else {
        format!(
            r#"<button onclick="window.ipc.postMessage('refresh')">{}</button>"#,
            t!("menu.refresh", locale = locale)
        )
    };
    Some(format!(
        r#"<div class="foot">{refresh}<button onclick="window.ipc.postMessage('settings')">{}<span class="chev">›</span></button><button onclick="window.ipc.postMessage('quit')">{}</button></div>"#,
        t!("menu.settings", locale = locale),
        t!("menu.quit", locale = locale),
    ))
}

/// The whole page around `body`, with the chosen theme (none means "follow the system").
fn page(body: &str, theme: Theme, lang: Lang) -> String {
    let lang = lang.code();
    let theme = match theme {
        Theme::Auto => "",
        Theme::Light => r#" data-theme="light""#,
        Theme::Dark => r#" data-theme="dark""#,
    };
    format!(
        r#"<!doctype html><html lang="{lang}"{theme}><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="{CSP}"><style>{STYLE}</style><script>{GUARD_SCRIPT}</script></head><body>{body}</body></html>"#
    )
}

/// Message the page sends for a choice (the inverse of `parse_choice`).
pub fn choice_message(choice: Choice) -> &'static str {
    match choice {
        Choice::ToggleFiveHour => "toggle:five_hour",
        Choice::ToggleWeek => "toggle:week",
        Choice::ToggleCodex => "toggle:codex",
        Choice::ToggleClaude => "toggle:claude",
        Choice::Bar(BarWindow::FiveHour) => "bar:five_hour",
        Choice::Bar(BarWindow::Week) => "bar:week",
        Choice::Alert(Alert::Off) => "alert:off",
        Choice::Alert(Alert::At50) => "alert:50",
        Choice::Alert(Alert::At75) => "alert:75",
        Choice::Alert(Alert::At90) => "alert:90",
        Choice::Theme(Theme::Auto) => "theme:auto",
        Choice::Theme(Theme::Light) => "theme:light",
        Choice::Theme(Theme::Dark) => "theme:dark",
        Choice::Language(Language::Auto) => "language:auto",
        Choice::Language(Language::Spanish) => "language:es",
        Choice::Language(Language::English) => "language:en",
    }
}

/// Every choice the settings view can send.
const ALL_CHOICES: [Choice; 16] = [
    Choice::ToggleFiveHour,
    Choice::ToggleWeek,
    Choice::ToggleCodex,
    Choice::ToggleClaude,
    Choice::Bar(BarWindow::FiveHour),
    Choice::Bar(BarWindow::Week),
    Choice::Alert(Alert::Off),
    Choice::Alert(Alert::At50),
    Choice::Alert(Alert::At75),
    Choice::Alert(Alert::At90),
    Choice::Theme(Theme::Auto),
    Choice::Theme(Theme::Light),
    Choice::Theme(Theme::Dark),
    Choice::Language(Language::Auto),
    Choice::Language(Language::Spanish),
    Choice::Language(Language::English),
];

/// The choice behind a message from the page; anything else is `None`.
pub fn parse_choice(message: &str) -> Option<Choice> {
    ALL_CHOICES
        .into_iter()
        .find(|choice| choice_message(*choice) == message)
}

fn post(choice: Choice) -> String {
    format!("window.ipc.postMessage('{}')", choice_message(choice))
}

fn switch_row(label: &str, on: bool, enabled: bool, choice: Choice) -> String {
    switch_button(label, on, enabled.then(|| post(choice)))
}

/// `onclick: None` draws it disabled.
fn switch_button(label: &str, on: bool, onclick: Option<String>) -> String {
    let state = if on { " on" } else { "" };
    match onclick {
        Some(onclick) => format!(
            r#"<button onclick="{onclick}">{label}<span class="sw{state}"></span></button>"#
        ),
        None => format!(r#"<button disabled>{label}<span class="sw{state}"></span></button>"#),
    }
}

fn segmented(options: &[(&str, Choice, bool)], enabled: bool) -> String {
    let buttons: String = options
        .iter()
        .map(|(label, choice, on)| {
            let class = if *on { r#" class="on""# } else { "" };
            if enabled && !on {
                format!(
                    r#"<button{class} onclick="{}">{label}</button>"#,
                    post(*choice)
                )
            } else if enabled {
                format!(r#"<button{class}>{label}</button>"#)
            } else {
                format!(r#"<button{class} disabled>{label}</button>"#)
            }
        })
        .collect();
    format!(r#"<div class="seg">{buttons}</div>"#)
}

fn group(title: &str, content: &str) -> String {
    format!(r#"<div class="group"><div class="gh">{title}</div>{content}</div>"#)
}

/// The settings view of the panel: the same options as the right-click menu, plus the theme,
/// the language and "start at login" (not shown with `LoginItem::Hidden`).
pub fn settings_view(settings: &Settings, login: LoginItem, lang: Lang) -> PanelView {
    let s = settings;
    let locale = lang.code();
    let autostart_label = t!("autostart.label", locale = locale);
    let show = [
        switch_row(
            &t!("settings.five_hour_window", locale = locale),
            s.show_five_hour,
            s.can_toggle_five_hour(),
            Choice::ToggleFiveHour,
        ),
        switch_row(
            &t!("settings.week_window", locale = locale),
            s.show_week,
            s.can_toggle_week(),
            Choice::ToggleWeek,
        ),
        switch_row(
            "Codex",
            s.show_codex,
            s.can_toggle_codex(),
            Choice::ToggleCodex,
        ),
        switch_row(
            "Claude",
            s.show_claude,
            s.can_toggle_claude(),
            Choice::ToggleClaude,
        ),
    ]
    .concat();
    let bar = s.effective_bar_window();
    let five_hours = window_name(crate::settings::FIVE_HOUR_MINUTES, lang);
    let week = window_name(crate::settings::WEEK_MINUTES, lang);
    let bar = segmented(
        &[
            (
                five_hours.as_str(),
                Choice::Bar(BarWindow::FiveHour),
                bar == BarWindow::FiveHour,
            ),
            (
                week.as_str(),
                Choice::Bar(BarWindow::Week),
                bar == BarWindow::Week,
            ),
        ],
        s.can_choose_bar_window(),
    );
    let alert_off = t!("settings.alert_off", locale = locale);
    let alert = segmented(
        &[Alert::Off, Alert::At50, Alert::At75, Alert::At90].map(|alert| {
            let label = match alert {
                Alert::Off => &*alert_off,
                Alert::At50 => "50 %",
                Alert::At75 => "75 %",
                Alert::At90 => "90 %",
            };
            (label, Choice::Alert(alert), alert == s.alert)
        }),
        true,
    );
    let (auto, light, dark) = (
        t!("theme.auto", locale = locale),
        t!("theme.light", locale = locale),
        t!("theme.dark", locale = locale),
    );
    let theme = segmented(
        &[
            (&*auto, Theme::Auto),
            (&*light, Theme::Light),
            (&*dark, Theme::Dark),
        ]
        .map(|(label, theme)| (label, Choice::Theme(theme), theme == s.theme)),
        true,
    );
    // Each language is named in itself, so it can be found whatever the current one is.
    let system = t!("language.system", locale = locale);
    let language = segmented(
        &[
            (&*system, Language::Auto),
            ("Español", Language::Spanish),
            ("English", Language::English),
        ]
        .map(|(label, language)| (label, Choice::Language(language), language == s.language)),
        true,
    );
    let general_title = t!("settings.general", locale = locale);
    let autostart = "window.ipc.postMessage('autostart')".to_string();
    let (general, general_height) = match login {
        LoginItem::Hidden => (String::new(), 0),
        LoginItem::Unavailable => (
            group(
                &general_title,
                &format!(
                    r#"<div class="card">{}</div><div class="note">{}</div>"#,
                    switch_button(&autostart_label, false, None),
                    t!("autostart.needs_install", locale = locale)
                ),
            ),
            GROUP_HEAD + ROW_HEIGHT + NOTE_HEIGHT,
        ),
        LoginItem::Off | LoginItem::On => (
            group(
                &general_title,
                &format!(
                    r#"<div class="card">{}</div>"#,
                    switch_button(&autostart_label, login == LoginItem::On, Some(autostart))
                ),
            ),
            GROUP_HEAD + ROW_HEIGHT,
        ),
    };
    let body = [
        format!(
            r#"<div class="top"><b>{}</b><button onclick="window.ipc.postMessage('back')">{}</button></div>"#,
            t!("menu.settings", locale = locale),
            t!("settings.done", locale = locale)
        ),
        group(
            &t!("settings.show", locale = locale),
            &format!(r#"<div class="card">{show}</div>"#),
        ),
        group(&t!("settings.bar_number", locale = locale), &bar),
        group(&t!("settings.alert", locale = locale), &alert),
        group(&t!("settings.theme", locale = locale), &theme),
        group(&t!("settings.language", locale = locale), &language),
        general,
        format!(r#"<div class="ver">{VERSION_TEXT}</div>"#),
    ]
    .concat();
    PanelView {
        html: page(&body, s.theme, lang),
        height: BASE_HEIGHT
            + TOP_HEIGHT
            + GROUP_HEAD * 5
            + ROW_HEIGHT * 4
            + ROW_BORDER * 3
            + SEGMENTED_HEIGHT * 4
            + general_height
            + VERSION_HEIGHT,
    }
}

/// The panel with the footer for the options that used to live only in the right-click menu.
pub fn panel_view_with<T: TimeZone>(
    report: &Report,
    now: DateTime<Utc>,
    zone: &T,
    login: LoginItem,
    refreshing: bool,
    settings: &Settings,
    lang: Lang,
) -> PanelView {
    // What the user hides never reaches the page.
    let report = &apply_visibility(report, settings);
    let mut body = String::new();
    let mut height = BASE_HEIGHT;
    let mut shown = 0;
    for data in report.selection.limits.values() {
        let limits = &data.limits;
        let windows: Vec<&Window> = [&limits.primary, &limits.secondary]
            .into_iter()
            .flatten()
            .collect();
        let credits = limits
            .credits
            .as_ref()
            .and_then(|credits| credits_text(credits, lang));
        // A limit without windows or credits (like `premium`) has nothing to show.
        if windows.is_empty() && credits.is_none() {
            continue;
        }
        let name = limit_name(&limits.limit_id);
        let plan = limits
            .plan_type
            .as_deref()
            .map(|plan| format!(r#"<span class="plan">{}</span>"#, escape(plan)))
            .unwrap_or_default();
        body.push_str(&format!(
            r#"<section class="limit"><div class="head"><span class="name">{}{}{}</span><span class="age">{}</span></div>"#,
            icon_svg(&limits.limit_id),
            escape(name),
            plan,
            escape(&age(data.date, now, lang)),
        ));
        height += LIMIT_HEAD + WINDOW_HEIGHT * windows.len() as u32;
        if shown > 0 {
            height += LIMIT_GAP;
        }
        for window in windows {
            let (html, alert) = window_html(window, now, zone, settings, lang);
            body.push_str(&html);
            if alert {
                height += ALERT_HEIGHT;
            }
        }
        if let Some(text) = credits {
            body.push_str(&format!(r#"<div class="extra">{}</div>"#, escape(&text)));
            height += EXTRA_HEIGHT;
        }
        body.push_str("</section>");
        shown += 1;
    }
    if shown == 0 && report.notices.is_empty() {
        body.push_str(&format!(
            r#"<div class="empty">{}</div>"#,
            t!("report.no_data", locale = lang.code())
        ));
        height += EMPTY_HEIGHT;
    }
    for notice in &report.notices {
        body.push_str(&format!(
            r#"<div class="notice">{}</div>"#,
            escape(&notice.text(lang))
        ));
        height += NOTICE_HEIGHT;
    }
    if let Some(footer) = footer_html(login, refreshing, lang) {
        body.push_str(&footer);
        height += FOOT_PADDING + BUTTON_HEIGHT * FOOT_BUTTONS;
    }
    PanelView {
        html: page(&body, settings.theme, lang),
        height,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        codex::RateLimits,
        selection_state::{LimitsData, Selection},
    };

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-20T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn window(percent: f64, minutes: u64, hours_ahead: i64) -> Window {
        Window {
            used_percent: percent,
            window_minutes: minutes,
            resets_at: now().timestamp() + hours_ahead * 3600,
        }
    }

    fn report(limits: Vec<RateLimits>, notices: Vec<crate::monitor::Notice>) -> Report {
        let map: BTreeMap<String, LimitsData> = limits
            .into_iter()
            .map(|limits| {
                (
                    limits.limit_id.clone(),
                    LimitsData {
                        date: now(),
                        limits,
                    },
                )
            })
            .collect();
        Report {
            selection: Selection {
                limits: map,
                invalid: 0,
            },
            notices,
        }
    }

    fn codex(primary: Option<Window>, secondary: Option<Window>) -> RateLimits {
        RateLimits {
            limit_id: "codex".into(),
            plan_type: Some("plus".into()),
            primary,
            secondary,
            credits: None,
        }
    }

    #[test]
    fn shows_each_window_with_percent_bar_and_reset() {
        let view = panel_view(
            &report(
                vec![codex(
                    Some(window(54.0, 300, 3)),
                    Some(window(27.0, 10080, 100)),
                )],
                vec![],
            ),
            now(),
            &Utc,
            Lang::Es,
        );
        assert!(view.html.contains("5 horas"));
        assert!(view.html.contains(">54%<"));
        assert!(view.html.contains("width:54%"));
        assert!(view.html.contains("Semana"));
        assert!(view.html.contains("se reinicia"));
        assert!(view.html.contains("plus"));
    }

    #[test]
    fn a_window_that_reset_shows_it_is_refreshing_instead_of_its_old_percent() {
        let reset = report(
            vec![codex(
                Some(window(46.0, 300, -1)),
                Some(window(27.0, 10080, 100)),
            )],
            vec![],
        );
        let es = panel_view(&reset, now(), &Utc, Lang::Es).html;
        assert!(
            es.contains(r#"<span class="pct refreshing">↻</span>"#),
            "{es}"
        );
        assert!(es.contains(r#"<div class="reset refreshing">Se reinició; actualizando…</div>"#));
        assert!(!es.contains(">46%<") && !es.contains("width:46%"));
        assert!(es.contains(">27%<"), "the week is still current");
        let en = panel_view(&reset, now(), &Utc, Lang::En).html;
        assert!(en.contains("Reset; refreshing…"));
    }

    #[test]
    fn the_panel_speaks_english() {
        let view = panel_view_with(
            &report(
                vec![codex(
                    Some(window(54.0, 300, 3)),
                    Some(window(27.0, 10080, 100)),
                )],
                vec![crate::monitor::Notice::CodexNotFound],
            ),
            now(),
            &Utc,
            LoginItem::Off,
            false,
            &Settings::default(),
            Lang::En,
        );
        assert!(view.html.contains(r#"<html lang="en""#));
        for text in [
            ">5 hours<",
            ">Week<",
            "resets <b>in 3 h</b>",
            "Codex not found on this computer",
            ">Refresh now<",
            "Settings<span",
            ">Quit<",
        ] {
            assert!(view.html.contains(text), "{text}: {}", view.html);
        }
        assert!(!view.html.contains("se reinicia"));
    }

    #[test]
    fn the_settings_view_speaks_english_and_offers_the_language() {
        let s = Settings::default();
        let html = settings_view(&s, LoginItem::Off, Lang::En).html;
        for text in [
            "<b>Settings</b>",
            ">Done<",
            ">Show<",
            "5-hour window<span",
            ">Menu bar number<",
            ">Alert above<",
            ">Theme<",
            ">Language<",
            r#"<button class="on">System</button>"#,
            ">Español<",
            ">English<",
            "Start at login<span",
        ] {
            assert!(html.contains(text), "{text}: {html}");
        }
        assert!(html.contains("'language:es'") && html.contains("'language:en'"));
        assert!(
            !html.contains("'language:auto'"),
            "the active one does not resend"
        );
        let spanish = Settings {
            language: Language::Spanish,
            ..s
        };
        let html = settings_view(&spanish, LoginItem::Off, Lang::Es).html;
        assert!(html.contains(r#"<html lang="es""#) && html.contains(">Idioma<"));
        assert!(html.contains(r#"<button class="on">Español</button>"#));
        assert!(html.contains(r#"<button class="on">Automático</button>"#));
    }

    #[test]
    fn known_tools_get_their_icon_and_others_none() {
        assert!(icon_svg("codex").contains("<svg"));
        assert!(icon_svg("claude").contains("<svg"));
        assert_ne!(icon_svg("codex"), icon_svg("claude"));
        assert_eq!(icon_svg("premium"), "");
        let mut claude = codex(Some(window(10.0, 300, 3)), None);
        claude.limit_id = "claude".into();
        let view = panel_view(&report(vec![claude], vec![]), now(), &Utc, Lang::Es);
        assert!(view.html.contains(r#"<span class="name"><svg"#));
        assert!(view.html.contains("Claude"));
    }

    #[test]
    fn color_level_follows_usage() {
        assert_eq!(level(0.0), "ok");
        assert_eq!(level(59.9), "ok");
        assert_eq!(level(60.0), "warn");
        assert_eq!(level(85.0), "bad");
        assert_eq!(level(100.0), "bad");
    }

    #[test]
    fn a_limit_without_windows_or_credits_is_hidden() {
        let premium = RateLimits {
            limit_id: "premium".into(),
            plan_type: Some("plus".into()),
            primary: None,
            secondary: None,
            credits: Some(Credits {
                has_credits: false,
                unlimited: false,
                balance: "0".into(),
            }),
        };
        let view = panel_view(
            &report(
                vec![codex(Some(window(10.0, 300, 3)), None), premium],
                vec![],
            ),
            now(),
            &Utc,
            Lang::Es,
        );
        assert!(!view.html.contains("premium"));
        assert!(!view.html.contains("Créditos extra"));
    }

    #[test]
    fn credits_are_shown_only_when_there_are_some() {
        let mut limits = codex(Some(window(10.0, 300, 3)), None);
        limits.credits = Some(Credits {
            has_credits: true,
            unlimited: false,
            balance: "12.5".into(),
        });
        let view = panel_view(&report(vec![limits], vec![]), now(), &Utc, Lang::Es);
        assert!(view.html.contains("Créditos extra: saldo 12.5"));
    }

    #[test]
    fn empty_report_says_there_is_no_data() {
        let view = panel_view(&report(vec![], vec![]), now(), &Utc, Lang::Es);
        assert!(view.html.contains("Aún no hay datos"));
    }

    #[test]
    fn notices_are_shown_and_escaped() {
        let view = panel_view(
            &report(
                vec![],
                vec![crate::monitor::Notice::FileUnreadable {
                    file: std::path::PathBuf::from("<b>\"x\" & 'y'</b>"),
                    kind: std::io::ErrorKind::NotFound,
                }],
            ),
            now(),
            &Utc,
            Lang::Es,
        );
        assert!(
            view.html
                .contains("&lt;b&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/b&gt;")
        );
        assert!(!view.html.contains("<b>"));
        assert!(!view.html.contains("Aún no hay datos"));
    }

    #[test]
    fn percent_is_clamped_for_the_bar() {
        let view = panel_view(
            &report(vec![codex(Some(window(130.0, 300, 3)), None)], vec![]),
            now(),
            &Utc,
            Lang::Es,
        );
        assert!(view.html.contains("width:100%"));
    }

    #[test]
    fn start_at_login_lives_in_the_settings_view() {
        let empty = report(vec![], vec![]);
        let main = |login| {
            panel_view_with(
                &empty,
                now(),
                &Utc,
                login,
                false,
                &Settings::default(),
                Lang::Es,
            )
            .html
        };
        assert!(!main(LoginItem::Hidden).contains("Salir"));
        assert!(main(LoginItem::On).contains("'quit'"));
        assert!(
            !main(LoginItem::On).contains("'autostart'"),
            "not in the footer any more"
        );
        let settings = |login| settings_view(&Settings::default(), login, Lang::Es);
        let on = settings(LoginItem::On).html;
        assert!(
            on.contains("'autostart'")
                && on.contains(r#"Iniciar al abrir sesión<span class="sw on">"#)
        );
        let off = settings(LoginItem::Off).html;
        assert!(
            off.contains("'autostart'")
                && off.contains(r#"Iniciar al abrir sesión<span class="sw">"#)
        );
        let unavailable = settings(LoginItem::Unavailable);
        assert!(
            !unavailable.html.contains("'autostart'") && unavailable.html.contains("instalada")
        );
        let hidden = settings(LoginItem::Hidden);
        assert!(!hidden.html.contains("Iniciar al abrir"));
        assert_eq!(
            settings(LoginItem::On).height,
            hidden.height + GROUP_HEAD + ROW_HEIGHT
        );
        assert_eq!(
            unavailable.height,
            hidden.height + GROUP_HEAD + ROW_HEIGHT + NOTE_HEIGHT
        );
    }

    #[test]
    fn the_page_reports_its_height_and_escape_closes_it() {
        let html = settings_view(&Settings::default(), LoginItem::Off, Lang::Es).html;
        assert!(html.contains("'h:'+") && html.contains("'escape'") && html.contains("'close'"));
        assert_eq!(parse_height("h:412"), Some(412));
        for message in ["h:", "h:-3", "h:12", "h:5000", "h:4.5", "412", "close"] {
            assert_eq!(parse_height(message), None, "{message}");
        }
    }

    #[test]
    fn footer_adds_height() {
        let empty = report(vec![], vec![]);
        let without = panel_view(&empty, now(), &Utc, Lang::Es).height;
        let with = panel_view_with(
            &empty,
            now(),
            &Utc,
            LoginItem::Off,
            false,
            &Settings::default(),
            Lang::Es,
        )
        .height;
        assert_eq!(with, without + FOOT_PADDING + BUTTON_HEIGHT * FOOT_BUTTONS);
    }

    #[test]
    fn height_grows_with_content() {
        let one = panel_view(
            &report(vec![codex(Some(window(1.0, 300, 3)), None)], vec![]),
            now(),
            &Utc,
            Lang::Es,
        );
        let two = panel_view(
            &report(
                vec![codex(
                    Some(window(1.0, 300, 3)),
                    Some(window(2.0, 10080, 50)),
                )],
                vec![crate::monitor::Notice::CodexNotFound],
            ),
            now(),
            &Utc,
            Lang::Es,
        );
        assert!(two.height > one.height);
        assert!(one.height >= BASE_HEIGHT + LIMIT_HEAD + WINDOW_HEIGHT);
    }

    // CA10
    #[test]
    fn remaining_time_is_shown_and_the_alerted_window_is_highlighted() {
        use crate::settings::Choice;
        let r = report(
            vec![codex(
                Some(window(80.0, 300, 3)),
                Some(window(20.0, 10080, 100)),
            )],
            vec![],
        );
        let settings = Settings::default();
        let html = panel_view_with(
            &r,
            now(),
            &Utc,
            LoginItem::Hidden,
            false,
            &settings,
            Lang::Es,
        )
        .html;
        assert!(html.contains("se reinicia <b>en 3 h</b> ("), "{html}");
        assert!(html.contains(r#"<span class="pct alert">80%</span>"#));
        assert!(html.contains("⚠ Pasaste el 75 %"));
        // Only the window over the threshold is flagged.
        assert_eq!(html.matches("alert-msg\"").count(), 1);
        assert!(html.contains(r#"<span class="pct">20%</span>"#));

        let at90 = settings.apply(Choice::Alert(Alert::At90));
        let html = panel_view_with(&r, now(), &Utc, LoginItem::Hidden, false, &at90, Lang::Es).html;
        assert!(!html.contains("Pasaste"));
        let off = settings.apply(Choice::Alert(Alert::Off));
        assert!(
            !panel_view_with(&r, now(), &Utc, LoginItem::Hidden, false, &off, Lang::Es)
                .html
                .contains("Pasaste")
        );
    }

    #[test]
    fn the_alert_adds_height_and_hidden_things_are_not_in_the_page() {
        use crate::settings::Choice;
        let r = report(
            vec![codex(
                Some(window(80.0, 300, 3)),
                Some(window(20.0, 10080, 100)),
            )],
            vec![],
        );
        let off = Settings::default().apply(Choice::Alert(Alert::Off));
        let plain = panel_view_with(&r, now(), &Utc, LoginItem::Hidden, false, &off, Lang::Es);
        let alerted = panel_view_with(
            &r,
            now(),
            &Utc,
            LoginItem::Hidden,
            false,
            &Settings::default(),
            Lang::Es,
        );
        assert_eq!(alerted.height, plain.height + ALERT_HEIGHT);

        let no_five = Settings::default().apply(Choice::ToggleFiveHour);
        let view = panel_view_with(
            &r,
            now(),
            &Utc,
            LoginItem::Hidden,
            false,
            &no_five,
            Lang::Es,
        );
        assert!(!view.html.contains("5 horas") && view.html.contains("Semana"));
        assert!(
            !view.html.contains("Pasaste"),
            "the hidden window cannot alert"
        );
        assert!(view.height < plain.height);

        let mut claude = codex(Some(window(10.0, 300, 3)), None);
        claude.limit_id = "claude".into();
        let both = report(
            vec![codex(Some(window(80.0, 300, 3)), None), claude],
            vec![],
        );
        let no_claude = Settings::default().apply(Choice::ToggleClaude);
        let html = panel_view_with(
            &both,
            now(),
            &Utc,
            LoginItem::Hidden,
            false,
            &no_claude,
            Lang::Es,
        )
        .html;
        assert!(html.contains("Codex") && !html.contains("Claude"));
    }

    // C3 / A5
    #[test]
    fn the_footer_has_a_refresh_button_that_shows_progress_and_the_page_has_a_csp() {
        let empty = report(vec![], vec![]);
        let page = |refreshing| {
            panel_view_with(
                &empty,
                now(),
                &Utc,
                LoginItem::Off,
                refreshing,
                &Settings::default(),
                Lang::Es,
            )
        };
        let idle = page(false);
        assert!(idle.html.contains("Actualizar ahora") && idle.html.contains("'refresh'"));
        let busy = page(true);
        assert!(busy.html.contains("Actualizando…") && !busy.html.contains("'refresh'"));
        assert_eq!(idle.height, busy.height);
        // CSP: no network, no external resources, and nothing that points outside the page.
        assert!(idle.html.contains("Content-Security-Policy"));
        assert!(idle.html.contains("default-src 'none'"));
        assert!(!idle.html.contains("http://") && !idle.html.contains("https://"));
        assert!(!idle.html.contains("<script src") && !idle.html.contains("<link"));
        // The browser's own menu and reload keys are blocked.
        assert!(idle.html.contains("contextmenu") && idle.html.contains("preventDefault"));
    }

    #[test]
    fn every_choice_message_parses_back_and_unknown_ones_do_not() {
        for choice in ALL_CHOICES {
            assert_eq!(parse_choice(choice_message(choice)), Some(choice));
        }
        for message in ["", "toggle:", "theme:sepia", "alert:75 ", "quit"] {
            assert_eq!(parse_choice(message), None, "{message}");
        }
    }

    #[test]
    fn the_footer_opens_the_settings_view() {
        let empty = report(vec![], vec![]);
        let html = panel_view_with(
            &empty,
            now(),
            &Utc,
            LoginItem::Off,
            false,
            &Settings::default(),
            Lang::Es,
        )
        .html;
        assert!(html.contains("Ajustes") && html.contains("'settings'"));
    }

    #[test]
    fn the_settings_view_shows_the_current_state_and_sends_choices() {
        let s = Settings::default();
        let html = settings_view(&s, LoginItem::Hidden, Lang::Es).html;
        for choice in [
            Choice::ToggleFiveHour,
            Choice::ToggleWeek,
            Choice::ToggleCodex,
            Choice::ToggleClaude,
            Choice::Bar(BarWindow::Week),
            Choice::Alert(Alert::Off),
            Choice::Alert(Alert::At90),
            Choice::Theme(Theme::Light),
            Choice::Theme(Theme::Dark),
        ] {
            assert!(
                html.contains(&format!("'{}'", choice_message(choice))),
                "{choice:?}"
            );
        }
        // The active option is marked and does not send itself again.
        assert!(
            html.contains(r#"<button class="on">75 %</button>"#),
            "{html}"
        );
        assert!(html.contains(r#"<button class="on">Automático</button>"#));
        assert!(html.contains(r#"<button class="on">5 horas</button>"#));
        assert!(!html.contains("'alert:75'") && !html.contains("'theme:auto'"));
        assert_eq!(html.matches(r#"class="sw on""#).count(), 4);
        assert!(!html.contains("'autostart'"));
        assert!(html.contains("'back'") && html.contains("Content-Security-Policy"));
    }

    #[test]
    fn the_last_visible_one_cannot_be_switched_off_and_the_bar_choice_needs_both() {
        let s = Settings::default()
            .apply(Choice::ToggleWeek)
            .apply(Choice::ToggleClaude);
        let html = settings_view(&s, LoginItem::Hidden, Lang::Es).html;
        assert!(!html.contains("'toggle:five_hour'") && !html.contains("'toggle:codex'"));
        assert!(html.contains("'toggle:week'") && html.contains("'toggle:claude'"));
        assert!(
            !html.contains("'bar:"),
            "bar choice disabled with one window"
        );
        assert!(html.contains(r#"<button class="on" disabled>5 horas</button>"#));
    }

    #[test]
    fn the_theme_is_set_on_the_page_and_auto_follows_the_system() {
        use crate::settings::Choice;
        let empty = report(vec![], vec![]);
        let page_for = |theme| {
            let s = Settings::default().apply(Choice::Theme(theme));
            (
                panel_view_with(&empty, now(), &Utc, LoginItem::Off, false, &s, Lang::Es).html,
                settings_view(&s, LoginItem::Hidden, Lang::Es).html,
            )
        };
        let (main, settings) = page_for(Theme::Auto);
        assert!(!main.contains("data-theme=\"") && !settings.contains("data-theme=\""));
        assert!(main.contains("prefers-color-scheme:dark"));
        let (main, settings) = page_for(Theme::Dark);
        assert!(main.contains(r#"<html lang="es" data-theme="dark">"#));
        assert!(settings.contains(r#"data-theme="dark""#));
        let (main, _) = page_for(Theme::Light);
        assert!(main.contains(r#"data-theme="light""#));
        assert!(main.contains(":root[data-theme=dark]"));
    }

    #[test]
    fn the_settings_view_shows_the_app_version() {
        let view = settings_view(&Settings::default(), LoginItem::Hidden, Lang::Es);
        let expected = format!("CuantoQuedapp {}", env!("CARGO_PKG_VERSION"));
        assert_eq!(VERSION_TEXT, expected);
        assert!(
            view.html
                .contains(&format!(r#"<div class="ver">{expected}</div>"#))
        );
        // Only in the settings view, not in the limits view.
        let empty = report(vec![], vec![]);
        let main = panel_view_with(
            &empty,
            now(),
            &Utc,
            LoginItem::Off,
            false,
            &Settings::default(),
            Lang::Es,
        );
        assert!(!main.html.contains("CuantoQuedapp "));
    }
}
