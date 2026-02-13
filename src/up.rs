#![allow(dead_code)]

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::cmd;
use crate::docker;
use crate::exit_codes;
use crate::mount_table;
use crate::naming::{is_dcx_managed_path, mount_name, relay_dir};
use crate::platform;
use crate::workspace::{find_devcontainer_config, resolve_workspace};

// ── Pure functions ────────────────────────────────────────────────────────────

/// Format the `--dry-run` plan message for `dcx up`.
pub fn dry_run_plan(workspace: &Path, mount_point: &Path) -> String {
    let devcontainer_cmd = cmd::display_cmd(
        "devcontainer",
        &["up", "--workspace-folder", &mount_point.to_string_lossy()],
    );
    format!(
        "Would mount: {} \u{2192} {}\nWould run: {devcontainer_cmd}",
        workspace.display(),
        mount_point.display(),
    )
}

/// Format the hash-collision error message for `dcx up`.
pub fn collision_error(workspace: &Path, found_source: &str, hash: &str) -> String {
    format!(
        "\u{2717} Mount point already exists but points to wrong source!\n\
         \x20 Expected: {}\n\
         \x20 Found:    {found_source}\n\n\
         Hash collision detected (both hash to {hash}).\n\
         This is extremely rare (~1 in 4 billion).\n\
         Run `dcx clean` to reset and retry.",
        workspace.display(),
    )
}

// ── OS helpers ────────────────────────────────────────────────────────────────

/// Return the UID of the file/directory at `path`, or `None` on error.
#[cfg(unix)]
fn file_uid(path: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.uid())
}

/// Return the current process UID, or `None` on error.
fn current_uid() -> Option<u32> {
    cmd::run_capture("id", &["-u"])
        .ok()
        .and_then(|out| out.stdout.trim().parse().ok())
}

