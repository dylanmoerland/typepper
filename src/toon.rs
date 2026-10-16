//! 2D cartoon renderer. A scene is a function from a point to a color (or
//! nothing); it's sampled once per quadrant pixel every frame and each cell
//! becomes a quadrant glyph (`▘▝▀▖▌▞▛▗▚▐▜▄▙▟`) with the two colors that cover
//! its 2×2 pixels best. Shapes are continuous, so motion is smooth at 60fps.
//!
//! Scene space mirrors a terminal cell of 10×20 units: origin at the area's
//! center, y down, one quadrant pixel = 5×10 units.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

pub type Rgb = [f64; 3];
pub type Px = Option<Rgb>;

const PX_W: f64 = 5.0;
const PX_H: f64 = 10.0;
/// Quadrant glyph by mask: top-left 1, top-right 2, bottom-left 4, bottom-right 8.
const QUADS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

/// Render `scene` into `area`; `scale` > 1 zooms out (scene units per unit).
/// Fully empty cells are left untouched so whatever is behind shows through.
pub fn draw(buf: &mut Buffer, area: Rect, scale: f64, scene: impl Fn(f64, f64) -> Px) {
    render(buf, area, scale, None, scene);
}

/// Like `draw`, but lit as if the flat shapes were rounded: `heat` (0..1)
/// sets how strongly the flames below rim-light them.
pub fn draw_lit(
    buf: &mut Buffer,
    area: Rect,
    scale: f64,
    heat: f64,
    scene: impl Fn(f64, f64) -> Px,
) {
    render(buf, area, scale, Some(heat), scene);
}

fn render(
    buf: &mut Buffer,
    area: Rect,
    scale: f64,
    heat: Option<f64>,
    scene: impl Fn(f64, f64) -> Px,
) {
    let area = area.intersection(buf.area);
    let (ox, oy) = (area.width as f64 * PX_W, area.height as f64 * PX_H);
    let (pw, ph) = (area.width as usize * 2, area.height as usize * 2);
    let mut px: Vec<Px> = (0..pw * ph)
        .map(|k| {
            let (i, j) = ((k % pw) as f64 + 0.5, (k / pw) as f64 + 0.5);
            scene((i * PX_W - ox) * scale, (j * PX_H - oy) * scale)
        })
        .collect();
    if let Some(heat) = heat {
        shade(&mut px, pw, ph, heat);
    }
    let page = page_rgb();
    for row in 0..area.height {
        for col in 0..area.width {
            let at = |k: usize| px[(row as usize * 2 + k / 2) * pw + col as usize * 2 + k % 2];
            let q: [Px; 4] = std::array::from_fn(at);
            if q.iter().all(Option::is_none) {
                continue;
            }
            let (glyph, fg, bg) = encode(q, page);
            // explicit colors on both sides: the light theme leaves sprites alone
            buf[(area.x + col, area.y + row)]
                .set_symbol(glyph)
                .set_fg(color(fg))
                .set_bg(color(bg));
        }
    }
}

/// Fake 3D: each silhouette's distance from its edge (two-pass chamfer, in
/// scene units since pixels are 5×10) becomes a dome height, the slope of
/// that dome a normal, and the normal gets a key light from the upper left,
/// a glint, and an orange rim from the flames below. Inky pixels (outlines,
/// pupils) stay inky so faces keep their definition.
fn shade(px: &mut [Px], pw: usize, ph: usize, heat: f64) {
    const DIAG: f64 = 11.2; // hypot(PX_W, PX_H)
    const DOME: f64 = 38.0;
    let mut d: Vec<f64> = px
        .iter()
        .map(|p| if p.is_some() { f64::MAX } else { 0.0 })
        .collect();
    let relax = |d: &mut Vec<f64>, k: usize, n: usize, w: f64| {
        if d[n] + w < d[k] {
            d[k] = d[n] + w;
        }
    };
    for j in 0..ph {
        for i in 0..pw {
            let k = j * pw + i;
            if d[k] == 0.0 {
                continue;
            }
            if i > 0 {
                relax(&mut d, k, k - 1, PX_W);
            }
            if j > 0 {
                relax(&mut d, k, k - pw, PX_H);
                if i > 0 {
                    relax(&mut d, k, k - pw - 1, DIAG);
                }
                if i + 1 < pw {
                    relax(&mut d, k, k - pw + 1, DIAG);
                }
            }
        }
    }
    for j in (0..ph).rev() {
        for i in (0..pw).rev() {
            let k = j * pw + i;
            if d[k] == 0.0 {
                continue;
            }
            if i + 1 < pw {
                relax(&mut d, k, k + 1, PX_W);
            }
            if j + 1 < ph {
                relax(&mut d, k, k + pw, PX_H);
                if i + 1 < pw {
                    relax(&mut d, k, k + pw + 1, DIAG);
                }
                if i > 0 {
                    relax(&mut d, k, k + pw - 1, DIAG);
                }
            }
        }
    }
    let height = |k: usize| (d[k].min(DOME) / DOME).sqrt() * DOME;
    let norm = |v: [f64; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v.map(|x| x / l)
    };
    let light = norm([-0.45, -0.7, 0.55]);
    let half = norm([light[0], light[1], light[2] + 1.0]); // viewer at +z
    for j in 0..ph {
        for i in 0..pw {
            let k = j * pw + i;
            let Some(c) = px[k] else { continue };
            if c[0] + c[1] + c[2] < 60.0 {
                continue;
            }
            let gx = (height(if i + 1 < pw { k + 1 } else { k })
                - height(if i > 0 { k - 1 } else { k }))
                / (2.0 * PX_W);
            let gy = (height(if j + 1 < ph { k + pw } else { k })
                - height(if j > 0 { k - pw } else { k }))
                / (2.0 * PX_H);
            let n = norm([-gx, -gy, 1.0]);
            let diffuse = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
            let spec = (n[0] * half[0] + n[1] * half[1] + n[2] * half[2])
                .max(0.0)
                .powi(40);
            let rim = (n[1] * 0.9 + 0.1).max(0.0).powi(2) * heat; // flames light it from below
            let k2 = 0.55 + 0.6 * diffuse;
            let flame = [255.0, 120.0, 30.0];
            px[k] = Some(std::array::from_fn(|ch| {
                (c[ch] * k2 + 255.0 * spec * 0.4 + flame[ch] * rim * 0.5).min(255.0)
            }));
        }
    }
}

