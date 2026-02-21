#![allow(dead_code)]

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

/// Build the argument list for `devcontainer exec`.
///
/// Passes both `--container-id` (reliable container lookup, bypasses config-hash
/// resolution) and `--workspace-folder` (so devcontainer reads the config and sets
/// the remote working directory to the workspace folder inside the container).
pub fn build_exec_args(
    container_id: &str,
    mount_point: &Path,
    config: Option<&Path>,
    command: &[String],
) -> Vec<String> {
    let mut args = vec![
        "exec".to_string(),
        "--container-id".to_string(),
        container_id.to_string(),
        "--workspace-folder".to_string(),
        mount_point.to_string_lossy().into_owned(),
    ];
    if let Some(cfg) = config {
        args.push("--config".to_string());
        args.push(cfg.to_string_lossy().into_owned());
    }
    if !command.is_empty() {
        args.push("--".to_string());
        for c in command {
            args.push(c.clone());
        }
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
    config: Option<PathBuf>,
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

    // 7. Delegate to `devcontainer exec`.
    // Pass --container-id for reliable container lookup AND --workspace-folder so
    // devcontainer reads the config and sets the remote working directory correctly
    // (landing the user in the workspace folder, not the container's home dir).
    // SIGINT is forwarded naturally to the child (same process group). No special handling needed.
    progress::step("Running exec in container...");
    let args = build_exec_args(&container_id, &mount_point, config.as_deref(), &command);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd::run_stream("devcontainer", &args_str).unwrap_or(exit_codes::PREREQ_NOT_FOUND)
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
        let args = build_exec_args("abc123", Path::new("/tmp/dcx-x"), None, &[]);
        let ci = args.iter().position(|a| a == "--container-id").unwrap();
        assert_eq!(args[ci + 1], "abc123");
    }

    #[test]
    fn exec_args_includes_workspace_folder() {
        let mount = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let args = build_exec_args("abc123", mount, None, &[]);
        let wf = args.iter().position(|a| a == "--workspace-folder").unwrap();
        assert_eq!(args[wf + 1], mount.to_string_lossy().as_ref());
    }

    #[test]
    fn exec_args_appends_command_after_separator() {
        let cmd = vec!["bash".to_string(), "-c".to_string(), "echo hi".to_string()];
        let args = build_exec_args("abc123", Path::new("/tmp/dcx-x"), None, &cmd);
        let sep = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[sep + 1], "bash");
        assert_eq!(args[sep + 2], "-c");
        assert_eq!(args[sep + 3], "echo hi");
    }

    #[test]
    fn exec_args_no_separator_when_command_empty() {
        let args = build_exec_args("abc123", Path::new("/tmp/dcx-x"), None, &[]);
        assert!(!args.contains(&"--".to_string()));
    }

    #[test]
    fn exec_args_includes_config_when_provided() {
        let cfg = Path::new("/home/user/project/.devcontainer/devcontainer.json");
        let args = build_exec_args("abc123", Path::new("/tmp/dcx-x"), Some(cfg), &[]);
        let ci = args.iter().position(|a| a == "--config").unwrap();
        assert_eq!(args[ci + 1], cfg.to_string_lossy().as_ref());
    }

    #[test]
    fn exec_args_no_config_flag_when_absent() {
        let args = build_exec_args("abc123", Path::new("/tmp/dcx-x"), None, &[]);
        assert!(!args.contains(&"--config".to_string()));
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
}
