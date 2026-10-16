//! Stateless effect layers: each is `fn(buf, test, now)` drawn straight into
//! the frame buffer, so they stack freely. Order is set in `ui::draw`.

use std::f64::consts::PI;
use std::time::Duration;

use web_time::Instant;

use ratatui::prelude::*;

use crate::engine::{HOT, RANKS, Test};

pub type Rgb = (u8, u8, u8);
pub const WHITE: Rgb = (255, 255, 255);
pub const RANK_RGB: [Rgb; 7] = [
    (120, 120, 120),
    (70, 150, 70),
    (110, 215, 80),
    (255, 215, 0),
    (255, 140, 0),
    (255, 50, 30),
    (255, 0, 200),
];
const REAPER: usize = RANKS.len() - 1;
const PEPPER_GREEN: Color = Color::Rgb(40, 170, 50);
const PEPPER_RED: Color = Color::Rgb(215, 35, 25);
const STEM: Color = Color::Rgb(95, 120, 40);

/// Peppers ripen red from ghost pepper up.
fn pepper_body(t: &Test) -> Color {
    if t.rank >= 5 {
        PEPPER_RED
    } else {
        PEPPER_GREEN
    }
}

/// Clipped write: effects can wander off-screen without panicking.
fn put(buf: &mut Buffer, x: i32, y: i32, sym: &str, style: Style) {
    let a = buf.area;
    if (a.left() as i32..a.right() as i32).contains(&x)
        && (a.top() as i32..a.bottom() as i32).contains(&y)
    {
        buf[(x as u16, y as u16)].set_symbol(sym).set_style(style);
    }
}

// ── background ──────────────────────────────────────────────────────────

/// Anime speed lines streaking left from habanero up.
pub fn speed_lines(buf: &mut Buffer, t: &Test, now: Instant) {
    if t.rank < HOT {
        return;
    }
    let every = [0, 0, 0, 0, 6, 4, 2][t.rank];
    let area = buf.area;
    let clock = anim_clock(t, now);
    let w = area.width as f64;
    for y in area.top()..area.bottom() {
        let seed = hash(y as u64, 9, 9) as u64;
        if !seed.is_multiple_of(every) {
            continue;
        }
        let len = 6 + seed % 14;
        let speed = 40.0 * t.rank as f64 + (seed % 40) as f64;
        let head = w - (clock * speed + seed as f64 * 7.0) % (w + len as f64);
        for k in 0..len {
            let fade = k as f64 / len as f64; // bright head, dim tail
            let sym = if k == 0 { "━" } else { "─" };
            put(
                buf,
                area.x as i32 + head as i32 + k as i32,
                y as i32,
                sym,
                Style::new().fg(lerp(WHITE, (50, 50, 50), fade)),
            );
        }
    }
}

const FLARE: f64 = 0.5; // seconds a rank-up flares the flames

