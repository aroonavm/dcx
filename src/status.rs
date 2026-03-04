#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use crate::categorize::{MountStatus, categorize};
use crate::docker;
use crate::exit_codes;
use crate::format::{StatusRow, format_status_table};
use crate::mount_table;
use crate::naming::{relay_dir, scan_relay};
use crate::platform;
use crate::progress;
use crate::up::staging_dir;

/// Check if sync daemon is running for a given mount point.
///
/// Returns `"running"` if the daemon PID file exists and the process is alive.
/// Returns `"stopped"` if the PID file doesn't exist or the process is dead.
/// Returns `"–"` if not applicable (mount not active).
fn daemon_status(mount_point: &Path, is_mounted: bool) -> String {
    if !is_mounted {
        return "–".to_string();
    }

    let staging = staging_dir(mount_point);
    let pid_file = staging.join(".sync-daemon.pid");

    // Try to read the PID file
    let pid_str = match std::fs::read_to_string(&pid_file) {
        Ok(content) => content,
        Err(_) => return "stopped".to_string(),
    };

    let pid = pid_str.trim();
    if pid.is_empty() {
        return "stopped".to_string();
    }

    // Check if the process is alive by sending signal 0 (doesn't kill, just checks)
    let output = Command::new("kill").arg("-0").arg(pid).output();

    match output {
        Ok(status) if status.status.success() => "running".to_string(),
        _ => "stopped".to_string(),
    }
}

/// Human-readable state label for a dcx mount entry.
///
/// `is_mounted` should be `is_fuse_mounted && is_accessible` (the caller's
/// combined health check). Delegates to `categorize` so label and categorization
/// logic cannot drift apart.
///
/// - Mounted and has a container → `"running"`
/// - Mounted but no container    → `"orphaned"`
/// - Not mounted                 → `"stale mount"`
pub fn mount_state_label(is_mounted: bool, has_container: bool) -> &'static str {
    match categorize(is_mounted, is_mounted, has_container) {
        MountStatus::Active => "running",
        MountStatus::Orphaned => "orphaned",
        MountStatus::Stale | MountStatus::Empty => "stale mount",
    }
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
            // Read network mode from container if it exists
            let network = container
                .as_ref()
                .and_then(|c| docker::read_network_mode(c));
            // Check sync daemon status
            let daemon = daemon_status(mount_point, is_mounted && is_accessible);
            StatusRow {
                workspace,
                mount,
                container,
                network,
                state: state.to_string(),
                daemon,
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

    // --- daemon_status ---

    #[test]
    fn daemon_status_not_mounted_returns_dash() {
        let mount = std::path::Path::new("/some/path");
        let status = daemon_status(mount, false);
        assert_eq!(status, "–");
    }

    #[test]
    fn daemon_status_missing_pid_file_returns_stopped() {
        // Create a staging dir without a .sync-daemon.pid file
        let staging_dir = tempfile::TempDir::new().unwrap();
        let status = daemon_status(staging_dir.path(), true);
        assert_eq!(status, "stopped");
    }

    #[test]
    fn daemon_status_empty_pid_file_returns_stopped() {
        // Create a staging dir with an empty or whitespace-only PID file
        let staging_dir = tempfile::TempDir::new().unwrap();
        let pid_file = staging_dir.path().join(".sync-daemon.pid");
        std::fs::write(&pid_file, b"   \n").unwrap();
        let status = daemon_status(staging_dir.path(), true);
        assert_eq!(status, "stopped");
    }

    #[test]
    fn daemon_status_invalid_pid_file_returns_stopped() {
        // PID file exists but contains non-numeric garbage
        let staging_dir = tempfile::TempDir::new().unwrap();
        let pid_file = staging_dir.path().join(".sync-daemon.pid");
        std::fs::write(&pid_file, b"not-a-pid").unwrap();
        let status = daemon_status(staging_dir.path(), true);
        // Since `kill -0 not-a-pid` will fail (not a valid PID),
        // the status should be "stopped"
        assert_eq!(status, "stopped");
    }

    #[test]
    fn daemon_status_nonexistent_pid_returns_stopped() {
        // PID file contains a PID that doesn't exist
        let staging_dir = tempfile::TempDir::new().unwrap();
        let pid_file = staging_dir.path().join(".sync-daemon.pid");
        // Use a very high PID that's unlikely to exist
        std::fs::write(&pid_file, b"999999999").unwrap();
        let status = daemon_status(staging_dir.path(), true);
        assert_eq!(status, "stopped");
    }

    #[test]
    fn daemon_status_with_trimmed_whitespace_returns_stopped() {
        // Test that PID is correctly trimmed from file before being used
        let staging_dir = tempfile::TempDir::new().unwrap();
        let pid_file = staging_dir.path().join(".sync-daemon.pid");
        // Write PID with surrounding whitespace (like from file read)
        std::fs::write(&pid_file, b"  999999999  \n").unwrap();
        let status = daemon_status(staging_dir.path(), true);
        // PID 999999999 doesn't exist, so it should be "stopped"
        assert_eq!(status, "stopped");
    }
}
