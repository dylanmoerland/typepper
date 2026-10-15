//! Roguelike run: a ladder of cursed fights ending in THE REAPER.
//! Your score in a fight is your damage; the foe's attack timer fills on its
//! own and faster with every miss. Win → +1♥ and pick a sauce. 0♥ → cooked.

use std::time::Duration;

use rand::RngExt;
use rand::seq::SliceRandom;
use web_time::Instant;

use crate::engine::{Knobs, Mode, Test, random_words};
use crate::mods::{Curse, SAUCES, Sauce};

pub struct Foe {
    pub name: &'static str,
    pub hp: u64,
    /// Seconds between attacks.
    pub attack: f64,
    pub curse: Curse,
    pub face: [&'static str; 3],
    pub color: (u8, u8, u8),
}

// ponytail: hp/attack tuned by eye with the demo bot; playtest before trusting them
pub const FOES: [Foe; 6] = [
    Foe {
        name: "CAPS GREMLIN",
        hp: 3_000,
        attack: 12.0,
        curse: Curse::Caps,
        face: [" ▲ ▲ ", "(ÒдÓ)", " ╯ ╰ "],
        color: (120, 200, 255),
    },
    Foe {
        name: "NUMBER GNOME",
        hp: 4_500,
        attack: 11.0,
        curse: Curse::Numbers,
        face: [" #_# ", "(°□°)", " ╯ ╰ "],
        color: (220, 210, 90),
    },
    Foe {
        name: "PUNCTUATION IMP",
        hp: 6_000,
        attack: 10.0,
        curse: Curse::Punctuation,
        face: [" ;,; ", "(¬‿¬)", " ╯ ╰ "],
        color: (200, 120, 255),
    },
    Foe {
        name: "MIRROR MITE",
        hp: 7_500,
        attack: 10.0,
        curse: Curse::Mirror,
        face: [" ┌─┐ ", "(ɘ_ɘ)", " ╯ ╰ "],
        color: (170, 230, 240),
    },
    Foe {
        name: "SALT GOBLIN",
        hp: 9_000,
        attack: 9.0,
        curse: Curse::Shuffle,
        face: [" ▄▄▄ ", "(ò_ó)", " ╯ ╰ "],
        color: (235, 235, 235),
    },
    Foe {
        name: "THE REAPER",
        hp: 16_000,
        attack: 7.0,
        curse: Curse::Fog,
        face: [" ▄▀▀▄", "(×_×)", "╱▐█▌╲"],
        color: (255, 0, 200),
    },
];

pub const START_HP: u32 = 5;
/// Attack charge each miss adds (1.0 = a full attack).
const MISS_CHARGE: f64 = 0.15;
/// How long the K.O. / defeat moment holds before moving on.
const OUTRO: Duration = Duration::from_millis(1200);

pub struct Fight {
    pub test: Test,
    pub foe: &'static Foe,
    /// Enemy attack timer, 0..1.
    pub charge: f64,
    pub hit_at: Option<Instant>,
    pub ended: Option<Instant>,
    seen_errors: u32,
    seen_heals: u32,
    last: Option<Instant>,
}

impl Fight {
    pub fn foe_hp(&self) -> u64 {
        self.foe.hp.saturating_sub(self.test.score)
    }
}

#[allow(clippy::large_enum_variant)] // only one Stage is ever alive
pub enum Stage {
    Fight(Fight),
    Reward {
        offers: Vec<Sauce>,
        pick: usize,
        at: Instant,
    },
    Over {
        won: bool,
        at: Instant,
    },
}

pub struct Run {
    pub hp: u32,
    pub max_hp: u32,
    pub floor: usize,
    pub sauces: Vec<Sauce>,
    pub score: u64,
    pub stage: Stage,
}

impl Run {
    pub fn new() -> Self {
        let mut run = Run {
            hp: START_HP,
            max_hp: START_HP,
            floor: 0,
            sauces: Vec::new(),
            score: 0,
            stage: Stage::Over {
                won: false,
                at: Instant::now(),
            },
        };
        run.stage = Stage::Fight(run.fight());
        run
    }

    fn fight(&self) -> Fight {
        let foe = &FOES[self.floor];
        // ponytail: 400 words outlasts any fight; stream words in if fights get longer
        let mut words: Vec<String> = random_words(400).into_iter().map(String::from).collect();
        let mut knobs = Knobs::default();
        self.sauces.iter().for_each(|s| s.tune(&mut knobs));
        foe.curse.apply(&mut words, &mut knobs);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let mut test = Test::new(Mode::Words(words.len()), &words);
        test.knobs = knobs;
        Fight {
            test,
            foe,
            charge: 0.0,
            hit_at: None,
            ended: None,
            seen_errors: 0,
            seen_heals: 0,
            last: None,
        }
    }

