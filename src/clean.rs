#![allow(dead_code)]

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use std::sync::atomic::Ordering;

use crate::categorize::{self, MountStatus};
use crate::cmd;
use crate::docker;
use crate::exit_codes;
use crate::format::{self, CleanEntry};
use crate::mount_table;
use crate::naming::relay_dir;
use crate::platform;
use crate::progress;
use crate::signals;
use crate::status::query_container;

// ── Pure functions ─────────────────────────────────────────────────────────────

/// Map a `MountStatus` to the "was:" label shown in the clean summary.
pub fn was_label(status: &MountStatus) -> &'static str {
    match status {
        MountStatus::Active => "running",
        MountStatus::Orphaned => "orphaned",
        MountStatus::Stale => "stale",
        MountStatus::Empty => "empty dir",
    }
}

/// Build the warning text for the `--all` confirmation prompt.
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

// ── Internal helpers ──────────────────────────────────────────────────────────

struct EntryInfo {
    path: PathBuf,
    workspace: Option<String>,
    status: MountStatus,
    container: Option<String>,
}

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

/// Perform cleanup for a single relay entry.
///
/// Returns the action label on success (e.g., `"unmounted"`, `"removed"`).
fn clean_one(mount_point: &Path, status: &MountStatus) -> Result<&'static str, String> {
    match status {
        MountStatus::Active => {
            let mount_str = mount_point.to_string_lossy();
            let code = cmd::run_stream(
                "devcontainer",
                &["down", "--workspace-folder", mount_str.as_ref()],
            )
            .unwrap_or(exit_codes::PREREQ_NOT_FOUND);
            if code != 0 {
                return Err(format!("devcontainer down failed (exit {code})"));
            }
            do_unmount(mount_point)?;
            remove_mount_dir(mount_point)?;
            Ok("stopped, unmounted")
        }
        MountStatus::Orphaned => {
            do_unmount(mount_point)?;
            remove_mount_dir(mount_point)?;
            Ok("unmounted")
        }
        MountStatus::Stale => {
            do_unmount(mount_point)?;
            remove_mount_dir(mount_point)?;
            Ok("unmounted")
        }
        MountStatus::Empty => {
            remove_mount_dir(mount_point)?;
            Ok("removed")
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx clean`.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_clean(home: &Path, all: bool, yes: bool) -> i32 {
    // Install SIGINT handler. If Ctrl+C arrives while an unmount is in progress,
    // we finish that entry's cleanup then exit (remaining entries are skipped).
    let interrupted = signals::interrupted_flag();

    // 1. Validate Docker/Colima is available.
    if !docker::is_docker_available() {
        eprintln!("Docker is not available. Is Colima running?");
        return exit_codes::RUNTIME_ERROR;
    }

    // 2. Scan relay dir for dcx-* entries.
    progress::step("Scanning relay directory...");
    let relay = relay_dir(home);
    let entry_paths = scan_relay(&relay);

    // 3. Categorize each entry.
    let table = platform::read_mount_table().unwrap_or_default();
    let entries: Vec<EntryInfo> = entry_paths
        .iter()
        .map(|p| {
            let workspace = mount_table::find_mount_source(&table, p).map(str::to_string);
            let is_fuse_mounted = workspace.is_some();
            let is_accessible = p.metadata().is_ok();
            let container = query_container(p);
            let has_container = container.is_some();
            EntryInfo {
                path: p.clone(),
                workspace,
                status: categorize::categorize(is_fuse_mounted, is_accessible, has_container),
                container,
            }
        })
        .collect();

    // 4. Count active mounts.
    let active_count = entries
        .iter()
        .filter(|e| e.status == MountStatus::Active)
        .count();

    // 5. In --all mode, prompt if active containers found (unless --yes).
    if all && active_count > 0 && !yes {
        let active_entries: Vec<(String, String, String)> = entries
            .iter()
            .filter(|e| e.status == MountStatus::Active)
            .map(|e| {
                let ws = e
                    .workspace
                    .clone()
                    .unwrap_or_else(|| "(unknown)".to_string());
                let mount = e
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let container = e.container.clone().unwrap_or_else(|| "(none)".to_string());
                (ws, mount, container)
            })
            .collect();
        let prompt_text = confirm_prompt(&active_entries);
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

    // 6. Determine entries to clean.
    let to_clean: Vec<&EntryInfo> = if all {
        entries.iter().collect()
    } else {
        entries
            .iter()
            .filter(|e| e.status != MountStatus::Active)
            .collect()
    };

    let active_left = if all { 0 } else { active_count };

    // 7. If nothing to clean, exit early.
    if to_clean.is_empty() {
        println!("Nothing to clean.");
        return exit_codes::SUCCESS;
    }

    // 8. Process each entry, continuing on failure.
    let mut cleaned: Vec<CleanEntry> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for entry in &to_clean {
        let mount_name_str = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        progress::step(&format!("Cleaning {mount_name_str}..."));
        match clean_one(&entry.path, &entry.status) {
            Ok(action) => {
                cleaned.push(CleanEntry {
                    workspace: entry.workspace.clone(),
                    mount: mount_name_str,
                    was: was_label(&entry.status).to_string(),
                    action: action.to_string(),
                });
            }
            Err(e) => {
                failures.push(format!("{}: {e}", entry.path.display()));
            }
        }
        // If SIGINT arrived during this entry's cleanup, finish it (already done above)
        // and exit without processing remaining entries.
        if interrupted.load(Ordering::Relaxed) {
            eprintln!("Signal received, finishing current unmount...");
            break;
        }
    }

    // 9. Print summary.
    if !cleaned.is_empty() {
        println!("{}", format::format_clean_summary(&cleaned, active_left));
    }

    // 10. Print failures.
    for f in &failures {
        eprintln!("Error: {f}");
    }

    if failures.is_empty() {
        exit_codes::SUCCESS
    } else {
        exit_codes::RUNTIME_ERROR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- was_label ---

    #[test]
    fn was_label_active_is_running() {
        assert_eq!(was_label(&MountStatus::Active), "running");
    }

    #[test]
    fn was_label_orphaned_is_orphaned() {
        assert_eq!(was_label(&MountStatus::Orphaned), "orphaned");
    }

    #[test]
    fn was_label_stale_is_stale() {
        assert_eq!(was_label(&MountStatus::Stale), "stale");
    }

    #[test]
    fn was_label_empty_is_empty_dir() {
        assert_eq!(was_label(&MountStatus::Empty), "empty dir");
    }

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
