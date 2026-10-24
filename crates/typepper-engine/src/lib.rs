//! typepper's rules, the same wherever they run: the game, the browser, and
//! the server that scores a run by playing its inputs again (see
//! docs/online.md). A `Round` is driven by inputs at whole milliseconds and a
//! fixed-step clock, with one seeded `Rng`, so the same `Setup` and inputs
//! always give the same round, at any frame rate.

use std::fmt;
use std::time::Duration;

use web_time::Instant;

pub mod lava;
mod rng;
mod test;

pub use lava::Lava;
pub use rng::Rng;
pub use test::*;

/// The rules' version: bump it when a change would alter results (balance,
/// the word list, how the rng is used, the timestep), not for anything else.
/// A replay only plays back right on the version that recorded it.
pub const VERSION: u32 = 1;

/// The clock ticks 120 times a second, whatever the frame rate.
const STEP: Duration = Duration::from_nanos(1_000_000_000 / 120);

/// How long `simulate` waits after the last input for a round to end itself.
const MAX_IDLE_MS: u32 = 600_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Time(u64),
    Words(usize),
    /// No clock, endless words; esc ends it.
    Zen,
    /// Endless words on a fuel clock that drains faster every level.
    Survival {
        start_level: u32,
    },
    /// Words fall toward rising lava; type them before they land (see `lava`).
    Lava {
        aim: Aim,
        start_level: u32,
    },
    /// Endless words with a cop on your tail that keeps speeding up; a wrong
    /// key stays put until you type the right one.
    Pursuit {
        start_level: u32,
    },
}

/// How FLOOR IS LAVA picks the word you're typing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aim {
    /// The first letter locks onto the lowest word starting with it.
    Lock,
    /// Words clear as soon as what you've typed spells one.
    Free,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Time(30)
    }
}

impl Mode {
    /// The level a leveled mode (scored by words survived) starts from.
    pub fn start_level(self) -> Option<u32> {
        match self {
            Mode::Survival { start_level }
            | Mode::Lava { start_level, .. }
            | Mode::Pursuit { start_level } => Some(start_level),
            _ => None,
        }
    }

    pub fn leveled(self) -> bool {
        self.start_level().is_some()
    }

    /// The same mode from level `level` (the rest have no levels).
    pub fn from_level(self, level: u32) -> Mode {
        match self {
            Mode::Survival { .. } => Mode::Survival { start_level: level },
            Mode::Lava { aim, .. } => Mode::Lava {
                aim,
                start_level: level,
            },
            Mode::Pursuit { .. } => Mode::Pursuit { start_level: level },
            m => m,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Mode::Time(s) => write!(f, "{s}s"),
            Mode::Words(n) => write!(f, "{n}w"),
            Mode::Zen => write!(f, "zen"),
            Mode::Survival { .. } => write!(f, "survival"),
            Mode::Lava { aim: Aim::Lock, .. } => write!(f, "lock-on"),
            Mode::Lava { aim: Aim::Free, .. } => write!(f, "free"),
            Mode::Pursuit { .. } => write!(f, "pursuit"),
        }
    }
}

/// What a round starts from: everything else comes from its inputs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Setup {
    pub mode: Mode,
    pub seed: u64,
}

/// What a player can do to a round. Restarting or leaving abandons it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    Char(char),
    Backspace,
    /// Zen's esc: the only mode the player ends.
    End,
}

pub enum Game {
    Test(Test),
    Lava(Lava),
}

/// One round, start to finish.
pub struct Round {
    pub setup: Setup,
    pub game: Game,
    /// Every input that did something, at its millisecond: the replay.
    pub log: Vec<(u32, Input)>,
    t0: Instant,
    ticks: u32,
    /// The latest millisecond seen; time never runs backwards.
    clock: u32,
}

impl Round {
    pub fn new(setup: Setup) -> Self {
        Self::starting(setup, Instant::now())
    }

