//! The badges shown to the left of the menu bar text (macOS): one small icon per tool, in the
//! same order as the numbers of the title, drawn by hand into an RGBA image.
//!
//! Same designs as the panel (`panel_view`): a terminal prompt for Codex, an eight-point star
//! for Claude. They are our own drawings, not the vendors' logos. Pure code, no window.

const BADGE: u32 = 32;
/// Sub-samples per axis for smooth edges.
const SAMPLES: u32 = 4;

pub struct MenuIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy)]
struct Segment(f32, f32, f32, f32);

struct Badge {
    color: [u8; 3],
    stroke: f32,
    segments: &'static [Segment],
}

/// Coordinates on a 20x20 grid, like the SVGs of the panel.
fn badge_for(tool: &str) -> Option<Badge> {
    match tool {
        "codex" => Some(Badge {
            color: [0x4c, 0x5f, 0xd5],
            stroke: 1.8,
            segments: &[
                Segment(5.5, 7.0, 9.0, 10.0),
                Segment(9.0, 10.0, 5.5, 13.0),
                Segment(11.0, 13.5, 14.5, 13.5),
            ],
        }),
        "claude" => Some(Badge {
            color: [0xd9, 0x77, 0x4f],
            stroke: 1.7,
            segments: &[
                Segment(10.0, 4.5, 10.0, 15.5),
                Segment(4.5, 10.0, 15.5, 10.0),
                Segment(6.1, 6.1, 13.9, 13.9),
                Segment(13.9, 6.1, 6.1, 13.9),
            ],
        }),
        _ => None,
    }
}

/// One square badge without margins (used inline in the title text).
pub fn badge(tool: &str) -> Option<MenuIcon> {
    let badge = badge_for(tool)?;
    let mut rgba = Vec::with_capacity((BADGE * BADGE * 4) as usize);
    for y in 0..BADGE {
        for x in 0..BADGE {
            rgba.extend_from_slice(&shade(&badge, x, y));
        }
    }
    Some(MenuIcon {
        rgba,
        width: BADGE,
        height: BADGE,
    })
}

/// Anti-aliased colour of one badge pixel by averaging sub-samples.
fn shade(badge: &Badge, x: u32, y: u32) -> [u8; 4] {
    let scale = 20.0 / BADGE as f32;
    let (mut inside, mut white) = (0u32, 0u32);
    for sy in 0..SAMPLES {
        for sx in 0..SAMPLES {
            let px = (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * scale;
            let py = (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * scale;
            if in_rounded_square(px, py) {
                inside += 1;
                if badge
                    .segments
                    .iter()
                    .any(|segment| distance(*segment, px, py) <= badge.stroke / 2.0)
                {
                    white += 1;
                }
            }
        }
    }
    if inside == 0 {
        return [0, 0, 0, 0];
    }
    let colored = inside - white;
    let mix = |channel: u8| {
        ((u32::from(channel) * colored + 255 * white) as f32 / inside as f32).round() as u8
    };
    let total = SAMPLES * SAMPLES;
    let alpha = (inside as f32 / total as f32 * 255.0).round() as u8;
    [
        mix(badge.color[0]),
        mix(badge.color[1]),
        mix(badge.color[2]),
        alpha,
    ]
}

/// 20x20 square with 5-unit corners.
fn in_rounded_square(x: f32, y: f32) -> bool {
    const RADIUS: f32 = 5.0;
    if !(0.0..=20.0).contains(&x) || !(0.0..=20.0).contains(&y) {
        return false;
    }
    let cx = x.clamp(RADIUS, 20.0 - RADIUS);
    let cy = y.clamp(RADIUS, 20.0 - RADIUS);
    (x - cx).powi(2) + (y - cy).powi(2) <= RADIUS * RADIUS
}

/// Distance from a point to a segment.
fn distance(Segment(x1, y1, x2, y2): Segment, px: f32, py: f32) -> f32 {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared == 0.0 {
        0.0
    } else {
        (((px - x1) * dx + (py - y1) * dy) / length_squared).clamp(0.0, 1.0)
    };
    ((px - (x1 + t * dx)).powi(2) + (py - (y1 + t * dy)).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(icon: &MenuIcon, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * icon.width + x) * 4) as usize;
        icon.rgba[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn a_badge_is_square_has_no_margin_and_only_known_tools_have_one() {
        let icon = badge("claude").unwrap();
        assert_eq!((icon.width, icon.height), (BADGE, BADGE));
        assert_eq!(icon.rgba.len(), (BADGE * BADGE * 4) as usize);
        assert!(badge("x").is_none());
        assert!(badge("").is_none());
        assert!(badge("codex").is_some());
    }

    #[test]
    fn badges_have_their_colour_with_transparent_corners() {
        let codex = badge("codex").unwrap();
        let claude = badge("claude").unwrap();
        // The very corner is cut by the rounding.
        assert_eq!(pixel(&codex, 0, 0)[3], 0);
        assert_eq!(pixel(&claude, BADGE - 1, 0)[3], 0);
        // A background pixel of each badge is opaque and has its colour.
        assert_eq!(pixel(&codex, 4, 16), [0x4c, 0x5f, 0xd5, 255]);
        assert_eq!(pixel(&claude, 4, 28), [0xd9, 0x77, 0x4f, 255]);
    }

    #[test]
    fn glyphs_are_white_and_differ_between_tools() {
        let codex = badge("codex").unwrap();
        let claude = badge("claude").unwrap();
        // The star crosses the centre of the claude badge; the prompt chevron tip is near it.
        assert_eq!(pixel(&claude, 16, 16), [255, 255, 255, 255]);
        assert_eq!(pixel(&codex, 14, 16), [255, 255, 255, 255]);
        let row =
            |icon: &MenuIcon| -> Vec<[u8; 4]> { (0..BADGE).map(|x| pixel(icon, x, 12)).collect() };
        assert_ne!(row(&codex), row(&claude));
    }
}
