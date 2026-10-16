//! Light theme as a last pass over the finished frame: everything is drawn
//! for a dark background, then `apply` repaints it for paper. Neutral colors
//! flip their lightness (white text → ink), saturated ones deepen so they stay
//! readable on paper, and cells with text on their own background (caret,
//! pepper faces, miss highlights) are left alone. Cells whose text and
//! background match are solid blocks (big digits, logo) and deepen like text.

use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::buffer::Buffer;
use ratatui::style::Color;

// ponytail: one global switch; pass a Theme through draw if themes ever multiply
static LIGHT: AtomicBool = AtomicBool::new(false);

pub fn set_light(on: bool) {
    LIGHT.store(on, Ordering::Relaxed);
}

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

const PAPER: Color = Color::Rgb(250, 247, 240);

/// The page behind everything: canvas padding on the web, and the color
/// sprites use for their empty pixels.
pub fn page() -> Color {
    if is_light() { PAPER } else { Color::Black }
}
const INK: Color = Color::Rgb(28, 26, 30);
/// Max lightness a saturated color keeps on paper.
const DEEPEN: f64 = 0.42;

/// Repaint a dark-theme frame for the light theme (no-op in dark).
pub fn apply(buf: &mut Buffer) {
    if !is_light() {
        return;
    }
    for cell in buf.content.iter_mut() {
        if cell.bg == Color::Reset {
            cell.bg = PAPER;
            cell.fg = on_paper(cell.fg);
        } else if cell.fg == cell.bg {
            cell.bg = on_paper(cell.bg);
            cell.fg = cell.bg;
        }
    }
}

fn on_paper(c: Color) -> Color {
    let (r, g, b) = match c {
        Color::Reset => return INK,
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::White => (240, 240, 240),
        Color::Gray => (190, 190, 190),
        Color::DarkGray => (120, 120, 120),
        Color::Red => (220, 40, 40),
        Color::LightRed => (255, 100, 100),
        Color::Green => (60, 180, 60),
        Color::Yellow => (230, 200, 0),
        Color::Cyan => (0, 190, 200),
        Color::Magenta => (220, 0, 200),
        other => return other,
    };
    let (h, s, l) = to_hsl(r, g, b);
    let l = if s < 0.25 { 1.0 - l } else { l.min(DEEPEN) };
    from_hsl(h, s, l)
}

fn to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |u: f64| ((u + m) * 255.0).round() as u8;
    Color::Rgb(to(r), to(g), to(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn lightness(c: Color) -> f64 {
        let Color::Rgb(r, g, b) = c else {
            panic!("{c:?}")
        };
        to_hsl(r, g, b).2
    }

    #[test]
    fn repaints_for_paper() {
        set_light(true);
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        buf[(0, 0)].set_fg(Color::White);
        buf[(1, 0)].set_fg(Color::Rgb(255, 215, 0)); // serrano yellow
        buf[(2, 0)].set_fg(Color::Black).set_bg(Color::Yellow); // caret on a space
        let gold = Color::Rgb(255, 215, 0);
        buf[(3, 0)].set_symbol("█").set_fg(gold).set_bg(gold); // big digit block
        apply(&mut buf);
        set_light(false);

        assert_eq!(buf[(0, 0)].bg, PAPER);
        assert!(lightness(buf[(0, 0)].fg) < 0.1, "white text becomes ink");
        assert!(lightness(buf[(1, 0)].fg) <= DEEPEN + 0.01, "yellow deepens");
        assert_eq!(buf[(2, 0)].bg, Color::Yellow, "text on own background kept");
        assert!(
            lightness(buf[(3, 0)].bg) <= DEEPEN + 0.01,
            "solid blocks deepen"
        );
        assert_eq!(buf[(3, 0)].fg, buf[(3, 0)].bg);
    }

    #[test]
    fn hsl_round_trips() {
        for (r, g, b) in [(255, 140, 0), (40, 170, 50), (255, 0, 200), (12, 34, 56)] {
            let (h, s, l) = to_hsl(r, g, b);
            assert_eq!(from_hsl(h, s, l), Color::Rgb(r, g, b));
        }
    }
}
