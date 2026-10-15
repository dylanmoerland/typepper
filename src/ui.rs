use std::time::Duration;

use web_time::Instant;

use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::{
    Axis, Block, BorderType, Chart, Clear, Dataset, GraphType, Padding, Paragraph,
};

use crate::engine::{HOT, MODES, Mode, RANKS, Test, scoville};
use crate::fx::{self, RANK_RGB, WHITE, anim_clock, hash, hsv, lerp, rank_color, since};
use crate::mods::Sauce;
use crate::run::{FOES, Fight, Run};

const FLASH: Duration = Duration::from_millis(120);
const SHAKE: Duration = Duration::from_millis(250);
const CALLOUT: Duration = Duration::from_millis(1200);
/// Esc can't close a browser tab, so the web title screen doesn't offer it.
const QUIT: &str = if cfg!(target_arch = "wasm32") {
    ""
} else {
    " · esc quit"
};
/// How long the panel reels after a foe lands a hit.
const OUCH: Duration = Duration::from_millis(400);

const DIGITS: [[&str; 5]; 10] = [
    ["███", "█ █", "█ █", "█ █", "███"],
    [" █ ", "██ ", " █ ", " █ ", "███"],
    ["███", "  █", "███", "█  ", "███"],
    ["███", "  █", "███", "  █", "███"],
    ["█ █", "█ █", "███", "  █", "  █"],
    ["███", "█  ", "███", "  █", "███"],
    ["███", "█  ", "███", "█ █", "███"],
    ["███", "  █", "  █", "  █", "  █"],
    ["███", "█ █", "███", "█ █", "███"],
    ["███", "█ █", "███", "  █", "███"],
];

fn column(area: Rect) -> Rect {
    let [_, col, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Max(86),
        Constraint::Fill(1),
    ])
    .areas(area);
    col
}

/// Typing screen (practice test or roguelike fight) or practice results.
pub fn draw(f: &mut Frame, t: &Test, fight: Option<(&Run, &Fight)>) {
    let now = Instant::now();
    let col = column(f.area());
    if t.finished.is_some() {
        results(f, col, t, now);
    } else {
        fx::speed_lines(f.buffer_mut(), t, now);
        fx::flames(f.buffer_mut(), t, now);
        let (panel, help) = typing(f, col, t, now, fight);
        fx::crowd(f.buffer_mut(), t, now, panel);
        fx::tumblers(f.buffer_mut(), t, now, panel, help);
    }
}

