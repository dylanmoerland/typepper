use std::fmt;
use std::time::Duration;

use web_time::Instant;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Time(u64),
    Words(usize),
}

pub const MODES: [Mode; 6] = [
    Mode::Time(15),
    Mode::Time(30),
    Mode::Time(60),
    Mode::Words(10),
    Mode::Words(25),
    Mode::Words(50),
];

impl Default for Mode {
    fn default() -> Self {
        MODES[1]
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Mode::Time(s) => write!(f, "{s}s"),
            Mode::Words(n) => write!(f, "{n}w"),
        }
    }
}

pub struct Rank {
    pub name: &'static str,
    /// Real-world Scoville heat units at the bottom of this rank.
    pub shu: u32,
    /// Heat needed to reach this rank.
    pub at: f64,
    pub callout: &'static str,
}

pub const RANKS: [Rank; 7] = [
    Rank {
        name: "BELL PEPPER",
        shu: 0,
        at: 0.0,
        callout: "",
    },
    Rank {
        name: "POBLANO",
        shu: 1_000,
        at: 8.0,
        callout: "",
    },
    Rank {
        name: "JALAPEÑO",
        shu: 2_500,
        at: 16.0,
        callout: "warming up",
    },
    Rank {
        name: "SERRANO",
        shu: 10_000,
        at: 30.0,
        callout: "NICE!",
    },
    Rank {
        name: "HABANERO",
        shu: 100_000,
        at: 48.0,
        callout: "SPICY!",
    },
    Rank {
        name: "GHOST PEPPER",
        shu: 855_000,
        at: 68.0,
        callout: "ON FIRE!!",
    },
    Rank {
        name: "CAROLINA REAPER",
        shu: 1_641_000,
        at: 88.0,
        callout: "REAPER MODE!!!",
    },
];

/// Heat → Scoville units, linear between rank anchors, 2.2M at max heat.
pub fn scoville(heat: f64) -> u32 {
    let r = RANKS.iter().rposition(|r| heat >= r.at).unwrap_or(0);
    let (next_at, next_shu) = RANKS
        .get(r + 1)
        .map_or((100.0, 2_200_000), |n| (n.at, n.shu));
    let k = ((heat - RANKS[r].at) / (next_at - RANKS[r].at)).clamp(0.0, 1.0);
    RANKS[r].shu + ((next_shu - RANKS[r].shu) as f64 * k) as u32
}

const WORDS: &str = include_str!("words.txt");

/// `n` random common English words.
pub fn random_words(n: usize) -> Vec<&'static str> {
    use rand::seq::IndexedRandom;
    let words: Vec<&str> = WORDS.split_whitespace().collect();
    let mut rng = rand::rng();
    (0..n).map(|_| *words.choose(&mut rng).unwrap()).collect()
}

/// Rank from which time counts as "hot" on the results screen.
pub const HOT: usize = 4;

const RANK_HYSTERESIS: f64 = 4.0;

/// Rules of one test. Defaults are the practice game; sauces and curses
/// (see `mods.rs`) tweak them for roguelike fights.
pub struct Knobs {
    /// Steady state heat = (wpm² / 600) · gain/1.2 / decay, so by default
    /// 40wpm ≈ poblano, 60 ≈ jalapeño, 90 ≈ serrano, 110 ≈ habanero, 135+ ≈ reaper.
    pub heat_gain: f64,
    /// Per second, exponential.
    pub heat_decay: f64,
    /// Heat kept after a mistake.
    pub miss_keep: f64,
    /// Multiplier steps above ×1, one per 20 combo.
    pub mult_steps: u32,
    /// First miss in each word doesn't break combo or cool you down.
    pub forgive_first_miss: bool,
    /// Count a heal every N combo (`Test::heals`).
    pub heal_every: Option<u32>,
    /// Only this many upcoming chars are visible.
    pub fog: Option<usize>,
}

