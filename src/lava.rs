//! FLOOR IS LAVA: words fall toward a lava pool; type them before they land.
//! Every landed word raises the lava, so the next ones land sooner; five and
//! you're cooked. The typing runs through `Test` (heat, combo, score, the
//! results screen), fed one correct letter at a time.
//!
//! Positions are fractions of the field (x across, y down, 1 = the bottom),
//! so the rules don't depend on the terminal size; `draw` maps them to cells.

use rand::RngExt;
use ratatui::prelude::*;
use web_time::Instant;

use crate::engine::{Aim, Mode, RANKS, Test, random_words};
use crate::fx::{self, fire, hash, lerp, mix, page_rgb, put, put_behind, rank_color, since};
use crate::settings::{calm, touch};
use crate::{menu, ui};

// ponytail: tuned on paper, like survival; playtest before trusting them
/// How far one landed word raises the lava, as a fraction of the field.
const RISE: f64 = 0.1;
/// Landings until the floor's gone.
const LANDINGS: usize = 5;
/// Seconds a word takes to fall the whole field at level 1.
const FALL: f64 = 9.0;
/// Seconds between new words at level 1.
const SPAWN: f64 = 2.4;
/// Each level falls and spawns this much faster.
const FASTER: f64 = 0.9;

pub struct Drop {
    pub word: Vec<char>,
    /// Across the field, 0..1 (the draw keeps the whole word inside).
    pub x: f64,
    /// Down the field, 0 = the top, 1 = the bottom.
    pub y: f64,
    id: u64,
}

/// A word typed away, for its burst and +points.
pub struct Clear {
    pub at: Instant,
    pub x: f64,
    pub y: f64,
    pub word: Vec<char>,
    pub points: u64,
}

pub struct Lava {
    pub test: Test,
    pub aim: Aim,
    pub drops: Vec<Drop>,
    /// Lock-on: the word being typed. (Free mode's letters can fit several.)
    pub lock: Option<u64>,
    /// Letters typed into the current word, in either aim.
    pub buffer: Vec<char>,
    pub cleared: Vec<Clear>,
    /// Landed words: when, where, how long (for the splash). Their count is
    /// how high the lava is.
    pub splashes: Vec<(Instant, f64, usize)>,
    next_id: u64,
    /// Game time of the next word (the test's clock runs from the first key,
    /// so the first word waits for you).
    next_spawn: f64,
    last: Option<Instant>,
    /// Score when the current word was started (for its +points).
    score_from: u64,
}

impl Lava {
    pub fn new(aim: Aim) -> Self {
        let mut l = Lava {
            test: Test::new(Mode::Lava(aim), &[]),
            aim,
            drops: Vec::new(),
            lock: None,
            buffer: Vec::new(),
            cleared: Vec::new(),
            splashes: Vec::new(),
            next_id: 0,
            next_spawn: SPAWN,
            last: None,
            score_from: 0,
        };
        l.spawn();
        l.drops[0].y = 0.1; // the first word hangs where you can see it
        l
    }

    /// Lava height, as a fraction of the field.
    pub fn lava(&self) -> f64 {
        self.splashes.len() as f64 * RISE
    }

    fn pace(&self) -> f64 {
        FASTER.powi(self.test.level() as i32 - 1)
    }

    /// A new word at the top: longer ones as levels go up, and (when it can)
    /// a first letter no other falling word has, so lock-on stays predictable.
    fn spawn(&mut self) {
        let cap = 4 + self.test.level() as usize;
        let words = random_words(40);
        let fits = |w: &&&str| w.chars().count() <= cap;
        let fresh = |w: &&&str| !self.drops.iter().any(|d| w.starts_with(d.word[0]));
        let word = words
            .iter()
            .find(|w| fits(w) && fresh(w))
            .or_else(|| words.iter().find(fits))
            .unwrap_or(&words[0]);
        self.next_id += 1;
        self.drops.push(Drop {
            word: word.chars().collect(),
            x: rand::rng().random_range(0.0..1.0),
            y: 0.0,
            id: self.next_id,
        });
    }

