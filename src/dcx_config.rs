use std::path::Path;

use serde::Deserialize;

// ── Serde types ───────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct DcxFileRaw {
    path: String,
}

#[derive(Deserialize, Default)]
struct DcxConfigRaw {
    #[serde(default)]
    files: Vec<DcxFileRaw>,
}

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Default)]
pub struct DcxConfig {
    /// Raw file location strings (may contain `~`). Expansion happens at call site.
    pub files: Vec<String>,
}

// ── Public functions ──────────────────────────────────────────────────────────

/// Parse a dcx_config.yaml string into a DcxConfig.
/// Returns an empty DcxConfig on any parse error.
pub fn parse_dcx_config(yaml: &str) -> DcxConfig {
    match serde_yaml::from_str::<DcxConfigRaw>(yaml) {
        Ok(raw) => DcxConfig {
            files: raw.files.into_iter().map(|f| f.path).collect(),
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_dcx_config ---

    #[test]
    fn parse_dcx_config_reads_files_list() {
        let yaml = "files:\n  - path: ~/.gitconfig\n  - path: ~/.claude.json\n";
        let cfg = parse_dcx_config(yaml);
        assert_eq!(cfg.files, vec!["~/.gitconfig", "~/.claude.json"]);
    }

    #[test]
    fn parse_dcx_config_empty_yaml_returns_empty() {
        let cfg = parse_dcx_config("");
        assert!(cfg.files.is_empty());
    }

    #[test]
    fn parse_dcx_config_missing_files_key_returns_empty() {
        let yaml = "image: ubuntu\n";
        let cfg = parse_dcx_config(yaml);
        assert!(cfg.files.is_empty());
    }

    #[test]
    fn parse_dcx_config_malformed_yaml_returns_empty() {
        let yaml = "files: [invalid: yaml: here\n";
        let cfg = parse_dcx_config(yaml);
        assert!(cfg.files.is_empty());
    }

    #[test]
    fn parse_dcx_config_tilde_paths_preserved_as_is() {
        let yaml = "files:\n  - path: ~/.gitconfig\n";
        let cfg = parse_dcx_config(yaml);
        // Tilde expansion is the caller's responsibility
        assert_eq!(cfg.files[0], "~/.gitconfig");
    }

    // --- read_dcx_config ---

    #[test]
    fn read_dcx_config_missing_file_returns_empty() {
        let cfg = read_dcx_config(std::path::Path::new("/nonexistent/__dcx_test_cfg__.yaml"));
        assert!(cfg.files.is_empty());
    }

    #[test]
    fn read_dcx_config_parses_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dcx_config.yaml");
        std::fs::write(&path, "files:\n  - path: ~/.gitconfig\n").unwrap();
        let cfg = read_dcx_config(&path);
        assert_eq!(cfg.files, vec!["~/.gitconfig"]);
    }
}
