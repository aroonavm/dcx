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
    fn sanitize_digits_unchanged() {
        assert_eq!(sanitize_name("proj123"), "proj123");
    }

    #[test]
    fn sanitize_hyphen_stays_hyphen() {
        assert_eq!(sanitize_name("my-project"), "my-project");
    }

    #[test]
    fn sanitize_non_alphanumeric_to_hyphen() {
        // Non-alphanumeric characters (dot, underscore, space, unicode) all become hyphens.
        assert_eq!(sanitize_name("my.project"), "my-project");
        assert_eq!(sanitize_name("my_project"), "my-project");
        assert_eq!(sanitize_name("my project"), "my-project");
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
    fn sanitize_preserves_exactly_30_chars() {
        let exactly_30 = "a".repeat(30);
        assert_eq!(sanitize_name(&exactly_30), exactly_30);
    }

    #[test]
    fn sanitize_empty_returns_empty() {
        assert_eq!(sanitize_name(""), "");
    }

    #[test]
    fn hash_is_8_lowercase_hex_chars() {
        let h = compute_hash("/home/user/myproject");
        assert_eq!(h.len(), 8);
        assert!(
            h.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
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
    fn hash_known_value() {
        // SHA256 of "/home/user/myproject" → first 8 hex chars.
        // Pins the hashing algorithm and encoding against silent regression.
        assert_eq!(compute_hash("/home/user/myproject"), "f227ecb4");
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
    fn mount_name_truncates_long_last_component() {
        // Last component is 40 chars; sanitized name must be capped at 30.
        let path = Path::new("/home/user/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let name = mount_name(path);
        // format: dcx-<≤30 chars>-<8 hex chars>
        let inner = name
            .strip_prefix("dcx-")
            .unwrap()
            .strip_suffix(&name[name.len() - 9..])
            .unwrap_or("");
        let _ = inner; // length check via total length
        // "dcx-" (4) + 30 + "-" (1) + 8 = 43
        assert_eq!(name.len(), 43, "got: {name}");
    }

    #[test]
    fn mount_name_root_path_produces_double_dash() {
        // Path::new("/").file_name() returns None → sanitized name is "".
        // Documents the known edge case: produces "dcx--<hash>".
        let name = mount_name(Path::new("/"));
        assert!(name.starts_with("dcx--"), "got: {name}");
    }

    #[test]
    fn mount_name_known_full_output() {
        // Pins the complete mount name format end-to-end.
        // SHA256("/home/user/myproject")[..8] == "f227ecb4" (verified by hash_known_value test).
        let path = Path::new("/home/user/myproject");
        assert_eq!(mount_name(path), "dcx-myproject-f227ecb4");
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
    fn is_dcx_managed_true_for_path_inside_dcx_subdir() {
        // A file/dir nested inside a dcx mount must also be detected.
        // Spec: "if workspace path starts with ~/.colima-mounts/dcx-"
        let relay = Path::new("/home/user/.colima-mounts");
        let path = Path::new("/home/user/.colima-mounts/dcx-foo-a1b2c3d4/subdir");
        assert!(is_dcx_managed_path(path, relay));
    }

    #[test]
    fn is_dcx_managed_false_for_relay_itself() {
        let relay = Path::new("/home/user/.colima-mounts");
        assert!(!is_dcx_managed_path(relay, relay));
    }
}
