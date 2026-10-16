mod app;
mod best;
mod engine;
mod fx;
mod lava;
mod menu;
mod pepper;
mod settings;
mod store;
mod theme;
mod ui;

use app::{App, Key};
use web_time::Instant;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::io::Result<()> {
    use std::time::Duration;

    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    let args: Vec<String> = std::env::args().collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    // ask the terminal for its background before raw mode; unanswered → dark
    let light = flag("--light")
        || !flag("--dark")
            && terminal_colorsaurus::theme_mode(Default::default())
                .is_ok_and(|m| m == terminal_colorsaurus::ThemeMode::Light);
    theme::set_light(light);

    let mut app = App::new(flag("--demo"));
    ratatui::run(|terminal| {
        loop {
            terminal.draw(|f| app.draw(f))?;
            // ~60fps for timers/effects; a keypress wakes poll immediately
            if event::poll(Duration::from_millis(16))?
                && let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press
            {
                let key = match k.code {
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Esc => Some(Key::Esc),
                    KeyCode::Tab => Some(Key::Tab),
                    KeyCode::Enter => Some(Key::Enter),
                    KeyCode::Left => Some(Key::Left),
                    KeyCode::Right => Some(Key::Right),
                    KeyCode::Up => Some(Key::Up),
                    KeyCode::Down => Some(Key::Down),
                    KeyCode::Backspace => Some(Key::Backspace),
                    KeyCode::F(2) => Some(Key::Settings),
                    KeyCode::Char(c) => Some(Key::Char(c)),
                    _ => None,
                };
                if key.is_some_and(|k| app.key(k)) {
                    return Ok(());
                }
            }
            app.tick(Instant::now());
        }
    })
}

/// Browser entry: waits for the chosen web font, then starts the game.
/// `?demo` in the URL turns on the autopilot.
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen_futures::JsFuture;

    let font = settings::FONTS[settings::Settings::load().font];
    wasm_bindgen_futures::spawn_local(async move {
        // the atlas measures cells and rasterizes glyphs as soon as it exists:
        // load the web font first, or the fallback gets baked in for good
        let doc = ratzilla::web_sys::window().and_then(|w| w.document());
        if font != "system"
            && let Some(load) = doc.map(|d| d.fonts().load(&format!("16px \"{font}\"")))
        {
            let _ = JsFuture::from(load).await;
        }
        start(font).expect("typepper failed to start");
    });
}

#[cfg(target_arch = "wasm32")]
fn start(font: &'static str) -> Result<(), Box<dyn std::error::Error>> {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ratzilla::backend::webgl2::WebGl2BackendOptions;
    use ratzilla::event::KeyCode;
    use ratzilla::ratatui::Terminal;
    use ratzilla::{FontAtlasConfig, WebGl2Backend, WebRenderer};

    let window = ratzilla::web_sys::window();
    let query = window
        .as_ref()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    let demo = query.contains("demo");
    let prefers_light = window
        .and_then(|w| {
            w.match_media("(prefers-color-scheme: light)")
                .ok()
                .flatten()
        })
        .is_some_and(|m| m.matches());
    theme::set_light(query.contains("light") || prefers_light && !query.contains("dark"));
    let coarse = ratzilla::web_sys::window()
        .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|m| m.matches());
    settings::set_touch(coarse || query.contains("touch"));
    let app = Rc::new(RefCell::new(App::new(demo)));

    // dynamic atlas rasterizes any glyph (blocks, faces, sparks) from the font stack
    let system = ["Menlo", "Consolas", "DejaVu Sans Mono", "monospace"];
    let stack: Vec<&str> = std::iter::once(font)
        .filter(|&f| f != "system")
        .chain(system)
        .collect();
    let backend = WebGl2Backend::new_with_options(
        WebGl2BackendOptions::new()
            .grid_id("game") // sized by the page, leaving room for the footer
            .font_atlas_config(FontAtlasConfig::dynamic(&stack, 16.0))
            .canvas_padding_color(theme::page()),
    )?;
    let mut terminal = Terminal::new(backend)?;

    terminal.on_key_event({
        let app = app.clone();
        move |k| {
            let key = match k.code {
                KeyCode::Esc => Key::Esc,
                KeyCode::Tab => Key::Tab,
                KeyCode::Enter => Key::Enter,
                KeyCode::Left => Key::Left,
                KeyCode::Right => Key::Right,
                KeyCode::Up => Key::Up,
                KeyCode::Down => Key::Down,
                KeyCode::Backspace => Key::Backspace,
                KeyCode::F(2) => Key::Settings,
                KeyCode::Char(c) if !k.ctrl && !k.alt => Key::Char(c),
                _ => return,
            };
            // nothing to quit to in a browser tab
            let _ = app.borrow_mut().key(key);
        }
    })?;

    // the page around the grid follows the theme (also when settings flip it)
    let mut painted = None;
    terminal.draw_web(move |f| {
        let mut app = app.borrow_mut();
        app.tick(Instant::now());
        app.draw(f);
        let light = theme::is_light();
        if painted != Some(light) {
            painted = Some(light);
            if let Some(root) = ratzilla::web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.document_element())
            {
                let _ = root.set_attribute("data-theme", if light { "light" } else { "dark" });
            }
        }
    });
    Ok(())
}
