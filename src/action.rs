use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus};

/// Replaces the current process image with `command`. Only returns on failure —
/// a successful exec never returns control to the caller.
pub fn exec_replace(mut command: Command) -> io::Error {
    command.exec()
}

pub fn kill(pid: u32) -> io::Result<ExitStatus> {
    // `kill -TERM 0` signals the caller's entire process group (including strays
    // itself), not a single process — refuse it rather than trusting callers to
    // never pass through a malformed `pid: 0` from `claude agents --json`.
    if pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to signal pid 0",
        ));
    }

    Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
}