/// Returns the HUD panel and help line rects so overlays can avoid them.
fn typing(
    f: &mut Frame,
    area: Rect,
    t: &Test,
    now: Instant,
    fight: Option<(&Run, &Fight)>,
) -> (Rect, Rect) {
    let [_, panel, _, help] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(18),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let clock = anim_clock(t, now);
    let color = rank_color(t.rank);
    let ouch = fight.is_some_and(|(_, fi)| since(fi.hit_at, now) < OUCH);
    // a landed hit knocks the whole panel around (if there's room to move)
    let panel = match ouch {
        true if panel.x > area.x && panel.right() < area.right() => Rect {
            x: if ((clock * 30.0) as u64).is_multiple_of(2) {
                panel.x - 1
            } else {
                panel.x + 1
            },
            ..panel
        },
        _ => panel,
    };

    // flames frame the panel instead of burning through it
    f.render_widget(Clear, panel);
    let panel_block = Block::bordered()
        .border_type(if ouch {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(if ouch {
            Style::new().red().bold()
        } else {
            Style::new().fg(lerp((0, 0, 0), RANK_RGB[t.rank], fx::PANEL_DIM))
        })
        .padding(Padding::new(2, 2, 1, 1));
    let inner = panel_block.inner(panel);
    f.render_widget(panel_block, panel);
    fx::rank_up_sweep(f.buffer_mut(), panel, t, now);

    let [header, _, hud, _, text, heat] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .areas(inner);

    if let Some((run, _)) = fight {
        let mut spans: Vec<Span> = (0..run.max_hp)
            .map(|i| {
                if i < run.hp {
                    Span::styled("♥ ", if ouch { Color::White } else { Color::Red })
                } else {
                    "♡ ".dark_gray()
                }
            })
            .collect();
        spans.push(format!(" FLOOR {}/{}", run.floor + 1, FOES.len()).dark_gray());
        if t.started.is_some() {
            spans.push(format!("  {:.0} wpm", t.wpm(now)).into());
            spans.push(format!("  {:.0}%", t.accuracy()).dark_gray());
        }
        f.render_widget(Line::from(spans), header);
    } else if t.started.is_none() {
        let modes: Vec<Span> = MODES
            .iter()
            .flat_map(|m| {
                let s = format!(" {m} ");
                let span = if *m == t.mode {
                    s.black().on_yellow().bold()
                } else {
                    s.dark_gray()
                };
                [span, " ".into()]
            })
            .collect();
        f.render_widget(Line::from(modes), header);
    } else {
        let progress = match t.mode {
            Mode::Time(s) => format!("{}", s.saturating_sub(t.elapsed(now).as_secs())),
            Mode::Words(n) => format!("{}/{n}", t.typed.iter().filter(|c| **c == ' ').count()),
        };
        let stats = Line::from(vec![
            progress.yellow().bold(),
            format!("  {:.0} wpm", t.wpm(now)).into(),
            format!("  {:.0}%", t.accuracy()).dark_gray(),
        ]);
        f.render_widget(stats, header);
    }
    let score = Line::from(vec![
        "SCORE ".dark_gray(),
        t.score.to_string().fg(color).bold(),
    ]);
    f.render_widget(score.right_aligned(), header);

    // HUD: fighting-game combo counter left, pepper rank right
    let [combo_area, foe_area, rank_area] = match fight {
        Some(_) => Layout::horizontal([
            Constraint::Length(24),
            Constraint::Fill(1),
            Constraint::Length(18),
        ])
        .areas(hud),
        None => {
            let [a, b] = Layout::horizontal([Constraint::Fill(1); 2]).areas(hud);
            [a, Rect::default(), b]
        }
    };
    if let Some((_, fi)) = fight {
        f.render_widget(Paragraph::new(foe_lines(fi, now)), foe_area);
    }
    let pop = t.combo > 0
        && t.combo.is_multiple_of(25)
        && since(t.last_key, now) < Duration::from_millis(200);
    let combo_color = match t.combo {
        0 => Color::DarkGray,
        _ if pop => Color::White,
        _ => color,
    };
    let mut lines: Vec<Line> = big(t.combo)
        .iter()
        .map(|r| Line::from(solid(r, combo_color)))
        .collect();
    lines[1].push_span("  HITS".dark_gray());
    lines[3].push_span(format!("  ×{}", t.multiplier()).fg(color).bold());
    f.render_widget(Paragraph::new(lines), combo_area);

    let mut rank_lines = vec![
        Line::from(""),
        Line::from(RANKS[t.rank].name).fg(color).bold(),
    ];
    if ouch {
        rank_lines.push(Line::from("OUCH! -1♥").red().bold());
    } else if let Some((n, at)) = t.broke
        && since(Some(at), now) < CALLOUT
    {
        rank_lines.push(Line::from(format!("COMBO BROKEN ×{n}")).red().bold());
    } else if since(t.rank_up, now) < CALLOUT && !RANKS[t.rank].callout.is_empty() {
        let p = since(t.rank_up, now).as_secs_f64() / CALLOUT.as_secs_f64();
        let c = if p < 0.15 {
            Color::White
        } else if p < 0.75 {
            color
        } else {
            Color::DarkGray
        };
        rank_lines.push(Line::from(RANKS[t.rank].callout).fg(c).bold());
    }
    let rank_lines: Vec<Line> = rank_lines.into_iter().map(Line::right_aligned).collect();
    f.render_widget(Paragraph::new(rank_lines), rank_area);

    // text box: shakes on a broken combo, border heats up with rank
    let shake = t.broke.is_some_and(|(_, at)| since(Some(at), now) < SHAKE);
    // centered with 1 spare column each side; shaking jitters it left/right
    let dx = match shake {
        true if ((clock * 30.0) as u64).is_multiple_of(2) => 0,
        true => 2,
        false => 1,
    };
    let text = Rect {
        x: text.x + dx,
        width: text.width.saturating_sub(2),
        ..text
    };
    let flashing = since(t.last_error, now) < FLASH;
    let border = match t.rank {
        _ if flashing => Color::Red,
        0..=2 => Color::DarkGray,
        3 => color,
        4 => lerp((90, 50, 0), RANK_RGB[4], 0.5 + 0.5 * (clock * 5.0).sin()),
        5 => hsv(20.0 + 15.0 * (clock * 8.0).sin(), 1.0, 1.0),
        _ => hsv(clock * 120.0, 0.9, 1.0),
    };
    let block = Block::bordered()
        .border_type(if t.rank >= 5 {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(border)
        .padding(Padding::horizontal(1));
    let text_inner = block.inner(text);
    f.render_widget(block, text);
    let (wrapped, first) = visible(t, text_inner.width as usize);
    let shown = &wrapped[first..(first + text_inner.height as usize).min(wrapped.len())];
    let lines: Vec<Line> = shown
        .iter()
        .map(|&(start, end)| {
            (start..end)
                .map(|i| match t.knobs.fog {
                    // fog curse: only the next few letters show; word shapes stay
                    Some(n) if i > t.typed.len() + n && t.target[i] != ' ' => {
                        Span::styled("░", Color::Rgb(45, 45, 55))
                    }
                    _ => Span::styled(t.target[i].to_string(), letter(t, i, clock)),
                })
                .collect()
        })
        .collect();
    f.render_widget(Paragraph::new(lines), text_inner);
    fx::sparks(f.buffer_mut(), t, now, |i| {
        let row = shown.iter().position(|&(s, e)| (s..e).contains(&i))?;
        Some((
            text_inner.x + (i - shown[row].0) as u16,
            text_inner.y + row as u16,
        ))
    });

    f.render_widget(heat_bar(t.heat, t.rank, heat.width as usize), heat);

    let hint = match fight {
        Some(_) => "esc abandon run",
        None if t.started.is_none() => "←/→ mode · tab restart · esc menu",
        None => "tab restart · esc menu",
    };
    // style the span, not the line: a styled Line recolors the flames across the whole row
    f.render_widget(Line::from(hint.dark_gray()).centered(), help);
    (panel, help)
}

/// Heat gauge colored by the rank each segment belongs to, plus a Scoville readout.
fn heat_bar(heat: f64, rank: usize, width: usize) -> Line<'static> {
    let w = width.saturating_sub(16).max(1);
    let mut spans: Vec<Span> = (0..w)
        .map(|j| {
            let v = (j as f64 + 0.5) / w as f64 * 100.0;
            if v <= heat {
                let r = RANKS.iter().rposition(|r| v >= r.at).unwrap_or(0);
                Span::styled("█", rank_color(r))
            } else {
                "░".dark_gray()
            }
        })
        .collect();
    let shu = scoville(heat).to_string();
    let shu = shu
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join(",");
    spans.push(format!(" {shu:>11}").fg(rank_color(rank)).bold());
    spans.push(" SHU".dark_gray());
    Line::from(spans)
}

/// Word-wrapped target and the first visible line (keeps the caret on line two).
fn visible(t: &Test, width: usize) -> (Vec<(usize, usize)>, usize) {
    let lines = wrap(&t.target, width.max(1));
    let caret = t.typed.len();
    let caret_line = lines
        .iter()
        .position(|&(_, end)| caret < end)
        .unwrap_or(lines.len() - 1);
    (lines, caret_line.saturating_sub(1))
}

fn letter(t: &Test, i: usize, clock: f64) -> Style {
    let c = t.target[i];
    let caret = t.typed.len();
    match t.typed.get(i) {
        Some(&typed) if typed != c => {
            return if c == ' ' {
                Style::new().on_red()
            } else {
                Style::new().red().underlined()
            };
        }
        Some(_) => {}
        None if i == caret => return Style::new().black().bg(rank_color(t.rank.max(3))),
        // at reaper the word you're about to type glows
        None if t.rank == 6 && !t.target[caret..i].contains(&' ') => return Style::new().gray(),
        None => return Style::new().dark_gray(),
    }
    let age = caret - 1 - i; // 0 = just typed
    let rgb = RANK_RGB[t.rank];
    let bold = |s: Style, on: bool| if on { s.bold() } else { s };
    match t.rank {
        0 | 1 => Style::new().white(),
        2 => bold(
            Style::new().fg(if age == 0 {
                Color::White
            } else {
                rank_color(2)
            }),
            age == 0,
        ),
        3 | 4 if age < 8 => bold(Style::new().fg(lerp(WHITE, rgb, age as f64 / 8.0)), age < 2),
        3 | 4 => Style::new().fg(lerp((0, 0, 0), rgb, 0.7)),
        5 if age < 3 => Style::new().fg(Color::Rgb(255, 244, 192)).bold(),
        5 => Style::new().fg(hsv(
            25.0 + 25.0 * (i as f64 * 0.35 - clock * 6.0).sin(),
            1.0,
            1.0,
        )),
        _ if hash(i as u64, 0, (clock * 12.0) as u64) < 8 => Style::new().white().bold(),
        _ => bold(
            Style::new().fg(hsv(i as f64 * 12.0 - clock * 240.0, 1.0, 1.0)),
            age < 3,
        ),
    }
}

/// Line ranges `[start, end)` breaking after spaces, each at most `width` chars.
fn wrap(text: &[char], width: usize) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let (mut start, mut last_space) = (0, None);
    for (i, &c) in text.iter().enumerate() {
        if i - start >= width {
            let brk = last_space.filter(|&s| s >= start).map_or(i, |s| s + 1);
            lines.push((start, brk));
            start = brk;
        }
        if c == ' ' {
            last_space = Some(i);
        }
    }
    lines.push((start, text.len()));
    lines
}

fn results(f: &mut Frame, area: Rect, t: &Test, now: Instant) {
    let [
        _,
        label,
        _,
        score,
        _,
        peak,
        _,
        cards,
        _,
        chart,
        _,
        details,
        _,
        help,
    ] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1), // SCORE
        Constraint::Length(1),
        Constraint::Length(5), // digits
        Constraint::Length(1),
        Constraint::Length(1), // peak
        Constraint::Length(1),
        Constraint::Length(3), // cards
        Constraint::Length(1),
        Constraint::Length(10), // chart
        Constraint::Length(1),
        Constraint::Length(1), // details
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let color = rank_color(t.peak_rank);

    f.render_widget(Line::from("SCORE").dark_gray().centered(), label);
    let digits: Vec<Line> = big(t.score)
        .iter()
        .map(|r| Line::from(solid(r, color)))
        .collect();
    f.render_widget(Paragraph::new(digits).centered(), score);
    let peak_line = Line::from(vec![
        "peak ".dark_gray(),
        RANKS[t.peak_rank].name.fg(color).bold(),
        format!(
            " · {:.1}s at {}+",
            t.hot_secs,
            RANKS[HOT].name.to_lowercase()
        )
        .dark_gray(),
    ]);
    f.render_widget(peak_line.centered(), peak);

    let card_areas: [Rect; 4] = Layout::horizontal([Constraint::Fill(1); 4])
        .spacing(1)
        .areas(cards);
    let stats = [
        ("wpm", format!("{:.0}", t.wpm(now)), Color::Yellow),
        ("acc", format!("{:.0}%", t.accuracy()), Color::Green),
        ("raw", format!("{:.0}", t.raw_wpm(now)), Color::Cyan),
        ("best combo", format!("×{}", t.best_combo), Color::Magenta),
    ];
    for ((label, value, c), a) in stats.into_iter().zip(card_areas) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(label)
            .dark_gray();
        f.render_widget(
            Paragraph::new(value.fg(c).bold()).centered().block(block),
            a,
        );
    }

    let secs = t.elapsed(now).as_secs_f64();
    let pts: Vec<(f64, f64)> = t
        .samples
        .iter()
        .enumerate()
        .map(|(i, w)| ((i as f64 + 1.0).min(secs), *w))
        .collect();
    let max_x = secs.max(2.0);
    let max_y = (t.samples.iter().cloned().fold(0.0, f64::max) * 1.2).max(10.0);
    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(color)
        .data(&pts);
    let chart_widget = Chart::new(vec![dataset])
        .x_axis(
            Axis::default()
                .bounds([1.0, max_x])
                .labels(["1s".to_string(), format!("{max_x:.1}s")])
                .dark_gray(),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, max_y])
                .labels(["0".to_string(), format!("{max_y:.0}")])
                .dark_gray(),
        );
    f.render_widget(chart_widget, chart);

    let line = format!(
        "{} · {:.1}s · {} correct · {} wrong · {} slips",
        t.mode,
        secs,
        t.correct(),
        t.typed.len() - t.correct(),
        t.errors
    );
    f.render_widget(Line::from(line).dark_gray().centered(), details);
    f.render_widget(
        Line::from("tab/enter again · ←/→ mode · esc menu")
            .dark_gray()
            .centered(),
        help,
    );
}