/// Pick the two most different colors of a cell's 4 pixels; every pixel
/// takes whichever is nearer. Empty pixels count as the page color.
fn encode(q: [Px; 4], page: Rgb) -> (&'static str, Rgb, Rgb) {
    let c = q.map(|p| p.unwrap_or(page));
    let (mut a, mut b, mut best) = (0, 0, -1.0);
    for m in 0..4 {
        for n in m + 1..4 {
            let d = dist2(c[m], c[n]);
            if d > best {
                (a, b, best) = (m, n, d);
            }
        }
    }
    if best <= 0.0 {
        return (" ", c[a], c[a]);
    }
    // empty pixels go to the background, so glyph seams blend into the page
    if q[a].is_some() && q[b].is_none() {
        (a, b) = (b, a);
    }
    let mask = (0..4)
        .filter(|&k| dist2(c[k], c[b]) < dist2(c[k], c[a]))
        .fold(0, |m, k| m | 1 << k);
    (QUADS[mask], c[b], c[a])
}

fn page_rgb() -> Rgb {
    match crate::theme::page() {
        Color::Rgb(r, g, b) => [r as f64, g as f64, b as f64],
        _ => [0.0; 3],
    }
}

fn color(c: Rgb) -> Color {
    Color::Rgb(c[0] as u8, c[1] as u8, c[2] as u8)
}

fn dist2(a: Rgb, b: Rgb) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

// ── shapes: signed distances, negative inside ───────────────────────────

pub fn circle(x: f64, y: f64, cx: f64, cy: f64, r: f64) -> f64 {
    (x - cx).hypot(y - cy) - r
}

pub fn ellipse(x: f64, y: f64, cx: f64, cy: f64, rx: f64, ry: f64) -> f64 {
    (((x - cx) / rx).hypot((y - cy) / ry) - 1.0) * rx.min(ry)
}

#[allow(clippy::too_many_arguments)]
pub fn capsule(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64, r: f64) -> f64 {
    let (pax, pay, bax, bay) = (x - ax, y - ay, bx - ax, by - ay);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
    (pax - bax * h).hypot(pay - bay * h) - r
}

/// Distance to a tube along `center(s)` with radius `radius(s)`, s in 0..1.
/// Returns (distance, s at the closest point, -1..1 across the tube).
pub fn tube(
    x: f64,
    y: f64,
    center: impl Fn(f64) -> (f64, f64),
    radius: impl Fn(f64) -> f64,
) -> (f64, f64, f64) {
    const N: usize = 30;
    let mut best = (f64::MAX, 0.0, 0.0);
    for k in 0..N {
        let s = k as f64 / (N - 1) as f64;
        let ((cx, cy), r) = (center(s), radius(s));
        let d = (x - cx).hypot(y - cy) - r;
        if d < best.0 {
            best = (d, s, (x - cx) / r.max(1.0));
        }
    }
    best
}

/// Rotate a point around (cx, cy).
pub fn rot(x: f64, y: f64, cx: f64, cy: f64, a: f64) -> (f64, f64) {
    let (s, c) = a.sin_cos();
    let (dx, dy) = (x - cx, y - cy);
    (cx + dx * c + dy * s, cy - dx * s + dy * c)
}

pub fn mix(a: Rgb, b: Rgb, k: f64) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k)
}

pub fn mul(a: Rgb, k: f64) -> Rgb {
    a.map(|v| (v * k).clamp(0.0, 255.0))
}

pub fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Cheap deterministic noise in 0..1.
pub fn hash(n: f64) -> f64 {
    let s = (n * 127.1).sin() * 43_758.545;
    s - s.floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_quadrants_and_keeps_empty_cells() {
        let red = Some([255.0, 0.0, 0.0]);
        assert_eq!(encode([red, None, red, None], [0.0; 3]).0, "▌");
        assert_eq!(encode([red, red, red, red], [0.0; 3]).0, " ");

        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        buf[(1, 0)].set_symbol("x");
        draw(&mut buf, area, 1.0, |x, _| {
            (x < 0.0).then_some([0.0, 200.0, 0.0])
        });
        assert_eq!(buf[(0, 0)].bg, Color::Rgb(0, 200, 0), "left cell painted");
        assert_eq!(buf[(1, 0)].symbol(), "x", "empty cell untouched");
    }
}