    pub fn key(&mut self, c: char, now: Instant) {
        if self.test.finished.is_some() {
            return;
        }
        // space between words is habit, not a mistake; in free mode it starts over
        if c == ' ' {
            if self.aim == Aim::Free {
                self.buffer.clear();
            }
            return;
        }
        let mut next = self.buffer.clone();
        next.push(c);
        let spells = |d: &Drop| d.word.starts_with(&next);
        // lock-on stays on its word; otherwise the lowest word these letters start
        let target = match self.lock {
            Some(id) => self.drops.iter().position(|d| d.id == id && spells(d)),
            None => lowest(&self.drops, spells),
        };
        let Some(i) = target else {
            return self.test.miss(now);
        };
        if self.buffer.is_empty() {
            self.score_from = self.test.score;
        }
        self.test.feed(c, now);
        self.buffer = next;
        if self.aim == Aim::Lock {
            self.lock = Some(self.drops[i].id);
        }
        // done: the locked word, or (free) the lowest word spelled out exactly
        let done = match self.aim {
            Aim::Lock => (self.drops[i].word == self.buffer).then_some(i),
            Aim::Free => lowest(&self.drops, |d| d.word == self.buffer),
        };
        if let Some(i) = done {
            self.clear(i, now);
        }
    }

    /// The letter that moves things along: the next one in the word being
    /// typed, else the first of the lowest word (the demo autopilot plays this).
    pub fn next_char(&self) -> Option<char> {
        let going = match self.lock {
            Some(id) => self.drops.iter().find(|d| d.id == id),
            None if self.buffer.is_empty() => None,
            None => self.drops.iter().find(|d| d.word.starts_with(&self.buffer)),
        };
        match going {
            Some(d) => d.word.get(self.buffer.len()).copied(),
            None => lowest(&self.drops, |_| true).map(|i| self.drops[i].word[0]),
        }
    }

    /// Lock-on: let go of the word (to switch). Free: take back a letter.
    pub fn backspace(&mut self) {
        match self.aim {
            Aim::Lock => {
                self.lock = None;
                self.buffer.clear();
            }
            Aim::Free => {
                self.buffer.pop();
            }
        }
    }

    fn clear(&mut self, i: usize, now: Instant) {
        let d = self.drops.remove(i);
        self.test.words += 1;
        self.cleared.push(Clear {
            at: now,
            x: d.x,
            y: d.y,
            points: self.test.score - self.score_from,
            word: d.word,
        });
        self.lock = None;
        self.buffer.clear();
    }

    pub fn tick(&mut self, now: Instant) {
        let dt = self
            .last
            .map_or(0.0, |l| now.saturating_duration_since(l).as_secs_f64());
        self.last = Some(now);
        self.test.tick(now);
        if self.test.started.is_none() || self.test.finished.is_some() {
            return;
        }
        let speed = 1.0 / (FALL * self.pace());
        for d in &mut self.drops {
            d.y += speed * dt;
        }
        // whatever reaches the lava burns, and the lava rises
        let surface = 1.0 - self.lava();
        let mut i = 0;
        while i < self.drops.len() {
            if self.drops[i].y < surface {
                i += 1;
                continue;
            }
            let d = self.drops.remove(i);
            if self.lock == Some(d.id) {
                self.lock = None;
            }
            self.splashes.push((now, d.x, d.word.len()));
            self.test.burn(now);
        }
        if !self.drops.iter().any(|d| d.word.starts_with(&self.buffer)) {
            self.buffer.clear();
        }
        if self.splashes.len() >= LANDINGS {
            return self.test.finish(now);
        }
        let clock = self.test.elapsed(now).as_secs_f64();
        while clock >= self.next_spawn {
            self.spawn();
            self.next_spawn += (SPAWN * self.pace()).max(0.6);
        }
    }

    /// Letters already typed into this word (lock-on: the locked word; free:
    /// every word the letters spell the start of).
    fn typed_in(&self, d: &Drop) -> usize {
        match self.lock {
            Some(id) if id == d.id => self.buffer.len(),
            None if d.word.starts_with(&self.buffer) => self.buffer.len(),
            _ => 0,
        }
    }
}

