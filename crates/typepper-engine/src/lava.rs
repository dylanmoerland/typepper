//! FLOOR IS LAVA: words fall toward a lava pool; type them before they land.
//! Every landed word raises the lava, so the next ones land sooner; five and
//! you're cooked. The typing runs through `Test` (heat, combo, score, the
//! results screen), fed one correct letter at a time.
//!
//! Positions are fractions of the field (x across, y down, 1 = the bottom),
//! so the rules don't depend on the terminal size; the game's `lava::draw`
//! maps them to cells.

use web_time::Instant;

use crate::rng::Rng;
use crate::test::{Test, random_words};
use crate::{Aim, Mode};

// ponytail: tuned on paper, like survival; playtest before trusting them
/// How far one landed word raises the lava, as a fraction of the field.
pub const RISE: f64 = 0.1;
/// Landings until the floor's gone.
pub const LANDINGS: usize = 5;
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
    /// A fresh field; `rng` drops the words (it ends up as `test.rng`).
    pub fn new(aim: Aim, start_level: u32, rng: Rng) -> Self {
        let mut test = Test::new(Mode::Lava { aim, start_level }, &[]);
        test.rng = rng;
        let mut l = Lava {
            test,
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
        libm::pow(FASTER, self.test.level() as f64 - 1.0)
    }

    /// A new word at the top: longer ones as levels go up, and (when it can)
    /// a first letter no other falling word has, so lock-on stays predictable.
    fn spawn(&mut self) {
        let cap = 4 + self.test.level() as usize;
        let words = random_words(&mut self.test.rng, 40);
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
            x: self.test.rng.unit(),
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
    pub fn typed_in(&self, d: &Drop) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn with(aim: Aim, words: &[(&str, f64)]) -> Lava {
        let mut l = Lava::new(aim, 1, Rng::new(1));
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