/// 5-row block glyphs: digits plus the letters of the logo.
fn glyph(c: char) -> Option<[&'static str; 5]> {
    Some(match c {
        '0'..='9' => DIGITS[c as usize - '0' as usize],
        'T' => ["███", " █ ", " █ ", " █ ", " █ "],
        'Y' => ["█ █", "█ █", "▀█▀", " █ ", " █ "],
        'P' => ["██▄", "█ █", "██▀", "█  ", "█  "],
        'E' => ["███", "█  ", "██ ", "█  ", "███"],
        'R' => ["██▄", "█ █", "██▀", "█ █", "█ █"],
        _ => return None,
    })
}

/// One row of a block glyph as cells. Full blocks also paint their
/// background: fonts rarely fill the whole cell with `█`, which leaves seams
/// between stacked rows (very visible on the web grid).
fn solid(row: &str, color: Color) -> Vec<Span<'static>> {
    row.chars()
        .map(|c| match c {
            '█' => Span::styled("█", Style::new().fg(color).bg(color)),
            c => Span::styled(c.to_string(), Style::new().fg(color).bold()),
        })
        .collect()
}

/// A number in 5-row block digits.
fn big(n: impl ToString) -> Vec<String> {
    let s = n.to_string();
    (0..5)
        .map(|row| {
            let parts: Vec<&str> = s.chars().filter_map(glyph).map(|g| g[row]).collect();
            parts.join(" ")
        })
        .collect()
}