impl Default for Knobs {
    fn default() -> Self {
        Knobs {
            heat_gain: 1.2,
            heat_decay: 0.33,
            miss_keep: 0.4,
            mult_steps: 3,
            forgive_first_miss: false,
            heal_every: None,
            fog: None,
        }
    }
}

/// One typing test. Pure state: every time-dependent call takes `now`.
#[derive(Default)]
pub struct Test {
    pub mode: Mode,
    pub target: Vec<char>,
    pub typed: Vec<char>,
    pub started: Option<Instant>,
    pub finished: Option<Instant>,
    pub keystrokes: u32,
    pub errors: u32,
    pub combo: u32,
    pub best_combo: u32,
    pub last_key: Option<Instant>,
    pub last_error: Option<Instant>,
    /// Combo that was broken (if worth mourning) and when.
    pub broke: Option<(u32, Instant)>,
    /// Cumulative wpm, one sample per elapsed second (plus a final one).
    pub samples: Vec<f64>,
    pub heat: f64,
    pub rank: usize,
    pub peak_rank: usize,
    pub rank_up: Option<Instant>,
    pub score: u64,
    /// Seconds spent at rank `HOT` or above.
    pub hot_secs: f64,
    /// Launch times of flying jalapeños (hot rank-ups and every 50 combo).
    pub jalapenos: Vec<Instant>,
    /// Correct keystrokes: when, and which target index (for caret sparks).
    pub hits: Vec<(Instant, usize)>,
    pub knobs: Knobs,
    /// Heals earned through `Knobs::heal_every`.
    pub heals: u32,
    last_tick: Option<Instant>,
    word_missed: bool,
}

impl Test {
    pub fn new(mode: Mode, words: &[&str]) -> Self {
        Self {
            mode,
            target: words.join(" ").chars().collect(),
            ..Default::default()
        }
    }

    pub fn type_char(&mut self, c: char, now: Instant) {
        if self.finished.is_some() || self.typed.len() >= self.target.len() {
            return;
        }
        self.started.get_or_insert(now);
        self.cool(now);
        self.keystrokes += 1;
        if self.target[self.typed.len()] == c {
            // instant wpm / 60: 12 / gap / 60
            let speed = self.last_key.map_or(1.0, |t| {
                (0.2 / now.saturating_duration_since(t).as_secs_f64().max(0.01)).clamp(0.5, 3.0)
            });
            self.heat = (self.heat + self.knobs.heat_gain * speed).min(100.0);
            self.combo += 1;
            self.best_combo = self.best_combo.max(self.combo);
            self.hits.push((now, self.typed.len()));
            if self.combo.is_multiple_of(50) {
                self.launch(now);
            }
            if self
                .knobs
                .heal_every
                .is_some_and(|n| self.combo.is_multiple_of(n))
            {
                self.heals += 1;
            }
            if c == ' ' {
                self.word_missed = false;
            }
            self.score += 10 * self.multiplier() as u64 * (1 + self.rank as u64);
        } else if self.knobs.forgive_first_miss && !self.word_missed {
            self.word_missed = true;
            self.errors += 1;
            self.last_error = Some(now);
        } else {
            self.word_missed = true;
            if self.combo >= 20 {
                self.broke = Some((self.combo, now));
            }
            self.heat *= self.knobs.miss_keep;
            self.errors += 1;
            self.combo = 0;
            self.last_error = Some(now);
        }
        self.last_key = Some(now);
        self.update_rank(now);
        self.typed.push(c);
        if matches!(self.mode, Mode::Words(_)) && self.typed.len() == self.target.len() {
            self.finish(now);
        }
    }

    pub fn backspace(&mut self) {
        if self.finished.is_none() {
            self.typed.pop();
        }
    }

