//! Platform-neutral app: what a key does, what a frame shows.
//! `main.rs` feeds it from the terminal (crossterm) or the browser (ratzilla).

use std::time::Duration;

use rand::RngExt;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::style::Stylize;
use ratatui::text::Line;
use web_time::Instant;

use crate::best::{Bests, Outcome};
use crate::engine::{Mode, Test, random_words};
use crate::menu::{self, Dialog, Menu};
use crate::settings::{ROWS, Settings};
use crate::ui;

pub enum Key {
    Esc,
    Tab,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Backspace,
    /// F2, or the web page’s ⚙ button on phones (no s key to press there).
    Settings,
    Char(char),
}

#[allow(clippy::large_enum_variant)] // only one Screen is ever alive
enum Screen {
    Title,
    Practice(Test),
}

pub struct App {
    screen: Screen,
    /// Kept across tests so esc lands back on the mode you played.
    menu: Menu,
    bests: Bests,
    settings: Settings,
    /// The theme the terminal (or browser) asked for, for `auto`.
    detected_light: bool,
    /// The font and theme this page started with: the web renderer takes
    /// both only at startup, so a different pick reloads the page.
    loaded: (usize, bool),
    /// The settings dialog, open over the menu.
    dialog: Option<Dialog>,
    /// The finished test's result against your best (once it's been scored).
    outcome: Option<Outcome>,
    scored: bool,
    since: Instant,
    bot: Option<Bot>,
    /// The last frame drawn, and the one crumbling away since a screen change.
    frame: Option<Buffer>,
    fade: Option<(Buffer, Instant)>,
}

impl App {
    pub fn new(demo: bool) -> Self {
        let now = Instant::now();
        let detected_light = crate::theme::is_light();
        let settings = Settings::load();
        settings.apply(detected_light);
        App {
            screen: Screen::Title,
            menu: Menu::new(now),
            bests: Bests::load(),
            settings,
            detected_light,
            loaded: (settings.font, crate::theme::is_light()),
            dialog: None,
            outcome: None,
            scored: false,
            since: now,
            bot: demo.then(Bot::new),
            frame: None,
            fade: None,
        }
    }

    /// Returns true when the player wants out (Esc on the title screen).
    pub fn key(&mut self, key: Key) -> bool {
        let now = Instant::now();
        let mut next = None;
        // tab mid-test restarts at once; every other screen change dissolves
        let mut instant = false;
        if let (Screen::Title, Some(d)) = (&self.screen, &mut self.dialog) {
            match key {
                Key::Up => d.row = (d.row + ROWS.len() - 1) % ROWS.len(),
                Key::Down => d.row = (d.row + 1) % ROWS.len(),
                Key::Left | Key::Right => {
                    let dir = if matches!(key, Key::Left) { -1 } else { 1 };
                    self.settings.change(ROWS[d.row], dir);
                    self.settings.apply(self.detected_light);
                    self.settings.save();
                    d.changed = Some(now);
                }
                Key::Esc | Key::Enter | Key::Char('s') | Key::Settings => {
                    self.dialog = None;
                    if (self.settings.font, crate::theme::is_light()) != self.loaded {
                        reload();
                    }
                }
                _ => {}
            }
            return false;
        }
        match (&mut self.screen, key) {
            (Screen::Title, Key::Esc) => return true,
            // zen has no clock: esc is how it ends
            (Screen::Practice(test), Key::Esc)
                if test.mode == Mode::Zen && test.started.is_some() && test.finished.is_none() =>
            {
                test.finish(now)
            }
            (_, Key::Esc) => {
                self.menu.back(now);
                next = Some(Screen::Title);
            }
            // enter was pressed: the card is springing, the test is coming
            (Screen::Title, _) if self.menu.pressing() => {}
            (Screen::Title, Key::Char('s') | Key::Settings) => {
                self.dialog = Some(Dialog::new(now));
            }
            (Screen::Title, Key::Left) => self.menu.swipe(-1, now),
            (Screen::Title, Key::Right) => self.menu.swipe(1, now),
            (Screen::Title, Key::Up) => self.menu.length(-1, now),
            (Screen::Title, Key::Down) => self.menu.length(1, now),
            (Screen::Title, Key::Enter | Key::Char(' ')) => self.menu.press(now),
            (Screen::Title, _) => {}
            (Screen::Practice(test), key) => match key {
                Key::Tab => {
                    instant = test.finished.is_none();
                    next = Some(practice(test.mode));
                }
                Key::Enter if test.finished.is_some() => next = Some(practice(test.mode)),
                Key::Backspace => test.backspace(now),
                Key::Char(c) => test.type_char(c, now),
                _ => {}
            },
        }
        if let Some(s) = next {
            self.go(s, now);
            if instant {
                self.fade = None;
            }
        }
        false
    }

