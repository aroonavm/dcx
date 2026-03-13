#![allow(dead_code)]

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::cmd;
use crate::docker;
use crate::exit_codes;
use crate::mount_table;
use crate::naming::{is_dcx_managed_path, mount_name, relay_dir};
use crate::platform;
use crate::progress;
use crate::workspace::resolve_workspace;

// ── Pure functions ────────────────────────────────────────────────────────────

/// Error message when no dcx mount exists for the workspace.
pub fn no_mount_error(workspace: &Path) -> String {
    format!(
        "No mount found for {}. Run `dcx up` first.",
        workspace.display()
    )
}

/// Error message when the mount point exists in the table but is inaccessible.
pub fn stale_mount_error() -> &'static str {
    "Mount is stale. Run `dcx up` to remount."
}

/// Choose the right error when no active mount is found.
///
/// If the relay directory still exists (created by a previous `dcx up`), the mount
/// became stale (FUSE died, system rebooted, etc.). If the directory is absent,
/// `dcx up` was never run for this workspace.
pub fn mount_not_found_error(workspace: &Path, mount_dir_exists: bool) -> String {
    if mount_dir_exists {
        stale_mount_error().to_string()
    } else {
        no_mount_error(workspace)
    }
}

/// Build the argument list for `docker exec`.
///
/// Uses `docker exec -w <workspace>` to set the working directory directly,
/// bypassing devcontainer exec entirely. The container's default user (set by
/// devcontainer during creation to `remoteUser`) is inherited automatically.
/// This avoids devcontainer exec's config resolution and lifecycle hook
/// re-execution, which caused concurrent session conflicts.
///
/// TTY flags:
/// - `-i` (stdin open): always included for input passthrough
/// - `-t` (pseudo-TTY): included when `tty=true` (interactive sessions);
///   omitted when stdin is a pipe (non-interactive commands)
pub fn build_exec_args(
    container_id: &str,
    workspace_path: &Path,
    tty: bool,
    command: &[String],
) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "-i".to_string()];
    if tty {
        args.push("-t".to_string());
    }
    args.push("-w".to_string());
    args.push(workspace_path.to_string_lossy().into_owned());
    args.push(container_id.to_string());
    for c in command {
        args.push(c.clone());
    }
    args
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx exec`.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_exec(
    home: &Path,
    workspace_folder: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    command: Vec<String>,
) -> i32 {
    // 1. Validate Docker/Colima is available.
    if !docker::is_docker_available() {
        eprintln!("Docker is not available. Is Colima running?");
        return exit_codes::RUNTIME_ERROR;
    }

    // 2. Resolve workspace path to absolute canonical path.
    let workspace = match resolve_workspace(workspace_folder.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return exit_codes::USAGE_ERROR;
        }
    };

    // 2b. Validate --config-dir if provided (kept for CLI compatibility).
    // With docker exec, config resolution is unnecessary — the container was already
    // configured by dcx up. We still validate the path exists for user feedback.
    if let Some(ref dir) = config_dir {
        let abs_dir = if dir.is_absolute() {
            dir.clone()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(dir))
                .unwrap_or(dir.clone())
        };
        if !abs_dir.exists() {
            eprintln!("Config directory not found: {}", abs_dir.display());
            return exit_codes::USAGE_ERROR;
        }
        if !abs_dir.is_dir() {
            eprintln!("Config path is not a directory: {}", abs_dir.display());
            return exit_codes::USAGE_ERROR;
        }
        let json = abs_dir.join("devcontainer.json");
        if !json.exists() {
            eprintln!("devcontainer.json not found in: {}", abs_dir.display());
            return exit_codes::USAGE_ERROR;
        }
    }
    progress::step(&format!(
        "Resolving workspace path: {}",
        workspace.display()
    ));

    // 3. Recursive mount guard — block nested dcx mounts.
    let relay = relay_dir(home);
    if is_dcx_managed_path(&workspace, &relay) {
        eprintln!(
            "Cannot use a dcx-managed mount point as a workspace. \
             Use the original workspace path instead."
        );
        return exit_codes::USAGE_ERROR;
    }

    // 4. Verify mount exists in the mount table.
    let name = mount_name(&workspace);
    let mount_point = relay.join(&name);
    let table = platform::read_mount_table().unwrap_or_default();
    let source_in_table = mount_table::find_mount_source(&table, &mount_point);

    if source_in_table.is_none() {
        // Mount directory existing means dcx up was run before but the mount went away.
        eprintln!(
            "{}",
            mount_not_found_error(&workspace, mount_point.exists())
        );
        return exit_codes::RUNTIME_ERROR;
    }

    // 5. Verify mount is healthy (accessible). In table but not accessible = zombie FUSE.
    if !mount_point.exists() {
        eprintln!("{}", stale_mount_error());
        return exit_codes::RUNTIME_ERROR;
    }

    // 6. Find the running container by its devcontainer.local_folder label.
    //    Using --container-id bypasses devcontainer's config-hash-based lookup entirely,
    //    which is more reliable than relying on devcontainer to resolve the config.
    let container_id = docker::find_devcontainer_by_workspace(&mount_point);
    let Some(container_id) = container_id else {
        eprintln!("No running devcontainer found for this workspace. Run `dcx up` first.");
        return exit_codes::RUNTIME_ERROR;
    };

    // 7. Print network mode if available
    if let Some(network_mode) = docker::read_network_mode(&container_id) {
        progress::step(&format!("Network: {}", network_mode));
    }

    // 8. Delegate to `docker exec` with `-w` to set working directory.
    // Uses docker exec directly instead of devcontainer exec to avoid:
    // - Config resolution issues (devcontainer reads source config, not override)
    // - Lifecycle hook re-execution (postAttachCommand races in concurrent sessions)
    // The container's default user is already set to remoteUser by devcontainer during
    // creation, so no `-u` flag is needed. SIGINT is forwarded naturally (same process group).
    progress::step("Running exec in container...");

    let tty = std::io::stdin().is_terminal();
    let args = build_exec_args(&container_id, &workspace, tty, &command);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd::run_stream("docker", &args_str).unwrap_or(exit_codes::PREREQ_NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- no_mount_error ---

    #[test]
    fn no_mount_error_message() {
        let ws = Path::new("/home/user/myproject");
        let msg = no_mount_error(ws);
        assert!(msg.contains("No mount found"), "got: {msg}");
        assert!(msg.contains("/home/user/myproject"), "got: {msg}");
        assert!(msg.contains("dcx up"), "got: {msg}");
    }

    // --- stale_mount_error ---

    #[test]
    fn stale_mount_error_message() {
        let msg = stale_mount_error();
        assert!(msg.contains("stale"), "got: {msg}");
        assert!(msg.contains("dcx up"), "got: {msg}");
        assert!(msg.contains("remount"), "got: {msg}");
    }

    // --- build_exec_args ---

    #[test]
    fn exec_args_includes_container_id() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, false, &[]);
        assert!(args.contains(&"abc123".to_string()));
    }

    #[test]
    fn exec_args_sets_working_directory() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, false, &[]);
        let wi = args.iter().position(|a| a == "-w").unwrap();
        assert_eq!(args[wi + 1], "/home/user/myproject");
    }

    #[test]
    fn exec_args_appends_command_directly() {
        let ws = Path::new("/home/user/myproject");
        let cmd = vec!["bash".to_string(), "-c".to_string(), "echo hi".to_string()];
        let args = build_exec_args("abc123", ws, false, &cmd);
        // Command follows container ID directly (no -- separator needed for docker exec)
        let cid_pos = args.iter().position(|a| a == "abc123").unwrap();
        assert_eq!(args[cid_pos + 1], "bash");
        assert_eq!(args[cid_pos + 2], "-c");
        assert_eq!(args[cid_pos + 3], "echo hi");
    }

    #[test]
    fn exec_args_no_command_when_empty() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, false, &[]);
        // exec -i -w <workspace> <container_id> (5 elements when tty=false)
        assert_eq!(args.len(), 5);
    }

    #[test]
    fn exec_args_uses_docker_exec_format() {
        let ws = Path::new("/home/user/myproject");
        let cmd = vec!["echo".to_string(), "hello".to_string()];
        let args = build_exec_args("abc123", ws, false, &cmd);
        assert_eq!(args[0], "exec");
        assert_eq!(args[1], "-i");
        assert_eq!(args[2], "-w");
        assert_eq!(args[3], "/home/user/myproject");
        assert_eq!(args[4], "abc123");
        assert_eq!(args[5], "echo");
        assert_eq!(args[6], "hello");
    }

    // --- mount_not_found_error ---

    #[test]
    fn mount_not_found_error_stale_when_dir_exists() {
        // If the relay directory still exists but the mount is gone, it's a stale state
        let ws = Path::new("/home/user/myproject");
        let msg = mount_not_found_error(ws, true);
        assert!(msg.contains("stale"), "got: {msg}");
    }

    #[test]
    fn mount_not_found_error_no_mount_when_dir_absent() {
        // If the relay directory never existed, no mount was ever set up
        let ws = Path::new("/home/user/myproject");
        let msg = mount_not_found_error(ws, false);
        assert!(msg.contains("No mount found"), "got: {msg}");
    }

    // --- TTY flag tests ---

    #[test]
    fn exec_args_always_includes_interactive_flag() {
        let ws = Path::new("/home/user/myproject");
        let args_tty = build_exec_args("abc123", ws, true, &[]);
        let args_no_tty = build_exec_args("abc123", ws, false, &[]);
        assert!(args_tty.contains(&"-i".to_string()), "got: {:?}", args_tty);
        assert!(
            args_no_tty.contains(&"-i".to_string()),
            "got: {:?}",
            args_no_tty
        );
    }

    #[test]
    fn exec_args_includes_tty_flag_when_true() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, true, &[]);
        assert!(args.contains(&"-t".to_string()), "got: {:?}", args);
    }

    #[test]
    fn exec_args_no_tty_flag_when_false() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, false, &[]);
        assert!(!args.contains(&"-t".to_string()), "got: {:?}", args);
    }
}
