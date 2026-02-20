#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::cmd;
use crate::docker;
use crate::exit_codes;
use crate::mount_table;
use crate::naming::{is_dcx_managed_path, mount_name, relay_dir};
use crate::platform;
use crate::progress;
use crate::signals;
use crate::up::tilde_path;
use crate::workspace::resolve_workspace;

// ── Pure functions ────────────────────────────────────────────────────────────

/// Informational message when no dcx mount exists for the workspace (idempotent).
pub fn nothing_to_do(workspace: &Path) -> String {
    format!("No mount found for {}. Nothing to do.", workspace.display())
}

/// Error message when the workspace directory no longer exists on disk.
pub fn workspace_missing_error() -> &'static str {
    "Workspace directory does not exist. Use `dcx clean` to remove stale mounts."
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx down`.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_down(home: &Path, workspace_folder: Option<PathBuf>) -> i32 {
    // Install SIGINT handler. If Ctrl+C arrives during container stop (step 7),
    // docker stop uses run_capture, so signal is not forwarded. Check interrupted
    // flag after the call returns. If Ctrl+C arrives during unmount (step 8),
    // we log a message and complete the unmount before exiting.
    let interrupted = signals::interrupted_flag();

    // 1. Validate Docker/Colima is available.
    if !docker::is_docker_available() {
        eprintln!("Docker is not available. Is Colima running?");
        return exit_codes::RUNTIME_ERROR;
    }

    // 2+3. Resolve workspace path; show down-specific message if it doesn't exist.
    let workspace = match resolve_workspace(workspace_folder.as_deref()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("{}", workspace_missing_error());
            return exit_codes::USAGE_ERROR;
        }
    };
    progress::step(&format!(
        "Resolving workspace path: {}",
        workspace.display()
    ));

    // 4. Recursive mount guard — block nested dcx mounts.
    let relay = relay_dir(home);
    if is_dcx_managed_path(&workspace, &relay) {
        eprintln!(
            "Cannot use a dcx-managed mount point as a workspace. \
             Use the original workspace path instead."
        );
        return exit_codes::USAGE_ERROR;
    }

    // 5. Compute mount point.
    let name = mount_name(&workspace);
    let mount_point = relay.join(&name);

    // 6. If no mount found: nothing to do.
    let table = platform::read_mount_table().unwrap_or_default();
    let source_in_table = mount_table::find_mount_source(&table, &mount_point);
    if source_in_table.is_none() {
        println!("{}", nothing_to_do(&workspace));
        return exit_codes::SUCCESS;
    }

    // 7. Stop the container using Docker.
    // Note: docker::stop_container uses run_capture (not run_stream), so SIGINT is not
    // forwarded to docker stop. Check interrupted flag after the call returns.
    progress::step("Stopping devcontainer...");
    if let Err(e) = docker::stop_container(&mount_point) {
        eprintln!("{e}");
        return exit_codes::RUNTIME_ERROR;
    }

    // 8. Unmount bindfs. If SIGINT arrived between steps 7 and 8 (or during unmount),
    // log the message and complete the unmount before exiting.
    let was_interrupted = interrupted.load(Ordering::Relaxed);
    if was_interrupted {
        eprintln!("Signal received, finishing unmount...");
    }
    let tilde_mp = tilde_path(&mount_point, home);
    progress::step(&format!("Unmounting {tilde_mp}..."));
    let prog = platform::unmount_prog();
    let args = platform::unmount_args(&mount_point);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match cmd::run_capture(prog, &args_str) {
        Ok(out) if out.status != 0 => {
            eprintln!("{prog} failed (exit {}): {}", out.status, out.stderr.trim());
            return exit_codes::RUNTIME_ERROR;
        }
        Err(e) => {
            eprintln!("{e}");
            return exit_codes::RUNTIME_ERROR;
        }
        Ok(_) => {}
    }

    // 9. Remove mount directory.
    if let Err(e) = std::fs::remove_dir(&mount_point) {
        eprintln!("Failed to remove {}: {e}", mount_point.display());
        return exit_codes::RUNTIME_ERROR;
    }

    if was_interrupted {
        return exit_codes::RUNTIME_ERROR;
    }

    progress::step("Done.");
    exit_codes::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- nothing_to_do ---

    #[test]
    fn nothing_to_do_message() {
        let ws = Path::new("/home/user/myproject");
        let msg = nothing_to_do(ws);
        assert!(msg.contains("No mount found"), "got: {msg}");
        assert!(msg.contains("/home/user/myproject"), "got: {msg}");
        assert!(msg.contains("Nothing to do"), "got: {msg}");
    }

    // --- workspace_missing_error ---

    #[test]
    fn workspace_missing_error_message() {
        let msg = workspace_missing_error();
        assert!(msg.contains("does not exist"), "got: {msg}");
        assert!(msg.contains("dcx clean"), "got: {msg}");
    }
}
