use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── Serde types ───────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct DcxFileRaw {
    path: String,
}

#[derive(Deserialize, Default)]
struct DcxUpConfigRaw {
    #[serde(default)]
    network: Option<String>,

    #[serde(default)]
    yes: Option<bool>,

    #[serde(default)]
    files: Vec<DcxFileRaw>,
}

#[derive(Deserialize, Default)]
struct DcxConfigRaw {
    #[serde(default)]
    up: DcxUpConfigRaw,
}

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Default)]
pub struct DcxUpConfig {
    /// Network isolation level (validated at call site). None means not set.
    pub network: Option<String>,

    /// Skip confirmation prompts. None means not set.
    pub yes: Option<bool>,

    /// Raw file location strings (may contain `~`). Expansion happens at call site.
    pub files: Vec<String>,
}

#[derive(Debug, PartialEq, Default)]
pub struct DcxConfig {
    pub up: DcxUpConfig,
}

// ── Public functions ──────────────────────────────────────────────────────────

/// Parse a dcx_config.yaml string into a DcxConfig.
/// Returns an empty DcxConfig on any parse error.
pub fn parse_dcx_config(yaml: &str) -> DcxConfig {
    match serde_yaml::from_str::<DcxConfigRaw>(yaml) {
        Ok(raw) => DcxConfig {
            up: DcxUpConfig {
                network: raw.up.network,
                yes: raw.up.yes,
                files: raw.up.files.into_iter().map(|f| f.path).collect(),
            },
        },
        Err(_) => DcxConfig::default(),
    }
}

/// Read and parse a dcx_config.yaml file.
/// Returns an empty DcxConfig if the file is missing or malformed.
pub fn read_dcx_config(path: &Path) -> DcxConfig {
    match std::fs::read_to_string(path) {
        Ok(content) => parse_dcx_config(&content),
        Err(_) => DcxConfig::default(),
    }
}

/// Find dcx_config.yaml in a workspace directory.
/// Checks .devcontainer/dcx_config.yaml then dcx_config.yaml at root.
pub fn find_dcx_config(workspace: &Path) -> Option<PathBuf> {
    let nested = workspace.join(".devcontainer").join("dcx_config.yaml");
    if nested.exists() {
        return Some(nested);
    }
    let top = workspace.join("dcx_config.yaml");
    if top.exists() {
        return Some(top);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_dcx_config ---

    #[test]
    fn parse_dcx_config_reads_up_files_list() {
        let yaml = "up:\n  files:\n    - path: ~/.gitconfig\n    - path: ~/.claude.json\n";
        let cfg = parse_dcx_config(yaml);
        assert_eq!(cfg.up.files, vec!["~/.gitconfig", "~/.claude.json"]);
    }

    #[test]
    fn parse_dcx_config_empty_yaml_returns_empty() {
        let cfg = parse_dcx_config("");
        assert!(cfg.up.files.is_empty());
    }

    #[test]
    fn parse_dcx_config_missing_up_section_defaults_to_empty() {
        let yaml = "image: ubuntu\n";
        let cfg = parse_dcx_config(yaml);
        assert!(cfg.up.files.is_empty());
    }

    #[test]
    fn parse_dcx_config_malformed_yaml_returns_empty() {
        let yaml = "up: [invalid: yaml: here\n";
        let cfg = parse_dcx_config(yaml);
        assert!(cfg.up.files.is_empty());
    }

    #[test]
    fn parse_dcx_config_tilde_paths_preserved_as_is() {
        let yaml = "up:\n  files:\n    - path: ~/.gitconfig\n";
        let cfg = parse_dcx_config(yaml);
        // Tilde expansion is the caller's responsibility
        assert_eq!(cfg.up.files[0], "~/.gitconfig");
    }

    #[test]
    fn parse_dcx_config_reads_up_network() {
        let yaml = "up:\n  network: open\n";
        let cfg = parse_dcx_config(yaml);
        assert_eq!(cfg.up.network, Some("open".to_string()));
    }

    #[test]
    fn parse_dcx_config_reads_up_yes() {
        let yaml = "up:\n  yes: true\n";
        let cfg = parse_dcx_config(yaml);
        assert_eq!(cfg.up.yes, Some(true));
    }

    #[test]
    fn parse_dcx_config_unknown_network_value_preserved_as_string() {
        let yaml = "up:\n  network: invalid_mode\n";
        let cfg = parse_dcx_config(yaml);
        assert_eq!(cfg.up.network, Some("invalid_mode".to_string()));
    }

    // --- read_dcx_config ---

    #[test]
    fn read_dcx_config_missing_file_returns_empty() {
        let cfg = read_dcx_config(std::path::Path::new("/nonexistent/__dcx_test_cfg__.yaml"));
        assert!(cfg.up.files.is_empty());
    }

    #[test]
    fn read_dcx_config_parses_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dcx_config.yaml");
        std::fs::write(&path, "up:\n  files:\n    - path: ~/.gitconfig\n").unwrap();
        let cfg = read_dcx_config(&path);
        assert_eq!(cfg.up.files, vec!["~/.gitconfig"]);
    }

    // --- find_dcx_config ---

    #[test]
    fn find_dcx_config_returns_nested_path_when_exists() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join(".devcontainer");
        std::fs::create_dir(&nested).unwrap();
        let cfg_path = nested.join("dcx_config.yaml");
        std::fs::write(&cfg_path, "up:\n  files: []\n").unwrap();
        let found = find_dcx_config(dir.path());
        assert_eq!(found, Some(cfg_path));
    }

    #[test]
    fn find_dcx_config_returns_root_path_when_nested_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("dcx_config.yaml");
        std::fs::write(&cfg_path, "up:\n  files: []\n").unwrap();
        let found = find_dcx_config(dir.path());
        assert_eq!(found, Some(cfg_path));
    }

    #[test]
    fn find_dcx_config_returns_none_when_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let found = find_dcx_config(dir.path());
        assert_eq!(found, None);
    }
}