/// Background fire. Embers from jalapeño up, flames rise with heat,
/// flare on every rank-up, and swallow the screen at reaper.
pub fn flames(buf: &mut Buffer, t: &Test, now: Instant) {
    if t.rank < 2 {
        return;
    }
    let area = buf.area;
    let clock = anim_clock(t, now);
    let h = area.height as f64;
    let flare = 1.0 + (1.0 - since(t.rank_up, now).as_secs_f64() / FLARE).max(0.0);
    let reaper = t.rank == REAPER;
    // flame height as a fraction of the screen; >1 at reaper so it licks the top
    let reach =
        ((t.heat - 30.0) / 70.0).max(0.0).powf(1.5) * if reaper { 1.6 } else { 1.0 } * flare;
    let ember_every = [0, 0, 14, 9, 6, 4, 2][t.rank];

    for x in area.left()..area.right() {
        let xf = x as f64;
        let wave = ((xf * 0.31 + clock * 4.0).sin() + (xf * 0.13 - clock * 2.3).sin()) / 2.0;
        let col_h = h * reach * (0.7 + 0.3 * wave);
        for y in area.top()..area.bottom() {
            let up = (area.bottom() - 1 - y) as f64; // rows from the bottom
            let noise = hash(x as u64, y as u64, (clock * 15.0) as u64) as f64 / 255.0;
            let v = 1.0 - up / col_h.max(0.01) + (noise - 0.5) * 0.35;
            if col_h > 0.5 && v > 0.05 {
                let v = v.min(1.0);
                let ch = [".", ":", "*", "▒", "▓", "█"][((v * 5.99) as usize).min(5)];
                let c = if reaper {
                    hsv(
                        300.0 + 60.0 * v + 20.0 * (clock * 3.0 + xf * 0.1).sin(),
                        1.0 - v * 0.6,
                        0.4 + 0.6 * v,
                    )
                } else {
                    fire(v)
                };
                buf[(x, y)].set_symbol(ch).set_fg(c);
            }
        }
        // embers drifting up, one lane every `ember_every` columns
        let seed = hash(x as u64, 0, 0) as u64;
        if seed.is_multiple_of(ember_every) {
            let speed = 3.0 + (seed % 5) as f64;
            let up = ((clock * speed + seed as f64) % h) as u16;
            let fade = 1.0 - up as f64 / h;
            buf[(x, area.bottom() - 1 - up)]
                .set_symbol(if up.is_multiple_of(3) { "*" } else { "·" })
                .set_fg(fire(0.3 + 0.7 * fade));
        }
    }
}

// ── panel ───────────────────────────────────────────────────────────────

pub const PANEL_DIM: f64 = 0.6; // resting panel border brightness
const SWEEP: f64 = 0.9; // seconds for rank-up comets to meet
const GLOW: f64 = 0.6; // afterglow fading back to resting
const COMET_TAIL: f64 = 14.0;

/// Rank-up: two comets race from top-center around the panel border,
/// meet at the bottom, light it in the rank color, then it fades back.
pub fn rank_up_sweep(buf: &mut Buffer, r: Rect, t: &Test, now: Instant) {
    let s = since(t.rank_up, now).as_secs_f64();
    if s >= SWEEP + GLOW || r.width < 2 || r.height < 2 {
        return;
    }
    let (x0, y0, x1, y1) = (r.left(), r.top(), r.right() - 1, r.bottom() - 1);
    let ring: Vec<(u16, u16)> = (x0..x1)
        .map(|x| (x, y0))
        .chain((y0..y1).map(|y| (x1, y)))
        .chain((x0 + 1..=x1).rev().map(|x| (x, y1)))
        .chain((y0 + 1..=y1).rev().map(|y| (x0, y)))
        .collect();
    let len = ring.len() as f64;
    let start = (r.width / 2) as f64;
    let head = s / SWEEP * len / 2.0;
    let rgb = RANK_RGB[t.rank];
    for (i, &(x, y)) in ring.iter().enumerate() {
        let off = (i as f64 - start).rem_euclid(len);
        let behind = head - off.min(len - off); // >0: a comet already passed here
        let c = if s >= SWEEP {
            lerp(rgb, mix((0, 0, 0), rgb, PANEL_DIM), (s - SWEEP) / GLOW)
        } else if behind < 0.0 {
            continue;
        } else if behind < COMET_TAIL {
            lerp(WHITE, rgb, behind / COMET_TAIL)
        } else {
            rank_color(t.rank)
        };
        buf[(x, y)].set_style(Style::new().fg(c).bold());
    }
}

/// How many crowd peppers show up per rank (capped by panel width).
const CROWD: [usize; 7] = [0, 0, 3, 6, 10, 16, usize::MAX];
const WAVE: f64 = 0.8; // seconds for a stadium wave to cross

