//! A 3D pepper rendered live in characters (think Ghostty's ghost, but
//! procedural): each cell casts a ray into a signed-distance pepper, lights
//! the hit, and picks a denser glyph the brighter it is. Shapes are a handful
//! of numbers, so peppers can morph into each other.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

/// Pepper body along +x in its own frame: stem end at -len/2, tip at +len/2.
#[derive(Clone, Copy)]
pub struct Shape {
    pub len: f64,
    pub girth: f64,
    /// How far the tip droops.
    pub bend: f64,
    /// Higher = fuller toward the tip before it closes.
    pub taper: f64,
    /// Ridges around the axis (bell and habanero lobes; 2 flattens it).
    pub lobes: f64,
    pub lobe_depth: f64,
    /// Fine wrinkly skin.
    pub wrinkles: f64,
    /// Coarse knobbly lumps (ghost, reaper).
    pub bumps: f64,
    /// The reaper's scorpion tail: the tip hooks back up.
    pub tail: f64,
    /// 0 = lying sideways, 1 = hanging stem-up.
    pub upright: f64,
    pub color: (f64, f64, f64),
}

const BASE: Shape = Shape {
    len: 2.0,
    girth: 0.5,
    bend: 0.0,
    taper: 2.0,
    lobes: 3.0,
    lobe_depth: 0.0,
    wrinkles: 0.0,
    bumps: 0.0,
    tail: 0.0,
    upright: 0.0,
    color: (0.0, 0.0, 0.0),
};

pub const BELL: Shape = Shape {
    len: 1.35,
    girth: 0.78,
    taper: 7.0,
    lobes: 4.0,
    lobe_depth: 0.14,
    upright: 1.0,
    color: (70.0, 175.0, 60.0),
    ..BASE
};

pub const POBLANO: Shape = Shape {
    len: 2.5,
    girth: 0.58,
    bend: 0.2,
    taper: 2.2,
    lobes: 2.0,
    lobe_depth: 0.2,
    color: (40.0, 125.0, 50.0),
    ..BASE
};

pub const JALAPENO: Shape = Shape {
    len: 2.7,
    girth: 0.42,
    bend: 0.35,
    taper: 2.6,
    lobe_depth: 0.03,
    color: (45.0, 175.0, 55.0),
    ..BASE
};

pub const SERRANO: Shape = Shape {
    len: 2.7,
    girth: 0.27,
    bend: 0.12,
    taper: 3.0,
    color: (110.0, 205.0, 60.0),
    ..BASE
};

pub const HABANERO: Shape = Shape {
    len: 1.7,
    girth: 0.62,
    bend: 0.12,
    taper: 1.4,
    lobes: 4.0,
    lobe_depth: 0.13,
    wrinkles: 0.035,
    upright: 1.0,
    color: (255.0, 125.0, 0.0),
    ..BASE
};

pub const GHOST: Shape = Shape {
    len: 2.6,
    girth: 0.38,
    bend: 0.45,
    taper: 1.8,
    wrinkles: 0.06,
    bumps: 0.06,
    color: (220.0, 35.0, 25.0),
    ..BASE
};

pub const REAPER: Shape = Shape {
    len: 1.8,
    girth: 0.55,
    bend: 0.25,
    taper: 1.3,
    lobes: 3.0,
    lobe_depth: 0.1,
    wrinkles: 0.04,
    bumps: 0.13,
    tail: 0.45,
    upright: 0.55,
    color: (185.0, 15.0, 30.0),
};

/// One pepper per rank, bell pepper → carolina reaper (matches `engine::RANKS`).
pub const RANK_SHAPES: [Shape; 7] = [BELL, POBLANO, JALAPENO, SERRANO, HABANERO, GHOST, REAPER];

impl Shape {
    pub fn lerp(&self, o: &Shape, k: f64) -> Shape {
        let m = |a: f64, b: f64| a + (b - a) * k;
        Shape {
            len: m(self.len, o.len),
            girth: m(self.girth, o.girth),
            bend: m(self.bend, o.bend),
            taper: m(self.taper, o.taper),
            lobes: m(self.lobes, o.lobes),
            lobe_depth: m(self.lobe_depth, o.lobe_depth),
            wrinkles: m(self.wrinkles, o.wrinkles),
            bumps: m(self.bumps, o.bumps),
            tail: m(self.tail, o.tail),
            upright: m(self.upright, o.upright),
            color: (
                m(self.color.0, o.color.0),
                m(self.color.1, o.color.1),
                m(self.color.2, o.color.2),
            ),
        }
    }
}

