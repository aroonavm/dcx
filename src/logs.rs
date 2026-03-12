#![allow(dead_code)]

use std::path::Path;

use crate::cmd;
use crate::docker;
use crate::exit_codes;
use crate::naming::{mount_name, relay_dir};
use crate::workspace::resolve_workspace;

// ── Pure functions ────────────────────────────────────────────────────────────

/// Build the argument list for `docker logs`.
///
/// Always includes `--timestamps`. Returns args in the order:
/// `logs`, `--timestamps`, optional flags (`--follow`, `--since`, `--until`, `--tail`), container ID.
pub fn build_logs_args(
    id: &str,
    follow: bool,
    since: Option<&str>,
    until: Option<&str>,
    tail: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["logs".to_string(), "--timestamps".to_string()];

    if follow {
        args.push("--follow".to_string());
    }
    if let Some(since_val) = since {
        args.push("--since".to_string());
        args.push(since_val.to_string());
    }
    if let Some(until_val) = until {
        args.push("--until".to_string());
        args.push(until_val.to_string());
    }
    if let Some(tail_val) = tail {
        args.push("--tail".to_string());
        args.push(tail_val.to_string());
    }

    args.push(id.to_string());
    args
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx logs`.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_logs(
    home: &Path,
    workspace_folder: Option<&Path>,
    follow: bool,
    since: Option<&str>,
    until: Option<&str>,
    tail: Option<&str>,
) -> i32 {
    // 1. Validate Docker/Colima is available.
    if !docker::is_docker_available() {
        eprintln!("Docker is not available. Is Colima running?");
        return exit_codes::RUNTIME_ERROR;
    }

    // 2. Resolve workspace path to absolute canonical path.
    let workspace = match resolve_workspace(workspace_folder) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return exit_codes::USAGE_ERROR;
        }
    };

    // 3. Compute mount point.
    let relay = relay_dir(home);
    let name = mount_name(&workspace);
    let mount_point = relay.join(&name);

    // 4. Find the container (running or stopped) by its devcontainer.local_folder label.
    let containers = docker::query_container_any(&mount_point);
    let Some(container_id) = containers.into_iter().next() else {
        eprintln!("No devcontainer found for this workspace. Run `dcx up` first.");
        return exit_codes::RUNTIME_ERROR;
    };

    // 5. Build and execute docker logs command.
    let args = build_logs_args(&container_id, follow, since, until, tail);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd::run_stream("docker", &args_str).unwrap_or(exit_codes::PREREQ_NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- build_logs_args ---

    #[test]
    fn build_logs_args_no_flags() {
        let args = build_logs_args("abc123", false, None, None, None);
        assert_eq!(args, vec!["logs", "--timestamps", "abc123"]);
    }

    #[test]
    fn build_logs_args_follow() {
        let args = build_logs_args("abc123", true, None, None, None);
        assert_eq!(args, vec!["logs", "--timestamps", "--follow", "abc123"]);
    }

    #[test]
    fn build_logs_args_since() {
        let args = build_logs_args("abc123", false, Some("10m"), None, None);
        assert_eq!(
            args,
            vec!["logs", "--timestamps", "--since", "10m", "abc123"]
        );
    }

    #[test]
    fn build_logs_args_follow_since_now() {
        let args = build_logs_args("abc123", true, Some("now"), None, None);
        assert_eq!(
            args,
            vec![
                "logs",
                "--timestamps",
                "--follow",
                "--since",
                "now",
                "abc123"
            ]
        );
    }

    #[test]
    fn build_logs_args_until() {
        let args = build_logs_args("abc123", false, None, Some("5m"), None);
        assert_eq!(
            args,
            vec!["logs", "--timestamps", "--until", "5m", "abc123"]
        );
    }

    #[test]
    fn build_logs_args_tail() {
        let args = build_logs_args("abc123", false, None, None, Some("20"));
        assert_eq!(args, vec!["logs", "--timestamps", "--tail", "20", "abc123"]);
    }

    #[test]
    fn build_logs_args_tail_all() {
        let args = build_logs_args("abc123", false, None, None, Some("all"));
        assert_eq!(
            args,
            vec!["logs", "--timestamps", "--tail", "all", "abc123"]
        );
    }

    #[test]
    fn build_logs_args_combined() {
        let args = build_logs_args("abc123", true, Some("10m"), None, Some("20"));
        assert_eq!(
            args,
            vec![
                "logs",
                "--timestamps",
                "--follow",
                "--since",
                "10m",
                "--tail",
                "20",
                "abc123"
            ]
        );
    }
}