    pub fn attack_secs(&self, foe: &Foe) -> f64 {
        foe.attack
            * if self.sauces.contains(&Sauce::Wasabi) {
                1.25
            } else {
                1.0
            }
    }

    pub fn tick(&mut self, now: Instant) {
        let attack = match &self.stage {
            Stage::Fight(f) => self.attack_secs(f.foe),
            _ => return,
        };
        let Stage::Fight(f) = &mut self.stage else {
            return;
        };
        f.test.tick(now);
        if let Some(end) = f.ended {
            if now.saturating_duration_since(end) >= OUTRO {
                self.advance(now);
            }
            return;
        }
        self.hp = (self.hp + f.test.heals - f.seen_heals).min(self.max_hp);
        f.seen_heals = f.test.heals;
        if f.test.started.is_none() {
            return; // the foe waits for your first key
        }
        let dt = f
            .last
            .map_or(0.0, |l| now.saturating_duration_since(l).as_secs_f64());
        f.last = Some(now);
        f.charge += dt / attack + MISS_CHARGE * (f.test.errors - f.seen_errors) as f64;
        f.seen_errors = f.test.errors;
        if f.charge >= 1.0 {
            f.charge = 0.0;
            self.hp = self.hp.saturating_sub(1);
            f.hit_at = Some(now);
        }
        if self.hp == 0 || f.foe_hp() == 0 {
            f.ended = Some(now);
        }
    }

    fn advance(&mut self, now: Instant) {
        if let Stage::Fight(f) = &self.stage {
            self.score += f.test.score;
        }
        self.stage = if self.hp == 0 {
            Stage::Over {
                won: false,
                at: now,
            }
        } else if self.floor + 1 == FOES.len() {
            Stage::Over { won: true, at: now }
        } else {
            self.hp = (self.hp + 1).min(self.max_hp); // breather between floors
            let mut offers: Vec<Sauce> = SAUCES
                .into_iter()
                .filter(|s| !self.sauces.contains(s))
                .collect();
            offers.shuffle(&mut rand::rng());
            offers.truncate(3);
            Stage::Reward {
                offers,
                pick: 0,
                at: now,
            }
        };
    }

    /// Take offer `i` on the reward screen and start the next floor.
    pub fn take(&mut self, i: usize) {
        let Stage::Reward { offers, .. } = &self.stage else {
            return;
        };
        let Some(&sauce) = offers.get(i) else { return };
        self.sauces.push(sauce);
        if sauce == Sauce::Garlic {
            self.max_hp += 2;
            self.hp += 2;
        }
        self.floor += 1;
        self.stage = Stage::Fight(self.fight());
    }

    /// Demo autopilot pick.
    pub fn take_any(&mut self) {
        self.take(rand::rng().random_range(0..3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fight(run: &mut Run) -> &mut Fight {
        match &mut run.stage {
            Stage::Fight(f) => f,
            _ => panic!("not fighting"),
        }
    }

    #[test]
    fn win_pick_then_get_cooked() {
        let mut now = Instant::now();
        let mut run = Run::new();
        let f = fight(&mut run);
        let c = f.test.target[0];
        f.test.type_char(c, now);
        f.test.score = FOES[0].hp; // as if typed hard
        run.tick(now);
        assert!(fight(&mut run).ended.is_some(), "K.O.");
        now += OUTRO;
        run.tick(now);
        let Stage::Reward { offers, .. } = &run.stage else {
            panic!("reward")
        };
        assert_eq!(offers.len(), 3);
        let taken = offers[1];
        run.take(1);
        assert_eq!((run.floor, run.sauces.as_slice()), (1, &[taken][..]));

        // stand still: the foe hits once per attack until you're cooked
        let attack = run.attack_secs(&FOES[1]);
        let c = fight(&mut run).test.target[0];
        fight(&mut run).test.type_char(c, now);
        run.tick(now);
        for _ in 0..run.max_hp {
            now += Duration::from_secs_f64(attack);
            run.tick(now);
        }
        assert_eq!(run.hp, 0);
        now += OUTRO;
        run.tick(now);
        assert!(matches!(run.stage, Stage::Over { won: false, .. }));
    }

    #[test]
    fn misses_charge_the_foe() {
        let now = Instant::now();
        let mut run = Run::new();
        let f = fight(&mut run);
        f.test.type_char('\u{1}', now); // never in the text
        f.test.type_char('\u{1}', now);
        run.tick(now);
        assert!((fight(&mut run).charge - 2.0 * MISS_CHARGE).abs() < 1e-9);
    }
}