/// Return the current user's login name from the `USER` env var.
fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Prompt the user for confirmation when the workspace is not owned by them.
///
/// Returns `true` if the user confirms, `false` if they decline or input fails.
fn confirm_non_owned(workspace: &Path, owner_uid: u32, current_uid: u32) -> bool {
    let current_name = current_username();
    eprintln!("\u{26a0} Directory not owned by current user:");
    eprintln!("  Owner:        UID {owner_uid}");
    eprintln!("  Current user: {current_name} (UID {current_uid})");
    eprintln!("  The directory will be mounted in the container as '{current_name}'.");
    eprintln!("  Access may fail if directory permissions don't allow it.");
    eprintln!("  Workspace: {}", workspace.display());
    eprint!("Continue? [y/N] ");
    let _ = io::stderr().flush();
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut line = String::new();
    if stdin_lock.read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

// ── Mount helpers ─────────────────────────────────────────────────────────────

/// Create `mount_point` and bind-mount `workspace` into it with `bindfs`.
///
/// On bindfs failure the directory is removed to avoid leaving an empty stray dir.
fn do_mount(workspace: &Path, mount_point: &Path) -> Result<(), String> {
    std::fs::create_dir_all(mount_point)
        .map_err(|e| format!("Failed to create {}: {e}", mount_point.display()))?;
    let out = cmd::run_capture(
        "bindfs",
        &[
            "--no-allow-other",
            &workspace.to_string_lossy(),
            &mount_point.to_string_lossy(),
        ],
    )?;
    if out.status != 0 {
        let _ = std::fs::remove_dir(mount_point);
        return Err(format!(
            "bindfs mount failed (exit {}): {}",
            out.status,
            out.stderr.trim()
        ));
    }
    Ok(())
}

/// Unmount `mount_point` using the platform-appropriate unmount command.
fn do_unmount(mount_point: &Path) -> Result<(), String> {
    let prog = platform::unmount_prog();
    let args = platform::unmount_args(mount_point);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = cmd::run_capture(prog, &args_str)?;
    if out.status != 0 {
        return Err(format!(
            "{prog} failed (exit {}): {}",
            out.status,
            out.stderr.trim()
        ));
    }
    Ok(())
}

/// Unmount and remove `mount_point`, then print "Mount rolled back." to stderr.
///
/// Errors during rollback are reported but do not abort the rollback.
fn rollback(mount_point: &Path) {
    if let Err(e) = do_unmount(mount_point) {
        eprintln!("Warning: rollback unmount failed: {e}");
    }
    if let Err(e) = std::fs::remove_dir(mount_point) {
        eprintln!("Warning: rollback rmdir failed: {e}");
    }
    eprintln!("Mount rolled back.");
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx up`.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_up(home: &Path, workspace_folder: Option<PathBuf>, dry_run: bool, yes: bool) -> i32 {
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

    // 3. Recursive mount guard — block nested dcx mounts.
    let relay = relay_dir(home);
    if is_dcx_managed_path(&workspace, &relay) {
        eprintln!(
            "Cannot use a dcx-managed mount point as a workspace. \
             Use the original workspace path instead."
        );
        return exit_codes::USAGE_ERROR;
    }

    // 4. Require a devcontainer configuration.
    if find_devcontainer_config(&workspace).is_none() {
        eprintln!(
            "No devcontainer configuration found in {}.",
            workspace.display()
        );
        return exit_codes::USAGE_ERROR;
    }

    // 5. Compute mount point.
    let name = mount_name(&workspace);
    let mount_point = relay.join(&name);

    // 6. Dry-run: print plan and exit without side effects.
    if dry_run {
        println!("{}", dry_run_plan(&workspace, &mount_point));
        return exit_codes::SUCCESS;
    }

    // 7. Auto-create relay directory.
    if !relay.exists()
        && let Err(e) = std::fs::create_dir_all(&relay)
    {
        eprintln!("Failed to create {}: {e}", relay.display());
        return exit_codes::RUNTIME_ERROR;
    }

    // 8. Non-owned directory warning — prompt unless --yes.
    if !yes {
        #[cfg(unix)]
        if let (Some(fuid), Some(cuid)) = (file_uid(&workspace), current_uid())
            && fuid != cuid
            && !confirm_non_owned(&workspace, fuid, cuid)
        {
            return exit_codes::USER_ABORTED;
        }
    }

    // 9. Mount handling: new / idempotent reuse / stale recovery / collision.
    let workspace_str = workspace.to_string_lossy();
    let table = platform::read_mount_table().unwrap_or_default();
    let source_in_table = mount_table::find_mount_source(&table, &mount_point).map(str::to_string);
    let is_accessible = mount_point.exists();

    let mounted_fresh = if is_accessible {
        match source_in_table.as_deref() {
            Some(source) if source == workspace_str.as_ref() => {
                // Healthy mount, source matches — idempotent reuse.
                false
            }
            Some(found_source) => {
                // Healthy mount, source differs — hash collision.
                let hash = &name[name.len() - 8..];
                eprintln!("{}", collision_error(&workspace, found_source, hash));
                return exit_codes::RUNTIME_ERROR;
            }
            None => {
                // Accessible dir but not in mount table — leftover dir, mount fresh.
                if let Err(e) = do_mount(&workspace, &mount_point) {
                    eprintln!("{e}");
                    return exit_codes::RUNTIME_ERROR;
                }
                true
            }
        }
    } else {
        // Not accessible: stale FUSE zombie or never existed.
        if source_in_table.is_some() {
            // In mount table but inaccessible — zombie FUSE, unmount first.
            if let Err(e) = do_unmount(&mount_point) {
                eprintln!("Failed to unmount stale mount: {e}");
                return exit_codes::RUNTIME_ERROR;
            }
        }
        // Create dir and mount (create_dir_all is a no-op if dir already exists).
        if let Err(e) = do_mount(&workspace, &mount_point) {
            eprintln!("{e}");
            return exit_codes::RUNTIME_ERROR;
        }
        true
    };

    // 10. Delegate to `devcontainer up` with rewritten workspace path.
    let code = cmd::run_stream(
        "devcontainer",
        &["up", "--workspace-folder", &mount_point.to_string_lossy()],
    )
    .unwrap_or(exit_codes::PREREQ_NOT_FOUND);

    // 11. Roll back the mount if devcontainer up failed (and we mounted this run).
    if code != 0 && mounted_fresh {
        rollback(&mount_point);
    }

    code
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- dry_run_plan ---

    #[test]
    fn dry_run_plan_contains_would_mount() {
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp);
        assert!(out.contains("Would mount:"), "got: {out}");
        assert!(out.contains("/home/user/myproject"), "got: {out}");
        assert!(out.contains("dcx-myproject-a1b2c3d4"), "got: {out}");
    }

    #[test]
    fn dry_run_plan_contains_would_run_devcontainer_up() {
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp);
        assert!(out.contains("Would run:"), "got: {out}");
        assert!(
            out.contains("devcontainer up --workspace-folder"),
            "got: {out}"
        );
        assert!(out.contains("dcx-myproject-a1b2c3d4"), "got: {out}");
    }

    #[test]
    fn dry_run_plan_arrow_between_workspace_and_mount() {
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp);
        let arrow_pos = out
            .find('\u{2192}')
            .expect("→ arrow not found in dry-run output");
        let ws_pos = out.find("/home/user/myproject").unwrap();
        let mp_pos = out.find("dcx-myproject-a1b2c3d4").unwrap();
        assert!(ws_pos < arrow_pos, "workspace must appear before →");
        assert!(arrow_pos < mp_pos, "→ must appear before mount point");
    }

    // --- collision_error ---

    #[test]
    fn collision_error_shows_expected_path() {
        let ws = Path::new("/home/bob/project-bar");
        let out = collision_error(ws, "/home/alice/project-foo", "a1b2c3d4");
        assert!(out.contains("Expected:"), "got: {out}");
        assert!(out.contains("/home/bob/project-bar"), "got: {out}");
    }

    #[test]
    fn collision_error_shows_found_source() {
        let ws = Path::new("/home/bob/project-bar");
        let out = collision_error(ws, "/home/alice/project-foo", "a1b2c3d4");
        assert!(out.contains("Found:"), "got: {out}");
        assert!(out.contains("/home/alice/project-foo"), "got: {out}");
    }

    #[test]
    fn collision_error_shows_hash() {
        let ws = Path::new("/home/bob/project-bar");
        let out = collision_error(ws, "/home/alice/project-foo", "a1b2c3d4");
        assert!(out.contains("a1b2c3d4"), "got: {out}");
    }

    #[test]
    fn collision_error_suggests_dcx_clean() {
        let ws = Path::new("/home/bob/project-bar");
        let out = collision_error(ws, "/home/alice/project-foo", "a1b2c3d4");
        assert!(out.contains("dcx clean"), "got: {out}");
    }

    #[test]
    fn collision_error_contains_cross_symbol() {
        let ws = Path::new("/home/bob/project-bar");
        let out = collision_error(ws, "/home/alice/project-foo", "a1b2c3d4");
        assert!(out.contains('\u{2717}'), "got: {out}");
    }
}