/// Tiny peppers standing on the panel, cheering. They hop idly, all jump on
/// rank-up, do the wave on every jalapeño toss (nonstop at reaper), and
/// flinch when you break a combo.
pub fn crowd(buf: &mut Buffer, t: &Test, now: Instant, panel: Rect) {
    let want = CROWD[t.rank];
    if want == 0 || panel.y < buf.area.y + 4 {
        return;
    }
    let slots = (panel.width.saturating_sub(4) / 5) as usize; // 3 wide + 2 gap
    let mut order: Vec<usize> = (0..slots).collect();
    order.sort_by_key(|&s| hash(s as u64, 3, 3)); // newcomers appear scattered
    let clock = anim_clock(t, now);
    let rank_up = since(t.rank_up, now).as_secs_f64();
    let toss = since(t.jalapenos.last().copied(), now).as_secs_f64();
    let flinch = since(t.broke.map(|b| b.1), now) < Duration::from_millis(600);
    let body = pepper_body(t);
    let face = match t.rank {
        _ if flinch => "°o°",
        r if r >= HOT => "^‿^",
        _ => "•‿•",
    };

    for &s in order.iter().take(want) {
        let frac = (s as f64 + 0.5) / slots as f64;
        let in_wave = |secs: f64| (secs / WAVE - frac).abs() < 0.08;
        let phase = hash(s as u64, 5, 5) as f64 / 40.0;
        let jump = !flinch
            && (rank_up < 0.35
                || in_wave(toss)
                || (t.rank == REAPER && in_wave(clock % (WAVE * 1.5)))
                || (clock * 2.0 + phase).sin() > 0.93);
        let jitter = match flinch {
            true if ((clock * 30.0) as u64).is_multiple_of(2) => 1,
            true => -1,
            false => 0,
        };
        let x = panel.x as i32 + 2 + s as i32 * 5 + jitter;
        let y = panel.y as i32 - 3 - i32::from(jump);
        put(
            buf,
            x + 1,
            y,
            if jump { "╯" } else { "╮" },
            Style::new().fg(STEM),
        );
        for (c, ch) in face.chars().enumerate() {
            put(
                buf,
                x + c as i32,
                y + 1,
                &ch.to_string(),
                Style::new().black().bg(body).bold(),
            );
        }
        for (c, ch) in ["▝", "█", "▘"].into_iter().enumerate() {
            put(buf, x + c as i32, y + 2, ch, Style::new().fg(body));
        }
    }
}

const SPARK_LIFE: f64 = 0.35;

/// Every correct key throws sparks off the letter, more per key as rank rises.
/// `pos` maps a target index to its screen cell (None if scrolled away).
pub fn sparks(buf: &mut Buffer, t: &Test, now: Instant, pos: impl Fn(usize) -> Option<(u16, u16)>) {
    if t.rank < 1 {
        return;
    }
    let per_key = 1 + t.rank / 2;
    for &(at, i) in t.hits.iter().rev() {
        let age = now.saturating_duration_since(at).as_secs_f64();
        if age > SPARK_LIFE {
            break;
        }
        let Some((ox, oy)) = pos(i) else { continue };
        let k = age / SPARK_LIFE;
        for s in 0..per_key {
            let vx = (hash(i as u64, s as u64, 7) as f64 / 255.0 - 0.5) * 16.0;
            let vy = -(2.0 + hash(i as u64, s as u64, 11) as f64 / 255.0 * 6.0);
            let x = (ox as f64 + vx * age).round() as i32;
            let y = (oy as f64 + vy * age + 12.0 * age * age).round() as i32; // gravity
            if (x, y) == (ox as i32, oy as i32) {
                continue;
            }
            let c = if t.rank == REAPER {
                hsv(i as f64 * 40.0 + age * 900.0, 1.0, 1.0)
            } else {
                fire(1.0 - k)
            };
            let a = buf.area;
            if !(a.left() as i32..a.right() as i32).contains(&x)
                || !(a.top() as i32..a.bottom() as i32).contains(&y)
            {
                continue;
            }
            // empty cell: draw the spark; letter: make it glint, keep it readable
            let cell = &mut buf[(x as u16, y as u16)];
            if cell.symbol() == " " {
                cell.set_symbol(["✦", "*", "·", "·"][((k * 4.0) as usize).min(3)])
                    .set_fg(c);
            } else {
                cell.set_style(Style::new().fg(c).bold());
            }
        }
    }
}

