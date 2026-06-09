mod animation;
mod args;
mod terminal;

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag set by the SIGINT (Ctrl+C) handler to request graceful termination.
static TERMINATED: AtomicBool = AtomicBool::new(false);

fn main() -> ExitCode {
    // Collect every argument after "gti" so we can forward them to git later.
    let git_args = args::parse_args();

    // Register a Ctrl+C / SIGINT handler *before* entering raw mode.
    // In raw mode Ctrl+C is delivered as a key event, but an external
    // "kill -INT" would otherwise leave the terminal in a broken state.
    ctrlc::set_handler(|| {
        TERMINATED.store(true, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    // ── Animation phase ──────────────────────────────────────────
    // The TerminalGuard enters the alternate screen, enables raw mode
    // and hides the cursor. When it is dropped the terminal is fully
    // restored, even if a panic occurs inside the animation loop.
    {
        let _guard = terminal::TerminalGuard::new();
        animation::run(&TERMINATED);
        // _guard dropped here → terminal restored
    }

    // ── Delegation phase ───────────────────────────────────────────
    // Transparently hand off to the real git so the user's intended
    // command still executes after the easter-egg animation.
    //
    // We return the exit code instead of calling std::process::exit()
    // so that all Rust/C destructors and stdio flushes run properly.
    let code = args::delegate_to_git(&git_args);
    ExitCode::from(code)
}
