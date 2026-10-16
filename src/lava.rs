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
use crate::fx::{
    self, RANK_RGB, WHITE, fire, hash, lerp, mix, page_rgb, put, put_behind, rank_color, since,
};
use crate::settings::{calm, touch};
use crate::{menu, ui};

// ponytail: tuned on paper, like survival; playtest before trusting them
/// How far one landed word raises the lava, as a fraction of the field.
const RISE: f64 = 0.1;
/// Lava this high and the floor's gone (five landings).
const MAX_LAVA: f64 = 0.5;
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
    /// Lock-on: the word being typed (by id) and how many letters are in.
    pub lock: Option<(u64, usize)>,
    /// Free: what's typed so far.
    pub buffer: Vec<char>,
    /// Lava height, as a fraction of the field.
    pub lava: f64,
    pub cleared: Vec<Clear>,
    /// Landed words: when, where, how long (for the splash).
    pub splashes: Vec<(Instant, f64, usize)>,
    next_id: u64,
    /// Game time: runs from the first key, so the first word waits for you.
    clock: f64,
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
            lava: 0.0,
            cleared: Vec::new(),
            splashes: Vec::new(),
            next_id: 0,
            clock: 0.0,
            next_spawn: SPAWN,
            last: None,
            score_from: 0,
        };
        l.spawn();
        l.drops[0].y = 0.1; // the first word hangs where you can see it
        l
    }

    fn level(&self) -> u32 {
        self.test.level()
    }

    fn pace(&self) -> f64 {
        FASTER.powi(self.level() as i32 - 1)
    }

    /// A new word at the top: longer ones as levels go up, and (when it can)
    /// a first letter no other falling word has, so lock-on stays predictable.
    fn spawn(&mut self) {
        let cap = 4 + self.level() as usize;
        let mut rng = rand::rng();
        let mut word: Vec<char> = Vec::new();
        for _ in 0..40 {
            let w: Vec<char> = random_words(1)[0].chars().collect();
            let fresh = !self.drops.iter().any(|d| d.word[0] == w[0]);
            if w.len() <= cap && (fresh || word.is_empty()) {
                word = w;
                if fresh {
                    break;
                }
            }
        }
        if word.is_empty() {
            word = random_words(1)[0].chars().collect();
        }
        self.next_id += 1;
        self.drops.push(Drop {
            word,
            x: rng.random_range(0.0..1.0),
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
            self.buffer.clear();
            return;
        }
        match self.aim {
            Aim::Lock => {
                let lowest = || {
                    self.drops
                        .iter()
                        .filter(|d| d.word[0] == c)
                        .max_by(|a, b| a.y.total_cmp(&b.y))
                        .map(|d| (d.id, 0))
                };
                let target = self.lock.or_else(lowest);
                let at = target.and_then(|(id, n)| {
                    let i = self.drops.iter().position(|d| d.id == id)?;
                    (self.drops[i].word[n] == c).then_some((i, n))
                });
                match at {
                    Some((i, n)) => {
                        if n == 0 {
                            self.score_from = self.test.score;
                        }
                        self.hit(c, now);
                        self.lock = Some((self.drops[i].id, n + 1));
                        if n + 1 == self.drops[i].word.len() {
                            self.clear(i, now);
                        }
                    }
                    None => self.test.miss(now),
                }
            }
            Aim::Free => {
                let mut next = self.buffer.clone();
                next.push(c);
                if !self.drops.iter().any(|d| d.word.starts_with(&next)) {
                    return self.test.miss(now);
                }
                if self.buffer.is_empty() {
                    self.score_from = self.test.score;
                }
                self.hit(c, now);
                self.buffer = next;
                let done = self
                    .drops
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.word == self.buffer)
                    .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
                    .map(|(i, _)| i);
                if let Some(i) = done {
                    self.clear(i, now);
                }
            }
        }
    }

    /// Lock-on: let go of the word (to switch). Free: take back a letter.
    pub fn backspace(&mut self) {
        match self.aim {
            Aim::Lock => self.lock = None,
            Aim::Free => {
                self.buffer.pop();
            }
        }
    }

    /// A correct letter: it joins the test's text and is typed there.
    fn hit(&mut self, c: char, now: Instant) {
        self.test.target.push(c);
        self.test.type_char(c, now);
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
        self.clock += dt;
        let speed = 1.0 / (FALL * self.pace());
        for d in &mut self.drops {
            d.y += speed * dt;
        }
        // whatever reaches the lava burns, and the lava rises
        let surface = 1.0 - self.lava;
        let mut i = 0;
        while i < self.drops.len() {
            if self.drops[i].y < surface {
                i += 1;
                continue;
            }
            let d = self.drops.remove(i);
            if self.lock.is_some_and(|(id, _)| id == d.id) {
                self.lock = None;
            }
            self.splashes.push((now, d.x, d.word.len()));
            self.lava += RISE;
            self.test.burn(now);
        }
        if !self.drops.iter().any(|d| d.word.starts_with(&self.buffer)) {
            self.buffer.clear();
        }
        if self.lava >= MAX_LAVA - 1e-9 {
            return self.test.finish(now);
        }
        while self.clock >= self.next_spawn {
            self.spawn();
            self.next_spawn += (SPAWN * self.pace()).max(0.6);
        }
    }

    /// Letters already typed into this word (lock-on: the locked word; free:
    /// every word the buffer spells the start of).
    fn typed_in(&self, d: &Drop) -> usize {
        match (self.aim, self.lock) {
            (Aim::Lock, Some((id, n))) if id == d.id => n,
            (Aim::Free, _) if d.word.starts_with(&self.buffer) => self.buffer.len(),
            _ => 0,
        }
    }
}