// ── foreground ──────────────────────────────────────────────────────────

const TOSS: f64 = 2.2; // seconds to bounce across
const TUMBLER: [[&str; 4]; 2] = [
    ["  ╮  ", "▗███▖", "█•‿•█", " ▀█▘ "],
    ["  ╭  ", "▗███▖", "█°o°█", " ▝█▀ "],
];

/// Jalapeños tossed across the space under the panel, bouncing three times
/// and wobbling between two frames; dust puffs where they land.
pub fn tumblers(buf: &mut Buffer, t: &Test, now: Instant, panel: Rect, help: Rect) {
    let area = buf.area;
    let floor = help.y as i32 - 1;
    let room = floor - panel.bottom() as i32 - 3; // free rows above a landed pepper
    let body = pepper_body(t);
    for (n, &at) in t.jalapenos.iter().enumerate() {
        let p = since(Some(at), now).as_secs_f64() / TOSS;
        if p >= 1.0 {
            continue;
        }
        let run = p * (area.width as f64 + 10.0) - 5.0;
        let x = area.x as i32
            + if n % 2 == 0 {
                run
            } else {
                area.width as f64 - 5.0 - run
            } as i32;
        let bounce = (p * 3.0 * PI).sin().abs();
        // tiny terminal: skim along the top row instead
        let top = if room < 0 {
            area.y as i32
        } else {
            floor - 3 - (bounce * room as f64) as i32
        };
        for (r, row) in TUMBLER[(p * 14.0) as usize % 2].iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                let style = match ch {
                    ' ' => continue,
                    '╮' | '╭' => Style::new().fg(STEM),
                    '•' | '‿' | '°' | 'o' => Style::new().black().bg(body).bold(),
                    _ => Style::new().fg(body),
                };
                put(buf, x + c as i32, top + r as i32, &ch.to_string(), style);
            }
        }
        if room >= 0 && bounce < 0.15 {
            let dust = Style::new().fg(Color::Gray);
            put(buf, x - 1, floor, "∙", dust);
            put(buf, x - 2, floor - 1, "·", dust);
            put(buf, x + 5, floor, "∙", dust);
            put(buf, x + 6, floor - 1, "·", dust);
        }
    }
}

// ── streak ──────────────────────────────────────────────────────────────

const SLAM: f64 = 1.4; // seconds a combo milestone stays up
const STREAK: u32 = 10; // combo where the caret catches fire
const SHATTER: f64 = 0.9;

/// Draw unless the cell holds a letter: effects pass behind the typing text.
fn put_behind(buf: &mut Buffer, x: i32, y: i32, sym: &str, style: Style) {
    let a = buf.area;
    if (a.left() as i32..a.right() as i32).contains(&x)
        && (a.top() as i32..a.bottom() as i32).contains(&y)
        && !buf[(x as u16, y as u16)]
            .symbol()
            .chars()
            .any(char::is_alphanumeric)
    {
        buf[(x as u16, y as u16)].set_symbol(sym).set_style(style);
    }
}

