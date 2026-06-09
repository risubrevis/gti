//! Terminal management: alternate screen, raw mode, cursor control.
//!
//! The [`TerminalGuard`] struct uses RAII to guarantee that the terminal
//! is always restored to its original state — even if a panic unwinds
//! through the animation code.
//!
//! Strategy:
//!   • **Primary path**: crossterm's `disable_raw_mode()` on drop.
//!   • **Safety net**: we save the original `termios` attributes via
//!     `libc::tcgetattr` *before* crossterm enters raw mode, and
//!     restore them with `libc::tcsetattr(TCSANOW)` as a fallback if
//!     crossterm's disable call fails.

use std::fs::File;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;

use crossterm::{
    cursor::{Hide, Show},
    execute,
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

// ── Public helpers ────────────────────────────────────────────────────

/// Return the current terminal size as `(columns, rows)`.
/// Falls back to 80×24 if the query fails.
pub fn size() -> (u16, u16) {
    terminal::size().unwrap_or((80, 24))
}

// ── RAII guard ────────────────────────────────────────────────────────

/// Acquires full control of the terminal on creation and restores it on drop.
///
/// Creation order:
///   1. Save original `termios` (safety net).
///   2. Enable raw mode via crossterm.
///   3. Enter alternate screen, disable line-wrap, hide cursor, clear.
///
/// Drop order:
///   1. Show cursor, enable line-wrap, leave alternate screen.
///   2. Flush all buffers.
///   3. Disable raw mode via crossterm (primary) or `tcsetattr` (fallback).
pub struct TerminalGuard {
    /// Kept alive so the fd stays valid for the fallback tcsetattr.
    _tty: File,
    /// Saved before any raw-mode changes.
    original_termios: libc::termios,
}

impl TerminalGuard {
    pub fn new() -> Self {
        // ── Safety net: save terminal attributes *before* raw mode ──
        let tty = File::open("/dev/tty").expect("gti: failed to open /dev/tty");
        let mut original_termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(tty.as_raw_fd(), &mut original_termios) } != 0 {
            panic!("gti: failed to save terminal attributes");
        }

        // ── Primary: enable raw mode via crossterm ──────────────────
        terminal::enable_raw_mode().expect("gti: failed to enable raw mode");

        // ── Visual setup via crossterm escape sequences ──────────────
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            DisableLineWrap,
            Hide,
            Clear(ClearType::All)
        )
        .expect("gti: failed to configure terminal");
        stdout.flush().expect("gti: failed to flush stdout");

        TerminalGuard {
            _tty: tty,
            original_termios,
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // 1. Visual restore — cursor, line-wrap, leave alternate screen.
        let mut stdout = io::stdout();
        execute!(stdout, Show, EnableLineWrap, LeaveAlternateScreen).ok();
        stdout.flush().ok();

        // 2. Flush C stdio so escape sequences reach the kernel.
        unsafe {
            libc::fflush(std::ptr::null_mut());
        }

        // 3. Disable raw mode — try crossterm first, fall back to
        //    direct tcsetattr if crossterm fails.
        if terminal::disable_raw_mode().is_err() {
            // Fallback: restore the exact attributes we saved.
            let fd = self._tty.as_raw_fd();
            unsafe {
                libc::tcsetattr(fd, libc::TCSANOW, &self.original_termios);
            }
        }
    }
}
