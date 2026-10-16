//! The six foe peppers, drawn with `toon`. One mood system (idle, flinch,
//! grin, wind-up, laugh, scared, dead) expressed through six different
//! faces, plus each foe's attack timing. Ported 1:1 from the HTML preview,
//! in its scene units (a cell is 10×20).

use std::f64::consts::PI;

use crate::toon::{Px, Rgb, capsule, circle, ellipse, hash, mix, mul, rot, smooth};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Cayenne,
    Naga,
    Pimiento,
    Morita,
    Bonnet,
    Reaper,
}

impl Kind {
    /// Seconds from the start of an attack until it lands (−1♥).
    pub fn hit_delay(self) -> f64 {
        match self {
            Kind::Cayenne => 0.55,
            Kind::Naga => 0.8,
            Kind::Pimiento => 0.55,
            Kind::Morita => 1.0,
            Kind::Bonnet => 1.2,
            Kind::Reaper => 0.25,
        }
    }

    /// Seconds the whole attack takes.
    pub fn attack_len(self) -> f64 {
        match self {
            Kind::Cayenne => 1.2,
            Kind::Naga => 1.3,
            Kind::Pimiento => 1.6,
            Kind::Morita => 2.3,
            Kind::Bonnet => 1.8,
            Kind::Reaper => 1.6,
        }
    }

    /// The whole foe at the origin, roaming-size. `attack` is the age of a
    /// running attack (some foes change shape mid-attack).
    pub fn sprite(self, x: f64, y: f64, t: f64, m: &Mood, attack: Option<f64>) -> Px {
        match self {
            Kind::Naga => {
                let (mut x, mut y) = (x, y);
                if let Some(a) = attack {
                    let dart = smooth((a - 0.6) / 0.12) * (1.0 - smooth((a - 0.9) / 0.2));
                    x -= dart * 30.0; // the head darts at the corner to bite
                    y -= dart * 40.0;
                }
                (x.abs() < 200.0 && y > -200.0 && y < 160.0)
                    .then(|| naga_full(x / 0.8, y / 0.8, t, m))
                    .flatten()
            }
            Kind::Reaper => (x.abs() < 220.0 && y.abs() < 200.0)
                .then(|| reaper(x / 0.62, y / 0.62, t, m))
                .flatten(),
            Kind::Cayenne => (x.abs() < 90.0 && y > -140.0 && y < 110.0)
                .then(|| cayenne(x / 0.55, (y + 52.0) / 0.55, t, m))
                .flatten(),
            Kind::Pimiento => (x.abs() < 130.0 && y.abs() < 120.0)
                .then(|| pimiento(x / 0.5, y / 0.5, t, m))
                .flatten(),
            Kind::Morita => (x.abs() < 110.0 && y.abs() < 120.0)
                .then(|| morita(x / 0.55, (y + 34.0) / 0.55, t, m))
                .flatten(),
            Kind::Bonnet => {
                let hat_out = attack.is_some_and(|a| a > 0.8 && a < 1.65);
                if x.abs() > 110.0 || !(-120.0..=100.0).contains(&y) || (hat_out && y < -18.0) {
                    return None; // its hat is out there, boomeranging
                }
                bonnet(x / 0.5, (y + 20.0) / 0.5, t, m)
            }
        }
    }
}

const WHITE: Rgb = [245.0, 242.0, 235.0];
const BLACK: Rgb = [12.0, 10.0, 14.0];
const TONGUE: Rgb = [235.0, 80.0, 95.0];
const STEM: Rgb = [95.0, 120.0, 40.0];
const CALYX: Rgb = [35.0, 110.0, 40.0];
const SWEAT: Rgb = [120.0, 200.0, 255.0];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Expr {
    Idle,
    Flinch,
    Grin,
    Windup,
    Laugh,
    Scared,
    Dead,
}

/// A face's state this frame.
pub struct Mood {
    pub expr: Expr,
    /// Remaining health, 0..1: bruise, crack and sweat as it drops.
    pub hp: f64,
    /// White-hot flash on a fresh hit, 1 → 0.
    pub flash: f64,
    dx: f64,
    dy: f64,
    lid: f64,
    pupil: f64,
    look: f64,
    brow: f64,
}

impl Mood {
    pub fn new(expr: Expr, hp: f64, flash: f64, t: f64) -> Self {
        use Expr::*;
        Mood {
            expr,
            hp,
            flash,
            dx: match expr {
                Flinch => 8.0,
                Windup => (t * 50.0).sin() * 2.4,
                _ => 0.0,
            },
            dy: match expr {
                Flinch => -4.0,
                Laugh => -(t * 16.0).sin().abs() * 7.0,
                _ => 0.0,
            },
            lid: match expr {
                Grin => 0.45,
                Windup => 0.25,
                Idle => blink(t, 3.1, 0.3),
                _ => 0.0,
            },
            pupil: match expr {
                Scared => 0.25,
                Windup => 0.38,
                _ => 0.46,
            },
            look: if expr == Idle {
                [-1.0, 0.0, 1.0, 0.0][((t + 0.3) / 1.2) as usize % 4]
            } else {
                0.0
            },
            brow: match expr {
                Idle => 4.0,
                Grin => 10.0,
                Windup => 15.0,
                Scared => -12.0,
                Flinch => 7.0,
                Laugh | Dead => 0.0,
            },
        }
    }
}

