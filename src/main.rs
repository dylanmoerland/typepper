mod app;
mod engine;
mod fx;
mod ui;

use app::{App, Key};
use web_time::Instant;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::io::Result<()> {
    use std::time::Duration;

    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    let mut app = App::new(std::env::args().any(|a| a == "--demo"));
    ratatui::run(|terminal| {
        loop {
            terminal.draw(|f| app.draw(f))?;
            // ~60fps for timers/effects; a keypress wakes poll immediately
            if event::poll(Duration::from_millis(16))?
                && let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press
            {
                let key = match k.code {
                    KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Tab => Some(Key::Tab),
                    KeyCode::Enter => Some(Key::Enter),
                    KeyCode::Left => Some(Key::Left),
                    KeyCode::Right => Some(Key::Right),
                    KeyCode::Backspace => Some(Key::Backspace),
                    KeyCode::Char(c) => Some(Key::Char(c)),
                    _ => None,
                };
                key.into_iter().for_each(|k| app.key(k));
            }
            app.tick(Instant::now());
        }
    })
}

/// Browser entry: WebGL2 grid, ticked and drawn every animation frame.
/// `?demo` in the URL turns on the autopilot.
#[cfg(target_arch = "wasm32")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ratzilla::backend::webgl2::WebGl2BackendOptions;
    use ratzilla::event::KeyCode;
    use ratzilla::ratatui::Terminal;
    use ratzilla::{FontAtlasConfig, WebGl2Backend, WebRenderer};

    let demo = ratzilla::web_sys::window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|q| q.contains("demo"));
    let app = Rc::new(RefCell::new(App::new(demo)));

    // dynamic atlas rasterizes any glyph (blocks, faces, sparks) from the font stack
    let font = FontAtlasConfig::dynamic(
        &["Menlo", "Consolas", "DejaVu Sans Mono", "monospace"],
        16.0,
    );
    let backend =
        WebGl2Backend::new_with_options(WebGl2BackendOptions::new().font_atlas_config(font))?;
    let mut terminal = Terminal::new(backend)?;

    terminal.on_key_event({
        let app = app.clone();
        move |k| {
            let key = match k.code {
                KeyCode::Tab => Key::Tab,
                KeyCode::Enter => Key::Enter,
                KeyCode::Left => Key::Left,
                KeyCode::Right => Key::Right,
                KeyCode::Backspace => Key::Backspace,
                KeyCode::Char(c) if !k.ctrl && !k.alt => Key::Char(c),
                _ => return,
            };
            app.borrow_mut().key(key);
        }
    })?;

    terminal.draw_web(move |f| {
        let mut app = app.borrow_mut();
        app.tick(Instant::now());
        app.draw(f);
    });
    Ok(())
}