/// Foe portrait, health and attack timer for the fight HUD.
fn foe_lines(fi: &Fight, now: Instant) -> Vec<Line<'static>> {
    let foe = fi.foe;
    let (r, g, b) = foe.color;
    let dead = fi.foe_hp() == 0;
    let struck = fi
        .test
        .hits
        .last()
        .is_some_and(|h| since(Some(h.0), now) < Duration::from_millis(80));
    let face_style = match () {
        _ if dead => Style::new().dark_gray(),
        _ if since(fi.hit_at, now) < OUCH => Style::new().red().bold(),
        _ if struck => Style::new().white().bold(),
        _ => Style::new().fg(Color::Rgb(r, g, b)).bold(),
    };
    let face = if dead {
        ["     ", "(x_x)", "     "]
    } else {
        foe.face
    };
    let hp_w = 20;
    let filled = (fi.foe_hp() as f64 / foe.hp as f64 * hp_w as f64).ceil() as usize;
    let charge = fi.charge.clamp(0.0, 1.0);
    let ticks = (charge * 10.0) as usize;
    let atk = lerp((255, 215, 0), (255, 30, 30), charge);
    vec![
        Line::from(vec![
            Span::styled(face[0], face_style),
            "  ".into(),
            Span::styled(if dead { "DEFEATED" } else { foe.name }, face_style),
        ]),
        Line::from(vec![
            Span::styled(face[1], face_style),
            "  ".into(),
            Span::styled("█".repeat(filled), Color::Rgb(r, g, b)),
            "░".repeat(hp_w - filled).dark_gray(),
            format!(" {}", fi.foe_hp()).dark_gray(),
        ]),
        Line::from(vec![
            Span::styled(face[2], face_style),
            "  ".into(),
            "ATK ".dark_gray(),
            Span::styled("▰".repeat(ticks), atk),
            "▱".repeat(10 - ticks).dark_gray(),
        ]),
        Line::from(format!("       {}", foe.curse.name()).dark_gray().italic()),
        if dead {
            Line::from("       K.O.!!").magenta().bold()
        } else {
            Line::default()
        },
    ]
}