fn blink(t: f64, period: f64, seed: f64) -> f64 {
    let p = (t + seed) % period;
    if p < 0.12 { (p / 0.12 * PI).sin() } else { 0.0 }
}

#[derive(Default)]
struct EyeStyle {
    lid: Option<f64>,
    lid_color: Rgb,
    rim: Option<Rgb>,
    sclera: Option<Rgb>,
    pupil: Option<f64>,
    pupil_color: Option<Rgb>,
    slit: bool,
}

/// One eye in any style, including the mood shapes (x x, > <, ^ ^).
#[allow(clippy::too_many_arguments)]
fn eye(
    x: f64,
    y: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    side: f64,
    m: &Mood,
    o: &EyeStyle,
) -> Px {
    match m.expr {
        Expr::Dead => {
            let r = rx.min(ry) * 0.7;
            let hit = capsule(x, y, cx - r, cy - r, cx + r, cy + r, 4.0) < 0.0
                || capsule(x, y, cx - r, cy + r, cx + r, cy - r, 4.0) < 0.0;
            return hit.then_some(BLACK);
        }
        Expr::Flinch => {
            let (d, w, h) = (-side, rx * 0.7, ry * 0.5);
            let hit = capsule(x, y, cx - w * d, cy - h, cx + w * d, cy, 4.0) < 0.0
                || capsule(x, y, cx + w * d, cy, cx - w * d, cy + h, 4.0) < 0.0;
            return hit.then_some(BLACK);
        }
        Expr::Laugh => {
            let a = ellipse(x, y, cx, cy + ry * 0.3, rx, ry * 0.75);
            return (a < 0.0 && a > -4.0 && y < cy + ry * 0.3).then_some(BLACK);
        }
        _ => {}
    }
    let d = ellipse(x, y, cx, cy, rx, ry);
    if d > 0.0 {
        return None;
    }
    if d > -3.2 {
        return Some(o.rim.unwrap_or(BLACK));
    }
    if y < cy - ry + o.lid.unwrap_or(m.lid) * 2.0 * ry {
        return Some(o.lid_color);
    }
    let (px, py) = (cx + m.look * rx * 0.4, cy);
    if circle(x, y, px - rx * 0.2, py - ry * 0.35, rx * 0.18) < 0.0 {
        return Some(WHITE);
    }
    let pupil = if o.slit {
        ellipse(x, y, px, py, rx * 0.16, ry * 0.75)
    } else {
        circle(x, y, px, py, o.pupil.unwrap_or(m.pupil) * rx.min(ry * 1.2))
    };
    if pupil < 0.0 {
        let angry = [190.0, 0.0, 0.0];
        return Some(
            o.pupil_color
                .unwrap_or(if m.expr == Expr::Windup { angry } else { BLACK }),
        );
    }
    Some(o.sclera.unwrap_or(WHITE))
}

#[allow(clippy::too_many_arguments)]
fn brow(
    x: f64,
    y: f64,
    cx: f64,
    cy: f64,
    len: f64,
    side: f64,
    tilt: f64,
    thick: f64,
    m: &Mood,
) -> Px {
    if m.expr == Expr::Dead {
        return None;
    }
    let (outer, inner) = (cx + side * len * 0.55, cx - side * len * 0.45);
    (capsule(x, y, outer, cy - tilt * 0.4, inner, cy + tilt, thick) < 0.0).then_some(BLACK)
}

