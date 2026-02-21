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

    #[cfg(target_os = "linux")]
    #[test]
    fn unmount_prog_linux_is_fusermount() {
        assert_eq!(unmount_prog(), "fusermount");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unmount_args_linux_is_fusermount_dash_u_mount_point() {
        let args = unmount_args(Path::new("/tmp/test"));
        assert_eq!(args, vec!["-u", "/tmp/test"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unmount_prog_macos_is_umount() {
        assert_eq!(unmount_prog(), "umount");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unmount_args_macos_has_one_element() {
        let args = unmount_args(Path::new("/tmp/test"));
        assert_eq!(args.len(), 1);
    }
}