/// The lowest falling word that passes `f`, if any.
fn lowest(drops: &[Drop], f: impl Fn(&Drop) -> bool) -> Option<usize> {
    drops
        .iter()
        .enumerate()
        .filter(|(_, d)| f(d))
        .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
        .map(|(i, _)| i)
}

const BURST: f64 = 0.6; // a cleared word flying apart
const SPLASH: f64 = 0.7; // a landed word's spatter
const RISING: f64 = 0.4; // the lava swelling up after a landing

/// Where a word of `len` letters at field position `(x, y)` starts on screen.
fn cell(field: Rect, x: f64, y: f64, len: usize) -> (i32, i32) {
    let span = field.height.saturating_sub(1) as f64;
    let cx = field.x as f64 + field.width.saturating_sub(len as u16) as f64 * x;
    (
        cx.round() as i32,
        field.y as i32 + (y * span).round() as i32,
    )
}

pub fn draw(f: &mut Frame, l: &Lava, now: Instant) {
    let col = ui::column(f.area());
    let [hud1, hud2, _, field, heat, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(col);
    // narrow screens (phones) get a compact HUD so its two sides don't collide
    let narrow = col.width < 64;
    fx::speed_lines(f.buffer_mut(), &l.test, now);
    hud(f, l, now, [hud1, hud2], narrow);
    let buf = f.buffer_mut();
    let surface = pool(buf, l, field, now);
    words(buf, l, field);
    if l.test.started.is_none() {
        let middle = Rect::new(field.x, field.y + field.height / 2, field.width, 1);
        Line::from("type a falling word to start".dark_gray())
            .centered()
            .render(middle, buf);
    }
    bursts(buf, l, field, now);
    splashes(buf, l, field, surface, now);

    let t = &l.test;
    f.render_widget(ui::heat_bar(t.heat, t.rank, heat.width as usize), heat);
    let esc = if touch() { "✕" } else { "esc" };
    let hint = match (l.aim, narrow || touch()) {
        (Aim::Lock, false) => format!("backspace lets go of a word · tab restart · {esc} menu"),
        (Aim::Lock, true) => format!("backspace let go · {esc} menu"),
        (Aim::Free, false) => format!("space starts over · tab restart · {esc} menu"),
        (Aim::Free, true) => format!("space starts over · {esc} menu"),
    };
    f.render_widget(
        Line::from(format!("  {hint}  ").dark_gray()).centered(),
        help,
    );
}

/// Mode, level, words; score on the right. Lava meter, speed; combo below.
fn hud(f: &mut Frame, l: &Lava, now: Instant, [top, bottom]: [Rect; 2], narrow: bool) {
    let t = &l.test;
    let color = rank_color(t.rank);
    let (r, g, b) = menu::group(t.mode).color;
    let name = match narrow {
        true => format!("LAVA {}", t.mode),
        false => menu::label(t.mode),
    };
    let left = Line::from(vec![
        name.fg(Color::Rgb(r, g, b)).bold(),
        format!("  LV{}", t.level()).yellow().bold(),
        format!("  {} word{}", t.words, if t.words == 1 { "" } else { "s" }).into(),
    ]);
    f.render_widget(left, top);
    let score = Line::from(vec![
        "SCORE ".dark_gray(),
        t.score.to_string().fg(color).bold(),
    ]);
    f.render_widget(score.right_aligned(), top);
    let landed = l.splashes.len().min(LANDINGS);
    let mut meter = vec![
        "LAVA ".dark_gray(),
        Span::styled("▰".repeat(landed), Color::Rgb(255, 70, 20)),
        "▱".repeat(LANDINGS - landed).dark_gray(),
    ];
    if t.started.is_some() {
        meter.push(format!("  {:.0} wpm", t.wpm(now)).into());
        meter.push(format!("  {:.0}%", t.accuracy()).dark_gray());
    }
    f.render_widget(Line::from(meter), bottom);
    let mut combo = vec![
        format!("×{}  ", t.multiplier()).fg(color).bold(),
        format!("{} ", t.combo).into(),
    ];
    if !narrow {
        // on a phone the heat bar's color already says the rank
        combo.push("combo  ".dark_gray());
        combo.push(RANKS[t.rank].name.fg(color).bold());
    }
    f.render_widget(Line::from(combo).right_aligned(), bottom);
}

/// The lava, swelling up for a moment after each landing. Returns its surface row.
fn pool(buf: &mut Buffer, l: &Lava, field: Rect, now: Instant) -> u16 {
    let secs = fx::anim_clock(&l.test, now);
    let swell = l.splashes.last().map_or(1.0, |s| {
        (since(Some(s.0), now).as_secs_f64() / RISING).min(1.0)
    });
    let shown = (l.lava() - RISE * (1.0 - swell)).max(0.0);
    let span = field.height.saturating_sub(1) as f64;
    let surface = field.y + ((1.0 - shown) * span).round() as u16;
    for y in surface..field.bottom() {
        for x in field.left()..field.right() {
            let (sym, v) = fx::lava_cell(x, y, y == surface, y - surface, secs);
            buf[(x, y)].set_symbol(sym).set_fg(fire(v));
        }
    }
    // bubbles popping just above the surface
    if !calm() && surface > field.y {
        for x in field.left()..field.right() {
            if hash(x as u64, 7, (secs * 2.0) as u64) < 5 {
                put(
                    buf,
                    x as i32,
                    surface as i32 - 1,
                    "°",
                    Style::new().fg(fire(0.9)),
                );
            }
        }
    }
    surface
}

/// The falling words: the typed part white-hot, the rest heating up near the lava.
fn words(buf: &mut Buffer, l: &Lava, field: Rect) {
    let page = page_rgb();
    let span = field.height.saturating_sub(1) as f64;
    for d in &l.drops {
        let (x, y) = cell(field, d.x, d.y, d.word.len());
        let n = l.typed_in(d);
        let danger = ((d.y - (1.0 - l.lava() - 0.35)) / 0.35).clamp(0.0, 1.0);
        let heat = mix((215, 215, 215), (255, 90, 40), danger);
        // between rows, a faint echo on the row it's moving into (or out of),
        // strongest halfway: the eye reads the step-by-row fall as smooth
        let fy = d.y * span;
        let frac = fy - fy.floor();
        let echo = 0.45 * (1.0 - (2.0 * frac - 1.0).abs());
        if echo > 0.03 {
            let ey = if frac < 0.5 { y + 1 } else { y - 1 };
            let style = Style::new().fg(lerp(page, heat, echo));
            for (i, ch) in d.word.iter().enumerate() {
                put_behind(buf, x + i as i32, ey, ch.encode_utf8(&mut [0; 4]), style);
            }
        }
        let rest = Style::new().fg(Color::Rgb(heat.0, heat.1, heat.2));
        let locked = n > 0 && l.aim == Aim::Lock;
        for (i, ch) in d.word.iter().enumerate() {
            let mut style = match i < n {
                true => Style::new().fg(fire(0.98)).bold(),
                false => rest,
            };
            if locked {
                style = style.underlined();
            }
            put(buf, x + i as i32, y, ch.encode_utf8(&mut [0; 4]), style);
        }
    }
}

/// Cleared words fly apart letter by letter, their points floating up.
fn bursts(buf: &mut Buffer, l: &Lava, field: Rect, now: Instant) {
    for (k, c) in l.cleared.iter().enumerate().rev() {
        let age = since(Some(c.at), now).as_secs_f64();
        if age > fx::POPUP {
            break;
        }
        let (x0, y0) = cell(field, c.x, c.y, c.word.len());
        if age < BURST {
            let style = Style::new().fg(fire(1.0 - age / BURST)).bold();
            for (i, ch) in c.word.iter().enumerate() {
                let seed = (k * 31 + i) as u64;
                let vx = (hash(seed, 1, 3) as f64 / 255.0 - 0.5) * 24.0;
                let vy = -(2.0 + hash(seed, 2, 3) as f64 / 255.0 * 6.0);
                let x = x0 as f64 + i as f64 + vx * age;
                let y = y0 as f64 + vy * age + 14.0 * age * age;
                put(
                    buf,
                    x.round() as i32,
                    y.round() as i32,
                    ch.encode_utf8(&mut [0; 4]),
                    style,
                );
            }
        }
        let mid = x0 + c.word.len() as i32 / 2;
        fx::popup(buf, mid, y0, c.points, l.test.rank, age / fx::POPUP);
    }
}

/// Landed words spatter up out of the lava.
fn splashes(buf: &mut Buffer, l: &Lava, field: Rect, surface: u16, now: Instant) {
    for (k, &(at, x, len)) in l.splashes.iter().enumerate().rev() {
        let age = since(Some(at), now).as_secs_f64();
        if age > SPLASH {
            break;
        }
        let x0 = cell(field, x, 0.0, len).0 as f64;
        let style = Style::new().fg(fire(1.0 - age / SPLASH));
        for p in 0..10u64 {
            let seed = k as u64 * 17 + p;
            let sx = x0 + (hash(seed, 4, 4) as f64 / 255.0) * len as f64;
            let vx = (hash(seed, 5, 4) as f64 / 255.0 - 0.5) * 10.0;
            let vy = 4.0 + hash(seed, 6, 4) as f64 / 255.0 * 6.0;
            let y = surface as f64 - vy * age + 12.0 * age * age;
            let sym = if p % 3 == 0 { "*" } else { "·" };
            put(
                buf,
                (sx + vx * age).round() as i32,
                y.round() as i32,
                sym,
                style,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn with(aim: Aim, words: &[(&str, f64)]) -> Lava {
        let mut l = Lava::new(aim);
        l.drops = words
            .iter()
            .enumerate()
            .map(|(i, (w, y))| Drop {
                word: w.chars().collect(),
                x: 0.5,
                y: *y,
                id: 100 + i as u64,
            })
            .collect();
        l
    }

    fn typ(l: &mut Lava, s: &str, now: Instant) {
        s.chars().for_each(|c| l.key(c, now));
    }

    #[test]
    fn lock_picks_the_lowest_and_misses_stay_put() {
        let now = Instant::now();
        let mut l = with(Aim::Lock, &[("cat", 0.2), ("cow", 0.6), ("dog", 0.4)]);
        typ(&mut l, "cx", now);
        assert_eq!(
            (l.lock, l.buffer.len()),
            (Some(101), 1),
            "locked on cow, the lowest c-word"
        );
        assert_eq!(l.test.errors, 1, "x missed without moving on");
        typ(&mut l, "ow", now);
        assert_eq!((l.test.words, l.drops.len(), l.lock), (1, 2, None));
        assert!(!l.drops.iter().any(|d| d.word == ['c', 'o', 'w']));
        assert_eq!(
            l.test.target,
            ['c', 'o', 'w'],
            "only typed letters, no streamed text"
        );
        assert_eq!(l.test.typed, l.test.target);
        l.key('d', now);
        l.backspace();
        assert_eq!(l.lock, None, "backspace lets go");
    }

    #[test]
    fn free_clears_on_an_exact_word() {
        let now = Instant::now();
        let mut l = with(Aim::Free, &[("cat", 0.2), ("car", 0.5)]);
        typ(&mut l, "ca", now);
        assert_eq!(l.drops.len(), 2, "still both possible");
        l.key('t', now);
        assert_eq!((l.test.words, l.drops.len()), (1, 1));
        assert!(l.buffer.is_empty());
        l.key('z', now);
        assert_eq!(l.test.errors, 1);
    }

    #[test]
    fn five_landings_and_youre_cooked() {
        let mut now = Instant::now();
        let mut l = with(Aim::Lock, &[]);
        l.key('#', now); // a miss still starts the clock
        l.tick(now);
        for _ in 0..2000 {
            now += Duration::from_millis(100);
            l.tick(now);
            if l.test.finished.is_some() {
                break;
            }
        }
        assert!(l.test.finished.is_some());
        assert_eq!(l.splashes.len(), 5);
        assert!((l.lava() - RISE * LANDINGS as f64).abs() < 1e-9);
    }
}
