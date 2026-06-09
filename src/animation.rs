//! Animation engine for pre-rendered ASCII dance frames.
//!
//! Frames are generated at compile time by `build.rs` from the text
//! files in `assets/dance*/`.  On each invocation one dance is picked
//! at random, played through once, then the function returns.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::Clear,
    terminal::ClearType,
};

use crate::terminal;

// Include the compile-time generated frame arrays and DANCES catalogue
include!(concat!(env!("OUT_DIR"), "/frames.rs"));

// ── Random dance selection ───────────────────────────────────────────

/// Pick a dance at random using the nanosecond component of the system
/// clock as a lightweight entropy source.
fn pick_dance() -> usize {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock is before UNIX epoch")
        .subsec_nanos();
    (nanos as usize) % DANCES.len()
}

// ── Frame dimensions ─────────────────────────────────────────────────

fn frame_width(frames: &[&str]) -> usize {
    frames
        .iter()
        .flat_map(|f| f.lines())
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
}

fn frame_height(frames: &[&str]) -> usize {
    frames.iter().map(|f| f.lines().count()).max().unwrap_or(0)
}

// ── Rendering ────────────────────────────────────────────────────────

/// Render a single pre-rendered frame, centred in the terminal.
fn render_frame(stdout: &mut io::Stdout, frame: &str, fw: usize, fh: usize) {
    let (term_cols, term_rows) = terminal::size();

    execute!(stdout, Clear(ClearType::All)).ok();

    let base_col = ((term_cols as isize - fw as isize) / 2).max(0) as u16;
    let base_row = ((term_rows as isize - fh as isize) / 2).max(0) as u16;

    for (i, line) in frame.lines().enumerate() {
        let row = base_row + i as u16;

        if row >= term_rows {
            break;
        }

        if line.is_empty() {
            continue;
        }

        let visible: String = line
            .chars()
            .take(term_cols.saturating_sub(base_col) as usize)
            .collect();

        if visible.trim().is_empty() {
            continue;
        }

        execute!(stdout, MoveTo(base_col, row)).ok();
        write!(stdout, "{}", visible).ok();
    }

    stdout.flush().ok();
}

// ── Animation loop ───────────────────────────────────────────────────

/// Pick a random dance and play it through once, then return.
///
/// Early exit is triggered by either:
///   • The SIGINT handler (via `terminated`), or
///   • Key events: `q`, `Esc`, or `Ctrl+C`.
pub fn run(terminated: &AtomicBool) {
    let idx = pick_dance();
    let (_name, frames, fps_ms) = DANCES[idx];

    let fw = frame_width(frames);
    let fh = frame_height(frames);
    let frame_budget = Duration::from_millis(fps_ms);

    let mut stdout = io::stdout();

    for frame in frames.iter() {
        let tick = Instant::now();

        // ── Check for early exit via SIGINT ──────────────────────────
        if terminated.load(Ordering::SeqCst) {
            return;
        }

        // Non-blocking key poll — raw mode delivers Ctrl+C as a key
        while event::poll(Duration::ZERO).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.code == KeyCode::Char('q')
                    || key.code == KeyCode::Esc
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    return;
                }
            }
        }

        // ── Render current frame ──────────────────────────────────
        render_frame(&mut stdout, frame, fw, fh);

        // ── Frame-rate limiter ──────────────────────────────────────
        let elapsed = tick.elapsed();
        if let Some(sleep) = frame_budget.checked_sub(elapsed) {
            thread::sleep(sleep);
        }
    }
}
