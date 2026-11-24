//! macOS only: sets the status item title as one attributed string with the tool badges drawn
//! inline, like `[>_] 54%  [✱] 45%`, using the system font of the menu bar. A number at or
//! above the alert threshold is drawn in the system red.
//!
//! `tray-icon` only offers a plain title plus one image, so this reaches the native status
//! item it exposes. Everything fails soft: `false` means "nothing was changed" and the caller
//! keeps a plain text title.
use objc2::{AnyThread, MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{
    NSAttributedStringAttachmentConveniences, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSImage, NSTextAttachment,
};
use objc2_foundation::{
    NSAttributedString, NSData, NSDictionary, NSMutableAttributedString, NSPoint, NSRect, NSSize,
    NSString,
};
use tray_icon::TrayIcon;

use crate::{menu_bar_icons, png};

/// Badge size in points (the image itself is 32 px, so it stays sharp on Retina).
const BADGE_POINTS: f64 = 15.0;
/// Moves the badge down so it sits on the text baseline.
const BASELINE_SHIFT: f64 = -3.0;
const SEPARATOR: &str = "  ";

/// What follows one tool's badge.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Limit id (`"codex"`, `"claude"`), which picks the badge.
    pub tool: &'static str,
    /// The number, like `"54%"`.
    pub value: String,
    /// Remark after the number (` · old data`), possibly empty. Never red.
    pub note: String,
    /// The value is drawn in red: the tool is at or above the alert threshold.
    pub alert: bool,
}

/// One entry per tool. `false` if anything is unavailable.
pub fn set_inline_title(item: &TrayIcon, entries: &[Entry]) -> bool {
    let (Some(mtm), Some(status_item)) = (MainThreadMarker::new(), item.ns_status_item()) else {
        return false;
    };
    let Some(button) = status_item.button(mtm) else {
        return false;
    };
    let Some(text) = build(entries) else {
        return false;
    };
    // The inline badges replace the item's image.
    button.setImage(None);
    button.setAttributedTitle(&text);
    true
}

fn build(entries: &[Entry]) -> Option<Retained<NSMutableAttributedString>> {
    let font = NSFont::menuBarFontOfSize(0.0);
    let normal: Retained<NSDictionary<_, AnyObject>> = unsafe {
        NSDictionary::from_slices(&[NSFontAttributeName], &[font.as_ref() as &AnyObject])
    };
    // The system red adapts to light and dark menu bars.
    let red_color = NSColor::systemRedColor();
    let red: Retained<NSDictionary<_, AnyObject>> = unsafe {
        NSDictionary::from_slices(
            &[NSFontAttributeName, NSForegroundColorAttributeName],
            &[
                font.as_ref() as &AnyObject,
                red_color.as_ref() as &AnyObject,
            ],
        )
    };
    let result = NSMutableAttributedString::new();
    for (index, entry) in entries.iter().enumerate() {
        let badge = menu_bar_icons::badge(entry.tool)?;
        let bytes = png::encode_rgba(&badge.rgba, badge.width, badge.height)?;
        let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&bytes))?;
        image.setSize(NSSize::new(BADGE_POINTS, BADGE_POINTS));
        let attachment = NSTextAttachment::new();
        attachment.setImage(Some(&image));
        attachment.setBounds(NSRect::new(
            NSPoint::new(0.0, BASELINE_SHIFT),
            NSSize::new(BADGE_POINTS, BADGE_POINTS),
        ));
        result.appendAttributedString(&NSAttributedString::attributedStringWithAttachment(
            &attachment,
        ));
        let separator = if index + 1 < entries.len() {
            SEPARATOR
        } else {
            ""
        };
        let value_style = if entry.alert { &red } else { &normal };
        append(&result, &format!(" {}", entry.value), value_style);
        append(&result, &format!("{}{separator}", entry.note), &normal);
    }
    Some(result)
}

fn append(
    result: &NSMutableAttributedString,
    text: &str,
    attributes: &NSDictionary<NSString, AnyObject>,
) {
    if text.is_empty() {
        return;
    }
    let styled = unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(attributes),
        )
    };
    result.appendAttributedString(&styled);
}
