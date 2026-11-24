//! macOS menu bar: one status item with each tool's badge inline before its number, like
//! `[>_] 54%  [✱] 45%` (see `mac_title`).
use chrono::{DateTime, Utc};
use tray_icon::TrayIcon;

use crate::{
    i18n::Lang,
    mac_title::{self, Entry},
    monitor::Report,
    presentation,
    settings::{Settings, apply_visibility},
};

/// What the item shows now, so the system is touched only when something changes.
#[derive(Default)]
pub struct Shown {
    entries: Option<Vec<Entry>>,
    /// The plain "reader lost" title, while it is the one shown.
    lost: Option<String>,
}

pub fn refresh(
    tray: &TrayIcon,
    shown: &mut Shown,
    report: &Report,
    reader_lost: bool,
    now: DateTime<Utc>,
    settings: &Settings,
    lang: Lang,
) {
    if reader_lost {
        // No badges: the text names the problem on its own.
        let title = presentation::title_reader_lost(lang);
        if shown.lost.as_ref() != Some(&title) {
            tray.set_title(Some(&title));
            shown.entries = None;
            shown.lost = Some(title);
        }
        return;
    }
    shown.lost = None;
    let parts = presentation::title_parts_with(report, now, settings, lang);
    let last = parts.tools.len().saturating_sub(1);
    let entries: Vec<Entry> = parts
        .tools
        .iter()
        .zip(&parts.values)
        .zip(&parts.alerts)
        .enumerate()
        .map(|(index, ((tool, value), alert))| Entry {
            tool,
            value: value.clone(),
            // The remark belongs to the last number, the one at the right end.
            note: if index == last {
                parts.note.clone()
            } else {
                String::new()
            },
            alert: *alert,
        })
        .collect();
    if shown.entries.as_ref() == Some(&entries) {
        return;
    }
    if mac_title::set_inline_title(tray, &entries) {
        shown.entries = Some(entries);
    } else {
        // Plain text with the names keeps everything readable without the badges.
        tray.set_title(Some(presentation::title(
            &apply_visibility(report, settings),
            now,
            lang,
        )));
        shown.entries = None;
    }
}
