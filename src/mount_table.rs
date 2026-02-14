#![allow(dead_code)]

use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct MountEntry {
    pub source: String,
    pub target: String,
}

/// Unescape octal sequences in a `/proc/mounts` field.
///
/// `/proc/mounts` encodes special characters as `\NNN` (three octal digits),
/// e.g. `\040` for space. This function decodes them back to their byte values.
fn unescape_proc_field(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1].wrapping_sub(b'0') < 8
            && bytes[i + 2].wrapping_sub(b'0') < 8
            && bytes[i + 3].wrapping_sub(b'0') < 8
        {
            let val =
                (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
            result.push(val);
            i += 4;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Parse `/proc/mounts` text (Linux) and return only `fuse.bindfs` or `fuse` entries (dcx mounts).
///
/// Format per line: `<source> <target> <fstype> <options> <dump> <pass>`
/// Special characters in paths are octal-escaped (e.g. `\040` for space).
/// Accepts both `fuse.bindfs` (normal case) and `fuse` (stale/orphaned mounts).
pub fn parse_proc_mounts(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let source = parts.next()?;
            let target = parts.next()?;
            let fstype = parts.next()?;
            // Accept both fuse.bindfs (normal dcx mounts) and fuse (stale mounts after interruption)
            if fstype == "fuse.bindfs" || fstype == "fuse" {
                Some(MountEntry {
                    source: unescape_proc_field(source),
                    target: unescape_proc_field(target),
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
    fn proc_mounts_ignores_non_fuse_lines() {
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
    fn proc_mounts_truncated_octal_escape_treated_as_literal() {
        // A backslash not followed by exactly 3 octal digits is passed through as-is.
        let text =
            "/home/user/bad\\04x /home/user/.colima-mounts/dcx-bad-abc12345 fuse.bindfs rw 0 0";
        let entries = parse_proc_mounts(text);
        assert_eq!(entries.len(), 1);
        // The '\' is kept literally since "04x" is not valid octal.
        assert_eq!(entries[0].source, "/home/user/bad\\04x");
    }

    #[test]
    fn proc_mounts_unescapes_spaces_in_paths() {
        // /proc/mounts encodes spaces as \040.
        let text = "/home/user/my\\040project \
                    /home/user/.colima-mounts/dcx-my-project-abc12345 \
                    fuse.bindfs rw 0 0";
        let entries = parse_proc_mounts(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "/home/user/my project");
        assert_eq!(
            entries[0].target,
            "/home/user/.colima-mounts/dcx-my-project-abc12345"
        );
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

    #[test]
    fn proc_mounts_accepts_fuse_type() {
        // Stale/orphaned mounts may show as just "fuse" instead of "fuse.bindfs"
        let text = "/home/user/proj /home/user/.colima-mounts/dcx-proj-abc12345 fuse rw,nosuid,nodev,relatime,user_id=1000,group_id=992,default_permissions 0 0";
        let entries = parse_proc_mounts(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "/home/user/proj");
        assert_eq!(
            entries[0].target,
            "/home/user/.colima-mounts/dcx-proj-abc12345"
        );
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
    fn mount_output_returns_multiple_bindfs_entries() {
        let text = "/Users/user/proj-a on /Users/user/.colima-mounts/dcx-proj-a-aaa11111 (bindfs, local)\n\
                    /Users/user/proj-b on /Users/user/.colima-mounts/dcx-proj-b-bbb22222 (bindfs, local)";
        let entries = parse_mount_output(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source, "/Users/user/proj-a");
        assert_eq!(entries[1].source, "/Users/user/proj-b");
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
    fn find_mount_source_returns_correct_when_not_first() {
        let entries = vec![
            MountEntry {
                source: "/home/user/proj-a".to_string(),
                target: "/home/user/.colima-mounts/dcx-proj-a-aaa11111".to_string(),
            },
            MountEntry {
                source: "/home/user/proj-b".to_string(),
                target: "/home/user/.colima-mounts/dcx-proj-b-bbb22222".to_string(),
            },
        ];
        let target = Path::new("/home/user/.colima-mounts/dcx-proj-b-bbb22222");
        assert_eq!(
            find_mount_source(&entries, target),
            Some("/home/user/proj-b")
        );
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
