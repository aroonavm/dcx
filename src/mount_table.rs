#![allow(dead_code)]

use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct MountEntry {
    pub source: String,
    pub target: String,
}

/// Parse `/proc/mounts` text (Linux) and return only `fuse.bindfs` entries.
///
/// Format per line: `<source> <target> <fstype> <options> <dump> <pass>`
pub fn parse_proc_mounts(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let source = parts.next()?;
            let target = parts.next()?;
            let fstype = parts.next()?;
            if fstype == "fuse.bindfs" {
                Some(MountEntry {
                    source: source.to_string(),
                    target: target.to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Parse `mount` command output (macOS) and return only `bindfs` entries.
///
/// Format per line: `<source> on <target> (<fstype>, ...)`
pub fn parse_mount_output(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            // Split on " on " to separate source from the rest.
            let (source, rest) = line.split_once(" on ")?;
            // Target is everything before " (".
            let target = rest.split_once(" (")?.0;
            // Options are inside the parentheses; first comma-delimited token is fstype.
            let opts = rest.split_once('(')?.1;
            let fstype = opts.split(',').next()?.trim();
            if fstype == "bindfs" {
                Some(MountEntry {
                    source: source.trim().to_string(),
                    target: target.trim().to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Return the source path for the given mount point, or `None` if not found.
pub fn find_mount_source<'a>(entries: &'a [MountEntry], target: &Path) -> Option<&'a str> {
    let target_str = target.to_str()?;
    entries
        .iter()
        .find(|e| e.target == target_str)
        .map(|e| e.source.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_proc_mounts ---

    #[test]
    fn proc_mounts_empty_input() {
        assert_eq!(parse_proc_mounts(""), vec![]);
    }

    #[test]
    fn proc_mounts_ignores_non_bindfs_lines() {
        let text = "sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0\n\
                    proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0\n\
                    tmpfs /tmp tmpfs rw 0 0";
        assert_eq!(parse_proc_mounts(text), vec![]);
    }

    #[test]
    fn proc_mounts_parses_fuse_bindfs_entry() {
        let text = "/home/user/myproject \
                    /home/user/.colima-mounts/dcx-myproject-a1b2c3d4 \
                    fuse.bindfs rw,nosuid,nodev 0 0";
        let entries = parse_proc_mounts(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "/home/user/myproject");
        assert_eq!(
            entries[0].target,
            "/home/user/.colima-mounts/dcx-myproject-a1b2c3d4"
        );
    }

    #[test]
    fn proc_mounts_filters_mixed_entries() {
        let text = "sysfs /sys sysfs rw 0 0\n\
                    /home/user/proj /home/user/.colima-mounts/dcx-proj-abc12345 fuse.bindfs rw 0 0\n\
                    tmpfs /tmp tmpfs rw 0 0";
        let entries = parse_proc_mounts(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "/home/user/proj");
    }

    #[test]
    fn proc_mounts_returns_multiple_bindfs_entries() {
        let text = "/home/user/proj-a /home/user/.colima-mounts/dcx-proj-a-aaa11111 fuse.bindfs rw 0 0\n\
                    /home/user/proj-b /home/user/.colima-mounts/dcx-proj-b-bbb22222 fuse.bindfs rw 0 0";
        let entries = parse_proc_mounts(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source, "/home/user/proj-a");
        assert_eq!(entries[1].source, "/home/user/proj-b");
    }

    // --- parse_mount_output (macOS) ---

    #[test]
    fn mount_output_empty_input() {
        assert_eq!(parse_mount_output(""), vec![]);
    }

    #[test]
    fn mount_output_ignores_non_bindfs_lines() {
        let text = "/dev/disk1s1 on / (apfs, local, journaled)\n\
                    devfs on /dev (devfs, local, nobrowse)";
        assert_eq!(parse_mount_output(text), vec![]);
    }

    #[test]
    fn mount_output_parses_bindfs_entry() {
        let text = "/Users/user/myproject on \
                    /Users/user/.colima-mounts/dcx-myproject-a1b2c3d4 \
                    (bindfs, local, nodev, nosuid)";
        let entries = parse_mount_output(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "/Users/user/myproject");
        assert_eq!(
            entries[0].target,
            "/Users/user/.colima-mounts/dcx-myproject-a1b2c3d4"
        );
    }

    #[test]
    fn mount_output_filters_mixed_entries() {
        let text = "/dev/disk1s1 on / (apfs, local, journaled)\n\
                    /Users/user/proj on /Users/user/.colima-mounts/dcx-proj-abc12345 (bindfs, local)\n\
                    devfs on /dev (devfs, local)";
        let entries = parse_mount_output(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "/Users/user/proj");
    }

    // --- find_mount_source ---

    #[test]
    fn find_mount_source_returns_source_when_found() {
        let entries = vec![MountEntry {
            source: "/home/user/proj".to_string(),
            target: "/home/user/.colima-mounts/dcx-proj-abc12345".to_string(),
        }];
        let target = Path::new("/home/user/.colima-mounts/dcx-proj-abc12345");
        assert_eq!(find_mount_source(&entries, target), Some("/home/user/proj"));
    }

    #[test]
    fn find_mount_source_returns_none_when_not_found() {
        let entries: Vec<MountEntry> = vec![];
        let target = Path::new("/home/user/.colima-mounts/dcx-proj-abc12345");
        assert_eq!(find_mount_source(&entries, target), None);
    }

    #[test]
    fn find_mount_source_returns_none_for_wrong_target() {
        let entries = vec![MountEntry {
            source: "/home/user/proj".to_string(),
            target: "/home/user/.colima-mounts/dcx-other-xyz98765".to_string(),
        }];
        let target = Path::new("/home/user/.colima-mounts/dcx-proj-abc12345");
        assert_eq!(find_mount_source(&entries, target), None);
    }
}