    fn starting(setup: Setup, t0: Instant) -> Self {
        let mut rng = Rng::new(setup.seed);
        let game = match setup.mode {
            Mode::Lava { aim, start_level } => Game::Lava(Lava::new(aim, start_level, rng)),
            mode => {
                // only word tests end on their own text; the rest stream more in
                let n = match mode {
                    Mode::Words(n) => n,
                    _ => 100,
                };
                let mut t = Test::new(mode, &random_words(&mut rng, n));
                t.rng = rng;
                Game::Test(t)
            }
        };
        Round {
            setup,
            game,
            log: Vec::new(),
            t0,
            ticks: 0,
            clock: 0,
        }
    }

    /// The typing underneath (lava's too).
    pub fn test(&self) -> &Test {
        match &self.game {
            Game::Test(t) => t,
            Game::Lava(l) => &l.test,
        }
    }

    /// `now` as the round's milliseconds (for the live game and the bot).
    pub fn ms(&self, now: Instant) -> u32 {
        let ms = now.saturating_duration_since(self.t0).as_millis();
        ms.min(u32::MAX as u128) as u32
    }

    fn at(&self, ms: u32) -> Instant {
        self.t0 + Duration::from_millis(ms.into())
    }

    /// Run the clock up to `to_ms`, one fixed step at a time.
    pub fn advance(&mut self, to_ms: u32) {
        self.clock = self.clock.max(to_ms);
        let to = self.at(self.clock);
        while self.test().finished.is_none() {
            let next = self.t0 + STEP * (self.ticks + 1);
            if next > to {
                break;
            }
            self.ticks += 1;
            match &mut self.game {
                Game::Test(t) => t.tick(next),
                Game::Lava(l) => l.tick(next),
            }
        }
    }

    /// An input at `at_ms` (never before the last one): the clock catches up
    /// first, then the input lands, and the log keeps it if it did anything.
    pub fn input(&mut self, at_ms: u32, input: Input) {
        let at_ms = at_ms.max(self.clock);
        self.advance(at_ms);
        if self.test().finished.is_some() {
            return;
        }
        let now = self.at(at_ms);
        match (&mut self.game, input) {
            (Game::Test(t), Input::Char(c)) => t.type_char(c, now),
            (Game::Test(t), Input::Backspace) => t.backspace(now),
            (Game::Test(t), Input::End) if t.mode == Mode::Zen && t.started.is_some() => {
                t.finish(now)
            }
            (Game::Lava(l), Input::Char(c)) => l.key(c, now),
            (Game::Lava(l), Input::Backspace) => l.backspace(),
            _ => return,
        }
        self.log.push((at_ms, input));
    }

    /// How a finished round scored.
    pub fn result(&self) -> Option<Outcome> {
        let t = self.test();
        let end = t.finished?;
        let wpm = t.wpm(end);
        Some(Outcome {
            score: match t.mode {
                Mode::Zen => None,
                m if m.leveled() => Some(t.words as f64),
                _ => Some(wpm.round()),
            },
            wpm,
            accuracy: t.accuracy(),
            words: t.words,
            ms: self.ms(end),
        })
    }

    /// The round these inputs make, played to its end (or given up on after
    /// `MAX_IDLE_MS` without one: zen without its esc, words left untyped).
    fn replayed(setup: Setup, inputs: &[(u32, Input)]) -> Round {
        let mut r = Round::starting(setup, Instant::now());
        for &(ms, input) in inputs {
            r.input(ms, input);
        }
        let give_up = r.clock.saturating_add(MAX_IDLE_MS);
        while r.test().finished.is_none() && r.clock < give_up {
            r.advance(r.clock + 1000);
        }
        r
    }
}

/// A finished round's numbers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Outcome {
    /// What it puts up against a best: words in leveled modes, wpm in the
    /// rest, nothing in zen.
    pub score: Option<f64>,
    pub wpm: f64,
    pub accuracy: f64,
    pub words: u32,
    /// When it ended, in the round's milliseconds.
    pub ms: u32,
}

