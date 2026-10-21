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
use crate::engine::{LEVELS, Mode, Test, random_words};
use crate::lava::{self, Lava};
use crate::menu::{self, Dialog, Menu, Pick};
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
    Lava(Lava),
}

impl Screen {
    /// The test being played (or just finished) on this screen.
    fn test(&self) -> Option<&Test> {
        match self {
            Screen::Practice(t) => Some(t),
            Screen::Lava(l) => Some(&l.test),
            Screen::Title => None,
        }
    }

    fn test_mut(&mut self) -> Option<&mut Test> {
        match self {
            Screen::Practice(t) => Some(t),
            Screen::Lava(l) => Some(&mut l.test),
            Screen::Title => None,
        }
    }
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
    /// The starting-level picker, over a leveled game before it begins.
    pick: Option<Pick>,
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
    /// `bot`: the demo tour or the autopilot typist, if any (see `Bot`).
    pub fn new(bot: Option<Bot>) -> Self {
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
            pick: None,
            outcome: None,
            scored: false,
            since: now,
            bot,
            frame: None,
            fade: None,
        }
    }

    /// Returns true when the player wants out (Esc on the title screen).
    pub fn key(&mut self, key: Key) -> bool {
        let now = Instant::now();
        let mut next = None;
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
        if let Some(p) = &mut self.pick {
            match key {
                Key::Left | Key::Up => p.step(-1, now),
                Key::Right | Key::Down => p.step(1, now),
                Key::Enter | Key::Char(' ') => self.start_from(),
                Key::Esc => {
                    self.menu.back(now);
                    self.go(Screen::Title, now);
                }
                _ => {}
            }
            return false;
        }
        // in any test: tab restarts (at once mid-test; every other screen change
        // dissolves), enter on the results plays again, from the same level
        let test = self.screen.test();
        if let Some((mode, start, done)) =
            test.map(|t| (t.mode, t.start_level, t.finished.is_some()))
            && (matches!(key, Key::Tab) || matches!(key, Key::Enter) && done)
        {
            let instant = matches!(key, Key::Tab) && !done;
            self.go(practice(mode, start), now);
            if instant {
                self.fade = None;
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
            (Screen::Lava(l), key) => match key {
                Key::Backspace => l.backspace(),
                Key::Char(c) => l.key(c, now),
                _ => {}
            },
            (Screen::Practice(test), key) => match key {
                Key::Backspace => test.backspace(now),
                Key::Char(c) => test.type_char(c, now),
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
        // demo: take the suggested starting level after a moment
        let touring = self.bot.as_ref().is_some_and(|b| b.tour);
        if touring && self.pick.as_ref().is_some_and(|p| p.age(now) > 0.8) {
            self.start_from();
        }
        let idle = now.saturating_duration_since(self.since);
        let picking = self.pick.is_some();
        // leveled modes ask for a starting level before they begin
        let mut ask = None;
        match &mut self.screen {
            Screen::Title if self.menu.press_done(now) => {
                let mode = self.menu.mode();
                next = Some(practice(mode, 1));
                ask = mode.leveled().then_some(mode);
            }
            // demo: show off the carousel for a lap, then play
            Screen::Title if touring && idle > Duration::from_millis(1200) => {
                if self.menu.lap_done() {
                    self.menu.press(now);
                } else {
                    self.menu.swipe(1, now);
                    self.since = now;
                }
            }
            Screen::Title => {}
            Screen::Practice(test) => {
                if let Some(bot) = self.bot.as_mut().filter(|_| !picking) {
                    bot.drive(test, now);
                    if touring
                        && test
                            .finished
                            .is_some_and(|f| now - f > Duration::from_secs(4))
                    {
                        next = Some(practice(test.mode, test.start_level));
                    }
                }
                test.tick(now);
            }
            Screen::Lava(l) => {
                if let Some(bot) = self.bot.as_mut().filter(|_| !picking) {
                    bot.drive_lava(l, now);
                    if touring
                        && l.test
                            .finished
                            .is_some_and(|f| now - f > Duration::from_secs(4))
                    {
                        next = Some(practice(l.test.mode, l.test.start_level));
                    }
                }
                l.tick(now);
            }
        }
        // finished: dissolve into results, and score it (demo runs don't count)
        if let Some(test) = self.screen.test()
            && let Some(end) = test.finished
            && !self.scored
        {
            self.scored = true;
            self.fade = self.frame.take().map(|b| (b, now));
            if self.bot.is_none() {
                self.outcome = self.bests.record(test, end);
            }
        }
        if let Some(s) = next {
            self.go(s, now);
        }
        if let Some(mode) = ask {
            self.pick = Some(Pick::new(self.menu.start_of(mode), now));
        }
    }

    /// Close the level picker and set the game to start from its choice.
    fn start_from(&mut self) {
        let Some(p) = self.pick.take() else { return };
        if let Some(t) = self.screen.test_mut() {
            t.start_level = LEVELS[p.choice];
            self.menu.set_start(t.mode, p.choice);
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
            Screen::Lava(l) if l.test.finished.is_some() => ui::draw(f, &l.test, self.outcome),
            Screen::Lava(l) => lava::draw(f, l, now),
        }
        if let (Some(p), Some(t)) = (&self.pick, self.screen.test()) {
            menu::pick(f, p, t.mode, &self.bests, now);
        }
        if self.bot.as_ref().is_some_and(|b| b.tour) {
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
        // keep the frame for the next dissolve, reusing last frame's memory
        let buf = f.buffer_mut();
        match &mut self.frame {
            Some(old) if old.area == buf.area => old.content.clone_from(&buf.content),
            _ => self.frame = Some(buf.clone()),
        }
    }

    fn go(&mut self, screen: Screen, now: Instant) {
        self.screen = screen;
        self.since = now;
        (self.outcome, self.scored) = (None, false);
        self.pick = None;
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

/// A fresh game of `mode`, starting at level `start` (leveled modes).
fn practice(mode: Mode, start: u32) -> Screen {
    if let Mode::Lava(aim) = mode {
        let mut l = Lava::new(aim);
        l.test.start_level = start;
        return Screen::Lava(l);
    }
    // only word tests end on their own text; the rest stream more in as you type
    let n = match mode {
        Mode::Words(n) => n,
        _ => 100,
    };
    let mut t = Test::new(mode, &random_words(n));
    t.start_level = start;
    Screen::Practice(t)
}

/// The autopilot: types whatever test is on screen with occasional fixed
/// typos, speeding up as it goes so every rank and effect shows up.
/// `--demo` / `?demo` is the full tour (40 → 180 wpm, it also swipes the menu,
/// picks levels and restarts); `--autopilot [wpm]` only types, at `wpm`,
/// leaving the menus to you, or to a recording script (see `demo/`).
pub struct Bot {
    next: Instant,
    fixing: bool,
    /// Typing speed at the start of each test.
    wpm: f64,
    /// Also drive the menus (the self-running demo).
    tour: bool,
    /// Chance of a (fixed) typo per key.
    typos: f64,
}

impl Bot {
    pub fn tour() -> Self {
        Bot {
            typos: 0.015,
            ..Bot::typist(40.0).touring()
        }
    }

    pub fn typist(wpm: f64) -> Self {
        Bot {
            next: Instant::now(),
            fixing: false,
            wpm,
            tour: false,
            // rare enough that a recording's run climbs to reaper, but the
            // odd combo shatter still shows
            typos: 0.004,
        }
    }

    fn touring(self) -> Self {
        Bot { tour: true, ..self }
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
            } else if rng.random_bool(self.typos) {
                t.type_char('#', at);
                // pursuit: a miss stays put, so there's nothing to take back
                self.fixing = t.mode != Mode::Pursuit;
            } else if let Some(&c) = t.target.get(t.typed.len()) {
                t.type_char(c, at);
            }
            self.next = self.after(t, at);
        }
    }

    /// FLOOR IS LAVA: finishes the word it's on, or starts the lowest one.
    fn drive_lava(&mut self, l: &mut Lava, now: Instant) {
        if l.test.started.is_none() && self.next + Duration::from_millis(500) < now {
            self.next = now;
        }
        while self.next <= now && l.test.finished.is_none() {
            let at = self.next;
            if let Some(c) = l.next_char() {
                l.key(c, at);
            }
            self.next = self.after(&l.test, at);
        }
    }

    /// When the next key falls, jittered: the tour speeds up from `wpm` (+8 a
    /// second, to 140 more, at most 200) so every rank shows; the typist keeps
    /// to `wpm`, so a recording gets the pace it asked for.
    fn after(&self, t: &Test, at: Instant) -> Instant {
        let ramp =
            (self.wpm + 8.0 * t.elapsed(at).as_secs_f64()).min((self.wpm + 140.0).min(200.0));
        let wpm = if self.tour { ramp } else { self.wpm };
        at + Duration::from_secs_f64(12.0 / wpm * rand::rng().random_range(0.7..1.3))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bot that fumbles its first key, then types on clean for one more.
    fn fumble(mode: Mode) -> Test {
        let t0 = Instant::now();
        let mut t = Test::new(mode, &["ab"; 10]);
        let mut bot = Bot {
            next: t0,
            typos: 1.0,
            ..Bot::typist(60.0)
        };
        bot.drive(&mut t, t0);
        bot.typos = 0.0;
        bot.drive(&mut t, bot.next);
        t
    }

    #[test]
    fn the_bot_fixes_its_typos_except_in_pursuit() {
        let t = fumble(Mode::Words(10));
        assert_eq!((t.typed.len(), t.erased.len()), (0, 1), "took the # back");
        let t = fumble(Mode::Pursuit);
        assert_eq!(t.typed, ['a'], "the miss stayed put, then it went on");
        assert_eq!((t.errors, t.erased.len()), (1, 0));
    }
}