/// Pose of the pepper in the world.
pub struct Pose {
    /// Spin around the vertical axis, radians.
    pub spin: f64,
    /// Tilt toward the camera, radians.
    pub tilt: f64,
}

type V3 = [f64; 3];

const RAMP: &[u8] = b" .,:;-=+*#%@";
/// Terminal cells are about twice as tall as wide.
const CELL_ASPECT: f64 = 2.1;
/// Everything fits in this sphere; rays that miss it skip marching.
const BOUND: f64 = 1.75;
/// Half-extent the view is framed on (peppers rarely reach BOUND).
const FRAME: f64 = 1.45;
const STEM: Color = Color::Rgb(70, 125, 40);

/// Render `shape` into `area`, leaving cells it doesn't cover untouched.
pub fn draw(buf: &mut Buffer, area: Rect, shape: &Shape, pose: &Pose) {
    let area = area.intersection(buf.area);
    if area.is_empty() {
        return;
    }
    // world units per cell, so the bounding sphere fits the area
    let unit = 2.0 * FRAME / (area.width as f64).min(area.height as f64 * CELL_ASPECT);
    let (cx, cy) = (area.width as f64 / 2.0, area.height as f64 / 2.0);
    let light = norm([-0.5, 0.75, -0.6]);
    let fill = norm([0.7, -0.3, -0.5]); // soft bounce from the lower right
    let half = [light[0], light[1], light[2] - 1.0]; // Blinn half-vector, viewer at -z
    let half = norm(half);

    for row in 0..area.height {
        for col in 0..area.width {
            let wx = (col as f64 + 0.5 - cx) * unit;
            let wy = -(row as f64 + 0.5 - cy) * unit * CELL_ASPECT;
            let r2 = wx * wx + wy * wy;
            if r2 >= BOUND * BOUND {
                continue;
            }
            let Some((p, stem)) = march(shape, pose, wx, wy, (BOUND * BOUND - r2).sqrt()) else {
                continue;
            };
            let n = normal(shape, pose, p);
            let diffuse = dot(n, light).max(0.0) + 0.3 * dot(n, fill).max(0.0);
            let spec = dot(n, half).max(0.0).powi(28);
            let rim = (1.0 + n[2]).powi(3) * 0.15; // facing away from the camera
            let lum = (0.22 + 0.65 * diffuse + 0.7 * spec + rim).min(1.0);
            let glyph = RAMP[((lum * (RAMP.len() - 1) as f64).round() as usize).max(1)];
            let color = if stem {
                STEM
            } else {
                let (r, g, b) = shape.color;
                let shade = 0.45 + 0.7 * diffuse;
                let c = |v: f64| (v * shade + 255.0 * spec).min(255.0) as u8;
                Color::Rgb(c(r), c(g), c(b))
            };
            buf[(area.x + col, area.y + row)]
                .set_symbol(std::str::from_utf8(&[glyph]).unwrap())
                .set_fg(color);
        }
    }
}

/// Sphere-trace from z = -depth to +depth; returns the hit point and
/// whether it landed on the stem.
fn march(s: &Shape, pose: &Pose, x: f64, y: f64, depth: f64) -> Option<(V3, bool)> {
    let mut z = -depth;
    for _ in 0..64 {
        let p = [x, y, z];
        let (d, stem) = scene(s, pose, p);
        if d < 0.004 {
            return Some((p, stem));
        }
        // the distance field is only roughly Lipschitz (bends, lobes): step shy
        z += d * 0.6;
        if z > depth {
            return None;
        }
    }
    None
}

fn normal(s: &Shape, pose: &Pose, p: V3) -> V3 {
    let e = 0.003;
    let d = |dx: f64, dy: f64, dz: f64| scene(s, pose, [p[0] + dx, p[1] + dy, p[2] + dz]).0;
    norm([
        d(e, 0.0, 0.0) - d(-e, 0.0, 0.0),
        d(0.0, e, 0.0) - d(0.0, -e, 0.0),
        d(0.0, 0.0, e) - d(0.0, 0.0, -e),
    ])
}