    pub fn tick(&mut self, now: Instant) {
        let mut next = None;
        let idle = now.saturating_duration_since(self.since);
        match &mut self.screen {
            Screen::Title if self.menu.press_done(now) => next = Some(practice(self.menu.mode())),
            // demo: show off the carousel for a lap, then play
            Screen::Title if self.bot.is_some() && idle > Duration::from_millis(1200) => {
                if self.menu.lap_done() {
                    self.menu.press(now);
                } else {
                    self.menu.swipe(1, now);
                    self.since = now;
                }
            }
            Screen::Title => {}
            Screen::Practice(test) => {
                if let Some(bot) = &mut self.bot {
                    bot.drive(test, now);
                    if test
                        .finished
                        .is_some_and(|f| now - f > Duration::from_secs(4))
                    {
                        next = Some(practice(test.mode));
                    }
                }
                test.tick(now);
                // finished: dissolve into results, and score it (demo runs don't count)
                if let Some(end) = test.finished
                    && !self.scored
                {
                    self.scored = true;
                    self.fade = self.frame.take().map(|b| (b, now));
                    if self.bot.is_none() {
                        self.outcome = self.bests.record(test, end);
                    }
                }
            }
        }
        if let Some(s) = next {
            self.go(s, now);
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let now = Instant::now();
        match &self.screen {
            Screen::Title => {
                menu::draw(f, &self.menu, &self.bests, now);
                if let Some(d) = &self.dialog {
                    menu::dialog(f, d, &self.settings, now);
                }
            }
            Screen::Practice(test) => ui::draw(f, test, self.outcome),
        }
        if self.bot.is_some() {
            f.render_widget(Line::from(" DEMO ").black().on_magenta().bold(), f.area());
        }
        crate::theme::apply(f.buffer_mut());
        if let Some((old, at)) = &self.fade {
            let k = now.saturating_duration_since(*at).as_secs_f64() / crate::fx::DISSOLVE;
            match k < 1.0 {
                true => crate::fx::dissolve(f.buffer_mut(), old, k),
                false => self.fade = None,
            }
        }
        self.frame = Some(f.buffer_mut().clone());
    }

    fn go(&mut self, screen: Screen, now: Instant) {
        self.screen = screen;
        self.since = now;
        (self.outcome, self.scored) = (None, false);
        self.fade = self.frame.take().map(|b| (b, now));
    }
}

/// A new font (or the canvas edge color for a new theme) only takes after a
/// fresh start on the web (see the web `main`); the terminal needs nothing.
fn reload() {
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = ratzilla::web_sys::window() {
        let _ = w.location().reload();
    }
}

fn practice(mode: Mode) -> Screen {
    // only word tests end on their own text; the rest stream more in as you type
    let n = match mode {
        Mode::Words(n) => n,
        _ => 100,
    };
    Screen::Practice(Test::new(mode, &random_words(n)))
}

/// Dev autopilot (`--demo` / `?demo`): types whatever test is on screen,
/// ramping 40 → 180 wpm with occasional fixed typos, so every rank and
/// effect shows up.
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
                t.backspace(at);
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
