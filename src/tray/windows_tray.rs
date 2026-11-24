//! Windows tray: no text, so the percentage is drawn into the icon (see `tray_badge`).
//! Compiled on every platform so it is type-checked on the Mac too; only used on Windows.
use chrono::{DateTime, Utc};
use tray_icon::{Icon, TrayIcon};

use crate::{
    i18n::Lang,
    monitor::Report,
    settings::Settings,
    tray_badge::{self, BadgeState, SIZE},
};

pub fn icon(state: BadgeState) -> Option<Icon> {
    Icon::from_rgba(tray_badge::render(state), SIZE as u32, SIZE as u32).ok()
}

/// What the tray already shows, so the icon is only replaced when it changes
/// (refreshing every second would churn GDI handles and flicker).
#[derive(Default)]
pub struct Shown {
    badge: Option<BadgeState>,
    tooltip: Option<String>,
}

/// Best effort: a failed update keeps the previous icon and is retried on the next tick.
pub fn refresh(
    tray: &TrayIcon,
    shown: &mut Shown,
    report: &Report,
    now: DateTime<Utc>,
    settings: &Settings,
    lang: Lang,
) {
    let badge = tray_badge::badge_state_with(report, now, settings);
    if shown.badge != Some(badge)
        && let Some(new_icon) = icon(badge)
        && tray.set_icon(Some(new_icon)).is_ok()
    {
        shown.badge = Some(badge);
    }
    let tooltip = tray_badge::tooltip_with(report, now, settings, lang);
    if shown.tooltip.as_deref() != Some(tooltip.as_str())
        && tray.set_tooltip(Some(&tooltip)).is_ok()
    {
        shown.tooltip = Some(tooltip);
    }
}