/// World point → (distance, is_stem).
fn scene(s: &Shape, pose: &Pose, p: V3) -> (f64, bool) {
    // world → pepper frame: undo tilt, spin, then the hanging pose
    let p = rot_x(p, -pose.tilt);
    let p = rot_y(p, -pose.spin);
    let p = rot_z(p, s.upright * std::f64::consts::FRAC_PI_2);
    pepper(s, p)
}

fn pepper(s: &Shape, p: V3) -> (f64, bool) {
    let half = s.len / 2.0;
    let [x, y, z] = p;
    let t = ((x + half) / s.len).clamp(0.0, 1.0); // 0 stem end → 1 tip
    let hook = ((t - 0.72) / 0.28).max(0.0);
    let axis_y = -s.bend * t * t + s.tail * hook * hook; // tip droops, a tail hooks back up
    let (dy, dz) = (y - axis_y, z);
    let radial = (dy * dy + dz * dz).sqrt();
    let theta = dz.atan2(dy);

    let shoulder = (t / 0.16).min(1.0);
    let shoulder = (1.0 - (1.0 - shoulder).powi(2)).sqrt(); // rounded stem end
    let taper = (1.0 - t.powf(s.taper)).max(0.0).sqrt(); // closes at the tip
    let lobes = 1.0 + s.lobe_depth * (s.lobes * theta).cos();
    let wrinkles = 1.0 + s.wrinkles * (x * 19.0).sin() * (theta * 6.0).sin();
    let bumps = 1.0 + s.bumps * (x * 7.0 + 1.3).sin() * (theta * 3.0).cos();
    let r = s.girth * shoulder * taper * lobes * wrinkles * bumps;

    let along = x - x.clamp(-half, half);
    let body = (along * along + radial * radial).sqrt() - r;

    // stem: a short capsule curling up out of the stem end
    let g = s.girth.min(0.5); // fat peppers don't get fat stems
    let a = [-half + 0.05, 0.0, 0.0];
    let b = [-half - g * 0.4, g * 0.3, 0.0];
    let stem = capsule(p, a, b, g * 0.16);
    if stem < body {
        (stem, true)
    } else {
        (body, false)
    }
}

fn capsule(p: V3, a: V3, b: V3, r: f64) -> f64 {
    let pa = sub(p, a);
    let ba = sub(b, a);
    let h = (dot(pa, ba) / dot(ba, ba)).clamp(0.0, 1.0);
    len(sub(pa, [ba[0] * h, ba[1] * h, ba[2] * h])) - r
}

fn rot_x([x, y, z]: V3, a: f64) -> V3 {
    let (s, c) = a.sin_cos();
    [x, y * c - z * s, y * s + z * c]
}

fn rot_y([x, y, z]: V3, a: f64) -> V3 {
    let (s, c) = a.sin_cos();
    [x * c + z * s, y, -x * s + z * c]
}

fn rot_z([x, y, z]: V3, a: f64) -> V3 {
    let (s, c) = a.sin_cos();
    [x * c - y * s, x * s + y * c, z]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn norm(a: V3) -> V3 {
    let l = len(a).max(1e-9);
    [a[0] / l, a[1] / l, a[2] / l]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pepper_is_solid_in_the_middle_and_empty_far_away() {
        let pose = Pose {
            spin: 0.0,
            tilt: 0.0,
        };
        for shape in RANK_SHAPES.iter().chain([&JALAPENO.lerp(&HABANERO, 0.5)]) {
            assert!(scene(shape, &pose, [0.0, 0.0, 0.0]).0 < 0.0);
            assert!(scene(shape, &pose, [0.0, 0.0, 1.6]).0 > 0.0);
        }
    }

    #[test]
    fn renders_a_lit_blob_centered() {
        let area = Rect::new(0, 0, 40, 16);
        let mut buf = Buffer::empty(area);
        draw(
            &mut buf,
            area,
            &JALAPENO,
            &Pose {
                spin: 0.3,
                tilt: 0.3,
            },
        );
        let inked = buf.content.iter().filter(|c| c.symbol() != " ").count();
        assert!(inked > 60, "drew {inked} cells");
        assert_ne!(buf[(20, 8)].symbol(), " ", "center is pepper");
        assert_eq!(buf[(0, 0)].symbol(), " ", "corner is background");
    }
}