/// The mouth for every mood; `rest` draws each foe's own idle mouth.
#[allow(clippy::too_many_arguments)]
fn mouth(
    x: f64,
    y: f64,
    cx: f64,
    cy: f64,
    w: f64,
    m: &Mood,
    t: f64,
    rest: &dyn Fn(f64, f64) -> Px,
    fangs: bool,
) -> Px {
    match m.expr {
        Expr::Idle => rest(x, y),
        Expr::Flinch => {
            if (x - cx).abs() < w * 0.7 && (y - cy).abs() < 9.0 {
                let edge = (y - cy).abs() > 7.0 || (x - cx).abs() > w * 0.7 - 2.0;
                let gap = (((x - cx + 60.0) % 10.0) - 5.0).abs() < 1.1 || (y - cy).abs() < 1.3;
                return Some(if edge || gap { BLACK } else { WHITE });
            }
            None
        }
        Expr::Grin | Expr::Laugh => {
            let e = ellipse(x, y, cx, cy - 10.0, w, w * 0.62);
            if e < 0.0 && y > cy - 8.0 {
                if e > -3.2 {
                    return Some(BLACK);
                }
                if m.expr == Expr::Grin {
                    if fangs {
                        let tooth =
                            (((x - cx + 60.0) % 16.0) - 8.0).abs() < 6.0 - (y - cy + 8.0) * 0.5;
                        return Some(if y < cy + 4.0 && tooth {
                            WHITE
                        } else {
                            [60.0, 0.0, 0.0]
                        });
                    }
                    let gap = (((x - cx + 60.0) % 11.0) - 5.5).abs() < 1.2;
                    return Some(if y < cy + 3.0 || !gap { WHITE } else { BLACK });
                }
                return Some(if y > cy + 10.0 {
                    TONGUE
                } else {
                    [60.0, 0.0, 0.0]
                });
            }
            None
        }
        Expr::Windup => {
            let e = ellipse(x, y, cx, cy + 2.0, w * 0.65, 18.0 + (t * 30.0).sin() * 2.5);
            if e < 0.0 {
                return Some(if e > -3.2 {
                    BLACK
                } else if y > cy + 8.0 {
                    TONGUE
                } else {
                    [60.0, 0.0, 0.0]
                });
            }
            let fang = ((x - cx).abs() - w * 0.35).abs() < 3.0 && y > cy - 16.0 && y < cy - 6.0;
            (fangs && fang).then_some(WHITE)
        }
        Expr::Scared => {
            let wave = cy + ((x - cx) * 0.3 + t * 12.0).sin() * 3.5;
            ((y - wave).abs() < 2.6 && (x - cx).abs() < w * 0.7).then_some(BLACK)
        }
        Expr::Dead => {
            if capsule(x, y, cx - w * 0.5, cy, cx + w * 0.5, cy, 3.0) < 0.0 {
                return Some(BLACK);
            }
            (capsule(x, y, cx + 8.0, cy + 2.0, cx + 12.0, cy + 20.0, 7.0) < 0.0).then_some(TONGUE)
        }
    }
}

/// Head fill: flash, dead tint, bruise, crack, cartoon outline, shine.
#[allow(clippy::too_many_arguments)]
fn head_skin(
    x: f64,
    y: f64,
    d: f64,
    u: f64,
    base: Rgb,
    outline: Rgb,
    m: &Mood,
    bruise: (f64, f64),
    crack_x: f64,
) -> Px {
    let mut base = base;
    if m.expr == Expr::Flinch {
        base = mix(base, [255.0, 215.0, 205.0], m.flash);
    }
    if m.expr == Expr::Dead {
        base = mix(base, [120.0, 110.0, 110.0], 0.6);
    }
    if m.hp <= 0.75 && m.expr != Expr::Dead && ellipse(x, y, bruise.0, bruise.1, 16.0, 11.0) < 0.0 {
        return Some(mix(base, [110.0, 40.0, 120.0], 0.6));
    }
    if m.hp <= 0.5
        && y > -40.0
        && y < 60.0
        && (x - (crack_x + (((y + 60.0) % 18.0) - 9.0).abs() * 1.4)).abs() < 1.8
    {
        return Some([45.0, 0.0, 0.0]);
    }
    let outline = if m.expr == Expr::Dead {
        [50.0, 30.0, 30.0]
    } else {
        outline
    };
    let shine = u < -0.45 && u > -0.72 && y > -60.0 && y < 40.0;
    skin(d, u, base, outline, shine.then(|| mix(base, WHITE, 0.42)))
}

fn skin(d: f64, u: f64, base: Rgb, outline: Rgb, shine: Option<Rgb>) -> Px {
    if d > 0.0 {
        return None;
    }
    if d > -5.5 {
        return Some(outline);
    }
    if shine.is_some() {
        return shine;
    }
    let edge = if u > 0.55 { 0.12 } else { 0.0 };
    Some(mul(base, 1.05 - 0.252 * u.clamp(-1.0, 1.0) - edge))
}

fn sweat(x: f64, y: f64, sx: f64, sy: f64, m: &Mood, t: f64) -> Px {
    if m.expr != Expr::Scared && !(m.hp <= 0.5 && m.expr != Expr::Dead) {
        return None;
    }
    let life = (t * 0.9) % 1.0;
    (ellipse(x, y, sx, sy + life * 45.0, 5.5, 9.0) < 0.0).then_some(SWEAT)
}

/// Mood motion: flinch jolt, wind-up shake, laugh bounce, dead slump.
fn shift(x: f64, y: f64, m: &Mood) -> (f64, f64) {
    let (x, y) = (x - m.dx, y - m.dy);
    if m.expr == Expr::Dead {
        let (x, y) = rot(x, y, 0.0, 160.0, 0.16);
        return (x, y - 14.0);
    }
    (x, y)
}

