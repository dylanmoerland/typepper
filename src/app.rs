//! Platform-neutral app: what a key does, what a frame shows.
//! `main.rs` feeds it from the terminal (crossterm) or the browser (ratzilla).

use std::time::Duration;

use rand::RngExt;
use rand::seq::IndexedRandom;
use ratatui::Frame;
use ratatui::style::Stylize;
use ratatui::text::Line;
use web_time::Instant;

use crate::engine::{MODES, Mode, Test};
use crate::ui;

const WORDS: &str = include_str!("words.txt");

pub enum Key {
    Tab,
    Enter,
    Left,
    Right,
    Backspace,
    Char(char),
}

pub struct App {
    mode: usize,
    test: Test,
    bot: Option<Bot>,
}

impl App {
    pub fn new(demo: bool) -> Self {
        App {
            mode: 1,
            test: new_test(MODES[1]),
            bot: demo.then(Bot::new),
        }
    }

    pub fn key(&mut self, key: Key) {
        let idle = self.test.started.is_none() || self.test.finished.is_some();
        match key {
            Key::Tab => self.restart(0),
            Key::Enter if self.test.finished.is_some() => self.restart(0),
            Key::Left if idle => self.restart(MODES.len() - 1),
            Key::Right if idle => self.restart(1),
            Key::Backspace => self.test.backspace(),
            Key::Char(c) => self.test.type_char(c, Instant::now()),
            _ => {}
        }
    }

    pub fn tick(&mut self, now: Instant) {
        if let Some(bot) = &mut self.bot {
            bot.drive(&mut self.test, now);
            if self
                .test
                .finished
                .is_some_and(|f| now - f > Duration::from_secs(4))
            {
                self.restart(0);
            }
        }
        self.test.tick(now);
    }

    pub fn draw(&self, f: &mut Frame) {
        ui::draw(f, &self.test);
        if self.bot.is_some() {
            f.render_widget(Line::from(" DEMO ").black().on_magenta().bold(), f.area());
        }
    }

    fn restart(&mut self, mode_step: usize) {
        self.mode = (self.mode + mode_step) % MODES.len();
        self.test = new_test(MODES[self.mode]);
        if let Some(bot) = &mut self.bot {
            *bot = Bot::new();
        }
    }
}

fn new_test(mode: Mode) -> Test {
    let words: Vec<&str> = WORDS.split_whitespace().collect();
    // ponytail: 300 words outlasts 60s even at 250wpm; stream words in if longer modes land
    let n = match mode {
        Mode::Time(_) => 300,
        Mode::Words(n) => n,
    };
    let mut rng = rand::rng();
    let picked: Vec<&str> = (0..n).map(|_| *words.choose(&mut rng).unwrap()).collect();
    Test::new(mode, &picked)
}

/// Dev autopilot (`--demo` / `?demo`): types the test itself, ramping
/// 40 → 180 wpm with occasional fixed typos, so every rank and effect shows up.
struct Bot {
    next: Instant,
    fixing: bool,
}

impl Bot {
    fn new() -> Self {
        Bot {
            next: Instant::now() + Duration::from_secs(1),
            fixing: false,
        }
    }

    fn drive(&mut self, t: &mut Test, now: Instant) {
        let mut rng = rand::rng();
        while self.next <= now && t.finished.is_none() {
            let at = self.next;
            if self.fixing {
                t.backspace();
                self.fixing = false;
            } else if rng.random_bool(0.015) {
                t.type_char('#', at);
                self.fixing = true;
            } else if let Some(&c) = t.target.get(t.typed.len()) {
                t.type_char(c, at);
            }
            let wpm = (40.0 + 8.0 * t.elapsed(at).as_secs_f64()).min(180.0);
            self.next = at + Duration::from_secs_f64(12.0 / wpm * rng.random_range(0.7..1.3));
        }
    }
}
