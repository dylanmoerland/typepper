//! Personal bests: one number per mode (wpm, or words survived), kept as
//! `label=value` lines in a file on desktop and in localStorage on the web.

use std::collections::BTreeMap;

use web_time::Instant;

use crate::engine::{Mode, Test};
use crate::menu::label;

/// What a finished test scored against your best.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub value: f64,
    pub was: Option<f64>,
}

impl Outcome {
    pub fn is_new(&self) -> bool {
        self.was.is_none_or(|w| self.value > w)
    }
}

#[derive(Default)]
pub struct Bests(BTreeMap<String, f64>);

impl Bests {
    pub fn load() -> Self {
        Self::parse(&storage::read().unwrap_or_default())
    }

    fn parse(text: &str) -> Self {
        Bests(
            text.lines()
                .filter_map(|l| {
                    let (k, v) = l.rsplit_once('=')?;
                    Some((k.to_string(), v.trim().parse().ok()?))
                })
                .collect(),
        )
    }

    fn to_text(&self) -> String {
        self.0.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
    }

    pub fn get(&self, m: Mode) -> Option<f64> {
        self.0.get(&label(m)).copied()
    }

    /// Score a finished test; a new best is kept (and saved). Zen has none.
    pub fn record(&mut self, t: &Test, now: Instant) -> Option<Outcome> {
        let value = match t.mode {
            Mode::Zen => return None,
            Mode::Survival => t.words as f64,
            _ => t.wpm(now).round(),
        };
        let out = Outcome {
            value,
            was: self.get(t.mode),
        };
        if out.is_new() {
            self.0.insert(label(t.mode), value);
            // ponytail: a failed save only loses this best; no error UI for it
            let _ = storage::write(&self.to_text());
        }
        Some(out)
    }
}

/// "87 wpm", "42 words".
pub fn show(m: Mode, v: f64) -> String {
    match m {
        Mode::Survival => format!("{v:.0} words"),
        _ => format!("{v:.0} wpm"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod storage {
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        let var = |k| std::env::var_os(k).map(PathBuf::from);
        let dir = var("XDG_DATA_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".local/share")))
            .or_else(|| var("APPDATA"))?;
        Some(dir.join("typepper").join("bests"))
    }

    pub fn read() -> Option<String> {
        std::fs::read_to_string(path()?).ok()
    }

    pub fn write(text: &str) -> std::io::Result<()> {
        let p = path().ok_or(std::io::ErrorKind::NotFound)?;
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, text)
    }
}

#[cfg(target_arch = "wasm32")]
mod storage {
    const KEY: &str = "typepper.bests";

    fn local() -> Option<ratzilla::web_sys::Storage> {
        ratzilla::web_sys::window()?.local_storage().ok()?
    }

    pub fn read() -> Option<String> {
        local()?.get_item(KEY).ok()?
    }

    pub fn write(text: &str) -> Result<(), ()> {
        local().ok_or(())?.set_item(KEY, text).map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_it_writes_and_skips_junk() {
        let b = Bests::parse("BLITZ 30s=87\njunk\nSURVIVAL=nope\nSPRINT 10w=102.5\n");
        assert_eq!(b.get(Mode::Time(30)), Some(87.0));
        assert_eq!(b.get(Mode::Words(10)), Some(102.5));
        assert_eq!(b.get(Mode::Survival), None);
        assert_eq!(Bests::parse(&b.to_text()).0, b.0);
    }

    #[test]
    fn only_a_higher_number_is_new() {
        let o = |value, was| Outcome { value, was };
        assert!(o(50.0, None).is_new());
        assert!(o(51.0, Some(50.0)).is_new());
        assert!(!o(50.0, Some(50.0)).is_new());
    }
}
