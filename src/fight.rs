//! The arena: the foe roams the screen (its distance to the panel is its
//! attack timer), attacks the panel in its own way, and the panel keeps the
//! scars. Your finished words fly out of the text at the foe.
//!
//! Game rules live in `run.rs`; this is only what you see, so it keeps its
//! own state (positions, trail, letters in flight, scars) and catches up with
//! the fight every frame. Units are scene units: a cell is 10×20, origin at
//! the screen's top-left.

use std::collections::VecDeque;
use std::f64::consts::PI;

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Color;
use web_time::Instant;

use crate::engine::{HOT, Test};
use crate::foes::{Expr, Kind, Mood, tam};
use crate::run::Fight;
use crate::toon::{self, Px, Rgb, capsule, circle, ellipse, hash, mix, rot, smooth};

const CW: f64 = 10.0;
const CH: f64 = 20.0;
const LETTER_FLIGHT: f64 = 0.5;
const DART_FLIGHT: f64 = 0.35;
const COMBO_COLORS: [(u8, u8, u8); 5] = [
    (232, 230, 227),
    (255, 215, 0),
    (255, 140, 0),
    (255, 58, 30),
    (255, 0, 200),
];

/// Where things are this frame, in scene units.
#[derive(Clone, Copy)]
pub struct Geo {
    pub screen: Rect,
    pub panel: Rect,
}

