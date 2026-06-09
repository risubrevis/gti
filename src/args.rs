/// Collect all command-line arguments after the program name,
/// i.e. everything the user typed after `gti` (e.g. `status`, `commit -m "msg"`).
pub fn parse_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

/// Execute the real `git` binary with the captured arguments, inheriting
/// stdin / stdout / stderr so the user sees normal git output.
///
/// Returns git's exit code so the caller can propagate it without
/// calling `std::process::exit()` (which skips C stdio flushes and
/// can leave the terminal in a broken state).
pub fn delegate_to_git(args: &[String]) -> u8 {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args);
    cmd.stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    match cmd.status() {
        Ok(status) => status.code().unwrap_or(1) as u8,
        Err(e) => {
            eprintln!("gti: failed to execute git: {e}");
            127
        }
    }
}