/// Play a round again from its inputs: how the server scores a run.
pub fn simulate(setup: Setup, inputs: &[(u32, Input)]) -> Option<Outcome> {
    Round::replayed(setup, inputs).result()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player with uneven hands: the right key mostly, a typo now and then
    /// (taken back), at 60-250 ms a key, while the game draws frames at
    /// `frames` (cycled). Zen ends with esc after `zen_keys`.
    fn play(mode: Mode, seed: u64, frames: &[u32]) -> Round {
        let mut r = Round::new(Setup { mode, seed });
        let mut hands = Rng::new(seed ^ 0x5eed);
        let (mut key_ms, mut frame_ms, mut keys) = (300u32, 0u32, 0);
        let mut frame = frames.iter().cycle();
        let mut fix = false;
        while r.test().finished.is_none() && key_ms < 400_000 {
            key_ms += 60 + hands.below(190) as u32;
            while frame_ms < key_ms {
                frame_ms += frame.next().unwrap();
                r.advance(frame_ms.min(key_ms));
            }
            let input = match &r.game {
                _ if fix => Input::Backspace,
                _ if hands.below(30) == 0 => Input::Char('#'),
                Game::Test(_) if mode == Mode::Zen && keys > 120 => Input::End,
                Game::Test(t) => Input::Char(t.target[t.typed.len()]),
                Game::Lava(l) => Input::Char(l.next_char().unwrap_or('x')),
            };
            fix = input == Input::Char('#') && !matches!(mode, Mode::Pursuit { .. });
            r.input(key_ms, input);
            keys += 1;
            // a slow patch: the clock (fuel, cop, falling words) runs on
            if keys == 150 {
                key_ms += 4000;
            }
        }
        r
    }

    #[test]
    fn a_round_plays_back_the_same_at_any_frame_rate() {
        let modes = [
            Mode::Time(15),
            Mode::Words(25),
            Mode::Zen,
            Mode::Survival { start_level: 4 },
            Mode::Pursuit { start_level: 7 },
            Mode::Lava {
                aim: Aim::Lock,
                start_level: 1,
            },
            Mode::Lava {
                aim: Aim::Free,
                start_level: 10,
            },
        ];
        for mode in modes {
            for (seed, frames) in [(1, &[8][..]), (2, &[16, 17]), (3, &[3, 41, 9, 120])] {
                let live = play(mode, seed, frames);
                let again = Round::replayed(live.setup, &live.log);
                let (a, b) = (live.test(), again.test());
                let what = format!("{mode:?} at {frames:?} frames");
                assert!(live.result().is_some(), "{what}: the round ended");
                assert_eq!(live.result(), again.result(), "{what}");
                assert_eq!(
                    (a.score, a.words, a.errors),
                    (b.score, b.words, b.errors),
                    "{what}"
                );
                assert_eq!(
                    (&a.typed, a.heat.to_bits()),
                    (&b.typed, b.heat.to_bits()),
                    "{what}"
                );
                assert_eq!(
                    again.log, live.log,
                    "{what}: the replay logs the same inputs"
                );
            }
        }
    }

    #[test]
    fn the_seed_picks_the_words() {
        let words = |seed| {
            Round::new(Setup {
                mode: Mode::Words(10),
                seed,
            })
            .test()
            .target
            .clone()
        };
        assert_eq!(words(42), words(42));
        assert_ne!(words(42), words(43));
    }

    #[test]
    fn rounds_without_an_end_give_up() {
        let setup = Setup {
            mode: Mode::Zen,
            seed: 1,
        };
        assert_eq!(
            simulate(setup, &[(100, Input::Char('a'))]),
            None,
            "zen needs its esc"
        );
        let setup = Setup {
            mode: Mode::Survival { start_level: 1 },
            seed: 1,
        };
        let out = simulate(setup, &[(100, Input::Char('#'))]).expect("the fuel runs out");
        assert_eq!(out.score, Some(0.0));
    }
}