/// Title: animated logo, Practice / Run.
pub fn title(f: &mut Frame, pick: usize, secs: f64) {
    let col = column(f.area());
    let [_, logo, _, tag, _, menu, _, about, _, help] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(col);
    // heat ripples through the letters, bell pepper → reaper
    let rows: Vec<Line> = (0..5)
        .map(|row| {
            "TYPEPPER"
                .chars()
                .enumerate()
                .flat_map(|(i, c)| {
                    let k = (secs * 2.0 - i as f64 * 0.4).sin() * 0.5 + 0.5;
                    let rank = 2 + (k * 4.99) as usize;
                    let mut cells = solid(glyph(c).unwrap()[row], rank_color(rank));
                    cells.push(" ".into());
                    cells
                })
                .collect()
        })
        .collect();
    f.render_widget(Paragraph::new(rows).centered(), logo);
    f.render_widget(
        Line::from("a spicy little typing game".dark_gray()).centered(),
        tag,
    );
    let item = |label: &str, on: bool| {
        let s = format!("  {label}  ");
        if on {
            s.black().on_yellow().bold()
        } else {
            s.dark_gray()
        }
    };
    let items = Line::from(vec![
        item("PRACTICE", pick == 0),
        "    ".into(),
        item("RUN", pick == 1),
    ]);
    f.render_widget(items.centered(), menu);
    let blurb = if pick == 0 {
        "timed tests from 15s to 50 words · chase your heat"
    } else {
        "6 cursed floors · collect sauces · face THE REAPER"
    };
    f.render_widget(Line::from(blurb.gray()).centered(), about);
    let theme = if crate::theme::is_light() {
        "light"
    } else {
        "dark"
    };
    let hint = format!("←/→ choose · enter start · t theme: {theme}{QUIT}");
    f.render_widget(Line::from(hint.dark_gray()).centered(), help);
}