// ── the six ─────────────────────────────────────────────────────────────

/// CAPS CAYENNE: tall, leaning, smug; its stem whips like a ponytail.
fn cayenne(x: f64, y: f64, t: f64, m: &Mood) -> Px {
    let (x, y) = shift(x, y, m);
    let (base, outline) = ([215.0, 38.0, 28.0], [90.0, 10.0, 8.0]);
    let lean = |yy: f64| 34.0 * ((60.0 - yy) / 170.0).max(0.0).powi(2);
    let whip = (t * 6.0).sin() * 0.7;
    let (tx, ty) = (lean(-110.0), -110.0);
    if capsule(
        x,
        y,
        tx,
        ty,
        tx + whip.sin() * 34.0,
        ty - whip.cos() * 30.0,
        5.5,
    ) < 0.0
    {
        return Some(STEM);
    }
    if ellipse(x, y, tx, ty + 4.0, 22.0, 8.0) < 0.0 {
        return Some(CALYX);
    }
    let cx = lean(y);
    let d = ellipse(x, y, cx, 50.0, 78.0, 162.0);
    if d > 0.0 {
        return None;
    }
    let smug = EyeStyle {
        lid_color: mul(base, 0.85),
        lid: (m.expr == Expr::Idle).then_some(m.lid.max(0.3)),
        ..Default::default()
    };
    for side in [-1.0, 1.0] {
        if let Some(c) = eye(x, y, cx + side * 27.0, -18.0, 17.0, 26.0, side, m, &smug) {
            return Some(c);
        }
        if let Some(c) = brow(
            x,
            y,
            cx + side * 27.0,
            -54.0,
            40.0,
            side,
            m.brow + 6.0,
            3.2,
            m,
        ) {
            return Some(c);
        }
    }
    if let Some(c) = sweat(x, y, cx + 58.0, -60.0, m, t) {
        return Some(c);
    }
    let smirk = |x: f64, y: f64| {
        (capsule(x, y, cx - 22.0, 44.0, cx + 20.0, 34.0, 3.2) < 0.0
            || circle(x, y, cx + 23.0, 33.0, 3.5) < 0.0)
            .then_some(BLACK)
    };
    if let Some(c) = mouth(x, y, cx, 40.0, 30.0, m, t, &smirk, false) {
        return Some(c);
    }
    if (y * 0.3).sin() > 0.93 && y < -70.0 {
        return Some(mul(base, 0.8)); // tight wrinkles near the top
    }
    head_skin(
        x,
        y,
        d,
        (x - cx) / 78.0,
        base,
        outline,
        m,
        (cx + 46.0, 18.0),
        cx - 50.0,
    )
}

/// NUMBER NAGA's head: a cobra hood with eye-spots, slit pupils, forked tongue.
fn naga(x: f64, y: f64, t: f64, m: &Mood) -> Px {
    let (x, y) = shift(x, y, m);
    let (base, outline) = ([235.0, 85.0, 25.0], [100.0, 30.0, 5.0]);
    let th = (y - 55.0).atan2(x);
    let rx = 98.0 * (1.0 + 0.05 * (x * 0.1).sin() * (th * 3.0).cos());
    let d = ellipse(x, y, 0.0, 55.0, rx, 110.0);
    if d < 0.0 {
        let snake = EyeStyle {
            slit: true,
            sclera: Some([250.0, 215.0, 50.0]),
            lid_color: mul(base, 0.85),
            ..Default::default()
        };
        for side in [-1.0, 1.0] {
            if let Some(c) = eye(x, y, side * 38.0, 2.0, 21.0, 24.0, side, m, &snake) {
                return Some(c);
            }
            if let Some(c) = brow(x, y, side * 38.0, -32.0, 44.0, side, m.brow, 4.5, m) {
                return Some(c);
            }
        }
        if let Some(c) = sweat(x, y, 70.0, -40.0, m, t) {
            return Some(c);
        }
        let fanged = |x: f64, y: f64| {
            if capsule(x, y, -26.0, 58.0, 26.0, 58.0, 3.0) < 0.0 {
                return Some(BLACK);
            }
            [-1.0, 1.0]
                .iter()
                .any(|side| y > 58.0 && y < 70.0 && (x - side * 12.0).abs() < (70.0 - y) * 0.35)
                .then_some(WHITE)
        };
        if let Some(c) = mouth(x, y, 0.0, 60.0, 34.0, m, t, &fanged, true) {
            return Some(c);
        }
        if (x * 0.22 + (y / 14.0).floor() * 1.6).sin() > 0.82
            && (((y + 200.0) % 14.0) - 7.0).abs() > 4.0
        {
            return Some(mul(base, 0.82)); // scales
        }
        return head_skin(x, y, d, x / rx, base, outline, m, (56.0, 34.0), -58.0);
    }
    let flick = if t % 1.6 < 0.35 {
        ((t % 1.6) / 0.35 * PI).sin()
    } else {
        0.0
    };
    if flick > 0.0 && m.expr != Expr::Dead {
        let tip = 62.0 + 30.0 + 40.0 * flick;
        if capsule(x, y, 0.0, 62.0, 0.0, tip, 3.0) < 0.0
            || capsule(x, y, 0.0, tip, -8.0, tip + 12.0, 2.5) < 0.0
            || capsule(x, y, 0.0, tip, 8.0, tip + 12.0, 2.5) < 0.0
        {
            return Some([220.0, 30.0, 60.0]);
        }
    }
    let hood = ellipse(x, y, 0.0, 40.0, 182.0, 128.0);
    if hood < 0.0 {
        if hood > -5.0 {
            return Some([70.0, 20.0, 5.0]);
        }
        for side in [-1.0, 1.0] {
            let spot = circle(x, y, side * 128.0, 10.0, 17.0);
            if spot < 0.0 {
                return Some(if spot > -6.0 {
                    [250.0, 220.0, 140.0]
                } else {
                    [40.0, 10.0, 5.0]
                });
            }
        }
        return Some(if (y * 0.15).sin() > 0.6 {
            [130.0, 45.0, 12.0]
        } else {
            [165.0, 60.0, 18.0]
        });
    }
    None
}

