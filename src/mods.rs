//! Built-in mods. Sauces are run perks you collect; curses are what each
//! enemy does to your text. Both only touch `Knobs` or the word list, so a
//! new mod is one variant plus one match arm.

use rand::RngExt;
use rand::seq::{IndexedRandom, SliceRandom};
use ratatui::style::Color;

use crate::engine::Knobs;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sauce {
    Sriracha,
    Chipotle,
    Tabasco,
    SourCream,
    Lime,
    Honey,
    Garlic,
    Wasabi,
}

pub const SAUCES: [Sauce; 8] = [
    Sauce::Sriracha,
    Sauce::Chipotle,
    Sauce::Tabasco,
    Sauce::SourCream,
    Sauce::Lime,
    Sauce::Honey,
    Sauce::Garlic,
    Sauce::Wasabi,
];

impl Sauce {
    pub fn name(self) -> &'static str {
        match self {
            Sauce::Sriracha => "Sriracha",
            Sauce::Chipotle => "Smoked Chipotle",
            Sauce::Tabasco => "Tabasco",
            Sauce::SourCream => "Sour Cream",
            Sauce::Lime => "Lime",
            Sauce::Honey => "Hot Honey",
            Sauce::Garlic => "Garlic Confit",
            Sauce::Wasabi => "Wasabi",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Sauce::Sriracha => "Combo multiplier goes up to ×6.",
            Sauce::Chipotle => "Heat cools 30% slower.",
            Sauce::Tabasco => "Heat builds 25% faster.",
            Sauce::SourCream => "A miss keeps 70% of your heat.",
            Sauce::Lime => "First miss in each word is forgiven.",
            Sauce::Honey => "Heal 1♥ every 50 combo.",
            Sauce::Garlic => "+2 max ♥, healed now.",
            Sauce::Wasabi => "Enemies attack 25% slower.",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Sauce::Sriracha => Color::Rgb(230, 40, 30),
            Sauce::Chipotle => Color::Rgb(160, 70, 40),
            Sauce::Tabasco => Color::Rgb(255, 90, 40),
            Sauce::SourCream => Color::Rgb(240, 240, 225),
            Sauce::Lime => Color::Rgb(150, 230, 60),
            Sauce::Honey => Color::Rgb(255, 190, 40),
            Sauce::Garlic => Color::Rgb(235, 220, 180),
            Sauce::Wasabi => Color::Rgb(120, 200, 90),
        }
    }

    /// Rule tweaks applied at the start of every fight.
    /// Garlic and Wasabi act on the run instead (see `run.rs`).
    pub fn tune(self, k: &mut Knobs) {
        match self {
            Sauce::Sriracha => k.mult_steps = 5,
            Sauce::Chipotle => k.heat_decay *= 0.7,
            Sauce::Tabasco => k.heat_gain *= 1.25,
            Sauce::SourCream => k.miss_keep = 0.7,
            Sauce::Lime => k.forgive_first_miss = true,
            Sauce::Honey => k.heal_every = Some(50),
            Sauce::Garlic | Sauce::Wasabi => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Curse {
    Caps,
    Numbers,
    Punctuation,
    Mirror,
    Shuffle,
    Fog,
}

impl Curse {
    pub fn name(self) -> &'static str {
        match self {
            Curse::Caps => "mIxEd CaSe",
            Curse::Numbers => "numbers sneak in",
            Curse::Punctuation => "punctuation, everywhere!",
            Curse::Mirror => "words come mirrored",
            Curse::Shuffle => "letters get shuffled",
            Curse::Fog => "you can barely see ahead",
        }
    }

    /// Curse the text (and, for fog, the rules) of a fight.
    pub fn apply(self, words: &mut [String], k: &mut Knobs) {
        let mut rng = rand::rng();
        for w in words.iter_mut() {
            match self {
                Curse::Caps if rng.random_bool(0.4) => {
                    *w = if rng.random_bool(0.5) {
                        w.to_uppercase()
                    } else {
                        capitalize(w)
                    };
                }
                Curse::Numbers if rng.random_bool(0.25) => {
                    *w = rng.random_range(1..10_000).to_string();
                }
                Curse::Punctuation if rng.random_bool(0.4) => {
                    w.push(*[',', '.', ';', '!', '?', ':'].choose(&mut rng).unwrap());
                }
                Curse::Mirror => *w = w.chars().rev().collect(),
                // typoglycemia: first and last letters stay put, so it stays readable
                Curse::Shuffle if w.chars().count() > 3 => {
                    let mut c: Vec<char> = w.chars().collect();
                    let n = c.len();
                    c[1..n - 1].shuffle(&mut rng);
                    *w = c.into_iter().collect();
                }
                _ => {}
            }
        }
        if self == Curse::Fog {
            k.fog = Some(8);
        }
    }
}

fn capitalize(w: &str) -> String {
    let mut c = w.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursed(curse: Curse, words: &[&str]) -> (Vec<String>, Knobs) {
        let mut w: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        let mut k = Knobs::default();
        curse.apply(&mut w, &mut k);
        (w, k)
    }

    #[test]
    fn curses_keep_words_typeable() {
        let words = ["pepper", "sauce", "hot", "reaper"];
        assert_eq!(
            cursed(Curse::Mirror, &words).0,
            ["reppep", "ecuas", "toh", "repaer"]
        );

        for (w, orig) in cursed(Curse::Shuffle, &words).0.iter().zip(words) {
            let (mut a, mut b): (Vec<char>, Vec<char>) =
                (w.chars().collect(), orig.chars().collect());
            assert_eq!((a[0], a[a.len() - 1]), (b[0], b[b.len() - 1]));
            a.sort();
            b.sort();
            assert_eq!(a, b, "same letters");
        }
        for w in cursed(Curse::Caps, &words).0 {
            assert_eq!(w.to_lowercase().len(), w.len());
        }
        assert_eq!(cursed(Curse::Fog, &words).1.fog, Some(8));
    }

    #[test]
    fn sauces_tune_knobs() {
        let mut k = Knobs::default();
        SAUCES.iter().for_each(|s| s.tune(&mut k));
        assert_eq!(k.mult_steps, 5);
        assert!(k.forgive_first_miss);
        assert_eq!(k.heal_every, Some(50));
        assert!((k.heat_decay - 0.33 * 0.7).abs() < 1e-9);
    }
}