fn hearts(run: &Run) -> Line<'static> {
    let mut spans: Vec<Span> = (0..run.max_hp)
        .map(|i| {
            if i < run.hp {
                "♥ ".red()
            } else {
                "♡ ".dark_gray()
            }
        })
        .collect();
    spans.push(format!("  SCORE {}", run.score).dark_gray());
    Line::from(spans)
}

fn sauce_list(run: &Run) -> Line<'static> {
    let mut spans = vec!["sauces: ".dark_gray()];
    if run.sauces.is_empty() {
        spans.push("none yet".dark_gray());
    }
    for (i, s) in run.sauces.iter().enumerate() {
        if i > 0 {
            spans.push(" · ".dark_gray());
        }
        spans.push(Span::styled(s.name(), s.color()));
    }
    Line::from(spans)
}

/// Floor cleared: pick one of three sauces.
pub fn reward(f: &mut Frame, run: &Run, offers: &[Sauce], pick: usize) {
    let col = column(f.area());
    let [_, title, life, _, cards, _, owned, _, help] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(7),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(col);
    let foe = &FOES[run.floor];
    let (r, g, b) = foe.color;
    let head = Line::from(vec![
        Span::styled(foe.name, Color::Rgb(r, g, b)).bold(),
        " DEFEATED · ".bold(),
        "pick a sauce".yellow().bold(),
    ]);
    f.render_widget(head.centered(), title);
    f.render_widget(hearts(run).centered(), life);
    let areas: Vec<Rect> = Layout::horizontal(vec![Constraint::Fill(1); offers.len()])
        .spacing(2)
        .split(cards)
        .to_vec();
    for (i, (sauce, area)) in offers.iter().zip(areas).enumerate() {
        let on = i == pick;
        let block = Block::bordered()
            .border_type(if on {
                BorderType::Thick
            } else {
                BorderType::Rounded
            })
            .border_style(if on {
                Style::new().fg(sauce.color())
            } else {
                Style::new().dark_gray()
            })
            .title(format!(" {} ", i + 1))
            .padding(Padding::horizontal(1));
        let body = vec![
            Line::from(Span::styled(sauce.name(), sauce.color()).bold()),
            Line::default(),
            Line::from(sauce.blurb()),
        ];
        let p = Paragraph::new(body)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .block(block);
        f.render_widget(if on { p } else { p.dark_gray() }, area);
    }
    f.render_widget(sauce_list(run).centered(), owned);
    f.render_widget(
        Line::from("←/→ choose · enter take · 1-3 quick pick · esc abandon".dark_gray()).centered(),
        help,
    );
}

/// Run over: cooked, or the reaper falls.
pub fn over(f: &mut Frame, run: &Run, won: bool) {
    let col = column(f.area());
    let [_, title, _, stats, owned, _, help] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(col);
    let head = if won {
        "T H E   R E A P E R   H A S   F A L L E N"
            .fg(rank_color(6))
            .bold()
    } else {
        "C O O K E D".red().bold()
    };
    f.render_widget(Line::from(head).centered(), title);
    let line = format!(
        "floor {}/{} · score {}",
        run.floor + 1,
        FOES.len(),
        run.score
    );
    f.render_widget(Line::from(line.gray()).centered(), stats);
    f.render_widget(sauce_list(run).centered(), owned);
    f.render_widget(
        Line::from("enter/tab new run · esc menu".dark_gray()).centered(),
        help,
    );
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wraps_after_spaces() {
        let text: Vec<char> = "aa bb cc dddddddd".chars().collect();
        assert_eq!(wrap(&text, 6), vec![(0, 6), (6, 9), (9, 15), (15, 17)]);
    }
}