/// NUMBER NAGA whole: the hooded head swaying on a coiled tail.
fn naga_full(x: f64, y: f64, t: f64, m: &Mood) -> Px {
    let sway = (t * 3.0).sin() * 6.0;
    let (hx, hy) = ((x - sway) / 0.55, (y + 40.0) / 0.55);
    if hy < 150.0
        && let Some(c) = naga(hx, hy, t, m)
    {
        return Some(c);
    }
    for (cy, rx, ry, upper) in [(120.0, 92.0, 26.0, false), (78.0, 70.0, 20.0, true)] {
        let cx = if upper { sway * 0.5 } else { 0.0 };
        let (outer, inner) = (
            ellipse(x, y, cx, cy, rx, ry),
            ellipse(x, y, cx, cy - 4.0, rx - 26.0, ry - 12.0),
        );
        if outer < 0.0 && inner > 0.0 {
            if outer > -4.0 {
                return Some([100.0, 30.0, 5.0]);
            }
            return Some(if (x * 0.2 + y * 0.3).sin() > 0.7 {
                [190.0, 65.0, 15.0]
            } else {
                [235.0, 85.0, 25.0]
            });
        }
    }
    None
}

/// PUNCTUATION PIMIENTO: a red heart peeking out of a green olive, marks orbiting.
fn pimiento(x: f64, y: f64, t: f64, m: &Mood) -> Px {
    let marks = punctuation(x, y, t);
    if let Some((c, true)) = marks {
        return Some(c);
    }
    let (x, y) = shift(x, y, m);
    let (olive, olive_dark) = ([105.0, 140.0, 45.0], [45.0, 65.0, 15.0]);
    let shell = ellipse(x, y, 0.0, 95.0, 168.0, 150.0);
    let hole = ellipse(x, y, 0.0, 18.0, 104.0, 82.0);
    if hole < 0.0 {
        let (hx, hy) = (x.abs(), y - 8.0);
        let lobe = circle(hx, hy, 36.0, -18.0, 52.0);
        let heart = if hy > -18.0 {
            lobe.min(hx * 0.9 + (hy + 18.0) * 0.85 - 70.0)
        } else {
            lobe
        };
        if heart < 0.0 {
            let (base, outline) = ([225.0, 30.0, 50.0], [95.0, 5.0, 20.0]);
            let innocent = EyeStyle {
                lid_color: mul(base, 0.85),
                pupil: (m.expr == Expr::Idle).then_some(0.58),
                ..Default::default()
            };
            for side in [-1.0, 1.0] {
                if let Some(c) = eye(x, y, side * 34.0, -6.0, 24.0, 28.0, side, m, &innocent) {
                    return Some(c);
                }
                let tilt = if m.expr == Expr::Idle { -6.0 } else { m.brow };
                if let Some(c) = brow(x, y, side * 34.0, -44.0, 40.0, side, tilt, 4.5, m) {
                    return Some(c);
                }
            }
            if let Some(c) = sweat(x, y, 66.0, -30.0, m, t) {
                return Some(c);
            }
            let sneaky = |x: f64, y: f64| {
                let e = ellipse(x, y, 6.0, 38.0, 22.0, 12.0);
                (e < 0.0 && e > -3.4 && y > 40.0).then_some(BLACK)
            };
            if let Some(c) = mouth(x, y, 0.0, 46.0, 30.0, m, t, &sneaky, false) {
                return Some(c);
            }
            return head_skin(x, y, heart, x / 90.0, base, outline, m, (52.0, 26.0), -60.0);
        }
        return Some([50.0, 8.0, 15.0]); // dark inside of the olive
    }
    if shell < 0.0 {
        if shell > -6.0 || (hole < 6.0 && hole > 0.0) {
            return Some(olive_dark);
        }
        let u = x / 168.0;
        if u < -0.5 && u > -0.75 && y > 40.0 && y < 120.0 {
            return Some([190.0, 215.0, 110.0]);
        }
        return Some(mul(olive, 1.05 - 0.3 * u));
    }
    marks.map(|(c, _)| c)
}

