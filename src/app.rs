//! Platform-neutral app: what a key does, what a frame shows.
//! `main.rs` feeds it from the terminal (crossterm) or the browser (ratzilla).

use std::time::Duration;

use rand::RngExt;
use ratatui::Frame;
use ratatui::style::Stylize;
use ratatui::text::Line;
use web_time::Instant;

use crate::engine::{MODES, Mode, Test, random_words};
use crate::run::{Run, Stage};
use crate::ui;

pub enum Key {
    Esc,
    Tab,
    Enter,
    Left,
    Right,
    Backspace,
    Char(char),
}

enum Screen {
    Title { pick: usize },
    Practice { mode: usize, test: Test },
    Run(Run),
}

pub struct App {
    screen: Screen,
    since: Instant,
    bot: Option<Bot>,
}

impl App {
    pub fn new(demo: bool) -> Self {
        App {
            screen: Screen::Title { pick: 1 },
            since: Instant::now(),
            bot: demo.then(Bot::new),
        }
    }

    /// Returns true when the player wants out (Esc on the title screen).
    pub fn key(&mut self, key: Key) -> bool {
        let now = Instant::now();
        let mut next = None;
        match (&mut self.screen, key) {
            (Screen::Title { .. }, Key::Esc) => return true,
            (_, Key::Esc) => next = Some(Screen::Title { pick: 1 }),
            (Screen::Title { pick }, Key::Left | Key::Right) => *pick ^= 1,
            (Screen::Title { .. }, Key::Char('t')) => crate::theme::toggle(),
            (Screen::Title { pick }, Key::Enter | Key::Char(' ')) => {
                next = Some(if *pick == 0 {
                    practice(1)
                } else {
                    Screen::Run(Run::new())
                });
            }
            (Screen::Title { .. }, _) => {}
            (Screen::Practice { mode, test }, key) => {
                let idle = test.started.is_none() || test.finished.is_some();
                match key {
                    Key::Tab => next = Some(practice(*mode)),
                    Key::Enter if test.finished.is_some() => next = Some(practice(*mode)),
                    Key::Left if idle => next = Some(practice(*mode + MODES.len() - 1)),
                    Key::Right if idle => next = Some(practice(*mode + 1)),
                    Key::Backspace => test.backspace(),
                    Key::Char(c) => test.type_char(c, now),
                    _ => {}
                }
            }
            (Screen::Run(run), key) => match (&mut run.stage, key) {
                (Stage::Fight(f), Key::Backspace) if f.ended.is_none() => f.test.backspace(),
                (Stage::Fight(f), Key::Char(c)) if f.ended.is_none() => f.test.type_char(c, now),
                (Stage::Reward { offers, pick, .. }, Key::Left) => {
                    *pick = (*pick + offers.len() - 1) % offers.len();
                }
                (Stage::Reward { offers, pick, .. }, Key::Right) => {
                    *pick = (*pick + 1) % offers.len();
                }
                (Stage::Reward { pick, .. }, Key::Enter) => {
                    let p = *pick;
                    run.take(p);
                }
                (Stage::Reward { .. }, Key::Char(c @ '1'..='3')) => {
                    run.take(c as usize - '1' as usize);
                }
                (Stage::Over { .. }, Key::Enter | Key::Tab) => next = Some(Screen::Run(Run::new())),
                _ => {}
            },
        }
        if let Some(s) = next {
            self.go(s, now);
        }
        false
    }

    pub fn tick(&mut self, now: Instant) {
        let mut next = None;
        let idle = now.saturating_duration_since(self.since);
        match &mut self.screen {
            Screen::Title { .. } => {
                if self.bot.is_some() && idle > Duration::from_secs(1) {
                    next = Some(Screen::Run(Run::new()));
                }
            }
            Screen::Practice { mode, test } => {
                if let Some(bot) = &mut self.bot {
                    bot.drive(test, now);
                    if test
                        .finished
                        .is_some_and(|f| now - f > Duration::from_secs(4))
                    {
                        next = Some(practice(*mode));
                    }
                }
                test.tick(now);
            }
            Screen::Run(run) => {
                if let Some(bot) = &mut self.bot {
                    match &mut run.stage {
                        Stage::Fight(f) if f.ended.is_none() => bot.drive(&mut f.test, now),
                        Stage::Reward { at, .. } if now - *at > Duration::from_millis(1500) => {
                            run.take_any();
                        }
                        Stage::Over { at, .. } if now - *at > Duration::from_secs(5) => {
                            next = Some(Screen::Run(Run::new()));
                        }
                        _ => {}
                    }
                }
                run.tick(now);
            }
        }
        if let Some(s) = next {
            self.go(s, now);
        }
    }

    pub fn draw(&self, f: &mut Frame) {
        let now = Instant::now();
        match &self.screen {
            Screen::Title { pick } => ui::title(f, *pick, (now - self.since).as_secs_f64()),
            Screen::Practice { test, .. } => ui::draw(f, test, None),
            Screen::Run(run) => match &run.stage {
                Stage::Fight(fight) => ui::draw(f, &fight.test, Some((run, fight))),
                Stage::Reward { offers, pick, .. } => ui::reward(f, run, offers, *pick),
                Stage::Over { won, .. } => ui::over(f, run, *won),
            },
        }
        if self.bot.is_some() {
            f.render_widget(Line::from(" DEMO ").black().on_magenta().bold(), f.area());
        }
        crate::theme::apply(f.buffer_mut());
    }

    fn go(&mut self, screen: Screen, now: Instant) {
        self.screen = screen;
        self.since = now;
    }
}

fn practice(mode: usize) -> Screen {
    let mode = mode % MODES.len();
    // ponytail: 300 words outlasts 60s even at 250wpm; stream words in if longer modes land
    let n = match MODES[mode] {
        Mode::Time(_) => 300,
        Mode::Words(n) => n,
    };
    Screen::Practice {
        mode,
        test: Test::new(MODES[mode], &random_words(n)),
    }
}

/// Dev autopilot (`--demo` / `?demo`): types whatever test is on screen,
/// ramping 40 → 180 wpm with occasional fixed typos, so every rank and
/// effect shows up. Also picks sauces and restarts runs.
struct Bot {
    next: Instant,
    fixing: bool,
}

impl Bot {
    fn new() -> Self {
        Bot {
            next: Instant::now(),
            fixing: false,
        }
    }

    fn drive(&mut self, t: &mut Test, now: Instant) {
        // after a menu or a new test, resync instead of bursting the backlog
        if t.started.is_none() && self.next + Duration::from_millis(500) < now {
            self.next = now;
        }
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