const BURST: f64 = 0.6; // a cleared word flying apart
const POPUP: f64 = 0.8; // its +points
const SPLASH: f64 = 0.7; // a landed word's spatter
const RISING: f64 = 0.4; // the lava swelling up after a landing

pub fn draw(f: &mut Frame, l: &Lava, now: Instant) {
    let t = &l.test;
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
    fx::speed_lines(f.buffer_mut(), t, now);
    let secs = fx::anim_clock(t, now);
    let color = rank_color(t.rank);

    // HUD: mode, level, words left; score right; lava meter, speed, combo below.
    // Narrow screens (phones) get a compact HUD so the two sides don't collide.
    let narrow = col.width < 64;
    let name = match narrow {
        true => format!("LAVA {}", t.mode),
        false => menu::label(t.mode),
    };
    let left = Line::from(vec![
        name.fg(Color::Rgb(255, 110, 20)).bold(),
        format!("  LV{}", l.level()).yellow().bold(),
        format!("  {} word{}", t.words, if t.words == 1 { "" } else { "s" }).into(),
    ]);
    f.render_widget(left, hud1);
    let score = Line::from(vec![
        "SCORE ".dark_gray(),
        t.score.to_string().fg(color).bold(),
    ]);
    f.render_widget(score.right_aligned(), hud1);
    let landed = (l.lava / RISE).round() as usize;
    let room = (MAX_LAVA / RISE).round() as usize;
    let mut meter = vec![
        "LAVA ".dark_gray(),
        Span::styled("▰".repeat(landed), Color::Rgb(255, 70, 20)),
        "▱".repeat(room - landed.min(room)).dark_gray(),
    ];
    if t.started.is_some() {
        meter.push(format!("  {:.0} wpm", t.wpm(now)).into());
        meter.push(format!("  {:.0}%", t.accuracy()).dark_gray());
    }
    f.render_widget(Line::from(meter), hud2);
    let mut combo = vec![
        format!("×{}  ", t.multiplier()).fg(color).bold(),
        format!("{} ", t.combo).into(),
    ];
    if !narrow {
        // the heat bar's color already says the rank on a phone
        combo.push("combo  ".dark_gray());
        combo.push(RANKS[t.rank].name.fg(color).bold());
    }
    let combo = Line::from(combo);
    f.render_widget(combo.right_aligned(), hud2);

    // the lava: it swells up for a moment after each landing
    let swell = l.splashes.last().map_or(1.0, |s| {
        (since(Some(s.0), now).as_secs_f64() / RISING).min(1.0)
    });
    let shown = (l.lava - RISE * (1.0 - swell)).max(0.0);
    let span = field.height.saturating_sub(1) as f64;
    let surface = field.y + ((1.0 - shown) * span).round() as u16;
    let buf = f.buffer_mut();
    let flicker = if calm() { 0 } else { (secs * 6.0) as u64 };
    for y in surface..field.bottom() {
        for x in field.left()..field.right() {
            let n = hash(x as u64, y as u64, flicker) as f64 / 255.0;
            let (sym, v) = match y == surface {
                true => {
                    let wave = (x as f64 * 0.5 + if calm() { 0.0 } else { secs * 3.0 }).sin();
                    (if wave > 0.3 { "~" } else { "≈" }, 0.85 + 0.15 * n)
                }
                false => {
                    let deep = (y - surface).min(4) as f64;
                    (
                        ["▓", "▒", "▓", "█"][(n * 3.99) as usize],
                        0.75 - 0.08 * deep + 0.15 * n,
                    )
                }
            };
            buf[(x, y)].set_symbol(sym).set_fg(fire(v));
        }
        // bubbles popping just above the surface
        if y == surface && !calm() && surface > field.y {
            for x in field.left()..field.right() {
                if hash(x as u64, 7, (secs * 2.0) as u64) < 5 {
                    put(buf, x as i32, y as i32 - 1, "°", Style::new().fg(fire(0.9)));
                }
            }
        }
    }

    // the falling words: the typed part white-hot, the rest heating up near the lava
    let page = page_rgb();
    let at = |d_x: f64, d_y: f64, len: usize| {
        let x = field.x as f64 + (field.width.saturating_sub(len as u16)) as f64 * d_x;
        (
            x.round() as i32,
            field.y as i32 + (d_y * span).round() as i32,
        )
    };
    for d in &l.drops {
        let (x, y) = at(d.x, d.y, d.word.len());
        let n = l.typed_in(d);
        let danger = ((d.y - (1.0 - l.lava - 0.35)) / 0.35).clamp(0.0, 1.0);
        let heat = mix((215, 215, 215), (255, 90, 40), danger);
        let rest = Color::Rgb(heat.0, heat.1, heat.2);
        let locked = n > 0 && l.aim == Aim::Lock;
        // between rows, a faint echo on the row it's moving into (or out of),
        // strongest halfway: the eye reads the step-by-row fall as smooth
        let fy = d.y * span;
        let frac = fy - fy.floor();
        let echo = 0.45 * (1.0 - (2.0 * frac - 1.0).abs());
        if echo > 0.03 {
            let ey = if frac < 0.5 { y + 1 } else { y - 1 };
            let style = Style::new().fg(lerp(page, heat, echo));
            for (i, ch) in d.word.iter().enumerate() {
                put_behind(buf, x + i as i32, ey, &ch.to_string(), style);
            }
        }
        for (i, ch) in d.word.iter().enumerate() {
            let mut style = match i < n {
                true => Style::new().fg(fire(0.98)).bold(),
                false => Style::new().fg(rest),
            };
            if locked {
                style = style.underlined();
            }
            put(buf, x + i as i32, y, &ch.to_string(), style);
        }
    }
    if t.started.is_none() {
        let hint = "type a falling word to start";
        let x = field.x as i32 + (field.width as i32 - hint.len() as i32) / 2;
        let y = field.y as i32 + field.height as i32 / 2;
        for (i, ch) in hint.chars().enumerate() {
            put(
                buf,
                x + i as i32,
                y,
                &ch.to_string(),
                Style::new().fg(Color::DarkGray),
            );
        }
    }

    // cleared words fly apart letter by letter, their points floating up
    let popup = mix(RANK_RGB[t.rank.max(2)], WHITE, 0.4);
    for (k, c) in l.cleared.iter().enumerate().rev() {
        let age = since(Some(c.at), now).as_secs_f64();
        if age > POPUP {
            break;
        }
        let (x0, y0) = at(c.x, c.y, c.word.len());
        if age < BURST {
            for (i, ch) in c.word.iter().enumerate() {
                let seed = (k * 31 + i) as u64;
                let vx = (hash(seed, 1, 3) as f64 / 255.0 - 0.5) * 24.0;
                let vy = -(2.0 + hash(seed, 2, 3) as f64 / 255.0 * 6.0);
                let x = x0 as f64 + i as f64 + vx * age;
                let y = y0 as f64 + vy * age + 14.0 * age * age;
                let s = Style::new().fg(fire(1.0 - age / BURST)).bold();
                put(buf, x.round() as i32, y.round() as i32, &ch.to_string(), s);
            }
        }
        let text = format!("+{}", c.points);
        let k = age / POPUP;
        let s = Style::new().fg(lerp(popup, page, k * k)).bold();
        let x = x0 + c.word.len() as i32 / 2 - text.len() as i32 / 2;
        for (i, ch) in text.chars().enumerate() {
            put(
                buf,
                x + i as i32,
                y0 - 1 - (k * 2.5) as i32,
                &ch.to_string(),
                s,
            );
        }
    }

    // landed words spatter up out of the lava
    for (k, &(at_, x, len)) in l.splashes.iter().enumerate().rev() {
        let age = since(Some(at_), now).as_secs_f64();
        if age > SPLASH {
            break;
        }
        let x0 = field.x as f64 + (field.width.saturating_sub(len as u16)) as f64 * x;
        for p in 0..10u64 {
            let seed = k as u64 * 17 + p;
            let sx = x0 + (hash(seed, 4, 4) as f64 / 255.0) * len as f64;
            let vx = (hash(seed, 5, 4) as f64 / 255.0 - 0.5) * 10.0;
            let vy = 4.0 + hash(seed, 6, 4) as f64 / 255.0 * 6.0;
            let y = surface as f64 - vy * age + 12.0 * age * age;
            let sym = if p % 3 == 0 { "*" } else { "·" };
            let s = Style::new().fg(fire(1.0 - age / SPLASH));
            put(
                buf,
                (sx + vx * age).round() as i32,
                y.round() as i32,
                sym,
                s,
            );
        }
    }

    f.render_widget(ui::heat_bar(t.heat, t.rank, heat.width as usize), heat);
    let hint = match (touch(), l.aim, narrow) {
        (true, ..) => "✕ menu",
        (false, Aim::Lock, false) => "backspace lets go of a word · tab restart · esc menu",
        (false, Aim::Lock, true) => "backspace let go · esc menu",
        (false, Aim::Free, false) => "space starts over · tab restart · esc menu",
        (false, Aim::Free, true) => "space starts over · esc menu",
    };
    f.render_widget(
        Line::from(format!("  {hint}  ").dark_gray()).centered(),
        help,
    );
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
        assert_eq!(l.lock, Some((101, 1)), "locked on cow, the lowest c-word");
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
        assert!(l.lava >= MAX_LAVA - 1e-9);
    }
}
