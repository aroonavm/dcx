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

    // 2b. Resolve --config to an absolute path and validate it exists.
    let config: Option<PathBuf> = if let Some(p) = config {
        let abs = if p.is_absolute() {
            p
        } else {
            std::env::current_dir().map(|cwd| cwd.join(&p)).unwrap_or(p)
        };
        if !abs.exists() {
            eprintln!("Config file not found: {}", abs.display());
            return exit_codes::USAGE_ERROR;
        }
        Some(abs)
    } else {
        None
    };

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
        eprintln!("{}", mount_not_found_error(&workspace, mount_point.exists()));
        return exit_codes::RUNTIME_ERROR;
    }

    // 5. Verify mount is healthy (accessible). In table but not accessible = zombie FUSE.
    if !mount_point.exists() {
        eprintln!("{}", stale_mount_error());
        return exit_codes::RUNTIME_ERROR;
    }

    // 6. Rewrite workspace path and delegate to `devcontainer exec`.
    // SIGINT is forwarded naturally to the child (same process group). No special handling needed.
    progress::step("Running exec in container...");
    let mount_str = mount_point.to_string_lossy().into_owned();
    let config_str = config.as_ref().map(|p| p.to_string_lossy().into_owned());
    let mut args = vec!["exec", "--workspace-folder", &mount_str];
    if let Some(ref s) = config_str {
        args.push("--config");
        args.push(s.as_str());
    }
    if !command.is_empty() {
        args.push("--");
        for c in &command {
            args.push(c.as_str());
        }
    }
    cmd::run_stream("devcontainer", &args).unwrap_or(exit_codes::PREREQ_NOT_FOUND)
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