/// ! ? ; , circling the pimiento: in front on the near side, behind on the far.
fn punctuation(x: f64, y: f64, t: f64) -> Option<(Rgb, bool)> {
    for k in 0..4 {
        let a = t * 1.1 + k as f64 * PI / 2.0;
        let (px, py, front) = (a.cos() * 190.0, 10.0 + a.sin() * 60.0, a.sin() > 0.0);
        let (dx, dy) = (x - px, y - py);
        let hit = match k {
            0 => {
                capsule(dx, dy, 0.0, -22.0, 0.0, 4.0, 5.0) < 0.0
                    || circle(dx, dy, 0.0, 18.0, 5.5) < 0.0
            }
            1 => {
                ((dx.hypot(dy + 12.0) - 13.0).abs() < 4.5 && !(dx < 0.0 && dy > -12.0))
                    || capsule(dx, dy, 0.0, 1.0, 0.0, 6.0, 4.5) < 0.0
                    || circle(dx, dy, 0.0, 20.0, 5.5) < 0.0
            }
            2 => {
                circle(dx, dy, 0.0, -10.0, 5.5) < 0.0
                    || circle(dx, dy, 0.0, 10.0, 5.5) < 0.0
                    || capsule(dx, dy, 0.0, 12.0, -5.0, 24.0, 3.5) < 0.0
            }
            _ => {
                circle(dx, dy, 0.0, 6.0, 6.0) < 0.0
                    || capsule(dx, dy, 0.0, 8.0, -6.0, 22.0, 3.8) < 0.0
            }
        };
        if hit {
            return Some((
                if front {
                    [255.0, 240.0, 150.0]
                } else {
                    [150.0, 140.0, 90.0]
                },
                front,
            ));
        }
    }
    None
}

/// MIRROR MORITA: smoked, wrinkled, a lopsided face that flips sides; smoke rises.
fn morita(x: f64, y: f64, t: f64, m: &Mood) -> Px {
    for k in 0..3 {
        for j in 0..6 {
            let life = (t * 0.35 + k as f64 / 3.0 + j as f64 * 0.07) % 1.0;
            let px = -40.0 + k as f64 * 40.0 + (life * 9.0 + k as f64 * 2.0).sin() * 18.0;
            if circle(x, y, px, -85.0 - life * 70.0, 8.0 * (1.0 - life)) < 0.0 {
                return Some(mix([150.0, 150.0, 155.0], [70.0, 70.0, 75.0], life));
            }
        }
    }
    let (x, y) = shift(x, y, m);
    let (base, outline) = ([125.0, 48.0, 35.0], [45.0, 10.0, 8.0]);
    if capsule(x, y, 0.0, -95.0, -14.0, -112.0, 5.0) < 0.0 {
        return Some([80.0, 70.0, 30.0]);
    }
    let th = (y - 60.0).atan2(x);
    let rx = 100.0 * (1.0 + 0.06 * (th * 7.0 + 0.5).sin());
    let d = ellipse(x, y, 0.0, 60.0, rx, 152.0);
    if d > 0.0 {
        return None;
    }
    // the mirror: every 2.4s the face flips sides, squashing through the middle
    let (ph, flip) = (t % 2.4, if (t / 2.4) as i64 % 2 == 1 { -1.0 } else { 1.0 });
    let squash = if ph < 0.25 {
        (ph / 0.25 * PI).cos().abs()
    } else {
        1.0
    };
    let fx = x * flip / squash.max(0.08);
    for (i, (ex, ey, erx, ery)) in [(-36.0, -14.0, 26.0, 30.0), (36.0, -6.0, 15.0, 17.0)]
        .into_iter()
        .enumerate()
    {
        let side = if i == 1 { 1.0 } else { -1.0 };
        let style = EyeStyle {
            lid_color: mul(base, 0.8),
            ..Default::default()
        };
        if let Some(c) = eye(fx, y, ex, ey, erx, ery, side, m, &style) {
            return Some(c);
        }
        if let Some(c) = brow(fx, y, ex, ey - ery - 12.0, erx * 2.0, side, m.brow, 4.5, m) {
            return Some(c);
        }
    }
    if let Some(c) = sweat(x, y, 70.0, -30.0, m, t) {
        return Some(c);
    }
    let crooked =
        |x: f64, y: f64| (capsule(x, y, -20.0, 46.0, 24.0, 54.0, 3.2) < 0.0).then_some(BLACK);
    if let Some(c) = mouth(fx, y, 4.0, 50.0, 30.0, m, t, &crooked, false) {
        return Some(c);
    }
    if (y * 0.22 + 4.0 * (x * 0.03).sin()).sin() > 0.62 {
        return Some(mul(base, 0.62)); // deep wrinkles
    }
    head_skin(x, y, d, x / rx, base, outline, m, (58.0, 30.0), -62.0)
}