/// Every 50 combo the number slams into `sky`: 3× → 2× → 1×, white-hot,
/// then it cools to the rank color and fades.
pub fn slam(buf: &mut Buffer, t: &Test, now: Instant, sky: Rect) {
    let Some((n, at)) = t.milestone else { return };
    let s = since(Some(at), now).as_secs_f64();
    if s >= SLAM || sky.height < 5 {
        return;
    }
    let scale = match s {
        s if s < 0.08 => 3,
        s if s < 0.16 => 2,
        _ => 1,
    };
    let rgb = RANK_RGB[t.rank];
    let c = match s {
        s if s < 0.25 => WHITE,
        s => mix(rgb, (40, 40, 40), (s - 0.9) / (SLAM - 0.9)),
    };
    let rows = crate::ui::big(n);
    let w = rows[0].chars().count() * scale;
    let x0 = sky.x as i32 + (sky.width as i32 - w as i32) / 2;
    let y0 = sky.y as i32 + (sky.height as i32 - 5 * scale as i32) / 2;
    let style = Style::new()
        .fg(Color::Rgb(c.0, c.1, c.2))
        .bg(Color::Rgb(c.0, c.1, c.2));
    for (r, row) in rows.iter().enumerate() {
        for (col, ch) in row.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let x = x0 + (col * scale + dx) as i32;
                    let y = y0 + (r * scale + dy) as i32;
                    if sky.contains(Position::new(x.max(0) as u16, y.max(0) as u16)) {
                        put(buf, x, y, "█", style);
                    }
                }
            }
        }
    }
}

/// From 10 combo the caret burns: its cell flickers like a coal and a flame
/// licks up from it, taller every 40 combo.
pub fn caret_flame(buf: &mut Buffer, t: &Test, now: Instant, caret: (u16, u16)) {
    if t.combo < STREAK {
        return;
    }
    let tick = (anim_clock(t, now) * 14.0) as u64;
    let noise = |k: u64| hash(k, tick, 4) as f64 / 255.0;
    if buf.area.contains(caret.into()) {
        buf[caret].set_bg(fire(0.6 + 0.4 * noise(9)));
    }
    let h = 1 + (t.combo / 40).min(2) as i32;
    for k in 0..h {
        let sway = if k == 0 {
            0
        } else {
            (noise(k as u64) * 2.99) as i32 - 1
        };
        let v = 1.0 - k as f64 / h as f64 * 0.6 - noise(k as u64 + 5) * 0.2;
        let sym = ["▲", "^", "'"][k as usize];
        let (x, y) = (caret.0 as i32 + sway, caret.1 as i32 - 1 - k);
        let a = buf.area;
        if !(a.left() as i32..a.right() as i32).contains(&x)
            || !(a.top() as i32..a.bottom() as i32).contains(&y)
        {
            continue;
        }
        // empty cell: draw the flame; letters and borders in the way catch the glow
        let cell = &mut buf[(x as u16, y as u16)];
        if cell.symbol() == " " {
            cell.set_symbol(sym);
        }
        cell.set_fg(fire(v));
    }
}

/// A broken combo's big digits crack apart and tumble off `at` (where the
/// combo counter sits), passing behind the text.
pub fn shatter(buf: &mut Buffer, t: &Test, now: Instant, at: Rect) {
    let Some((n, when)) = t.broke else { return };
    let s = since(Some(when), now).as_secs_f64();
    if s >= SHATTER {
        return;
    }
    let k = s / SHATTER;
    let sym = ["█", "▓", "▒", "░"][((k * 4.0) as usize).min(3)];
    let style = Style::new().fg(lerp((255, 60, 40), (60, 20, 20), k));
    for (r, row) in crate::ui::big(n).iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let seed = (r * 31 + c) as u64;
            let vx = (hash(seed, 1, n as u64) as f64 / 255.0 - 0.5) * 30.0;
            let vy = -(hash(seed, 2, n as u64) as f64 / 255.0) * 8.0;
            let x = at.x as f64 + c as f64 + vx * s;
            let y = at.y as f64 + r as f64 + vy * s + 30.0 * s * s; // gravity
            put_behind(buf, x.round() as i32, y.round() as i32, sym, style);
        }
    }
}

// ── keystrokes & words ──────────────────────────────────────────────────

const POP: f64 = 0.08; // correct key flash
const MISS: f64 = 0.12; // wrong key shake
const CRUMB: f64 = 0.45; // backspaced letter falling away
const WORD_SWEEP: f64 = 0.16;
const SPARKLE: f64 = 0.4;
const POPUP: f64 = 0.7; // +points floating up
pub const PUNCH: f64 = 0.3; // multiplier step

