#![allow(dead_code)]

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::cmd;
use crate::docker;
use crate::exit_codes;
use crate::format::{self, CleanEntry};
use crate::mount_table;
use crate::naming::{mount_name, relay_dir};
use crate::platform;
use crate::progress;
use crate::signals;
use crate::workspace::resolve_workspace;

// ── Pure functions ─────────────────────────────────────────────────────────────

/// Build the warning text for the confirmation prompt when stopping containers.
///
/// `entries` is a list of `(workspace_display, mount_name, container_id)` tuples.
/// The caller is responsible for printing the final "Continue? [y/N] " prompt.
pub fn confirm_prompt(entries: &[(String, String, String)]) -> String {
    let count = entries.len();
    let mut lines = Vec::new();
    lines.push(format!(
        "\u{26a0} {} active container{} will be stopped:",
        count,
        if count == 1 { "" } else { "s" }
    ));
    for (ws, mount, container) in entries {
        lines.push(format!(
            "  - {}  \u{2192}  {}  (container: {})",
            ws, mount, container
        ));
    }
    lines.join("\n")
}

// ── Internal helpers ───────────────────────────────────────────────────────────

/// Scan `relay` for all `dcx-*` subdirectories and return their sorted paths.
fn scan_relay(relay: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(relay) else {
        return vec![];
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name();
            if name.to_string_lossy().starts_with("dcx-") {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    dirs.sort();
    dirs
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

/// Remove the relay directory entry at `mount_point`.
fn remove_mount_dir(mount_point: &Path) -> Result<(), String> {
    std::fs::remove_dir(mount_point)
        .map_err(|e| format!("Failed to remove {}: {e}", mount_point.display()))
}

/// Perform full cleanup for a single mount entry: stop container, remove container, remove image, unmount, remove dir.
///
/// `container_id` is optional; if provided, it will be used directly. If None, we skip remove_container and remove_image.
/// Returns the action label on success.
fn clean_one(mount_point: &Path, container_id: Option<&str>) -> Result<String, String> {
    // Stop the container (idempotent if not found)
    docker::stop_container(mount_point)?;

    // Remove container if we have its ID. Must get image ID before removing container!
    if let Some(id) = container_id {
        // Get the image ID first (while container still exists for inspection)
        let image_id = docker::get_image_id(id)?;
        // Then remove the container
        docker::remove_container(id)?;
        // Finally remove the image
        docker::remove_image(&image_id)?;
    }

    // Check if mounted before unmounting. Only unmount if directory is actually mounted.
    let table = platform::read_mount_table().unwrap_or_default();
    if mount_table::find_mount_source(&table, mount_point).is_some() {
        do_unmount(mount_point)?;
    }

    // Remove directory (mandatory)
    remove_mount_dir(mount_point)?;

    Ok("cleaned".to_string())
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx clean`.
///
/// Without `--all`: cleans only the current workspace.
/// With `--all`: cleans all dcx-managed workspaces.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_clean(home: &Path, workspace_folder: Option<PathBuf>, all: bool, yes: bool) -> i32 {
    // Install SIGINT handler. If Ctrl+C arrives while an unmount is in progress,
    // we finish that entry's cleanup then exit (remaining entries are skipped).
    let interrupted = signals::interrupted_flag();

    // 1. Validate Docker/Colima is available.
    if !docker::is_docker_available() {
        eprintln!("Docker is not available. Is Colima running?");
        return exit_codes::RUNTIME_ERROR;
    }

    progress::step("Scanning relay directory...");
    let relay = relay_dir(home);

    // Mode 1: Default (no `--all`) — clean current workspace only
    if !all {
        // Resolve workspace path
        let workspace = match resolve_workspace(workspace_folder.as_deref()) {
            Ok(p) => p,
            Err(_) => {
                eprintln!("Workspace directory does not exist.");
                return exit_codes::USAGE_ERROR;
            }
        };

        // Compute mount point
        let name = mount_name(&workspace);
        let mount_point = relay.join(&name);

        let mut cleaned_count = 0;
        let mut errors = Vec::new();

        // Clean current workspace's mount if it exists
        if mount_point.exists() {
            // Find container (running or stopped)
            let container_any = docker::query_container_any(&mount_point);

            // If container found and running, check with user
            #[allow(clippy::collapsible_if)]
            if let Some(ref container_id) = container_any {
                if docker::query_container(&mount_point).is_some() && !yes {
                    // Container is running, prompt user
                    let mount_name_str = mount_point
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let entries = vec![(
                        workspace.display().to_string(),
                        mount_name_str,
                        container_id.clone(),
                    )];
                    let prompt_text = confirm_prompt(&entries);
                    eprintln!("{prompt_text}");
                    eprint!("\nContinue? [y/N] ");
                    let _ = io::stderr().flush();
                    let stdin = io::stdin();
                    let mut input = String::new();
                    if stdin.lock().read_line(&mut input).is_err() {
                        return exit_codes::RUNTIME_ERROR;
                    }
                    if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                        return exit_codes::USER_ABORTED;
                    }
                }
            }

            match clean_one(&mount_point, container_any.as_deref()) {
                Ok(action) => {
                    println!(
                        "Cleaned {}  →  {}  ({})",
                        workspace.display(),
                        mount_point
                            .file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default(),
                        action
                    );
                    cleaned_count += 1;
                }
                Err(e) => {
                    errors.push(e.clone());
                }
            }
        }

        // Also clean up any orphaned MOUNTS in the relay directory (mounted but no container)
        // But only if they are actually mounted (not just empty directories)
        let table = platform::read_mount_table().unwrap_or_default();
        progress::step("Checking for orphaned mounts...");
        if let Ok(entries) = std::fs::read_dir(&relay) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.file_name().unwrap_or_default().to_string_lossy();

                // Skip non-dcx mounts
                if !name.starts_with("dcx-") {
                    continue;
                }

                // Skip the current workspace's mount (already handled)
                if path == mount_point {
                    continue;
                }

                // Only clean if this mount is actually mounted (check mount table)
                if mount_table::find_mount_source(&table, &path).is_none() {
                    // Not mounted, skip it (leave empty directories alone)
                    continue;
                }

                // Mounted but potentially orphaned - check for container
                if docker::query_container_any(&path).is_some() {
                    // Container exists, don't clean
                    continue;
                }

                // Mounted but no container for this mount - clean it up
                match clean_one(&path, None) {
                    Ok(_) => {
                        cleaned_count += 1;
                    }
                    Err(e) => {
                        errors.push(e);
                    }
                }
            }
        }

        if cleaned_count == 0 && errors.is_empty() {
            println!("Nothing to clean.");
        }

        if errors.is_empty() {
            exit_codes::SUCCESS
        } else {
            eprintln!("Error: {}", errors[0]);
            exit_codes::RUNTIME_ERROR
        }
    } else {
        // Mode 2: `--all` — clean all dcx-managed workspaces
        let entry_paths = scan_relay(&relay);
        let mut cleaned: Vec<CleanEntry> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        // Collect running containers for confirmation (if there are entries)
        let running_containers: Vec<(String, String, String)> = entry_paths
            .iter()
            .filter_map(|mount_point| {
                if let Some(container_id) = docker::query_container(mount_point) {
                    let mount_name_str = mount_point
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    Some(("(unknown)".to_string(), mount_name_str, container_id))
                } else {
                    None
                }
            })
            .collect();

        // Prompt if there are running containers (unless --yes)
        if !running_containers.is_empty() && !yes {
            let prompt_text = confirm_prompt(&running_containers);
            eprintln!("{prompt_text}");
            eprint!("\nContinue? [y/N] ");
            let _ = io::stderr().flush();
            let stdin = io::stdin();
            let mut input = String::new();
            if stdin.lock().read_line(&mut input).is_err() {
                return exit_codes::RUNTIME_ERROR;
            }
            if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                return exit_codes::USER_ABORTED;
            }
        }

        // Clean all entries, continuing on failure
        for mount_point in &entry_paths {
            let mount_name_str = mount_point
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            progress::step(&format!("Cleaning {mount_name_str}..."));

            let container_id = docker::query_container_any(mount_point);

            match clean_one(mount_point, container_id.as_deref()) {
                Ok(_) => {
                    cleaned.push(CleanEntry {
                        workspace: None,
                        mount: mount_name_str,
                        was: "mount".to_string(),
                        action: "cleaned".to_string(),
                    });
                }
                Err(e) => {
                    failures.push(format!("{}: {e}", mount_point.display()));
                }
            }

            // If SIGINT arrived during this entry's cleanup, finish it and exit
            if interrupted.load(Ordering::Relaxed) {
                eprintln!("Signal received, finishing current unmount...");
                break;
            }
        }

        // Clean up orphaned containers and images (not associated with existing mounts)
        progress::step("Cleaning up orphaned containers...");
        match docker::clean_orphaned_containers() {
            Ok(removed) if removed > 0 => {
                progress::step(&format!("Removed {removed} orphaned container(s)."));
            }
            Ok(_) => {
                // No orphaned containers found
            }
            Err(e) => {
                eprintln!("Warning: Could not clean orphaned containers: {e}");
            }
        }

        progress::step("Cleaning up orphaned images...");
        match docker::clean_orphaned_images() {
            Ok(removed) if removed > 0 => {
                progress::step(&format!("Removed {removed} dangling image(s)."));
            }
            Ok(_) => {
                // No dangling images found
            }
            Err(e) => {
                eprintln!("Warning: Could not clean orphaned images: {e}");
            }
        }

        // Print summary
        if !cleaned.is_empty() {
            println!("{}", format::format_clean_summary(&cleaned, 0));
        } else if entry_paths.is_empty() {
            println!("Nothing to clean.");
        }

        // Print failures
        for f in &failures {
            eprintln!("Error: {f}");
        }

        if failures.is_empty() {
            exit_codes::SUCCESS
        } else {
            exit_codes::RUNTIME_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- confirm_prompt ---

    #[test]
    fn confirm_prompt_shows_count() {
        let entries = vec![
            (
                "/home/user/project-a".to_string(),
                "dcx-project-a-a1b2c3d4".to_string(),
                "abc123".to_string(),
            ),
            (
                "/home/user/project-b".to_string(),
                "dcx-project-b-e5f6g7h8".to_string(),
                "def456".to_string(),
            ),
        ];
        let out = confirm_prompt(&entries);
        assert!(out.contains("2 active containers"), "got: {out}");
    }

    #[test]
    fn confirm_prompt_singular_for_one_entry() {
        let entries = vec![(
            "/home/user/project-a".to_string(),
            "dcx-project-a-a1b2c3d4".to_string(),
            "abc123".to_string(),
        )];
        let out = confirm_prompt(&entries);
        assert!(out.contains("1 active container"), "got: {out}");
        assert!(
            !out.contains("1 active containers"),
            "must not pluralize for 1: got: {out}"
        );
    }

    #[test]
    fn confirm_prompt_lists_each_entry() {
        let entries = vec![(
            "/home/user/project-a".to_string(),
            "dcx-project-a-a1b2c3d4".to_string(),
            "abc123".to_string(),
        )];
        let out = confirm_prompt(&entries);
        assert!(out.contains("/home/user/project-a"), "got: {out}");
        assert!(out.contains("dcx-project-a-a1b2c3d4"), "got: {out}");
        assert!(out.contains("abc123"), "got: {out}");
    }

    #[test]
    fn confirm_prompt_contains_warning_symbol() {
        let entries = vec![(
            "/home/user/project-a".to_string(),
            "dcx-project-a-a1b2c3d4".to_string(),
            "abc123".to_string(),
        )];
        let out = confirm_prompt(&entries);
        assert!(out.contains('\u{26a0}'), "got: {out}");
    }

    #[test]
    fn confirm_prompt_shows_will_be_stopped() {
        let entries = vec![(
            "/home/user/project-a".to_string(),
            "dcx-project-a-a1b2c3d4".to_string(),
            "abc123".to_string(),
        )];
        let out = confirm_prompt(&entries);
        assert!(out.contains("will be stopped"), "got: {out}");
    }
}