    /// Advance the clock: cool heat, record per-second samples, end timed tests.
    pub fn tick(&mut self, now: Instant) {
        if self.started.is_none() || self.finished.is_some() {
            return;
        }
        self.cool(now);
        if let Mode::Time(s) = self.mode
            && self.elapsed(now) >= Duration::from_secs(s)
        {
            return self.finish(now);
        }
        while self.samples.len() < self.elapsed(now).as_secs() as usize {
            self.samples.push(self.wpm(now));
        }
    }

    /// Guitar Hero style: ×1 → ×4 by default, one step per 20 combo.
    pub fn multiplier(&self) -> u32 {
        1 + (self.combo / 20).min(self.knobs.mult_steps)
    }

    fn cool(&mut self, now: Instant) {
        let dt = self
            .last_tick
            .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f64());
        self.last_tick = Some(now);
        if self.rank >= HOT {
            self.hot_secs += dt;
        }
        self.heat *= (-self.knobs.heat_decay * dt).exp();
        self.update_rank(now);
    }

    fn update_rank(&mut self, now: Instant) {
        while self.rank + 1 < RANKS.len() && self.heat >= RANKS[self.rank + 1].at {
            self.rank += 1;
            self.rank_up = Some(now);
            if self.rank >= HOT {
                self.launch(now);
            }
        }
        // hysteresis so hovering on a threshold doesn't spam rank-ups
        while self.rank > 0 && self.heat < RANKS[self.rank].at - RANK_HYSTERESIS {
            self.rank -= 1;
        }
        self.peak_rank = self.peak_rank.max(self.rank);
    }

    /// Queue a jalapeño, spaced out so simultaneous triggers don't stack.
    fn launch(&mut self, now: Instant) {
        let at = self
            .jalapenos
            .last()
            .map_or(now, |&l| now.max(l + Duration::from_millis(400)));
        self.jalapenos.push(at);
    }

    fn finish(&mut self, now: Instant) {
        self.finished = Some(now);
        self.samples.push(self.wpm(now));
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        let Some(start) = self.started else {
            return Duration::ZERO;
        };
        let d = self
            .finished
            .unwrap_or(now)
            .saturating_duration_since(start);
        match self.mode {
            Mode::Time(s) => d.min(Duration::from_secs(s)),
            Mode::Words(_) => d,
        }
    }

    pub fn correct(&self) -> usize {
        self.typed
            .iter()
            .zip(&self.target)
            .filter(|(a, b)| a == b)
            .count()
    }

    pub fn wpm(&self, now: Instant) -> f64 {
        self.per_minute(self.correct(), now)
    }

    pub fn raw_wpm(&self, now: Instant) -> f64 {
        self.per_minute(self.typed.len(), now)
    }

    fn per_minute(&self, chars: usize, now: Instant) -> f64 {
        let secs = self.elapsed(now).as_secs_f64();
        // first second is pure noise (1 char in 50ms = 240wpm)
        if secs < 1.0 {
            0.0
        } else {
            chars as f64 / 5.0 / (secs / 60.0)
        }
    }

    pub fn accuracy(&self) -> f64 {
        if self.keystrokes == 0 {
            return 100.0;
        }
        100.0 * (self.keystrokes - self.errors) as f64 / self.keystrokes as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(t: &mut Test, s: &str, now: Instant) {
        s.chars().for_each(|c| t.type_char(c, now));
    }

    /// Types the next `n` target chars, one every `gap_ms`. Returns the clock.
    fn type_steady(t: &mut Test, n: usize, gap_ms: u64, mut now: Instant) -> Instant {
        for _ in 0..n {
            now += Duration::from_millis(gap_ms);
            let c = t.target[t.typed.len()];
            t.type_char(c, now);
        }
        now
    }

    #[test]
    fn words_mode_scores_and_finishes() {
        let t0 = Instant::now();
        let mut t = Test::new(Mode::Words(2), &["hello", "world"]);
        type_str(&mut t, "hellp", t0);
        assert_eq!((t.errors, t.combo, t.best_combo), (1, 0, 4));
        t.backspace();
        type_str(&mut t, "o world", t0 + Duration::from_secs(6));
        let end = t.finished.expect("finished on last char");
        assert_eq!(t.correct(), 11);
        assert_eq!(t.best_combo, 7);
        assert_eq!(t.hits.len(), 11);
        assert_eq!(t.hits.last().map(|h| h.1), Some(10));
        // 11 chars / 5 / 0.1 min = 22 wpm
        assert!((t.wpm(end) - 22.0).abs() < 1e-9);
        assert!((t.accuracy() - 100.0 * 11.0 / 12.0).abs() < 1e-9);
        type_str(&mut t, "x", end);
        assert_eq!(t.typed.len(), 11, "input ignored after finish");
    }

    #[test]
    fn time_mode_samples_and_caps() {
        let t0 = Instant::now();
        let mut t = Test::new(Mode::Time(15), &["a"; 100]);
        t.type_char('a', t0);
        t.tick(t0 + Duration::from_millis(3500));
        assert_eq!(t.samples.len(), 3);
        t.tick(t0 + Duration::from_secs(20));
        assert!(t.finished.is_some());
        assert_eq!(
            t.elapsed(t0 + Duration::from_secs(99)),
            Duration::from_secs(15)
        );
        assert_eq!(t.samples.len(), 4);
    }

    #[test]
    fn knobs_forgive_heal_and_cap() {
        let t0 = Instant::now();
        let mut t = Test::new(Mode::Words(3), &["abc", "def", "ghi"]);
        t.knobs.forgive_first_miss = true;
        t.knobs.heal_every = Some(4);
        t.knobs.mult_steps = 0;
        type_str(&mut t, "abc d", t0);
        t.type_char('x', t0); // first miss in "def": forgiven
        assert_eq!((t.combo, t.errors), (5, 1));
        t.backspace();
        t.type_char('y', t0); // second miss in the same word: counts
        assert_eq!(t.combo, 0);
        t.backspace();
        type_str(&mut t, "ef g", t0);
        t.type_char('x', t0); // new word, forgiven again
        assert_eq!(t.combo, 4);
        assert_eq!(t.heals, 2, "combo 4 twice");
        assert_eq!(t.multiplier(), 1);
    }

    #[test]
    fn scoville_follows_rank_anchors() {
        assert_eq!(scoville(0.0), 0);
        assert_eq!(scoville(RANKS[6].at), 1_641_000);
        assert_eq!(scoville(100.0), 2_200_000);
        let s: Vec<u32> = (0..=100).map(|h| scoville(h as f64)).collect();
        assert!(s.windows(2).all(|w| w[0] <= w[1]), "monotonic");
    }

    #[test]
    fn heat_rewards_speed_and_punishes_misses() {
        let t0 = Instant::now();
        let mut slow = Test::new(Mode::Time(600), &["a"; 500]);
        type_steady(&mut slow, 200, 300, t0); // 40wpm
        assert!(
            slow.rank <= 1,
            "40wpm stays mild, got {}",
            RANKS[slow.rank].name
        );

        let mut fast = Test::new(Mode::Time(600), &["a"; 500]);
        let now = type_steady(&mut fast, 200, 100, t0); // 120wpm
        assert_eq!(RANKS[fast.rank].name, "GHOST PEPPER");
        assert_eq!(fast.multiplier(), 4);
        // habanero + ghost rank-ups, combo 50/100/150/200
        assert_eq!(fast.jalapenos.len(), 6);
        assert_eq!(slow.jalapenos.len(), 4, "slow gets combo jalapeños only");
        // same char count: speed alone earns 2x+ (a timed test also gives 3x the chars)
        assert!(
            fast.score > slow.score * 2,
            "{} vs {}",
            fast.score,
            slow.score
        );

        fast.type_char('x', now + Duration::from_millis(100));
        assert_eq!(fast.broke.map(|(n, _)| n), Some(200));
        assert!(fast.rank <= 3, "a miss cools you down");
        assert_eq!(fast.multiplier(), 1);
    }
}
