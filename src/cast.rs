//! The cast: animated cartoon peppers drawn with `toon`. Each one is a pure
//! function of (time, point), so they animate smoothly at any frame rate.
//! Mascots celebrate your peak rank on the results screen (the foes live in
//! `foes.rs`).

use std::f64::consts::PI;

use crate::toon::{Px, Rgb, capsule, circle, ellipse, hash, mix, mul, rot, smooth, tube};

const WHITE: Rgb = [245.0, 242.0, 235.0];
const BLACK: Rgb = [12.0, 10.0, 14.0];
const TONGUE: Rgb = [235.0, 80.0, 95.0];
const GLOVE: Rgb = [250.0, 250.0, 250.0];
const STEM: Rgb = [95.0, 120.0, 40.0];
const CALYX: Rgb = [35.0, 110.0, 40.0];
const SWEAT: Rgb = [120.0, 200.0, 255.0];
const BLUSH: Rgb = [235.0, 120.0, 120.0];

// ── shared parts ────────────────────────────────────────────────────────

/// Body fill: cartoon outline, lit from the left, optional shine stripe.
fn skin(d: f64, u: f64, s: f64, base: Rgb, outline: Rgb, shine: Option<Rgb>) -> Px {
    if d > 0.0 {
        return None;
    }
    if d > -5.5 {
        return Some(outline);
    }
    if let Some(shine) = shine
        && (-0.7..-0.38).contains(&u)
        && (0.12..0.62).contains(&s)
    {
        return Some(shine);
    }
    let edge = if u > 0.55 { 0.12 } else { 0.0 };
    Some(mul(base, 1.05 - 0.25 * u.clamp(-1.0, 1.0) - edge))
}

/// A white cartoon glove on the end of a thin arm.
#[allow(clippy::too_many_arguments)]
fn arm(x: f64, y: f64, ax: f64, ay: f64, hx: f64, hy: f64, sleeve: Rgb) -> Px {
    let g = circle(x, y, hx, hy, 9.0);
    if g < 0.0 {
        return Some(if g > -2.5 { BLACK } else { GLOVE });
    }
    (capsule(x, y, ax, ay, hx, hy, 3.2) < 0.0).then_some(sleeve)
}

fn stem(x: f64, y: f64, top: f64, lean: f64) -> Px {
    if capsule(x, y, 0.0, top, lean, top - 22.0, 5.0) < 0.0 {
        return Some(STEM);
    }
    (ellipse(x, y, 0.0, top + 2.0, 21.0, 7.5) < 0.0).then_some(CALYX)
}

struct Eye {
    lid: f64,
    lid_color: Rgb,
    look_x: f64,
    look_y: f64,
    pupil: f64,
    pupil_color: Rgb,
    sclera: Rgb,
}

