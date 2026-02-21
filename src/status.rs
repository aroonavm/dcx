#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crate::docker;
use crate::exit_codes;
use crate::format::{StatusRow, format_status_table};
use crate::mount_table;
use crate::naming::relay_dir;
use crate::platform;
use crate::progress;

/// Human-readable state label for a dcx mount entry.
///
/// - Mounted and has a container → `"running"`
/// - Mounted but no container    → `"orphaned"`
/// - Not mounted                 → `"stale mount"`
pub fn mount_state_label(is_mounted: bool, has_container: bool) -> &'static str {
    match (is_mounted, has_container) {
        (true, true) => "running",
        (true, false) => "orphaned",
        (false, _) => "stale mount",
    }
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
            let name_str = name.to_string_lossy();
            if name_str.starts_with("dcx-") {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    dirs.sort();
    dirs
}

/// Scan all dcx-managed mounts, query their state, and print the status table.
///
/// Returns `exit_codes::SUCCESS` (0) on success, `exit_codes::RUNTIME_ERROR` (1) if Docker
/// is not available.
pub fn run_status(home: &Path) -> i32 {
    if !docker::is_docker_available() {
        eprintln!("Docker is not available. Is Colima running?");
        return exit_codes::RUNTIME_ERROR;
    }

    progress::step("Scanning workspaces...");
    let relay = relay_dir(home);
    let mounts = scan_relay(&relay);

    if mounts.is_empty() {
        println!("No active workspaces.");
        return exit_codes::SUCCESS;
    }

    let mount_table = platform::read_mount_table().unwrap_or_default();

    let rows: Vec<StatusRow> = mounts
        .iter()
        .map(|mount_point| {
            let workspace =
                mount_table::find_mount_source(&mount_table, mount_point).map(str::to_string);
            let is_mounted = workspace.is_some();
            let is_accessible = mount_point.metadata().is_ok();
            let container = docker::query_container(mount_point);
            let has_container = container.is_some();
            let state = mount_state_label(is_mounted && is_accessible, has_container);
            let mount = mount_point
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            StatusRow {
                workspace,
                mount,
                container,
                state: state.to_string(),
            }
        })
        .collect();

    let output = format_status_table(&rows);
    println!("{output}");
    exit_codes::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- mount_state_label ---

    #[test]
    fn label_running_when_mounted_with_container() {
        assert_eq!(mount_state_label(true, true), "running");
    }

    #[test]
    fn label_orphaned_when_mounted_no_container() {
        assert_eq!(mount_state_label(true, false), "orphaned");
    }

    #[test]
    fn label_stale_when_not_mounted() {
        assert_eq!(mount_state_label(false, false), "stale mount");
    }

    #[test]
    fn label_stale_ignores_container_flag() {
        // When not mounted, the has_container flag is irrelevant — always "stale mount".
        assert_eq!(mount_state_label(false, true), "stale mount");
    }
}
