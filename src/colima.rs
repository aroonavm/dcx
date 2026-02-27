//! Pure functions for parsing and filtering Colima mounts from colima.yaml.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A mount entry from colima.yaml.
#[derive(Debug, Clone, PartialEq)]
pub struct ColimaMount {
    pub location: String,
    pub writable: bool,
}

/// Private serde type for deserialization only.
#[derive(Deserialize)]
struct ColimaConfig {
    #[serde(default)]
    mounts: Vec<ColimaMountRaw>,
}

/// Private serde type for deserialization only.
#[derive(Deserialize)]
struct ColimaMountRaw {
    location: String,
    #[serde(default)]
    writable: bool,
}

/// Returns the platform-specific path to the colima config file.
/// Linux: `home/.config/colima/default/colima.yaml`
/// macOS: `home/.colima/default/colima.yaml`
pub fn colima_config_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join(".colima/default/colima.yaml")
    }
    #[cfg(not(target_os = "macos"))]
    {
        home.join(".config/colima/default/colima.yaml")
    }
}

/// Parses colima.yaml and extracts mount entries.
/// Returns empty vec on malformed YAML or missing mounts key.
pub fn parse_colima_mounts(yaml: &str) -> Vec<ColimaMount> {
    match serde_yaml::from_str::<ColimaConfig>(yaml) {
        Ok(config) => config
            .mounts
            .into_iter()
            .map(|m| ColimaMount {
                location: m.location,
                writable: m.writable,
            })
            .collect(),
        Err(_) => vec![],
    }
}

/// Filters out ~/.colima-mounts entries (and trailing-slash variants).
pub fn filter_relay_mounts(mounts: Vec<ColimaMount>) -> Vec<ColimaMount> {
    mounts
        .into_iter()
        .filter(|m| {
            let normalized = m.location.trim_end_matches('/');
            normalized != "~/.colima-mounts"
        })
        .collect()
}

/// Expands ~ in a location string using the provided home path.
/// Example: expand_tilde("~/.claude", "/home/user") → "/home/user/.claude"
pub fn expand_tilde(location: &str, home: &Path) -> PathBuf {
    if let Some(stripped) = location.strip_prefix("~/") {
        home.join(stripped)
    } else if location == "~" {
        home.to_path_buf()
    } else {
        PathBuf::from(location)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colima_config_path_linux() {
        #[cfg(not(target_os = "macos"))]
        {
            let home = Path::new("/home/user");
            let path = colima_config_path(home);
            assert_eq!(
                path,
                PathBuf::from("/home/user/.config/colima/default/colima.yaml")
            );
        }
    }

    #[test]
    fn test_colima_config_path_macos() {
        #[cfg(target_os = "macos")]
        {
            let home = Path::new("/Users/user");
            let path = colima_config_path(home);
            assert_eq!(
                path,
                PathBuf::from("/Users/user/.colima/default/colima.yaml")
            );
        }
    }

    #[test]
    fn test_parse_colima_mounts_standard() {
        let yaml = r#"
mounts:
  - location: ~/.claude
    writable: true
  - location: ~/.gitconfig
    writable: false
"#;
        let mounts = parse_colima_mounts(yaml);
        assert_eq!(mounts.len(), 2);
        assert_eq!(
            mounts[0],
            ColimaMount {
                location: "~/.claude".to_string(),
                writable: true,
            }
        );
        assert_eq!(
            mounts[1],
            ColimaMount {
                location: "~/.gitconfig".to_string(),
                writable: false,
            }
        );
    }

    #[test]
    fn test_parse_colima_mounts_writable_defaults_false() {
        let yaml = r#"
mounts:
  - location: ~/.gitconfig
"#;
        let mounts = parse_colima_mounts(yaml);
        assert_eq!(mounts.len(), 1);
        assert!(!mounts[0].writable);
    }

    #[test]
    fn test_parse_colima_mounts_empty_file() {
        let yaml = "";
        let mounts = parse_colima_mounts(yaml);
        assert_eq!(mounts.len(), 0);
    }

    #[test]
    fn test_parse_colima_mounts_missing_mounts_key() {
        let yaml = "someOtherKey: value";
        let mounts = parse_colima_mounts(yaml);
        assert_eq!(mounts.len(), 0);
    }

    #[test]
    fn test_parse_colima_mounts_malformed_yaml() {
        let yaml = "{ invalid yaml content";
        let mounts = parse_colima_mounts(yaml);
        assert_eq!(mounts.len(), 0);
    }

    #[test]
    fn test_filter_relay_mounts_removes_colima_mounts() {
        let mounts = vec![
            ColimaMount {
                location: "~/.claude".to_string(),
                writable: true,
            },
            ColimaMount {
                location: "~/.colima-mounts".to_string(),
                writable: true,
            },
            ColimaMount {
                location: "~/.gitconfig".to_string(),
                writable: false,
            },
        ];
        let filtered = filter_relay_mounts(mounts);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].location, "~/.claude");
        assert_eq!(filtered[1].location, "~/.gitconfig");
    }

    #[test]
    fn test_filter_relay_mounts_removes_trailing_slash_variant() {
        let mounts = vec![ColimaMount {
            location: "~/.colima-mounts/".to_string(),
            writable: true,
        }];
        let filtered = filter_relay_mounts(mounts);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_expand_tilde_with_slash() {
        let home = Path::new("/home/user");
        let path = expand_tilde("~/.claude", home);
        assert_eq!(path, PathBuf::from("/home/user/.claude"));
    }

    #[test]
    fn test_expand_tilde_alone() {
        let home = Path::new("/home/user");
        let path = expand_tilde("~", home);
        assert_eq!(path, PathBuf::from("/home/user"));
    }

    #[test]
    fn test_expand_tilde_absolute_path_unchanged() {
        let home = Path::new("/home/user");
        let path = expand_tilde("/etc/config", home);
        assert_eq!(path, PathBuf::from("/etc/config"));
    }

    #[test]
    fn test_expand_tilde_relative_path_unchanged() {
        let home = Path::new("/home/user");
        let path = expand_tilde("./config", home);
        assert_eq!(path, PathBuf::from("./config"));
    }
}
