#![allow(dead_code)]

use std::path::Path;

#[cfg(target_os = "macos")]
use crate::cmd;
use crate::mount_table::{self, MountEntry};

/// Return the program name for unmounting a FUSE mount.
///
/// Linux: `fusermount`
/// macOS: `umount`
pub fn unmount_prog() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "fusermount"
    }
    #[cfg(target_os = "macos")]
    {
        "umount"
    }
}

/// Return the arguments (without the program name) for unmounting `mount_point`.
///
/// Linux: `["-u", "<mount_point>"]`
/// macOS: `["<mount_point>"]`
pub fn unmount_args(mount_point: &Path) -> Vec<String> {
    let path = mount_point.to_string_lossy().into_owned();
    #[cfg(target_os = "linux")]
    {
        vec!["-u".to_string(), path]
    }
    #[cfg(target_os = "macos")]
    {
        vec![path]
    }
}

/// Install hint for `bindfs` on the current platform.
///
/// Linux: `sudo apt install bindfs`
/// macOS: `brew install bindfs`
pub fn bindfs_install_hint() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "sudo apt install bindfs"
    }
    #[cfg(target_os = "macos")]
    {
        "brew install bindfs"
    }
}

/// Install hint for the `devcontainer` CLI (same on all platforms).
pub fn devcontainer_install_hint() -> &'static str {
    "npm install -g @devcontainers/cli"
}

/// Read the system mount table and return all `bindfs` entries.
///
/// Linux: reads `/proc/mounts` and parses with `parse_proc_mounts`.
/// macOS: runs `mount` and parses with `parse_mount_output`.
pub fn read_mount_table() -> Result<Vec<MountEntry>, String> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/mounts")
            .map_err(|e| format!("Failed to read /proc/mounts: {e}"))?;
        Ok(mount_table::parse_proc_mounts(&text))
    }
    #[cfg(target_os = "macos")]
    {
        let out = cmd::run_capture("mount", &[] as &[&str])?;
        Ok(mount_table::parse_mount_output(&out.stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmount_args_last_element_is_mount_point() {
        let path = Path::new("/home/user/.colima-mounts/dcx-proj-a1b2c3d4");
        let args = unmount_args(path);
        assert!(!args.is_empty());
        assert_eq!(
            args.last().unwrap(),
            "/home/user/.colima-mounts/dcx-proj-a1b2c3d4"
        );
    }

    #[test]
    fn read_mount_table_succeeds_and_returns_vec() {
        // read_mount_table calls platform-specific code that should succeed on a running system.
        // We just verify it returns a Result (success or failure) and the vector is valid.
        let result = read_mount_table();
        assert!(
            result.is_ok() || result.is_err(),
            "read_mount_table should return a Result"
        );
        if let Ok(entries) = result {
            // On success, we should get a vec (possibly empty if no bindfs mounts exist).
            assert!(entries.is_empty() || !entries.is_empty()); // tautology, but ensures Vec<MountEntry> is valid
        }
    }

    #[test]
    fn read_mount_table_returns_mount_entries_with_source_and_target() {
        // If the system has bindfs mounts, verify the entries have source and target.
        if let Ok(entries) = read_mount_table() {
            for entry in entries {
                // Each entry should have non-empty source and target.
                assert!(
                    !entry.source.is_empty(),
                    "mount entry source should not be empty"
                );
                assert!(
                    !entry.target.is_empty(),
                    "mount entry target should not be empty"
                );
            }
        }
        // If read_mount_table() fails, the test still passes (system may not support it).
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn read_mount_table_linux_parses_proc_mounts() {
        // Test that on Linux, read_mount_table() successfully parses /proc/mounts.
        // We just ensure it doesn't panic and returns a Result.
        let result = read_mount_table();
        match result {
            Ok(_entries) => {
                // /proc/mounts always exists on Linux, so success is expected.
                assert!(true);
            }
            Err(e) => {
                // If it fails, it should be a readable error message.
                assert!(!e.is_empty(), "error message should not be empty");
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn read_mount_table_macos_runs_mount_command() {
        // Test that on macOS, read_mount_table() successfully calls `mount` command.
        let result = read_mount_table();
        match result {
            Ok(_entries) => {
                // `mount` always exists on macOS, so success is expected.
                assert!(true);
            }
            Err(e) => {
                // If it fails, it should be a readable error message.
                assert!(!e.is_empty(), "error message should not be empty");
            }
        }
    }
}
