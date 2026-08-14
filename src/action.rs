use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus};

/// Replaces the current process image with `command`. Only returns on failure —
/// a successful exec never returns control to the caller.
pub fn exec_replace(mut command: Command) -> io::Error {
    command.exec()
}

pub fn kill(pid: u32) -> io::Result<ExitStatus> {
    Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
}
