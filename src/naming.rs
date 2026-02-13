#![allow(dead_code)]

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Sanitize a path component: replace non-alphanumeric chars with `-`, max 30 chars.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(30)
        .collect()
}

/// Compute SHA256 of `abs_path` and return the first 8 hex characters.
pub fn compute_hash(abs_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(abs_path.as_bytes());
    let result = hasher.finalize();
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    hex[..8].to_string()
}

/// Compute the dcx mount name for an absolute path: `dcx-<name>-<hash>`.
pub fn mount_name(abs_path: &Path) -> String {
    let name = abs_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sanitized = sanitize_name(&name);
    let hash = compute_hash(&abs_path.to_string_lossy());
    format!("dcx-{sanitized}-{hash}")
}

/// Return the relay directory: `<home>/.colima-mounts`.
pub fn relay_dir(home: &Path) -> PathBuf {
    home.join(".colima-mounts")
}

/// Return true if `path` is inside a dcx-managed mount (`<relay>/dcx-*`).
pub fn is_dcx_managed_path(path: &Path, relay: &Path) -> bool {
    if let Ok(rel) = path.strip_prefix(relay)
        && let Some(std::path::Component::Normal(name)) = rel.components().next()
    {
        return name
            .to_str()
            .map(|s| s.starts_with("dcx-"))
            .unwrap_or(false);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_alphanumeric_unchanged() {
        assert_eq!(sanitize_name("myproject"), "myproject");
    }

    #[test]
    fn sanitize_hyphen_stays_hyphen() {
        assert_eq!(sanitize_name("my-project"), "my-project");
    }

    #[test]
    fn sanitize_dot_to_hyphen() {
        assert_eq!(sanitize_name("my.project"), "my-project");
    }

    #[test]
    fn sanitize_underscore_to_hyphen() {
        assert_eq!(sanitize_name("my_project"), "my-project");
    }

    #[test]
    fn sanitize_space_to_hyphen() {
        assert_eq!(sanitize_name("my project"), "my-project");
    }

    #[test]
    fn sanitize_unicode_non_ascii_to_hyphen() {
        assert_eq!(sanitize_name("héllo"), "h-llo");
    }

    #[test]
    fn sanitize_truncates_at_30_chars() {
        let long = "a".repeat(40);
        let result = sanitize_name(&long);
        assert_eq!(result.len(), 30);
        assert!(result.chars().all(|c| c == 'a'));
    }

    #[test]
    fn sanitize_empty_returns_empty() {
        assert_eq!(sanitize_name(""), "");
    }

    #[test]
    fn hash_is_8_lowercase_hex_chars() {
        let h = compute_hash("/home/user/myproject");
        assert_eq!(h.len(), 8);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(
            compute_hash("/home/user/myproject"),
            compute_hash("/home/user/myproject")
        );
    }

    #[test]
    fn hash_differs_for_different_paths() {
        assert_ne!(
            compute_hash("/home/user/project-a"),
            compute_hash("/home/user/project-b")
        );
    }

    #[test]
    fn mount_name_has_dcx_prefix_and_hash_suffix() {
        let path = Path::new("/home/user/myproject");
        let name = mount_name(path);
        assert!(name.starts_with("dcx-myproject-"), "got: {name}");
        // format: dcx-<name>-<8 hex chars>
        let suffix = name.trim_start_matches("dcx-myproject-");
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn mount_name_sanitizes_last_component() {
        let path = Path::new("/home/user/my.project");
        let name = mount_name(path);
        assert!(name.starts_with("dcx-my-project-"), "got: {name}");
    }

    #[test]
    fn mount_name_is_deterministic() {
        let path = Path::new("/home/user/myproject");
        assert_eq!(mount_name(path), mount_name(path));
    }

    #[test]
    fn relay_dir_appends_colima_mounts() {
        let home = Path::new("/home/user");
        assert_eq!(relay_dir(home), PathBuf::from("/home/user/.colima-mounts"));
    }

    #[test]
    fn is_dcx_managed_true_for_dcx_subdir() {
        let relay = Path::new("/home/user/.colima-mounts");
        let path = Path::new("/home/user/.colima-mounts/dcx-foo-a1b2c3d4");
        assert!(is_dcx_managed_path(path, relay));
    }

    #[test]
    fn is_dcx_managed_false_for_non_dcx_subdir() {
        let relay = Path::new("/home/user/.colima-mounts");
        let path = Path::new("/home/user/.colima-mounts/foo");
        assert!(!is_dcx_managed_path(path, relay));
    }

    #[test]
    fn is_dcx_managed_false_for_path_outside_relay() {
        let relay = Path::new("/home/user/.colima-mounts");
        let path = Path::new("/home/user/something");
        assert!(!is_dcx_managed_path(path, relay));
    }

    #[test]
    fn is_dcx_managed_false_for_relay_itself() {
        let relay = Path::new("/home/user/.colima-mounts");
        assert!(!is_dcx_managed_path(relay, relay));
    }
}