/// Every correct key pops: its cell flashes in the rank color, then settles.
pub fn key_pop(
    buf: &mut Buffer,
    t: &Test,
    now: Instant,
    pos: impl Fn(usize) -> Option<(u16, u16)>,
) {
    let page = page_rgb();
    let flash = mix(page, mix(RANK_RGB[t.rank], WHITE, 0.2), 0.55);
    for &(at, i) in t.hits.iter().rev() {
        let age = since(Some(at), now).as_secs_f64();
        if age > POP {
            break;
        }
        if let Some(p) = pos(i) {
            buf[p].set_bg(lerp(flash, page, age / POP));
        }
    }
}

/// A wrong key: the letter you missed shakes in place and the caret blinks red.
pub fn miss_shake(
    buf: &mut Buffer,
    t: &Test,
    now: Instant,
    pos: impl Fn(usize) -> Option<(u16, u16)>,
    caret: (u16, u16),
) {
    let age = since(t.last_error, now).as_secs_f64();
    if age >= MISS {
        return;
    }
    if buf.area.contains(caret.into()) {
        buf[caret].set_bg(Color::Rgb(200, 30, 30));
    }
    let Some(i) = t.typed.len().checked_sub(1) else {
        return;
    };
    let dx = [1, -1, 1, 0][((age / MISS * 4.0) as usize).min(3)];
    if t.typed[i] == t.target[i] || dx == 0 {
        return; // already fixed, or at rest
    }
    let Some((x, y)) = pos(i) else { return };
    let cell = buf[(x, y)].clone();
    buf[(x, y)].set_symbol(" ");
    put(buf, x as i32 + dx, y as i32, cell.symbol(), cell.style());
}

/// Backspaced letters pop up out of the line and fade (red if they were
/// wrong), over the typed text rather than the text still to come.
pub fn crumbs(buf: &mut Buffer, t: &Test, now: Instant, pos: impl Fn(usize) -> Option<(u16, u16)>) {
    let page = page_rgb();
    for &(at, i, c) in t.erased.iter().rev() {
        let age = since(Some(at), now).as_secs_f64();
        if age > CRUMB {
            break;
        }
        let Some((x, y)) = pos(i) else { continue };
        if c == ' ' {
            continue;
        }
        let k = age / CRUMB;
        let base = if t.target.get(i) == Some(&c) {
            (200, 200, 200)
        } else {
            (255, 70, 60)
        };
        let drift = if hash(i as u64, 3, 1).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let (cx, cy) = (
            x as f64 + drift * k * 1.5,
            y as f64 - 1.0 - 2.5 * k + 1.5 * k * k,
        );
        let style = Style::new().fg(lerp(base, page, k));
        put(
            buf,
            cx.round() as i32,
            cy.round() as i32,
            &c.to_string(),
            style,
        );
    }
}

/// A finished word: a bright head sweeps across it; clean words throw a
/// sparkle up off their last letter.
pub fn word_sweep(
    buf: &mut Buffer,
    t: &Test,
    now: Instant,
    pos: impl Fn(usize) -> Option<(u16, u16)>,
) {
    let page = page_rgb();
    let rgb = mix(RANK_RGB[t.rank.max(2)], WHITE, 0.2);
    for w in t.words_done.iter().rev() {
        let age = since(Some(w.at), now).as_secs_f64();
        if age > SPARKLE {
            break;
        }
        let len = w.end.saturating_sub(w.start);
        if age < WORD_SWEEP && len > 0 {
            let head = w.start + ((age / WORD_SWEEP) * len as f64) as usize;
            for i in w.start..=head.min(w.end - 1) {
                let Some(p) = pos(i) else { continue };
                let style = match head - i {
                    0 => Style::new()
                        .fg(Color::Black)
                        .bg(Color::Rgb(rgb.0, rgb.1, rgb.2)),
                    1 | 2 => Style::new().fg(Color::White).bold(),
                    _ => continue,
                };
                buf[p].set_style(style);
            }
        }
        if w.clean
            && let Some((x, y)) = pos(w.end - 1)
        {
            let k = age / SPARKLE;
            let sym = if k < 0.5 { "✦" } else { "·" };
            put_behind(
                buf,
                x as i32 + 1,
                y as i32 - 1 - (k * 2.0) as i32,
                sym,
                Style::new().fg(lerp(WHITE, page, k)),
            );
        }
    }
}