/// SALT SCOTCH BONNET: squashed lantern, tartan tam-o'-shanter, bushy mustache, salt.
fn bonnet(x: f64, y: f64, t: f64, m: &Mood) -> Px {
    for k in 0..12 {
        let k = k as f64;
        let life = (t * 0.6 + hash(k)) % 1.0;
        let (px, py) = (
            -170.0 + hash(k + 3.0) * 340.0 + (t + k).sin() * 6.0,
            -130.0 + life * 260.0,
        );
        if (x - px).abs() < 3.5 && (y - py).abs() < 5.0 {
            return Some([245.0, 245.0, 250.0]);
        }
    }
    let (x, y) = shift(x, y, m);
    let (base, outline) = ([255.0, 175.0, 25.0], [120.0, 60.0, 0.0]);
    let tilt = 0.12
        + if m.expr == Expr::Laugh {
            (t * 16.0).sin() * 0.1
        } else {
            0.0
        };
    let (hx, hy) = rot(x, y, 10.0, -46.0, tilt);
    if circle(hx, hy, 22.0, -74.0, 15.0) < 0.0 {
        return Some([210.0, 40.0, 50.0]); // pom-pom
    }
    if let Some(c) = tam(hx, hy, 10.0, -50.0) {
        return Some(c);
    }
    let th = (y - 70.0).atan2(x);
    let rx = 150.0 * (1.0 + 0.07 * (5.0 * th).cos());
    let d = ellipse(x, y, 0.0, 70.0, rx, 105.0);
    if d > 0.0 {
        return None;
    }
    let style = EyeStyle {
        lid_color: mul(base, 0.85),
        ..Default::default()
    };
    for side in [-1.0, 1.0] {
        if let Some(c) = eye(x, y, side * 40.0, 2.0, 19.0, 22.0, side, m, &style) {
            return Some(c);
        }
        if brow(x, y, side * 40.0, -30.0, 46.0, side, m.brow, 6.0, m).is_some() {
            return Some([150.0, 70.0, 20.0]); // bushy
        }
    }
    if let Some(c) = sweat(x, y, 96.0, -10.0, m, t) {
        return Some(c);
    }
    let wig = if matches!(m.expr, Expr::Laugh | Expr::Windup) {
        (t * 20.0).sin() * 4.0
    } else {
        0.0
    };
    for side in [-1.0, 1.0] {
        let (mx, my) = (side * 32.0, 40.0 + wig * side);
        let curl = if (x - mx).abs() < 30.0 {
            3.0 * (x * 0.5).sin()
        } else {
            0.0
        };
        let st = ellipse(x, y, mx, my, 40.0, 15.0 + curl);
        if st < 0.0 {
            return Some(if st > -3.0 {
                [110.0, 45.0, 10.0]
            } else {
                [205.0, 95.0, 30.0]
            }); // mustache
        }
    }
    if let Some(c) = mouth(x, y, 0.0, 68.0, 28.0, m, t, &|_, _| None, false) {
        return Some(c);
    }
    head_skin(x, y, d, x / rx, base, outline, m, (80.0, 40.0), -86.0)
}

/// The tartan tam-o'-shanter (also thrown as a boomerang), centered at (cx, cy).
pub fn tam(x: f64, y: f64, cx: f64, cy: f64) -> Px {
    let cap = ellipse(x, y, cx, cy, 108.0, 26.0);
    if cap >= 0.0 {
        return None;
    }
    if cap > -4.0 {
        return Some([70.0, 10.0, 20.0]);
    }
    if y > cy + 14.0 {
        return Some([40.0, 60.0, 120.0]); // band
    }
    let plaid =
        (((x + 300.0) % 22.0) - 11.0).abs() < 2.5 || (((y + 300.0) % 14.0) - 7.0).abs() < 1.8;
    Some(if plaid {
        [30.0, 90.0, 60.0]
    } else {
        [190.0, 30.0, 45.0]
    })
}

