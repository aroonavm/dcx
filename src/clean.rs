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

// ── Data structures ───────────────────────────────────────────────────────────

/// A plan for cleaning a single workspace mount.
/// Separates observation (scan phase) from execution.
#[derive(Clone, Debug)]
struct CleanPlan {
    /// Path to the mount point
    mount_point: PathBuf,
    /// Mount name (e.g. dcx-myproject-a1b2c3d4)
    mount_name: String,
    /// State before cleaning: "running", "orphaned", "stale", or "empty dir"
    state: String,
    /// Container ID if one exists (populated during scan)
    container_id: Option<String>,
    /// Runtime image ID (populated during scan if container exists)
    runtime_image_id: Option<String>,
    /// Build image name (populated when purge=true)
    build_image_name: Option<String>,
    /// Volumes associated with the container (populated when purge=true)
    volumes: Vec<String>,
    /// Whether the mount is currently mounted
    is_mounted: bool,
}

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

/// Execute the cleanup for a single plan.
///
/// Performs: stop container, remove container, remove runtime image, remove build image (if purge),
/// remove volumes (if purge), unmount, remove directory.
/// Returns (state_before, action_taken) tuple.
fn execute_one(plan: &CleanPlan) -> Result<(String, String), String> {
    // Stop the container (idempotent if not found)
    docker::stop_container(&plan.mount_point)?;

    // Remove container if we have its ID
    if let Some(ref container_id) = plan.container_id {
        docker::remove_container(container_id)?;
    }

    // Remove runtime image if we have its ID
    if let Some(ref image_id) = plan.runtime_image_id {
        docker::remove_image(image_id)?;
    }

    // Remove build image if purge is enabled
    if let Some(ref build_image) = plan.build_image_name
        && let Err(e) = docker::remove_base_image(build_image)
    {
        eprintln!("Note: Could not remove build image {build_image}: {e}");
    }

    // Remove volumes if purge is enabled
    for volume in &plan.volumes {
        if let Err(e) = docker::remove_volume(volume) {
            eprintln!("Note: Could not remove volume {volume}: {e}");
        }
    }

    // Unmount if mounted
    if plan.is_mounted {
        do_unmount(&plan.mount_point)?;
    }

    // Remove directory (mandatory)
    remove_mount_dir(&plan.mount_point)?;

    let action = if plan.container_id.is_some() {
        "stopped, removed".to_string()
    } else {
        "removed".to_string()
    };

    Ok((plan.state.clone(), action))
}