/// +points float up off each finished word.
pub fn points(buf: &mut Buffer, t: &Test, now: Instant, pos: impl Fn(usize) -> Option<(u16, u16)>) {
    let page = page_rgb();
    let rgb = mix(RANK_RGB[t.rank.max(2)], WHITE, 0.4);
    for w in t.words_done.iter().rev() {
        let age = since(Some(w.at), now).as_secs_f64();
        if age > POPUP {
            break;
        }
        let Some((x, y)) = pos((w.start + w.end) / 2) else {
            continue;
        };
        let text = format!("+{}", w.points);
        let k = age / POPUP;
        let (x0, y0) = (
            x as i32 - text.len() as i32 / 2,
            y as i32 - 1 - (k * 2.5) as i32,
        );
        let style = Style::new().fg(lerp(rgb, page, k * k)).bold();
        for (j, ch) in text.chars().enumerate() {
            put(buf, x0 + j as i32, y0, &ch.to_string(), style);
        }
    }
}

/// The multiplier stepping up: a ring of dust bursts out around `at`.
pub fn mult_punch(buf: &mut Buffer, t: &Test, now: Instant, at: (u16, u16)) {
    let age = since(t.mult_up, now).as_secs_f64();
    if age >= PUNCH {
        return;
    }
    let k = age / PUNCH;
    let r = 1.0 + 3.0 * k;
    let style = Style::new().fg(lerp(WHITE, page_rgb(), k));
    for step in 0..20 {
        let a = step as f64 / 20.0 * 2.0 * PI;
        let (x, y) = (at.0 as f64 + 2.0 * r * a.cos(), at.1 as f64 + r * a.sin());
        put_behind(buf, x.round() as i32, y.round() as i32, "·", style);
    }
}

// ── helpers ─────────────────────────────────────────────────────────────

/// The page color as RGB, for fading things into the background.
pub fn page_rgb() -> Rgb {
    match crate::theme::page() {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

/// Seconds since the test started; drives every animation.
pub fn anim_clock(t: &Test, now: Instant) -> f64 {
    t.started
        .map_or(0.0, |s| now.saturating_duration_since(s).as_secs_f64())
}

pub fn since(then: Option<Instant>, now: Instant) -> Duration {
    then.map_or(Duration::MAX, |t| now.saturating_duration_since(t))
}

/// Cheap deterministic noise in 0..256.
pub fn hash(a: u64, b: u64, c: u64) -> u8 {
    let h = (a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ c)
        .wrapping_mul(0x1656_67B1_9E37_79F9);
    (h >> 56) as u8
}

/// Dark red → orange → yellow → white-hot.
pub fn fire(v: f64) -> Color {
    hsv(55.0 * v, 1.0 - (v - 0.75).max(0.0) * 3.0, 0.35 + 0.65 * v)
}

pub fn rank_color(r: usize) -> Color {
    let (red, g, b) = RANK_RGB[r];
    Color::Rgb(red, g, b)
}

pub fn mix(a: Rgb, b: Rgb, k: f64) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * k) as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

pub fn lerp(a: Rgb, b: Rgb, k: f64) -> Color {
    let (r, g, b) = mix(a, b, k);
    Color::Rgb(r, g, b)
}

pub fn hsv(h: f64, s: f64, v: f64) -> Color {
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |u: f64| ((u + m) * 255.0) as u8;
    Color::Rgb(to(r), to(g), to(b))
}
