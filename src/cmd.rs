#![allow(dead_code)]

use std::ffi::OsStr;
use std::process::{Command, Stdio};

/// Output captured from a subprocess.
pub struct CaptureOutput {
    pub stdout: String,
    pub stderr: String,
    /// The process exit code, or 1 if the process was killed by a signal.
    pub status: i32,
}

/// Run `prog` with `args`, capturing stdout and stderr.
///
/// Returns `Err` if the process could not be spawned (e.g. program not found).
/// A non-zero exit code is NOT an error; it is returned in `CaptureOutput.status`.
pub fn run_capture<S: AsRef<OsStr>>(prog: &str, args: &[S]) -> Result<CaptureOutput, String> {
    let output = Command::new(prog)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run {prog}: {e}"))?;
    Ok(CaptureOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status.code().unwrap_or(1),
    })
}

/// Format a command as a human-readable string for `--dry-run` output.
///
/// Returns `"<prog> <arg1> <arg2> ..."`. Callers are responsible for prefixing
/// with context (e.g. `"Would run: {}"`).
pub fn display_cmd<S: AsRef<OsStr>>(prog: &str, args: &[S]) -> String {
    let mut parts = vec![prog.to_string()];
    for arg in args {
        parts.push(arg.as_ref().to_string_lossy().into_owned());
    }
    parts.join(" ")
}

/// Run `prog` with `args`, streaming stdout and stderr to the parent process.
///
/// Returns the child's exit code, or `Err` if the process could not be spawned.
pub fn run_stream<S: AsRef<OsStr>>(prog: &str, args: &[S]) -> Result<i32, String> {
    let status = Command::new(prog)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("Failed to run {prog}: {e}"))?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- display_cmd ---

    #[test]
    fn display_cmd_prog_only() {
        assert_eq!(display_cmd("docker", &[] as &[&str]), "docker");
    }

    #[test]
    fn display_cmd_with_args() {
        assert_eq!(
            display_cmd("devcontainer", &["up", "--workspace-folder", "/tmp/test"]),
            "devcontainer up --workspace-folder /tmp/test"
        );
    }

    #[test]
    fn display_cmd_path_with_spaces() {
        assert_eq!(
            display_cmd(
                "bindfs",
                &["--no-allow-other", "/my project/path", "/relay/mount"]
            ),
            "bindfs --no-allow-other /my project/path /relay/mount"
        );
    }

    // --- run_capture / run_stream ---

    #[test]
    fn run_capture_nonexistent_command_is_err() {
        let result = run_capture("__dcx_nonexistent__", &[] as &[&str]);
        assert!(result.is_err());
    }

    #[test]
    fn run_stream_nonexistent_command_is_err() {
        let result = run_stream("__dcx_nonexistent__", &[] as &[&str]);
        assert!(result.is_err());
    }
}