impl Eye {
    fn new(lid_color: Rgb) -> Self {
        Eye {
            lid: 0.0,
            lid_color,
            look_x: 0.0,
            look_y: 0.0,
            pupil: 0.45,
            pupil_color: BLACK,
            sclera: WHITE,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn eye(x: f64, y: f64, ex: f64, ey: f64, rx: f64, ry: f64, o: &Eye) -> Px {
    let d = ellipse(x, y, ex, ey, rx, ry);
    if d > 0.0 {
        return None;
    }
    if d > -2.5 {
        return Some(BLACK);
    }
    if y < ey - ry + o.lid * 2.0 * ry {
        return Some(o.lid_color);
    }
    let (px, py) = (ex + o.look_x * rx * 0.45, ey + o.look_y * ry * 0.35);
    if circle(x, y, px - rx * 0.18, py - ry * 0.3, rx * 0.16) < 0.0 {
        return Some(WHITE); // glint
    }
    if circle(x, y, px, py, o.pupil * rx) < 0.0 {
        return Some(o.pupil_color);
    }
    Some(o.sclera)
}

/// 0..1..0 blink pulse once per `period` seconds.
fn blink(t: f64, period: f64, seed: f64) -> f64 {
    let p = (t + seed) % period;
    if p < 0.12 { (p / 0.12 * PI).sin() } else { 0.0 }
}

/// Closed happy eye: ∩ arc.
fn arc_eye(x: f64, y: f64, ex: f64, ey: f64, r: f64) -> Px {
    let a = ellipse(x, y, ex, ey + r * 0.3, r, r * 0.75);
    (a < 0.0 && a > -3.2 && y < ey + r * 0.3).then_some(BLACK)
}

// ── mascots: one per rank, celebrating on the results screen ───────────

/// Your peak pepper doing its thing. Designed for 30×14 cells at scale 1.
pub fn mascot(rank: usize, t: f64, x: f64, y: f64) -> Px {
    match rank {
        0 => bell(x, y, t),
        1 => poblano(x, y, t),
        2 => jalapeno(x, y, t),
        3 => serrano(x, y, t),
        4 => habanero(x, y, t),
        5 => ghost(x, y, t),
        _ => reaper(x, y, t),
    }
}

/// BELL PEPPER: unimpressed. Shrugs, glances away, yawns.
fn bell(x: f64, y: f64, t: f64) -> Px {
    let (base, outline) = ([70.0, 175.0, 60.0], [20.0, 75.0, 25.0]);
    let ph = (t % 3.2) / 3.2;
    let shrug = smooth((ph - 0.55) / 0.1) * (1.0 - smooth((ph - 0.85) / 0.1));
    for side in [-1.0, 1.0] {
        if let Some(c) = arm(
            x,
            y,
            side * 46.0,
            22.0,
            side * 70.0,
            30.0 - shrug * 38.0,
            outline,
        ) {
            return Some(c);
        }
    }
    let y = y - shrug * 4.0; // shoulders up
    if let Some(c) = stem(x, y, -58.0, 8.0) {
        return Some(c);
    }
    let dy = y - 18.0;
    let th = dy.atan2(x);
    let rx = 72.0 * (1.0 + 0.08 * (4.0 * th + 0.5).cos());
    let rho = (x / rx).hypot(dy / 74.0);
    if rho > 1.0 {
        return None;
    }
    let lid = Eye {
        lid: 0.5,
        look_x: -0.7,
        ..Eye::new(mul(base, 0.9))
    };
    for ex in [-20.0, 20.0] {
        if let Some(c) = eye(x, y, ex, -6.0, 11.0, 11.0, &lid) {
            return Some(c);
        }
    }
    let yph = (t % 5.0) / 5.0;
    let yawn = smooth((yph - 0.7) / 0.08) * (1.0 - smooth((yph - 0.92) / 0.06));
    if yawn > 0.05 {
        let m = ellipse(x, y, 0.0, 30.0, 11.0, 3.0 + 11.0 * yawn);
        if m < 0.0 {
            return Some(if m > -2.5 { BLACK } else { [70.0, 10.0, 20.0] });
        }
    } else if capsule(x, y, -12.0, 30.0, 12.0, 30.0, 2.6) < 0.0 {
        return Some(BLACK);
    }
    skin((rho - 1.0) * 60.0, x / rx, 0.3, base, outline, None)
}

/// POBLANO: calm and content. Slow nod, hands folded.
fn poblano(x: f64, y: f64, t: f64) -> Px {
    let (base, outline) = ([40.0, 125.0, 50.0], [12.0, 50.0, 20.0]);
    let (x, y) = rot(
        x,
        y + 3.0 * (t * 4.0).sin(),
        0.0,
        110.0,
        0.06 * (t * 2.0).sin(),
    );
    for side in [-1.0, 1.0] {
        if let Some(c) = arm(x, y, side * 42.0, 10.0, side * 18.0, 40.0, outline) {
            return Some(c);
        }
    }
    if let Some(c) = stem(x, y, -80.0, -10.0) {
        return Some(c);
    }
    let (d, s, u) = tube(
        x,
        y,
        |s| (10.0 * s * s, -78.0 + s * 186.0),
        |s| 46.0 * (0.86 + 0.14 * (s / 0.15).min(1.0)) * (1.0 - s.powf(2.6)).max(0.0).sqrt(),
    );
    if d > 0.0 {
        return None;
    }
    for ex in [-16.0, 16.0] {
        if let Some(c) = arc_eye(x, y, ex, -36.0, 10.0) {
            return Some(c);
        }
    }
    let m = ellipse(x, y, 0.0, -24.0, 14.0, 9.0);
    if m < 0.0 && m > -3.0 && y > -22.0 {
        return Some(BLACK); // closed smile
    }
    if ellipse(x, y, -28.0, -20.0, 7.0, 4.0) < 0.0 || ellipse(x, y, 28.0, -20.0, 7.0, 4.0) < 0.0 {
        return Some(BLUSH);
    }
    skin(d, u, s, base, outline, Some([110.0, 190.0, 110.0]))
}

/// JALAPEÑO: happy hops with squash & stretch, gloves waving.
fn jalapeno(x: f64, y: f64, t: f64) -> Px {
    let (base, outline) = ([40.0, 170.0, 55.0], [16.0, 68.0, 26.0]);
    let y = y + 14.0;
    let period = 0.72;
    let air = ((t % period) / period * PI).sin();
    let squash = (1.0 - air).powi(6);
    let (sy, sx) = (
        1.0 - 0.2 * squash + 0.08 * air,
        1.0 + 0.17 * squash - 0.05 * air,
    );
    let (foot, hop) = (112.0, air * 34.0);
    let (x, y) = rot(x, y, 0.0, foot - hop, 0.07 * (t * 2.0 * PI / period).sin());
    let (x, y) = (x / sx, (y - foot + hop) / sy + foot);

    for side in [-1.0, 1.0] {
        let (hx, hy) = (
            side * (62.0 + 6.0 * (t * 9.0 + side).sin()),
            -22.0 - air * 22.0,
        );
        if let Some(c) = arm(x, y, side * 30.0, 8.0, hx, hy, [20.0, 70.0, 28.0]) {
            return Some(c);
        }
    }
    if let Some(c) = stem(x, y, -84.0, 12.0) {
        return Some(c);
    }
    let (d, s, u) = tube(
        x,
        y,
        |s| (26.0 * s.powi(4) - 2.0, -80.0 + s * 190.0),
        |s| 38.0 * (0.86 + 0.14 * (s / 0.15).min(1.0)) * (1.0 - s.powf(2.3)).max(0.0).sqrt(),
    );
    if d > 0.0 {
        return None;
    }
    let look = Eye {
        lid: blink(t, 2.7, 0.4),
        look_x: (t * 1.3).sin() * 0.8,
        pupil: 0.48,
        ..Eye::new(base)
    };
    for ex in [-14.0, 14.0] {
        if let Some(c) = eye(x, y, ex, -38.0, 9.0, 12.0, &look) {
            return Some(c);
        }
    }
    if ellipse(x, y, -27.0, -16.0, 7.0, 4.0) < 0.0 || ellipse(x, y, 27.0, -16.0, 7.0, 4.0) < 0.0 {
        return Some(BLUSH);
    }
    let m = ellipse(x, y, 0.0, -20.0, 15.0, 12.0 + 4.0 * air);
    if m < 0.0 && y > -20.0 {
        return Some(if m > -2.5 {
            BLACK
        } else if y > -12.0 + 4.0 * air {
            TONGUE
        } else {
            [70.0, 10.0, 20.0]
        });
    }
    skin(d, u, s, base, outline, Some([150.0, 230.0, 140.0]))
}

/// SERRANO: pumped. Fist pumps, star eyes, little hops.
fn serrano(x: f64, y: f64, t: f64) -> Px {
    let (base, outline) = ([110.0, 205.0, 60.0], [30.0, 80.0, 20.0]);
    let pump = (t * 5.0).sin().abs();
    let y = y + 8.0 + pump * 6.0;
    if let Some(c) = arm(x, y, 22.0, -6.0, 44.0, -66.0 - 28.0 * pump, outline) {
        return Some(c);
    }
    if let Some(c) = arm(x, y, -22.0, 4.0, -46.0, 30.0, outline) {
        return Some(c);
    }
    if let Some(c) = stem(x, y, -88.0, 10.0) {
        return Some(c);
    }
    let (d, s, u) = tube(
        x,
        y,
        |s| (14.0 * s.powi(3), -86.0 + s * 196.0),
        |s| 27.0 * (0.85 + 0.15 * (s / 0.15).min(1.0)) * (1.0 - s.powf(3.0)).max(0.0).sqrt(),
    );
    if d > 0.0 {
        return None;
    }
    // twinkling star eyes
    let r = 10.0 * (0.85 + 0.15 * (t * 12.0).sin());
    for ex in [-11.0, 11.0] {
        let (dx, dy) = (((x - ex) / r).abs(), ((y + 50.0) / r).abs());
        if dx.powf(0.6) + dy.powf(0.6) < 1.0 {
            return Some([255.0, 225.0, 60.0]);
        }
    }
    let m = ellipse(x, y, 0.0, -32.0, 13.0, 11.0);
    if m < 0.0 && y > -32.0 {
        return Some(if m > -2.5 {
            BLACK
        } else if y > -26.0 {
            TONGUE
        } else {
            [70.0, 10.0, 20.0]
        });
    }
    skin(d, u, s, base, outline, Some([180.0, 240.0, 140.0]))
}

/// HABANERO: sweating buckets, panting, fanning its mouth, steam rising.
fn habanero(x: f64, y: f64, t: f64) -> Px {
    let (base, outline) = ([255.0, 128.0, 0.0], [120.0, 45.0, 0.0]);
    let x = x - (t * 61.0).sin() * 1.5; // trembling
    for k in 0..5 {
        let (k, life) = (k as f64, (t * 0.7 + k as f64 / 5.0) % 1.0);
        let r = 6.0 + life * 9.0 - life * life * 12.0;
        let (px, py) = (
            -30.0 + k * 15.0 + (t * 2.0 + k).sin() * 8.0,
            -95.0 - life * 40.0,
        );
        if r > 0.0 && circle(x, y, px, py, r) < 0.0 {
            return Some(mix([235.0, 235.0, 240.0], [120.0, 120.0, 130.0], life));
        }
    }
    for k in 0..4 {
        let life = (t * 1.3 + k as f64 * 0.27) % 1.0;
        let side = if k % 2 == 1 { 1.0 } else { -1.0 };
        let (px, py) = (
            side * (48.0 + life * 40.0),
            -55.0 + life * 60.0 + life * life * 60.0,
        );
        if ellipse(x, y, px, py, 4.5, 7.0) < 0.0 {
            return Some(SWEAT);
        }
    }
    let fan = (t * 22.0).sin();
    if let Some(c) = arm(
        x,
        y,
        40.0,
        30.0,
        52.0 + fan * 6.0,
        5.0 + fan * 10.0,
        outline,
    ) {
        return Some(c);
    }
    if let Some(c) = arm(x, y, -44.0, 30.0, -66.0, 40.0, outline) {
        return Some(c);
    }
    if let Some(c) = stem(x - 2.0, y, -80.0, 8.0) {
        return Some(c);
    }
    let dy = y - 5.0;
    let th = dy.atan2(x);
    let pinch = if dy > 0.0 {
        1.0 - 0.55 * (dy / 90.0).powi(2)
    } else {
        1.0
    };
    let rx = 66.0 * pinch * (1.0 + 0.03 * (t * 9.0).sin()) * (1.0 + 0.07 * (4.0 * th + 0.4).cos());
    let rho = (x / rx).hypot(dy / 88.0);
    if rho > 1.0 {
        return None;
    }
    let pained = Eye {
        lid: 0.55,
        look_y: 0.6,
        pupil: 0.42,
        ..Eye::new(mul(base, 0.85))
    };
    for ex in [-20.0, 20.0] {
        if let Some(c) = eye(x, y, ex, -30.0, 11.0, 10.0, &pained) {
            return Some(c);
        }
        let tilt = if ex < 0.0 { (4.0, -2.0) } else { (-2.0, 4.0) };
        if capsule(
            x,
            y,
            ex - 12.0,
            -46.0 + tilt.0,
            ex + 12.0,
            -46.0 + tilt.1,
            2.6,
        ) < 0.0
        {
            return Some(outline); // worried brows
        }
    }
    let pant = 0.5 + 0.5 * (t * 14.0).sin();
    if capsule(x, y, 8.0, 22.0, 12.0 + pant * 4.0, 40.0 + pant * 6.0, 6.0) < 0.0 {
        return Some(TONGUE);
    }
    let m = ellipse(x, y, 4.0, 14.0, 15.0, 9.0 + pant * 5.0);
    if m < 0.0 {
        return Some(if m > -2.5 { BLACK } else { [70.0, 10.0, 10.0] });
    }
    let u = x / rx;
    let shine = ((-0.65..-0.4).contains(&u) && (-60.0..20.0).contains(&dy))
        .then_some([255.0, 210.0, 120.0]);
    skin((rho - 1.0) * 60.0, u, 0.3, base, outline, shine)
}

/// GHOST PEPPER: floats, wavy ghost tail, spooky glow, "booo".
fn ghost(x: f64, y: f64, t: f64) -> Px {
    let (base, outline) = ([220.0, 35.0, 25.0], [80.0, 5.0, 5.0]);
    let y = y - 10.0 * (t * 1.6).sin();
    let x = x - 4.0 * (t * 0.9).sin();
    for side in [-1.0, 1.0] {
        let hy = -14.0 + 6.0 * (t * 3.0 + side).sin();
        if let Some(c) = arm(x, y, side * 34.0, 0.0, side * 58.0, hy, outline) {
            return Some(c);
        }
    }
    if let Some(c) = stem(x, y, -84.0, -12.0) {
        return Some(c);
    }
    let (d, s, u) = tube(
        x,
        y,
        |s| (0.0, -80.0 + s * 170.0),
        |s| 40.0 * (0.8 + 0.2 * (s / 0.15).min(1.0)),
    );
    let hem = 72.0 + 9.0 * (x * 0.22 + t * 6.0).sin();
    if y > hem && d < 0.0 {
        return None; // ragged ghost tail
    }
    if d > 0.0 {
        let glow = 0.5 + 0.5 * (t * 3.0).sin();
        return (d < 9.0 && y < hem).then_some(mix(
            [40.0, 0.0, 0.0],
            [110.0, 10.0, 10.0],
            glow * (1.0 - d / 9.0),
        ));
    }
    for ex in [-15.0, 15.0] {
        if ellipse(x, y, ex, -40.0, 9.0, 14.0) < 0.0 {
            let wander = (t * 1.1 + ex).sin() * 3.0;
            return Some(if circle(x, y, ex + wander, -44.0, 3.0) < 0.0 {
                WHITE
            } else {
                BLACK
            });
        }
    }
    let boo = 0.7 + 0.3 * (t * 4.0).sin();
    if ellipse(x, y, 0.0, -8.0, 7.0 * boo, 10.0 * boo) < 0.0 {
        return Some(BLACK);
    }
    skin(d, u, s, base, outline, Some([255.0, 120.0, 100.0]))
}

/// CAROLINA REAPER: hooded, glowing eyes, scythe swings, fire breath.
fn reaper(x: f64, y: f64, t: f64) -> Px {
    let (base, outline, hood) = ([175.0, 15.0, 30.0], [60.0, 0.0, 10.0], [28.0, 18.0, 36.0]);
    let y = y + 16.0;
    let burst = (t % 2.6) / 2.6;
    if burst < 0.38 {
        for k in 0..14 {
            let life = ((burst / 0.38) * 1.6 + k as f64 / 14.0) % 1.0;
            let px = -16.0 - life * 120.0;
            let py = 2.0 + (k as f64 * 3.1 + t * 9.0).sin() * life * 26.0;
            if circle(x, y, px, py, 4.0 + life * 13.0 - life * life * 10.0) < 0.0 {
                return Some(match life {
                    l if l < 0.25 => [255.0, 245.0, 190.0],
                    l if l < 0.55 => [255.0, 180.0, 30.0],
                    _ => [230.0, 60.0, 20.0],
                });
            }
        }
    }
    for k in 0..8 {
        let (k, life) = (k as f64, (t * 0.5 + hash(k as f64)) % 1.0);
        let px = -90.0 + hash(k + 9.0) * 180.0 + (t * 2.0 + k).sin() * 8.0;
        if circle(x, y, px, 120.0 - life * 260.0, 2.6) < 0.0 {
            return Some(if life < 0.5 {
                [255.0, 170.0, 40.0]
            } else {
                [200.0, 60.0, 20.0]
            });
        }
    }
    // scythe, with a big swing every few seconds
    let cycle = t % 3.4;
    let swing = 0.28 * (t * 2.4).sin()
        + 0.5 * smooth((cycle - 2.6) * 2.0) * (1.0 - smooth((cycle - 3.0) * 3.0));
    let (sx, sy) = rot(x, y, 58.0, 30.0, -swing);
    if capsule(sx, sy, 58.0, 70.0, 64.0, -100.0, 3.6) < 0.0 {
        return Some([110.0, 70.0, 40.0]);
    }
    let bd = (sx - 14.0).hypot(sy + 92.0);
    if bd < 52.0 && bd > 40.0 && sx < 62.0 && sy < -82.0 + (sx - 14.0) * 0.25 && sx > -34.0 {
        return Some(if !(43.0..=49.0).contains(&bd) {
            [120.0, 120.0, 135.0]
        } else {
            [215.0, 215.0, 225.0]
        });
    }
    let g = circle(sx, sy, 58.0, 30.0, 9.0);
    if g < 0.0 {
        return Some(if g > -2.5 {
            BLACK
        } else {
            [235.0, 235.0, 235.0]
        });
    }
    if capsule(x, y, 34.0, 22.0, 58.0, 30.0, 3.2) < 0.0 {
        return Some(outline);
    }
    let hood_d = ellipse(x, y, 0.0, -44.0, 56.0, 54.0);
    if hood_d < 0.0 && y < 2.0 && ellipse(x, y, 0.0, -34.0, 36.0, 32.0) > 0.0 {
        return Some(if hood_d > -3.0 {
            [70.0, 50.0, 90.0]
        } else {
            mix(
                hood,
                [60.0, 40.0, 75.0],
                ((-y - 40.0) / 60.0).clamp(0.0, 1.0),
            )
        });
    }
    let glow = 0.7 + 0.3 * (t * 17.0).sin() * (t * 5.3).sin();
    for ex in [-15.0, 15.0] {
        let e = ellipse(x, y, ex, -40.0, 9.0, 6.0 + 2.0 * glow);
        if e < 0.0 {
            return Some(if e > -2.0 {
                [255.0, 120.0, 20.0]
            } else {
                mix([255.0, 230.0, 120.0], [255.0, 255.0, 240.0], glow)
            });
        }
        let tilt = if ex < 0.0 { (-5.0, 4.0) } else { (4.0, -5.0) };
        if capsule(
            x,
            y,
            ex - 13.0,
            -52.0 + tilt.0,
            ex + 11.0,
            -52.0 + tilt.1,
            2.6,
        ) < 0.0
        {
            return Some(BLACK);
        }
    }
    if ellipse(x, y, 0.0, -18.0, 22.0, 8.0) < 0.0 && y > -22.0 {
        let tooth = (((x + 30.0) % 8.0) - 4.0).abs() * 1.8;
        return Some(if y < -20.0 + tooth * 0.9 || y > -10.0 - tooth * 0.6 {
            WHITE
        } else {
            [40.0, 0.0, 0.0]
        });
    }
    let (d, s, u) = tube(
        x,
        y,
        |s| {
            let hook = ((s - 0.8) / 0.2).max(0.0);
            let lift = ((s - 0.82) / 0.18).max(0.0);
            (
                -6.0 - 40.0 * s.powi(3) + 30.0 * hook * hook,
                -70.0 + s * 175.0 - 60.0 * lift * lift,
            )
        },
        |s| (44.0 + 5.0 * (s * 28.0).sin()) * (1.0 - s.powf(1.6)).max(0.0).sqrt() + 2.0,
    );
    skin(d, u, s, base, outline, Some([235.0, 90.0, 90.0]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inked(f: impl Fn(f64, f64) -> Px) -> usize {
        (-150..150)
            .step_by(5)
            .flat_map(|x| (-130..130).step_by(10).map(move |y| (x as f64, y as f64)))
            .filter(|&(x, y)| f(x, y).is_some())
            .count()
    }

    #[test]
    fn every_mascot_shows_up_at_any_time() {
        for rank in 0..7 {
            for t in [0.0, 0.37, 1.9, 4.6] {
                assert!(
                    inked(|x, y| mascot(rank, t, x, y)) > 150,
                    "rank {rank} at {t}s"
                );
            }
        }
    }
}