/// THE CAROLINA REAPER: hooded, glowing eyes, fangs, scorpion tail over its shoulder.
fn reaper(x: f64, y: f64, t: f64, m: &Mood) -> Px {
    let (x, y) = shift(x, y, m);
    let glowing = m.expr != Expr::Dead;
    let sting = (t * 3.0).sin() * 6.0
        + if m.expr == Expr::Windup {
            (t * 40.0).sin() * 4.0
        } else {
            0.0
        };
    for k in 0..9 {
        let s = k as f64 / 8.0;
        let px = 120.0 + 70.0 * (s * 2.4).sin() - s * 30.0;
        let py = 110.0 - s * 200.0 + (s * 3.0).sin() * 10.0 + if k == 8 { sting } else { 0.0 };
        let seg = circle(x, y, px, py, 20.0 - s * 9.0);
        if seg < 0.0 {
            return Some(if seg > -4.0 {
                [50.0, 0.0, 10.0]
            } else {
                [150.0, 15.0, 30.0]
            });
        }
        if k == 8 && capsule(x, y, px, py, px - 28.0, py + 18.0 + sting, 4.0) < 0.0 {
            return Some([230.0, 220.0, 200.0]); // stinger
        }
    }
    let hood = ellipse(x, y, 0.0, 40.0, 150.0, 150.0);
    let opening = ellipse(x, y, 0.0, 40.0, 96.0, 92.0);
    if hood < 0.0 && opening > 0.0 {
        return Some(if hood > -5.0 {
            [70.0, 50.0, 90.0]
        } else {
            mix(
                [28.0, 18.0, 36.0],
                [60.0, 40.0, 75.0],
                ((-y + 40.0) / 160.0).max(0.0),
            )
        });
    }
    if opening > 0.0 {
        return None;
    }
    let (base, outline) = ([175.0, 15.0, 30.0], [60.0, 0.0, 10.0]);
    let glow = 0.7 + 0.3 * (t * 17.0).sin() * (t * 5.3).sin();
    let style = EyeStyle {
        lid_color: [40.0, 0.0, 10.0],
        rim: Some(if glowing { [255.0, 110.0, 20.0] } else { BLACK }),
        sclera: Some(if glowing {
            mix([255.0, 220.0, 100.0], [255.0, 255.0, 230.0], glow)
        } else {
            WHITE
        }),
        pupil_color: Some(if glowing { [255.0, 70.0, 0.0] } else { BLACK }),
        ..Default::default()
    };
    for side in [-1.0, 1.0] {
        if let Some(c) = eye(x, y, side * 36.0, 8.0, 22.0, 18.0, side, m, &style) {
            return Some(c);
        }
        if let Some(c) = brow(x, y, side * 36.0, -22.0, 46.0, side, m.brow + 5.0, 4.5, m) {
            return Some(c);
        }
    }
    if let Some(c) = sweat(x, y, 70.0, -10.0, m, t) {
        return Some(c);
    }
    let jagged = |x: f64, y: f64| {
        if (y - 66.0).abs() < 9.0 && x.abs() < 30.0 {
            let tooth = (((x + 60.0) % 10.0) - 5.0).abs();
            return Some(if y < 64.0 + tooth * 0.6 || y > 72.0 - tooth * 0.5 {
                WHITE
            } else {
                [40.0, 0.0, 0.0]
            });
        }
        None
    };
    if let Some(c) = mouth(x, y, 0.0, 70.0, 36.0, m, t, &jagged, true) {
        return Some(c);
    }
    let bumps = 1.0 + 0.08 * (x * 0.11).sin() * (y * 0.09).cos();
    let d = ellipse(x, y, 0.0, 60.0, 110.0 * bumps, 130.0);
    head_skin(x, y, d, x / 110.0, base, outline, m, (60.0, 46.0), -60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [Kind; 6] = [
        Kind::Cayenne,
        Kind::Naga,
        Kind::Pimiento,
        Kind::Morita,
        Kind::Bonnet,
        Kind::Reaper,
    ];

    #[test]
    fn every_foe_shows_every_mood() {
        use Expr::*;
        for kind in KINDS {
            for expr in [Idle, Flinch, Grin, Windup, Laugh, Scared, Dead] {
                let m = Mood::new(expr, 0.4, 1.0, 1.3);
                let (mut inked, mut dark) = (0, 0);
                // fine enough to catch thin strokes (^ ^ eyes at half scale)
                for x in (-200..200).step_by(2) {
                    for y in (-180..160).step_by(3) {
                        if let Some(c) = kind.sprite(x as f64, y as f64, 1.3, &m, None) {
                            inked += 1;
                            dark += usize::from(c == BLACK);
                        }
                    }
                }
                assert!(inked > 1200, "{kind:?} {expr:?} drew {inked}");
                assert!(dark > 12, "{kind:?} {expr:?} has face features ({dark})");
            }
        }
    }

    #[test]
    fn attacks_land_before_they_end() {
        for kind in KINDS {
            assert!(kind.hit_delay() < kind.attack_len(), "{kind:?}");
        }
    }
}
