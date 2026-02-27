#![allow(dead_code)]

use std::io::{self, BufRead, Write};
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
use crate::workspace::{find_devcontainer_config, resolve_workspace};

// ── Pure functions ────────────────────────────────────────────────────────────

/// Abbreviate `path` with `~` if it starts with `home`.
pub fn tilde_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rel) => {
            let rel_str = rel.to_string_lossy();
            if rel_str.is_empty() {
                "~".to_string()
            } else {
                format!("~/{rel_str}")
            }
        }
        Err(_) => path.display().to_string(),
    }
}

/// Format the `--dry-run` plan message for `dcx up`.
pub fn dry_run_plan(
    workspace: &Path,
    mount_point: &Path,
    home: &Path,
    config: Option<&Path>,
) -> String {
    let tilde_mount = tilde_path(mount_point, home);
    let mut args = vec![
        "up".to_string(),
        "--workspace-folder".to_string(),
        tilde_mount.clone(),
    ];
    if let Some(cfg) = config {
        args.push("--config".to_string());
        args.push(cfg.to_string_lossy().into_owned());
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let devcontainer_cmd = cmd::display_cmd("devcontainer", &args_ref);
    format!(
        "Would mount: {} \u{2192} {tilde_mount}\nWould run: {devcontainer_cmd}",
        workspace.display(),
    )
}

/// Format the hash-collision error message for `dcx up`.
pub fn collision_error(workspace: &Path, found_source: &str, hash: &str) -> String {
    format!(
        "\u{2717} Mount point already exists but points to wrong source!\n\
         \x20\x20Expected: {}\n\
         \x20\x20Found:    {found_source}\n\n\
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

/// Look up the username for a UID by parsing `/etc/passwd`.
///
/// Returns the username if found, or `"UID <n>"` as a fallback.
fn username_for_uid(uid: u32) -> String {
    if let Ok(content) = std::fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let mut fields = line.splitn(7, ':');
            let name = fields.next().unwrap_or("");
            let _ = fields.next(); // password
            let uid_field = fields.next().unwrap_or("");
            if uid_field.parse::<u32>().ok() == Some(uid) {
                return name.to_string();
            }
        }
    }
    format!("UID {uid}")
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Prompt the user for confirmation when the workspace is not owned by them.
///
/// Returns `true` if the user confirms, `false` if they decline or input fails.
fn confirm_non_owned(workspace: &Path, owner_uid: u32, current_uid: u32) -> bool {
    let owner_name = username_for_uid(owner_uid);
    let current_name = current_username();
    eprintln!(
        "\u{26a0}\u{fe0f}  Directory {} is owned by {owner_name} (UID {owner_uid})",
        workspace.display()
    );
    eprintln!("    Current user is {current_name} (UID {current_uid})");
    eprintln!();
    eprintln!("    In the container, you'll run as {current_name} ({current_uid}).");
    eprintln!("    You'll have read/write access only if the directory permissions allow it.");
    eprintln!();
    eprint!("Proceed? [y/N] ");
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
pub fn run_up(
    home: &Path,
    workspace_folder: Option<PathBuf>,
    config: Option<PathBuf>,
    dry_run: bool,
    yes: bool,
) -> i32 {
    // Install SIGINT handler before any mount operations so Ctrl+C triggers rollback
    // rather than leaving an orphaned mount.
    let interrupted = signals::interrupted_flag();

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

    // 4. Require a devcontainer configuration.
    if config.is_none() && find_devcontainer_config(&workspace).is_none() {
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
        println!(
            "{}",
            dry_run_plan(&workspace, &mount_point, home, config.as_deref())
        );
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
                let tilde_mp = tilde_path(&mount_point, home);
                progress::step(&format!("Mounting workspace to {tilde_mp}..."));
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
        let tilde_mp = tilde_path(&mount_point, home);
        progress::step(&format!("Mounting workspace to {tilde_mp}..."));
        // Create dir and mount (create_dir_all is a no-op if dir already exists).
        if let Err(e) = do_mount(&workspace, &mount_point) {
            eprintln!("{e}");
            return exit_codes::RUNTIME_ERROR;
        }
        true
    };

    // 9.5. Network mode enforcement: if an existing container was started with a different
    // dcx.network-mode, remove it so devcontainer up creates a fresh container with the
    // requested mode. Handles containers that survived dcx down for any reason.
    let requested_network =
        std::env::var("DCX_NETWORK_MODE").unwrap_or_else(|_| "minimal".to_string());
    let stale_containers: Vec<String> = docker::query_container_any(&mount_point)
        .into_iter()
        .filter(|id| docker::read_network_mode(id).as_deref() != Some(requested_network.as_str()))
        .collect();
    if !stale_containers.is_empty() {
        progress::step("Recreating container for new network mode...");
        if let Err(e) = docker::stop_container(&mount_point) {
            eprintln!("{e}");
            if mounted_fresh {
                rollback(&mount_point);
            }
            return exit_codes::RUNTIME_ERROR;
        }
        for id in &stale_containers {
            if let Err(e) = docker::remove_container(id) {
                eprintln!("{e}");
                if mounted_fresh {
                    rollback(&mount_point);
                }
                return exit_codes::RUNTIME_ERROR;
            }
        }
    }

    // 10. Delegate to `devcontainer up` with rewritten workspace path.
    // Check the interrupted flag before starting devcontainer: if SIGINT arrived
    // in the window between do_mount returning and here, roll back and exit cleanly.
    if interrupted.load(Ordering::Relaxed) {
        if mounted_fresh {
            rollback(&mount_point);
        }
        return exit_codes::RUNTIME_ERROR;
    }
    // If Ctrl+C is pressed during devcontainer up, devcontainer (same process group)
    // is killed, run_stream returns non-zero, and we roll back below.
    progress::step("Starting devcontainer...");
    let mount_str = mount_point.to_string_lossy();
    let config_str = config.as_ref().map(|p| p.to_string_lossy().into_owned());

    // Pass the relay mount path to devcontainer as the workspace folder.
    // The relay mount is the only path that devcontainer can access (it's visible to Docker/Colima).
    // devcontainer will read the devcontainer.json from the relay mount via the bindfs mount,
    // so the config must be accessible there.
    let mut dc_args = vec![
        "up",
        "--workspace-folder",
        mount_str.as_ref(),
    ];
    if let Some(ref s) = config_str {
        dc_args.push("--config");
        dc_args.push(s.as_str());
    }
    let code = cmd::run_stream("devcontainer", &dc_args).unwrap_or(exit_codes::PREREQ_NOT_FOUND);

    // 11. Roll back on failure (if we mounted this run) and return RUNTIME_ERROR.
    // This handles both normal devcontainer failures and Ctrl+C (SIGINT kills the child,
    // returning non-zero, which lands here for rollback).
    // The spec requires exit code 1 (not the child's exit code) when dcx up fails
    // after rollback, because the failure is a dcx error, not a pass-through.
    if code != 0 {
        if mounted_fresh {
            rollback(&mount_point);
        }
        return exit_codes::RUNTIME_ERROR;
    }

    // 12. Tag the base image for later cleanup by `dcx clean --purge`.
    // Non-fatal: if tagging fails (e.g. no "image" field in devcontainer.json),
    // purge will simply skip base image removal for this workspace.
    if let Some(base_image) = docker::get_base_image_name(&workspace, config.as_deref())
        && let Err(e) = docker::tag_base_image(&base_image, &name)
    {
        eprintln!("Warning: Could not tag base image: {e}");
    }

    progress::step("Done.");
    exit_codes::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- tilde_path ---

    #[test]
    fn tilde_path_abbreviates_home_prefix() {
        let home = Path::new("/home/user");
        let path = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        assert_eq!(
            tilde_path(path, home),
            "~/.colima-mounts/dcx-myproject-a1b2c3d4"
        );
    }

    #[test]
    fn tilde_path_leaves_non_home_path_unchanged() {
        let home = Path::new("/home/user");
        let path = Path::new("/tmp/something");
        assert_eq!(tilde_path(path, home), "/tmp/something");
    }

    #[test]
    fn tilde_path_home_dir_itself_is_tilde() {
        let home = Path::new("/home/user");
        assert_eq!(tilde_path(home, home), "~");
    }

    #[test]
    fn tilde_path_does_not_match_sibling_dir() {
        // /home/user2 must NOT be abbreviated for home=/home/user.
        let home = Path::new("/home/user");
        let path = Path::new("/home/user2/.colima-mounts/dcx-proj-a1b2c3d4");
        assert_eq!(
            tilde_path(path, home),
            "/home/user2/.colima-mounts/dcx-proj-a1b2c3d4"
        );
    }

    // --- dry_run_plan ---

    #[test]
    fn dry_run_plan_contains_would_mount() {
        let home = Path::new("/home/user");
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp, home, None);
        assert!(out.contains("Would mount:"), "got: {out}");
        assert!(out.contains("/home/user/myproject"), "got: {out}");
        assert!(out.contains("dcx-myproject-a1b2c3d4"), "got: {out}");
    }

    #[test]
    fn dry_run_plan_uses_tilde_for_mount_path() {
        let home = Path::new("/home/user");
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp, home, None);
        assert!(
            out.contains("~/.colima-mounts/dcx-myproject-a1b2c3d4"),
            "mount path must use tilde abbreviation, got: {out}"
        );
        assert!(
            !out.contains("/home/user/.colima-mounts"),
            "mount path must not use absolute path, got: {out}"
        );
    }

    #[test]
    fn dry_run_plan_contains_would_run_devcontainer_up() {
        let home = Path::new("/home/user");
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp, home, None);
        assert!(out.contains("Would run:"), "got: {out}");
        assert!(
            out.contains("devcontainer up --workspace-folder"),
            "got: {out}"
        );
        assert!(out.contains("dcx-myproject-a1b2c3d4"), "got: {out}");
    }

    #[test]
    fn dry_run_plan_arrow_between_workspace_and_mount() {
        let home = Path::new("/home/user");
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp, home, None);
        let arrow_pos = out
            .find('\u{2192}')
            .expect("→ arrow not found in dry-run output");
        let ws_pos = out.find("/home/user/myproject").unwrap();
        let mp_pos = out.find("dcx-myproject-a1b2c3d4").unwrap();
        assert!(ws_pos < arrow_pos, "workspace must appear before →");
        assert!(arrow_pos < mp_pos, "→ must appear before mount point");
    }

    #[test]
    fn dry_run_plan_includes_config_flag_when_provided() {
        let home = Path::new("/home/user");
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let cfg = Path::new("/home/user/myproject/.devcontainer/full/devcontainer.json");
        let out = dry_run_plan(ws, mp, home, Some(cfg));
        assert!(out.contains("--config"), "got: {out}");
        assert!(
            out.contains("/home/user/myproject/.devcontainer/full/devcontainer.json"),
            "got: {out}"
        );
    }

    #[test]
    fn dry_run_plan_no_config_flag_when_absent() {
        let home = Path::new("/home/user");
        let ws = Path::new("/home/user/myproject");
        let mp = Path::new("/home/user/.colima-mounts/dcx-myproject-a1b2c3d4");
        let out = dry_run_plan(ws, mp, home, None);
        assert!(!out.contains("--config"), "got: {out}");
    }

    // --- current_username ---

    #[test]
    fn current_username_returns_env_var_value() {
        // USER (or USERNAME on Windows) is set in any normal test environment.
        // The function must return whatever is in the env — not "unknown".
        let name = current_username();
        let expected = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());
        assert_eq!(name, expected);
    }

    // --- username_for_uid ---

    #[test]
    fn username_for_uid_returns_root_for_uid_zero() {
        // UID 0 is always "root" on Linux and macOS.
        let name = username_for_uid(0);
        assert_eq!(name, "root", "UID 0 should resolve to 'root'");
    }

    #[test]
    fn username_for_uid_returns_fallback_for_unknown_uid() {
        // UID 4_294_967_294 is highly unlikely to appear in /etc/passwd.
        let uid = 4_294_967_294u32;
        assert_eq!(username_for_uid(uid), format!("UID {uid}"));
    }

    // --- collision_error ---

    #[test]
    fn collision_error_contains_all_required_fields() {
        let ws = Path::new("/home/bob/project-bar");
        let out = collision_error(ws, "/home/alice/project-foo", "a1b2c3d4");
        assert!(out.contains('\u{2717}'), "missing cross symbol: {out}");
        assert!(out.contains("Expected:"), "missing Expected label: {out}");
        assert!(
            out.contains("/home/bob/project-bar"),
            "missing workspace: {out}"
        );
        assert!(out.contains("Found:"), "missing Found label: {out}");
        assert!(
            out.contains("/home/alice/project-foo"),
            "missing found source: {out}"
        );
        assert!(out.contains("a1b2c3d4"), "missing hash: {out}");
        assert!(
            out.contains("dcx clean"),
            "missing dcx clean suggestion: {out}"
        );
    }
}
