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

    // --- run_capture ---

    #[test]
    fn run_capture_echo_stdout() {
        let out = run_capture("echo", &["hello"]).unwrap();
        assert_eq!(out.stdout.trim(), "hello");
    }

    #[test]
    fn run_capture_true_exits_zero() {
        let out = run_capture("true", &[] as &[&str]).unwrap();
        assert_eq!(out.status, 0);
    }

    #[test]
    fn run_capture_false_exits_nonzero() {
        let out = run_capture("false", &[] as &[&str]).unwrap();
        assert_ne!(out.status, 0);
    }

    #[test]
    fn run_capture_nonexistent_command_is_err() {
        let result = run_capture("__dcx_nonexistent__", &[] as &[&str]);
        assert!(result.is_err());
    }

    #[test]
    fn run_capture_stderr_captured() {
        // sh -c 'echo err >&2' writes to stderr only.
        let out = run_capture("sh", &["-c", "echo err >&2"]).unwrap();
        assert_eq!(out.stderr.trim(), "err");
        assert!(out.stdout.trim().is_empty());
    }

    // --- run_stream ---

    #[test]
    fn run_stream_true_returns_zero() {
        let code = run_stream("true", &[] as &[&str]).unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn run_stream_false_returns_nonzero() {
        let code = run_stream("false", &[] as &[&str]).unwrap();
        assert_ne!(code, 0);
    }

    #[test]
    fn run_stream_nonexistent_command_is_err() {
        let result = run_stream("__dcx_nonexistent__", &[] as &[&str]);
        assert!(result.is_err());
    }

    #[test]
    fn run_stream_passes_exit_code_through() {
        // sh -c 'exit 42' exits with code 42.
        let code = run_stream("sh", &["-c", "exit 42"]).unwrap();
        assert_eq!(code, 42);
    }
}
