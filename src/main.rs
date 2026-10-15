mod engine;
mod fx;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use engine::{MODES, Mode, Test};
use rand::RngExt;
use rand::seq::IndexedRandom;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::style::Stylize;
use ratatui::text::Line;

const WORDS: &str = include_str!("words.txt");

struct App {
    mode: usize,
    test: Test,
    bot: Option<Bot>,
}

/// Dev autopilot (`--demo`): types the test itself, ramping 40 → 180 wpm
/// with occasional fixed typos, so every rank and effect shows up.
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

fn main() -> io::Result<()> {
    ratatui::run(|terminal| {
        let bot = std::env::args().any(|a| a == "--demo").then(Bot::new);
        let mut app = App {
            mode: 1,
            test: new_test(MODES[1]),
            bot,
        };
        app.run(terminal)
    })
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|f| {
                ui::draw(f, &self.test);
                if self.bot.is_some() {
                    f.render_widget(Line::from(" DEMO ").black().on_magenta().bold(), f.area());
                }
            })?;
            // ~60fps for timers/effects; a keypress wakes poll immediately
            if event::poll(Duration::from_millis(16))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                let idle = self.test.started.is_none() || self.test.finished.is_some();
                match key.code {
                    KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Tab => self.restart(0),
                    KeyCode::Enter if self.test.finished.is_some() => self.restart(0),
                    KeyCode::Left if idle => self.restart(MODES.len() - 1),
                    KeyCode::Right if idle => self.restart(1),
                    KeyCode::Backspace => self.test.backspace(),
                    KeyCode::Char(c) => self.test.type_char(c, Instant::now()),
                    _ => {}
                }
            }
            let now = Instant::now();
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
    }

    fn restart(&mut self, mode_step: usize) {
        self.mode = (self.mode + mode_step) % MODES.len();
        self.test = new_test(MODES[self.mode]);
        if let Some(bot) = &mut self.bot {
            *bot = Bot::new();
        }
    }
}