impl Geo {
    fn mid_x(&self) -> f64 {
        (self.screen.x as f64 + self.screen.width as f64 / 2.0) * CW
    }
    fn top(&self) -> f64 {
        self.panel.y as f64 * CH
    }
    fn bottom(&self) -> f64 {
        self.panel.bottom() as f64 * CH
    }
    fn left(&self) -> f64 {
        self.panel.x as f64 * CW
    }
    fn right(&self) -> f64 {
        self.panel.right() as f64 * CW
    }
    /// How far a foe may wander sideways and stay on screen.
    fn roam(&self, want: f64) -> f64 {
        want.min(self.screen.width as f64 * CW / 2.0 - 130.0)
            .max(40.0)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scar {
    /// A corner bitten off (naga).
    Bite { right: bool },
    /// A crack through the top and bottom border, at a fraction of the width.
    Crack(f64),
    /// A green poison stain with drips (reaper's sting).
    Poison(f64),
    /// A smoke burn (morita).
    Scorch(f64),
    /// A punctuation dart stuck in the top border, at a column.
    Dart(char, u16),
    /// The right side caved in (bonnet's hat).
    Dent,
}

struct Letter {
    idx: usize,
    ch: char,
    born: f64,
    last: bool,
    dmg: u64,
    color: Color,
    start: Option<(f64, f64)>,
}

struct Dart {
    ch: char,
    col: u16,
    born: f64,
    from: (f64, f64),
    bounce: bool,
}

pub struct Arena {
    born: Instant,
    t: f64,
    x: f64,
    y: f64,
    kb: f64,
    trail: VecDeque<(f64, f64, f64)>,
    letters: Vec<Letter>,
    pops: Vec<(f64, f64, f64, u64)>,
    sparks: Vec<(f64, f64, f64)>,
    darts: Vec<Dart>,
    pub scars: Vec<Scar>,
    struck: f64,
    seen_hits: usize,
    seen_landing: Option<Instant>,
    seen_attack: Option<Instant>,
    last_score: u64,
    thrown: usize,
    stung: bool,
}

impl Arena {
    pub fn new(fight: &Fight, geo: &Geo) -> Self {
        Arena {
            born: fight.born,
            t: 0.0,
            x: geo.mid_x() + geo.screen.width as f64 * CW, // walks in from the right
            y: geo.top() - 160.0,
            kb: 0.0,
            trail: VecDeque::new(),
            letters: Vec::new(),
            pops: Vec::new(),
            sparks: Vec::new(),
            darts: Vec::new(),
            scars: Vec::new(),
            struck: f64::NEG_INFINITY,
            seen_hits: 0,
            seen_landing: None,
            seen_attack: None,
            last_score: 0,
            thrown: 0,
            stung: false,
        }
    }

    /// Catch up with the fight: new words fly, attacks spawn effects, landed
    /// hits leave scars, the foe moves.
    pub fn update(&mut self, now: Instant, fight: &Fight, geo: &Geo) {
        if fight.born != self.born {
            *self = Arena::new(fight, geo);
        }
        let t = now.saturating_duration_since(self.born).as_secs_f64();
        let dt = (t - self.t).clamp(0.0, 0.1);
        self.t = t;
        let kind = fight.foe.kind;
        self.launch_words(&fight.test, geo);

        // letters reaching the foe: the last one of a word lands the blow
        let (fx, fy) = (self.x, self.y);
        let mut struck = None;
        self.letters.retain(|l| {
            if t - l.born < LETTER_FLIGHT {
                return true;
            }
            if l.last {
                struck = Some((l.dmg, l.color));
            }
            false
        });
        if let Some((dmg, _)) = struck {
            self.struck = t;
            self.kb += 38.0;
            self.pops.push((fx, fy - 70.0, t, dmg));
            self.sparks.push((fx, fy, t));
        }
        self.pops.retain(|p| t - p.2 < 0.8);
        self.sparks.retain(|s| t - s.2 < 0.35);
        self.kb *= (-5.0 * dt).exp();

        // attack timelines
        if fight.attack_at != self.seen_attack {
            (self.seen_attack, self.thrown, self.stung) = (fight.attack_at, 0, false);
        }
        if let Some(a) = self.attack_age(fight, now) {
            if kind == Kind::Pimiento {
                while self.thrown < 5 && a >= 0.15 + self.thrown as f64 * 0.12 {
                    let col = 6
                        + (hash(t * 13.0 + self.thrown as f64)
                            * (geo.panel.width.saturating_sub(12)) as f64)
                            as u16;
                    let ch = ['!', '?', ';', ',', '!'][self.thrown];
                    self.darts.push(Dart {
                        ch,
                        col,
                        born: t,
                        from: (fx, fy),
                        bounce: self.thrown == 2,
                    });
                    self.thrown += 1;
                }
            }
            if kind == Kind::Reaper && a >= 0.9 && !self.stung {
                self.stung = true;
                self.scars.push(Scar::Poison(0.2 + 0.6 * hash(t)));
            }
        }
        let scars = &mut self.scars;
        self.darts.retain(|d| {
            if t - d.born < DART_FLIGHT || (d.bounce && t - d.born < 1.6) {
                return true;
            }
            if !d.bounce {
                scars.push(Scar::Dart(d.ch, d.col));
            }
            false
        });
        if fight.hit_at != self.seen_landing {
            self.seen_landing = fight.hit_at;
            let n = self.scars.len();
            let scar = match kind {
                Kind::Naga => Some(Scar::Bite {
                    right: n.is_multiple_of(2),
                }),
                Kind::Reaper => Some(Scar::Crack(0.3 + 0.3 * hash(t))),
                Kind::Cayenne => Some(Scar::Crack(
                    ((geo.mid_x() + 120.0 - geo.left()) / (geo.right() - geo.left()))
                        .clamp(0.05, 0.95),
                )),
                Kind::Morita => Some(Scar::Scorch(0.2 + 0.6 * hash(t))),
                Kind::Bonnet => Some(Scar::Dent),
                Kind::Pimiento => None, // its darts are the scars
            };
            self.scars.extend(scar);
        }

        // the foe moves: roaming, or wherever its attack needs it
        if fight.foe_hp() == 0 {
            self.y -= 900.0 * dt; // K.O.: launched off the top
            return;
        }
        let k = smooth((fight.charge - 0.2) / 0.6);
        let (tx, ty, snap) = match self.attack_age(fight, now) {
            Some(a) => attack_pos(kind, geo, a),
            None => {
                let (x, y) = roam(kind, geo, t, k);
                (x, y, false)
            }
        };
        let follow = if snap { 1.0 } else { 1.0 - (-4.0 * dt).exp() };
        self.x += (tx - self.x) * follow;
        self.y += (ty - self.y) * follow;
        self.trail.push_back((self.x + self.kb, self.y, t));
        let keep = trail_spec(kind).map_or(0.0, |s| s.secs) + 0.05;
        while self.trail.front().is_some_and(|p| t - p.2 > keep) {
            self.trail.pop_front();
        }
    }

    fn attack_age(&self, fight: &Fight, now: Instant) -> Option<f64> {
        fight
            .attack_at
            .map(|a| now.saturating_duration_since(a).as_secs_f64())
    }

    /// Each newly finished word lifts out of the text, letter by letter.
    fn launch_words(&mut self, test: &Test, _geo: &Geo) {
        let hits = &test.hits[self.seen_hits.min(test.hits.len())..];
        for &(at, i) in hits {
            if test.target.get(i) != Some(&' ') {
                continue;
            }
            let start = test.target[..i]
                .iter()
                .rposition(|&c| c == ' ')
                .map_or(0, |p| p + 1);
            let dmg = test.score - self.last_score;
            self.last_score = test.score;
            let (r, g, b) = COMBO_COLORS[(test.combo as usize / 15).min(4)];
            let born = at.saturating_duration_since(self.born).as_secs_f64();
            for (k, idx) in (start..i).enumerate() {
                self.letters.push(Letter {
                    idx,
                    ch: test.target[idx],
                    born: born + k as f64 * 0.035,
                    last: idx + 1 == i,
                    dmg,
                    color: Color::Rgb(r, g, b),
                    start: None,
                });
            }
        }
        self.seen_hits = test.hits.len();
    }

    fn mood(&self, fight: &Fight, now: Instant) -> Mood {
        let t = self.t;
        let since = |at: Option<Instant>| {
            at.map_or(f64::MAX, |a| now.saturating_duration_since(a).as_secs_f64())
        };
        let test = &fight.test;
        let expr = match () {
            _ if fight.foe_hp() == 0 => Expr::Dead,
            _ if t - self.struck < 0.22 => Expr::Flinch,
            _ if since(fight.hit_at) < 0.8 => Expr::Grin,
            _ if fight.charge >= 0.8 || fight.attack_at.is_some() => Expr::Windup,
            _ if since(test.broke.map(|b| b.1)) < 1.1 => Expr::Laugh,
            _ if test.rank >= HOT => Expr::Scared,
            _ => Expr::Idle,
        };
        let hp = fight.foe_hp() as f64 / fight.foe.hp as f64;
        Mood::new(expr, hp, 1.0 - smooth((t - self.struck) / 0.08), t)
    }

    /// Background layer: the foe and its trail, lit. Draw before the panel.
    pub fn draw_back(&self, buf: &mut Buffer, fight: &Fight, now: Instant, geo: &Geo) {
        let kind = fight.foe.kind;
        let mood = self.mood(fight, now);
        let attack = self.attack_age(fight, now);
        let (ox, oy) = (
            geo.screen.width as f64 * CW / 2.0,
            geo.screen.height as f64 * CH / 2.0,
        );
        let (fx, fy, t) = (self.x + self.kb, self.y, self.t);
        let spec = trail_spec(kind).filter(|_| fight.foe_hp() > 0 && self.trail.len() > 2);
        let bounds = spec.map(|s| {
            let pad = s.r0 + s.flutter + 10.0;
            self.trail
                .iter()
                .fold((f64::MAX, f64::MIN, f64::MAX, f64::MIN), |b, p| {
                    (
                        b.0.min(p.0 - pad),
                        b.1.max(p.0 + pad),
                        b.2.min(p.1 + s.lift - pad),
                        b.3.max(p.1 + s.lift + pad),
                    )
                })
        });
        let heat = fight.test.rank as f64 / 6.0;
        toon::draw_lit(buf, geo.screen, 1.0, heat, |x, y| {
            let (x, y) = (x + ox, y + oy);
            if let Some(c) = kind.sprite(x - fx, y - fy, t, &mood, attack) {
                return Some(c);
            }
            let (spec, b) = (spec?, bounds?);
            (x > b.0 && x < b.1 && y > b.2 && y < b.3)
                .then(|| ribbon(&self.trail, &spec, x, y, t))
                .flatten()
        });
    }

    /// The panel after it's drawn: scars, then the attack's deformation.
    pub fn deform_panel(&self, buf: &mut Buffer, fight: &Fight, now: Instant, panel: Rect) {
        let panel = panel.intersection(buf.area);
        let (w, h) = (panel.width as usize, panel.height as usize);
        if w < 4 || h < 4 {
            return;
        }
        let mut cells: Vec<Cell> = (0..h * w)
            .map(|k| buf[(panel.x + (k % w) as u16, panel.y + (k / w) as u16)].clone())
            .collect();
        for r in 0..h {
            for c in 0..w {
                scar_cell(&mut cells[r * w + c], &self.scars, r, c, h, w, self.t);
            }
        }
        let d = self
            .attack_age(fight, now)
            .map(|a| deformation(fight.foe.kind, a))
            .unwrap_or_default();
        let blank = Cell::default();
        for r in 0..h {
            for c in 0..w {
                buf[(panel.x + c as u16, panel.y + r as u16)] = if d.is_none() {
                    cells[r * w + c].clone()
                } else {
                    blank.clone()
                };
            }
        }
        if d.is_none() {
            return;
        }
        let mid = (h - 1) as f64 / 2.0;
        let order: Vec<usize> = if d.flip < 0.0 {
            (0..h).rev().collect()
        } else {
            (0..h).collect()
        };
        for r in order {
            let wave = (PI * r as f64 / (h - 1) as f64).sin();
            let inset = (4.0 * wave * d.squeeze).round() as usize;
            for c in 0..w {
                if c < inset || c + inset >= w {
                    continue;
                }
                let mut cell = cells[r * w + c].clone();
                if inset > 0 && r > 0 && r + 1 < h && (c == inset || c + 1 + inset == w) {
                    cell = cells[r * w + if c == inset { 0 } else { w - 1 }].clone(); // the border bends in
                }
                let (mut row, mut col) = (r as f64, c as f64);
                if d.split > 0.0
                    && (c as f64) > (w as f64 * (0.3 + 0.35 * r as f64 / (h - 1) as f64)).round()
                {
                    row += d.split.round();
                    col += (3.0 * d.split).round();
                }
                if d.squash > 0.0 {
                    row = (h - 1) as f64 - ((h - 1 - r) as f64 * (1.0 - 0.35 * d.squash)).round();
                    if c == 0 || c + 1 == w {
                        col += if c == 0 { -1.0 } else { 1.0 } * (2.0 * d.squash * wave).round();
                    }
                }
                if d.jitter > 0.0
                    && r > 0
                    && r + 1 < h
                    && hash(c as f64 * 7.0 + (self.t * 20.0).floor() + r as f64 * 3.0)
                        < 0.35 * d.jitter
                {
                    row -= 1.0; // letters jump
                }
                if d.flip != 1.0 {
                    row = (mid + (r as f64 - mid) * d.flip).round();
                }
                let (x, y) = (panel.x as f64 + col, panel.y as f64 + row);
                if x >= buf.area.left() as f64
                    && x < buf.area.right() as f64
                    && y >= buf.area.top() as f64
                    && y < buf.area.bottom() as f64
                {
                    buf[(x as u16, y as u16)] = cell;
                }
            }
        }
    }

    /// Foreground: attack effects and impact sparks, over the panel.
    pub fn draw_front(&self, buf: &mut Buffer, fight: &Fight, now: Instant, geo: &Geo) {
        let kind = fight.foe.kind;
        let attack = self.attack_age(fight, now);
        if attack.is_none() && self.sparks.is_empty() {
            return;
        }
        let (ox, oy) = (
            geo.screen.width as f64 * CW / 2.0,
            geo.screen.height as f64 * CH / 2.0,
        );
        let d = attack.map(|a| deformation(kind, a)).unwrap_or_default();
        let poison = self.scars.iter().rev().find_map(|s| match s {
            Scar::Poison(at) => Some(*at),
            _ => None,
        });
        toon::draw(buf, geo.screen, 1.0, |x, y| {
            let (x, y) = (x + ox, y + oy);
            for &(sx, sy, born) in &self.sparks {
                let age = (self.t - born) / 0.35;
                for k in 0..8 {
                    let (a, r) = (k as f64 * PI / 4.0 + 0.3, 10.0 + age * 60.0);
                    if circle(
                        x,
                        y,
                        sx + a.cos() * r,
                        sy + a.sin() * r * 0.6,
                        6.0 * (1.0 - age),
                    ) < 0.0
                    {
                        return Some([255.0, 230.0, 140.0]);
                    }
                }
            }
            let a = attack?;
            match kind {
                Kind::Naga => coil(geo, x, y, d.squeeze),
                Kind::Reaper => slash_and_sting(geo, x, y, a, poison.unwrap_or(0.5)),
                Kind::Cayenne => dust(geo, x, y, a),
                Kind::Morita => smoke(geo, x, y, self.t, a),
                Kind::Bonnet => salt_and_hat(geo, x, y, self.t, a),
                Kind::Pimiento => None,
            }
        });
    }

    /// Text on top: flying words, darts, damage numbers. `pos` maps a target
    /// index to its cell on screen (None if scrolled away).
    pub fn draw_text(
        &mut self,
        buf: &mut Buffer,
        geo: &Geo,
        pos: impl Fn(usize) -> Option<(u16, u16)>,
        caret: (u16, u16),
    ) {
        let t = self.t;
        let put = |buf: &mut Buffer, x: f64, y: f64, s: &str, color: Color| {
            let (col, row) = ((x / CW).round(), (y / CH).round());
            let a = buf.area;
            if col >= a.left() as f64
                && col < a.right() as f64
                && row >= a.top() as f64
                && row < a.bottom() as f64
            {
                buf[(col as u16, row as u16)].set_symbol(s).set_fg(color);
            }
        };
        let (fx, fy) = (self.x, self.y);
        for l in &mut self.letters {
            if t < l.born {
                continue;
            }
            let (sx, sy) = *l.start.get_or_insert_with(|| {
                let (c, r) = pos(l.idx).unwrap_or(caret);
                (c as f64 * CW, r as f64 * CH)
            });
            let p = ((t - l.born) / LETTER_FLIGHT).clamp(0.0, 1.0);
            let (mx, my) = ((sx + fx) / 2.0, sy.min(fy) - 140.0);
            let x = (1.0 - p).powi(2) * sx + 2.0 * (1.0 - p) * p * mx + p * p * fx;
            let y = (1.0 - p).powi(2) * sy + 2.0 * (1.0 - p) * p * my + p * p * fy;
            put(buf, x, y, &l.ch.to_string(), l.color);
        }
        let dart = Color::Rgb(255, 224, 138);
        for d in &self.darts {
            let p = ((t - d.born) / DART_FLIGHT).clamp(0.0, 1.0);
            let (tx, ty) = ((geo.panel.x + d.col) as f64 * CW, geo.top());
            if p < 1.0 {
                let x = d.from.0 + (tx - d.from.0) * p;
                let y = d.from.1 + (ty - d.from.1) * p - (p * PI).sin() * 60.0;
                put(buf, x, y, &d.ch.to_string(), dart);
            } else if d.bounce {
                put(buf, tx, caret.1 as f64 * CH, &d.ch.to_string(), dart); // ricocheted into the text
            }
        }
        for &(x, y, born, n) in &self.pops {
            let age = (t - born) / 0.8;
            let fade = toon::mix([255.0, 224.0, 138.0], [40.0, 36.0, 30.0], age);
            let color = Color::Rgb(fade[0] as u8, fade[1] as u8, fade[2] as u8);
            for (k, ch) in format!("-{n}").chars().enumerate() {
                put(
                    buf,
                    x - 20.0 + k as f64 * CW,
                    y - age * 40.0,
                    &ch.to_string(),
                    color,
                );
            }
        }
    }
}

// ── per-foe motion and deformation ──────────────────────────────────────

fn roam(kind: Kind, g: &Geo, t: f64, k: f64) -> (f64, f64) {
    let (mid, top) = (g.mid_x(), g.top());
    let half = (g.right() - g.left()) / 2.0;
    match kind {
        Kind::Naga => (
            mid + (1.0 - k) * g.roam(330.0) * (t * 0.45).sin() + k * (half - 40.0),
            top - 70.0,
        ),
        Kind::Reaper => (
            mid + (1.0 - k) * g.roam(360.0) * (t * 0.3).sin() + 30.0 * (t * 1.7).sin(),
            top - 180.0 + 25.0 * (t * 1.1).sin() + 70.0 * k,
        ),
        Kind::Cayenne => (
            mid + (1.0 - 0.7 * k) * g.roam(380.0) * (t * 0.5).sin(),
            top - 66.0 - (t * (3.0 + 3.0 * k)).sin().abs() * (40.0 + 60.0 * k),
        ),
        Kind::Pimiento => (
            mid + g.roam(360.0) * (t * 0.35).sin(),
            top - 185.0 + 20.0 * (t * 1.4).sin() + 40.0 * k,
        ),
        Kind::Morita => (
            mid + g.roam(330.0) * (t * 0.25 + 1.0).sin(),
            top - 170.0 + 20.0 * (t * 0.8).sin() + 30.0 * k,
        ),
        Kind::Bonnet => (
            mid + (1.0 - 0.5 * k) * g.roam(360.0) * (t * 0.4).sin(),
            top - 62.0 - (t * 5.0).sin().abs() * 26.0,
        ),
    }
}

/// Where the foe goes during its attack; `true` = jump there exactly.
fn attack_pos(kind: Kind, g: &Geo, a: f64) -> (f64, f64, bool) {
    let (mid, top) = (g.mid_x(), g.top());
    let half = (g.right() - g.left()) / 2.0;
    match kind {
        Kind::Naga => (mid + half - 40.0, top - 70.0, false),
        Kind::Reaper => (mid, top - 110.0, false),
        Kind::Cayenne => {
            // leap up, then slam down into the panel
            let y = if a < 0.4 {
                (top - 66.0) + (-300.0) * smooth(a / 0.4)
            } else if a < 0.55 {
                (top - 366.0) + 322.0 * ((a - 0.4) / 0.15)
            } else {
                top - 66.0
            };
            (mid + 120.0, y, true)
        }
        Kind::Pimiento => (mid - g.roam(200.0), top - 160.0, false),
        Kind::Morita => (mid, top - 150.0, false),
        Kind::Bonnet => (mid - g.roam(220.0), top - 62.0, false),
    }
}

struct Deform {
    squeeze: f64,
    split: f64,
    squash: f64,
    jitter: f64,
    /// 1 = upright, -1 = upside down.
    flip: f64,
}

impl Default for Deform {
    fn default() -> Self {
        Deform {
            squeeze: 0.0,
            split: 0.0,
            squash: 0.0,
            jitter: 0.0,
            flip: 1.0,
        }
    }
}

impl Deform {
    fn is_none(&self) -> bool {
        self.squeeze < 0.01
            && self.split < 0.01
            && self.squash < 0.01
            && self.jitter < 0.01
            && self.flip == 1.0
    }
}

/// 0→1 around `from`, back to 0 after `to`.
fn window(a: f64, from: f64, to: f64) -> f64 {
    smooth((a - from) / 0.12) * (1.0 - smooth((a - to) / 0.2))
}

fn deformation(kind: Kind, a: f64) -> Deform {
    let none = Deform::default();
    match kind {
        Kind::Naga => Deform {
            squeeze: smooth(a / 0.5) * (1.0 - smooth((a - 0.95) / 0.3)),
            ..none
        },
        Kind::Reaper => Deform {
            split: smooth((a - 0.2) / 0.08) * (1.0 - smooth((a - 0.75) / 0.2)),
            ..none
        },
        Kind::Cayenne => Deform {
            squash: smooth((a - 0.5) / 0.05) * (1.0 - smooth((a - 0.7) / 0.35)),
            jitter: window(a, 0.52, 0.85),
            ..none
        },
        Kind::Morita => Deform {
            flip: match a {
                a if a < 0.8 => 1.0,
                a if a < 1.2 => (smooth((a - 0.8) / 0.4) * PI).cos(),
                a if a < 1.8 => -1.0,
                a => ((1.0 - smooth((a - 1.8) / 0.4)) * PI).cos(),
            },
            ..none
        },
        Kind::Pimiento | Kind::Bonnet => none,
    }
}

fn scar_cell(cell: &mut Cell, scars: &[Scar], r: usize, c: usize, h: usize, w: usize, t: f64) {
    let border = r == 0 || r + 1 == h;
    let gray = Color::Rgb(201, 194, 176);
    for s in scars {
        match *s {
            Scar::Bite { right } => {
                let cc = if right { w - 1 - c } else { c } as f64;
                if r <= 2 && cc <= 4.0 - r as f64 * 1.5 {
                    *cell = Cell::default();
                    return;
                }
                if r <= 2 && cc <= 5.0 - r as f64 * 1.5 {
                    cell.set_symbol(if r == 0 { "▄" } else { "▐" })
                        .set_fg(Color::Rgb(90, 42, 16));
                }
            }
            Scar::Crack(at) => {
                let d = c as i64 - (at * w as f64).round() as i64;
                if border && d.abs() <= 2 {
                    let sym = if d == 0 {
                        "╳"
                    } else if (d < 0) == (r == 0) {
                        "╱"
                    } else {
                        "╲"
                    };
                    cell.set_symbol(sym).set_fg(gray);
                }
                if r == 1 && d.abs() == 1 {
                    cell.set_symbol("╲").set_fg(Color::Rgb(90, 88, 96));
                }
            }
            Scar::Poison(at) => {
                let cx = (at * w as f64).round() as usize;
                let green = Color::Rgb(93, 214, 44);
                if r == 0 && c.abs_diff(cx) <= 6 {
                    cell.set_fg(green);
                }
                if !border && c == cx && (t * 3.0 + r as f64).floor() as i64 % 4 == 0 {
                    cell.set_symbol("┊").set_fg(green);
                }
            }
            Scar::Scorch(at) => {
                let d = c.abs_diff((at * w as f64).round() as usize);
                if border && d <= 5 {
                    let (sym, col) = if d <= 2 {
                        ("▓", (58, 42, 34))
                    } else {
                        ("▒", (90, 64, 48))
                    };
                    cell.set_symbol(sym).set_fg(Color::Rgb(col.0, col.1, col.2));
                }
            }
            Scar::Dart(ch, col) if r == 0 && c == col as usize => {
                cell.set_symbol(&ch.to_string())
                    .set_fg(Color::Rgb(255, 224, 138));
            }
            Scar::Dent => {
                let (from, to) = (h * 3 / 10, h * 3 / 10 + 3);
                if (from..=to).contains(&r) && c + 1 == w {
                    *cell = Cell::default();
                    return;
                }
                if (from..=to).contains(&r) && c + 2 == w {
                    let sym = if r == from {
                        "╮"
                    } else if r == to {
                        "╯"
                    } else {
                        "│"
                    };
                    cell.set_symbol(sym);
                }
            }
            _ => {}
        }
    }
}

// ── trails: smooth secondary motion ─────────────────────────────────────

#[derive(Clone, Copy)]
struct TrailSpec {
    secs: f64,
    r0: f64,
    lift: f64,
    flutter: f64,
    near: Rgb,
    far: Rgb,
    edge: Option<Rgb>,
    banded: bool,
}

fn trail_spec(kind: Kind) -> Option<TrailSpec> {
    match kind {
        Kind::Reaper => Some(TrailSpec {
            secs: 1.0,
            r0: 42.0,
            lift: 110.0,
            flutter: 26.0,
            near: [60.0, 40.0, 85.0],
            far: [22.0, 14.0, 30.0],
            edge: Some([110.0, 80.0, 150.0]),
            banded: false,
        }),
        Kind::Naga => Some(TrailSpec {
            secs: 1.1,
            r0: 20.0,
            lift: 40.0,
            flutter: 6.0,
            near: [235.0, 85.0, 25.0],
            far: [190.0, 65.0, 15.0],
            edge: Some([100.0, 30.0, 5.0]),
            banded: true,
        }),
        Kind::Cayenne => Some(TrailSpec {
            secs: 0.22,
            r0: 24.0,
            lift: 0.0,
            flutter: 0.0,
            near: [200.0, 40.0, 30.0],
            far: [70.0, 10.0, 10.0],
            edge: None,
            banded: false,
        }),
        Kind::Pimiento => Some(TrailSpec {
            secs: 0.45,
            r0: 22.0,
            lift: 10.0,
            flutter: 4.0,
            near: [120.0, 150.0, 50.0],
            far: [40.0, 55.0, 15.0],
            edge: None,
            banded: false,
        }),
        Kind::Morita | Kind::Bonnet => None,
    }
}

/// A ribbon along the foe's recent path: tapering toward the tail, fluttering.
fn ribbon(trail: &VecDeque<(f64, f64, f64)>, s: &TrailSpec, x: f64, y: f64, t: f64) -> Px {
    let mut best: Option<(f64, Rgb)> = None;
    let mut along = 0.0;
    let mut i = trail.len() - 1;
    while i >= 2 {
        let (a, b) = (trail[i], trail[i - 2]);
        let u = (t - a.2) / s.secs;
        if u >= 1.0 {
            break;
        }
        let wob = |u: f64, k: f64| s.flutter * u * (u * 9.0 - t * 7.0 + k).sin();
        let (ax, ay, bx, by) = (
            a.0,
            a.1 + s.lift + wob(u, 0.0),
            b.0,
            b.1 + s.lift + wob(u, 0.4),
        );
        let d = capsule(x, y, ax, ay, bx, by, 0.0);
        let r = s.r0 * (1.0 - u).powf(0.8);
        if d < r && best.is_none_or(|(bu, _)| u < bu) {
            let c = match s.edge {
                Some(e) if d > r - 3.5 => e,
                _ if s.banded => {
                    if ((along + d) * 0.18).sin() > 0.5 {
                        s.far
                    } else {
                        s.near
                    }
                }
                _ => mix(s.near, s.far, u),
            };
            best = Some((u, c));
        }
        along += (bx - ax).hypot(by - ay);
        i -= 2;
    }
    best.map(|b| b.1)
}

// ── attack effects (foreground) ─────────────────────────────────────────

/// Naga's body wrapped around the squeezed panel.
fn coil(g: &Geo, x: f64, y: f64, sq: f64) -> Px {
    if sq < 0.05 {
        return None;
    }
    let (top, bottom) = (g.top(), g.bottom());
    let h = g.panel.height as f64;
    let row = ((y - top) / CH).clamp(0.0, h - 1.0);
    let inset = (4.0 * (PI * row / (h - 1.0)).sin() * sq).round() * CW;
    let (left, right, mid) = (g.left() + inset, g.right() - inset, (top + bottom) / 2.0);
    if ((y - mid).atan2((x - g.mid_x()) * 0.35)).abs() > sq * PI * 1.05 {
        return None;
    }
    let side = y > top && y < bottom && ((x - right).abs() < 9.0 || (x - left).abs() < 9.0);
    let lid =
        x > left && x < right && ((y - top - 4.0).abs() < 9.0 || (y - bottom + 4.0).abs() < 9.0);
    (side || lid).then_some(if (x * 0.25 + y * 0.25).sin() > 0.6 {
        [190.0, 65.0, 15.0]
    } else {
        [235.0, 85.0, 25.0]
    })
}

/// Reaper: a slash beam across the panel, then the sting with poison drips.
fn slash_and_sting(g: &Geo, x: f64, y: f64, a: f64, poison_at: f64) -> Px {
    let (top, bottom, left, w) = (g.top(), g.bottom(), g.left(), g.right() - g.left());
    let fade = 1.0 - smooth((a - 0.25) / 0.3);
    if fade > 0.0 {
        let v = (y - top) / (bottom - top);
        let lx = left + w * (0.3 + 0.35 * v);
        if v > -0.1 && v < smooth(a / 0.2) * 1.1 && (x - lx).abs() < 5.0 * fade + 1.0 {
            return Some(if (x - lx).abs() < 2.0 {
                [255.0, 255.0, 255.0]
            } else {
                [200.0, 160.0, 255.0]
            });
        }
    }
    if a > 0.85 && a < 1.5 {
        let sx = left + poison_at * w;
        let stab = smooth((a - 0.85) / 0.1) * (1.0 - smooth((a - 1.05) / 0.2));
        if capsule(
            x,
            y,
            sx + 40.0,
            top - 140.0 + stab * 60.0,
            sx,
            top - 80.0 + stab * 76.0,
            4.0,
        ) < 0.0
        {
            return Some([230.0, 220.0, 200.0]);
        }
        for k in 0..4 {
            let life = ((a - 0.95) * 1.6 + k as f64 * 0.25) % 1.0;
            if a > 0.95
                && ellipse(
                    x,
                    y,
                    sx + (k as f64 - 1.5) * 8.0,
                    top + 10.0 + life * 120.0,
                    4.0,
                    6.0,
                ) < 0.0
            {
                return Some([93.0, 214.0, 44.0]);
            }
        }
    }
    None
}

/// Cayenne: dust puffs where it lands.
fn dust(g: &Geo, x: f64, y: f64, a: f64) -> Px {
    let p = (a - 0.55) / 0.4;
    if !(0.0..=1.0).contains(&p) {
        return None;
    }
    let lx = g.mid_x() + 120.0;
    for k in 0..6 {
        let side = if k % 2 == 1 { 1.0 } else { -1.0 };
        let cx = lx + side * (30.0 + p * 90.0 + k as f64 * 8.0);
        let cy = g.top() - 6.0 - p * 20.0 - (k / 2) as f64 * 6.0;
        if circle(x, y, cx, cy, 9.0 * (1.0 - p) + 3.0) < 0.0 {
            return Some(mix([200.0, 190.0, 170.0], [90.0, 85.0, 80.0], p));
        }
    }
    None
}

/// Morita: a smoke cloud rolling over the panel.
fn smoke(g: &Geo, x: f64, y: f64, t: f64, a: f64) -> Px {
    let amt = smooth(a / 0.3) * (1.0 - smooth((a - 0.9) / 0.5));
    let (top, bottom, left, w) = (g.top(), g.bottom(), g.left(), g.right() - g.left());
    if amt <= 0.02 || y < top - 60.0 || y > bottom + 20.0 {
        return None;
    }
    let span = (bottom - top).min(200.0);
    for k in 0..14 {
        let k = k as f64;
        let cx = left + (k / 13.0) * w + (t * 1.3 + k).sin() * 20.0;
        let cy = top + 40.0 + (k * 2.1 + t).sin() * span * 0.3;
        if circle(
            x,
            y,
            cx,
            cy,
            (34.0 + 14.0 * (k * 1.7 + t * 2.0).sin()) * amt,
        ) < 0.0
        {
            return Some(if (x * 0.05 + y * 0.07 + t).sin() > 0.3 {
                [120.0, 118.0, 125.0]
            } else {
                [90.0, 88.0, 96.0]
            });
        }
    }
    None
}

/// Bonnet: salt piling on the panel, then its hat thrown like a boomerang.
fn salt_and_hat(g: &Geo, x: f64, y: f64, t: f64, a: f64) -> Px {
    let (top, bottom) = (g.top(), g.bottom());
    let home = g.mid_x() - g.roam(220.0);
    let pile_x = home + 70.0;
    let pile = smooth(a / 0.8) * (1.0 - smooth((a - 1.4) / 0.4));
    if pile > 0.0
        && y < top + 6.0
        && y > top - 40.0 * pile
        && (x - pile_x).abs() < (top + 6.0 - y) * 1.6
    {
        return Some(if hash(x * 3.1 + y) > 0.5 {
            [245.0, 245.0, 250.0]
        } else {
            [200.0, 200.0, 210.0]
        });
    }
    if pile > 0.3 {
        for k in 0..10 {
            let k = k as f64;
            let life = (t * 0.8 + hash(k)) % 1.0;
            if (x - (pile_x - 120.0 + hash(k + 5.0) * 260.0)).abs() < 3.0
                && (y - (top + life * (bottom - top))).abs() < 5.0
            {
                return Some([240.0, 240.0, 245.0]);
            }
        }
    }
    let p = ((a - 0.8) / 0.85).clamp(0.0, 1.0);
    if p > 0.0 && p < 1.0 {
        let out = (p * PI).sin();
        let hx = home + (g.right() - 10.0 - home) * out;
        let hy = top - 70.0 + (p * PI * 2.0).sin() * 50.0 + out * 90.0;
        let (rx, ry) = rot(x, y, hx, hy, t * 14.0);
        // the thrown tam, at half size like the bonnet itself
        if circle(rx, ry, hx + 6.0, hy - 12.0, 7.0) < 0.0 {
            return Some([210.0, 40.0, 50.0]);
        }
        return tam((rx - hx) / 0.5 + hx, (ry - hy) / 0.5 + hy, hx, hy);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deformations_return_to_rest() {
        for kind in [
            Kind::Cayenne,
            Kind::Naga,
            Kind::Pimiento,
            Kind::Morita,
            Kind::Bonnet,
            Kind::Reaper,
        ] {
            assert!(
                deformation(kind, 0.0).is_none() || kind == Kind::Morita,
                "{kind:?} starts at rest"
            );
            assert!(
                deformation(kind, kind.attack_len() + 0.5).is_none(),
                "{kind:?} ends at rest"
            );
        }
        assert!(
            deformation(Kind::Morita, 1.5).flip < 0.0,
            "morita holds the box upside down"
        );
        assert!(deformation(Kind::Naga, 0.6).squeeze > 0.8);
    }

    #[test]
    fn morita_flips_the_panel_and_naga_squeezes_it() {
        use crate::run::{FOES, Run, Stage};
        use std::time::Duration;
        let mut run = Run::new();
        let Stage::Fight(fight) = &mut run.stage else { panic!("fighting") };
        let screen = Rect::new(0, 0, 40, 30);
        let panel = Rect::new(5, 20, 30, 9);
        let draw_panel = |buf: &mut Buffer| {
            for c in panel.left()..panel.right() {
                buf[(c, panel.top())].set_symbol("T");
                buf[(c, panel.bottom() - 1)].set_symbol("B");
            }
        };
        let geo = Geo { screen, panel };
        let now = Instant::now();

        fight.foe = &FOES[3]; // morita
        fight.attack_at = Some(now - Duration::from_secs_f64(1.5));
        let arena = Arena::new(fight, &geo);
        let mut buf = Buffer::empty(screen);
        draw_panel(&mut buf);
        arena.deform_panel(&mut buf, fight, now, panel);
        assert_eq!(buf[(10, panel.top())].symbol(), "B", "upside down: bottom border on top");
        assert_eq!(buf[(10, panel.bottom() - 1)].symbol(), "T");

        fight.foe = &FOES[1]; // naga
        fight.attack_at = Some(now - Duration::from_secs_f64(0.6));
        let mut buf = Buffer::empty(screen);
        draw_panel(&mut buf);
        for r in panel.top()..panel.bottom() {
            buf[(panel.left(), r)].set_symbol("│");
        }
        arena.deform_panel(&mut buf, fight, now, panel);
        let mid = panel.top() + 4;
        assert_eq!(buf[(panel.left(), mid)].symbol(), " ", "squeezed: the side moved in");
        assert_eq!(buf[(panel.left() + 4, mid)].symbol(), "│");
    }

    #[test]
    fn scars_mark_the_border() {
        let (h, w) = (9, 40);
        let mut cell = Cell::default();
        cell.set_symbol("─");
        scar_cell(&mut cell, &[Scar::Crack(0.5)], 0, 20, h, w, 0.0);
        assert_eq!(cell.symbol(), "╳");
        let mut corner = Cell::default();
        corner.set_symbol("╮");
        scar_cell(
            &mut corner,
            &[Scar::Bite { right: true }],
            0,
            w - 1,
            h,
            w,
            0.0,
        );
        assert_eq!(corner.symbol(), " ", "bitten off");
        let mut top = Cell::default();
        scar_cell(&mut top, &[Scar::Dart('?', 7)], 0, 7, h, w, 0.0);
        assert_eq!(top.symbol(), "?");
    }
}