/// Scan a single mount point and build a CleanPlan.
///
/// This is a read-only operation that queries mount table, container state, and image information.
/// Does NOT mutate any state.
fn scan_one(mount_point: &Path, purge: bool, workspace: Option<&Path>) -> CleanPlan {
    let mount_name = mount_point
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Check if mounted and determine state
    let table = platform::read_mount_table().unwrap_or_default();
    let is_mounted = mount_table::find_mount_source(&table, mount_point).is_some();
    let mount_source = mount_table::find_mount_source(&table, mount_point);

    // Resolve workspace: prefer caller-supplied, fall back to mount table
    let workspace_path: Option<&Path> = workspace.or_else(|| mount_source.map(Path::new));

    // Check for container
    let container_id = docker::query_container_any(mount_point);
    let has_container = container_id.is_some();

    // Determine state
    let state = categorize_mount_state(mount_point, has_container);

    // Get runtime image ID if container exists
    let runtime_image_id = if let Some(ref cid) = container_id {
        docker::get_image_id(cid).ok()
    } else {
        None
    };

    // Get build image name and volumes only when purge=true
    let build_image_name = if purge {
        workspace_path.and_then(docker::get_base_image_name)
    } else {
        None
    };

    let volumes = if purge {
        if let Some(ref cid) = container_id {
            docker::get_container_volumes(cid).unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    CleanPlan {
        mount_point: mount_point.to_path_buf(),
        mount_name,
        state,
        container_id: container_id.clone(),
        runtime_image_id,
        build_image_name,
        volumes,
        is_mounted,
    }
}

/// Categorize the state of a mount before cleaning.
///
/// Returns a human-readable state string: "running", "orphaned", "stale", or "empty dir"
fn categorize_mount_state(mount_point: &Path, has_container: bool) -> String {
    let table = platform::read_mount_table().unwrap_or_default();
    let is_in_mount_table = mount_table::find_mount_source(&table, mount_point).is_some();
    let is_accessible = mount_point.exists();

    if is_in_mount_table && is_accessible {
        if has_container {
            "running".to_string()
        } else {
            "orphaned".to_string()
        }
    } else if is_in_mount_table && !is_accessible {
        "stale".to_string()
    } else if !is_in_mount_table && is_accessible {
        "empty dir".to_string()
    } else {
        // Directory doesn't exist and not mounted — shouldn't happen, but classify as empty
        "empty dir".to_string()
    }
}

/// Perform full cleanup for a single mount entry: stop container, remove container, remove
/// runtime image, optionally remove build image and volumes, unmount, remove dir.
///
/// `container_id` is optional; if None, container/image removal is skipped.
/// `purge`: if true, also removes the build image and Docker volumes. Non-fatal on failure.
/// Returns a tuple of (state_before_cleaning, action_taken).
fn clean_one(
    mount_point: &Path,
    container_id: Option<&str>,
    purge: bool,
    workspace: Option<&Path>,
) -> Result<(String, String), String> {
    // Determine state before cleanup
    let has_container = container_id.is_some();
    let state_before = categorize_mount_state(mount_point, has_container);

    // Stop the container (idempotent if not found)
    docker::stop_container(mount_point)?;

    // Remove container if we have its ID. Must get image ID before removing container!
    if let Some(id) = container_id {
        // Get the image ID first (while container still exists for inspection)
        let image_id = docker::get_image_id(id)?;
        // Then remove the container
        docker::remove_container(id)?;
        // Finally remove the runtime image (vsc-dcx-*-uid)
        docker::remove_image(&image_id)?;
    }

    // Resolve workspace source: prefer the caller-supplied workspace path,
    // fall back to mount table lookup (needed for --all mode where workspace is unknown).
    let table = platform::read_mount_table().unwrap_or_default();
    let mount_source = mount_table::find_mount_source(&table, mount_point);
    let workspace_path: Option<&Path> = workspace.or_else(|| mount_source.map(Path::new));

    // Optionally remove the build image (e.g. dcx-dev:latest).
    // Non-fatal: the image may be shared with other workspaces or already removed.
    if purge
        && let Some(ws) = workspace_path
        && let Some(build_image) = docker::get_base_image_name(ws)
        && let Err(e) = docker::remove_base_image(&build_image)
    {
        eprintln!("Note: Could not remove build image {build_image}: {e}");
    }

    // Unmount if mounted.
    if mount_source.is_some() {
        do_unmount(mount_point)?;
    }

    // Remove directory if it exists
    if mount_point.exists() {
        remove_mount_dir(mount_point)?;
    }

    let action = if has_container {
        "stopped, removed".to_string()
    } else {
        "removed".to_string()
    };

    Ok((state_before, action))
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx clean`.
///
/// Without `--all`: cleans only the current workspace.
/// With `--all`: cleans all dcx-managed workspaces.
/// With `--dry-run`: shows what would be cleaned without executing.
/// With `--purge`: also removes the build image and Docker volumes.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_clean(
    home: &Path,
    workspace_folder: Option<PathBuf>,
    all: bool,
    yes: bool,
    purge: bool,
    dry_run: bool,
) -> i32 {
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

    // Handle --dry-run for default mode (no `--all`)
    if !all && dry_run {
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

        if mount_point.exists() || purge {
            let plan = scan_one(&mount_point, purge, Some(&workspace));
            let dry_run_plan = format::DryRunPlan {
                mount_name: plan.mount_name,
                state: plan.state,
                container_id: plan.container_id,
                runtime_image_id: plan.runtime_image_id,
                build_image_name: plan.build_image_name,
                volumes: plan.volumes,
                is_mounted: plan.is_mounted,
            };
            let output = format::format_dry_run(&[dry_run_plan]);
            if output.trim().is_empty() {
                println!("Nothing to clean for {}.", workspace.display());
            } else {
                println!("{output}");
            }
        } else {
            println!("Nothing to clean for {}.", workspace.display());
        }
        return exit_codes::SUCCESS;
    }

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

        // Find container (running or stopped) if mount exists
        let container_any = if mount_point.exists() {
            docker::query_container_any(&mount_point)
        } else {
            None
        };

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

        // Clean if there's anything to do: mount exists, or purge wants build image
        if mount_point.exists() || purge {
            match clean_one(
                &mount_point,
                container_any.as_deref(),
                purge,
                Some(&workspace),
            ) {
                Ok((was_state, action)) => {
                    println!("Cleaned {}:", workspace.display());
                    println!(
                        "  {}  was: {}  → {}",
                        mount_point
                            .file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default(),
                        was_state,
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

                // Mounted but no container for this mount - clean it up (no purge for orphaned)
                match clean_one(&path, None, false, None) {
                    Ok((was_state, action)) => {
                        println!("  {}  was: {}  → {}", name, was_state, action);
                        cleaned_count += 1;
                    }
                    Err(e) => {
                        errors.push(e);
                    }
                }
            }
        }

        // Fallback: clean any vsc-dcx-* or dangling images that weren't caught above.
        // Handles the case where the container was already removed externally before dcx clean ran.
        progress::step("Checking for orphaned images...");
        match docker::clean_orphaned_images() {
            Ok(removed) if removed > 0 => {
                progress::step(&format!("Removed {removed} orphaned image(s)."));
            }
            Ok(_) => {}
            Err(e) => {
                errors.push(format!("Warning: Could not clean orphaned images: {e}"));
            }
        }

        if cleaned_count == 0 && errors.is_empty() {
            println!("Nothing to clean for {}.", workspace.display());
        } else if errors.is_empty() {
            progress::step("Done.");
        }

        if errors.is_empty() {
            exit_codes::SUCCESS
        } else {
            eprintln!("Error: {}", errors[0]);
            exit_codes::RUNTIME_ERROR
        }
    } else {
        // Handle --dry-run for --all mode
        if dry_run {
            let entry_paths = scan_relay(&relay);
            let plans: Vec<format::DryRunPlan> = entry_paths
                .iter()
                .map(|mp| {
                    let plan = scan_one(mp, purge, None);
                    format::DryRunPlan {
                        mount_name: plan.mount_name,
                        state: plan.state,
                        container_id: plan.container_id,
                        runtime_image_id: plan.runtime_image_id,
                        build_image_name: plan.build_image_name,
                        volumes: plan.volumes,
                        is_mounted: plan.is_mounted,
                    }
                })
                .collect();
            println!("{}", format::format_dry_run(&plans));
            return exit_codes::SUCCESS;
        }

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

            match clean_one(mount_point, container_id.as_deref(), purge, None) {
                Ok((was_state, action)) => {
                    cleaned.push(CleanEntry {
                        workspace: None,
                        mount: mount_name_str,
                        was: was_state,
                        action,
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

    // --- scan_one ---

    #[test]
    fn scan_one_finds_build_image_from_workspace_when_mount_missing() {
        // Simulates `dcx clean --purge` after a previous `dcx clean` removed the mount.
        // The mount point doesn't exist, but the workspace has devcontainer.json.
        let workspace = tempfile::tempdir().unwrap();
        let devcontainer_dir = workspace.path().join(".devcontainer");
        std::fs::create_dir_all(&devcontainer_dir).unwrap();
        std::fs::write(
            devcontainer_dir.join("devcontainer.json"),
            r#"{ "image": "dcx-dev:latest" }"#,
        )
        .unwrap();

        // Mount point that does NOT exist (simulates post-clean state)
        let fake_mount = PathBuf::from("/tmp/dcx-nonexistent-00000000");

        let plan = scan_one(&fake_mount, true, Some(workspace.path()));

        assert_eq!(
            plan.build_image_name.as_deref(),
            Some("dcx-dev:latest"),
            "purge should find build image from workspace even when mount is gone"
        );
    }

    #[test]
    fn scan_one_no_build_image_without_purge() {
        let workspace = tempfile::tempdir().unwrap();
        let devcontainer_dir = workspace.path().join(".devcontainer");
        std::fs::create_dir_all(&devcontainer_dir).unwrap();
        std::fs::write(
            devcontainer_dir.join("devcontainer.json"),
            r#"{ "image": "dcx-dev:latest" }"#,
        )
        .unwrap();

        let fake_mount = PathBuf::from("/tmp/dcx-nonexistent-00000000");

        let plan = scan_one(&fake_mount, false, Some(workspace.path()));

        assert_eq!(
            plan.build_image_name, None,
            "without purge, build image should not be resolved"
        );
    }
}
