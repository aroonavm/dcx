#![allow(dead_code)]

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::cmd;
use crate::docker::strip_jsonc_comments;

/// True if devcontainer.json at `config_path` contains a `build.dockerfile` key.
pub fn has_build_dockerfile(config_path: &Path) -> bool {
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let stripped = strip_jsonc_comments(&content);
    stripped.contains("\"dockerfile\"")
}

/// Stable image tag (content-hash) derived from devcontainer.json file bytes.
/// Returns `"dcx-base:<8-char-hex>"` (tag IS the hash — no `:latest` suffix).
pub fn content_tag(config_path: &Path) -> String {
    let bytes = std::fs::read(config_path).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    format!("dcx-base:{}", &hex[..8])
}

/// Expand `${localEnv:VAR:default}` patterns in `value`.
///
/// Replaces each occurrence with the value of env var `VAR`, or `default` if
/// the variable is not set. Handles multiple occurrences; leaves unknown
/// patterns with a missing closing `}` unchanged.
fn expand_local_env(value: &str) -> String {
    let mut result = String::new();
    let mut remaining = value;

    while let Some(start) = remaining.find("${localEnv:") {
        result.push_str(&remaining[..start]);
        let rest = &remaining[start + "${localEnv:".len()..];
        if let Some(end) = rest.find('}') {
            let inner = &rest[..end];
            let (var_name, default) = if let Some(colon_pos) = inner.find(':') {
                (&inner[..colon_pos], &inner[colon_pos + 1..])
            } else {
                (inner, "")
            };
            let expanded = std::env::var(var_name).unwrap_or_else(|_| default.to_string());
            result.push_str(&expanded);
            remaining = &rest[end + 1..];
        } else {
            // No closing brace — emit as-is and stop trying
            result.push_str("${localEnv:");
            remaining = rest;
        }
    }
    result.push_str(remaining);
    result
}

/// Build the base image from the Dockerfile in config dir, tagged as `tag`.
///
/// Reads `build.args` from devcontainer.json and expands `${localEnv:VAR:default}`.
/// Streams output (progress visible to user). Returns the docker exit code.
pub fn build_base_image(config_path: &Path, tag: &str) -> i32 {
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read config: {e}");
            return 1;
        }
    };
    let stripped = strip_jsonc_comments(&content);
    let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap_or_default();
    let config_dir = config_path.parent().unwrap_or(Path::new("."));

    let mut args: Vec<String> = vec!["build".to_string(), "-t".to_string(), tag.to_string()];

    // Extract and expand build args from devcontainer.json
    if let Some(build_args) = parsed
        .get("build")
        .and_then(|b| b.get("args"))
        .and_then(|a| a.as_object())
    {
        for (key, val) in build_args {
            let val_str = val.as_str().unwrap_or("").to_string();
            let expanded = expand_local_env(&val_str);
            args.push("--build-arg".to_string());
            args.push(format!("{key}={expanded}"));
        }
    }

    args.push(config_dir.to_string_lossy().into_owned());

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd::run_stream("docker", &args_ref).unwrap_or(1)
}

/// Write a temp devcontainer.json replacing `"build":{...}` with `"image":"<name>"`.
///
/// Caller must hold the returned `NamedTempFile` for the lifetime of devcontainer up;
/// the file is deleted when the temp file is dropped.
pub fn temp_config_with_image(
    config_path: &Path,
    image_name: &str,
) -> Result<tempfile::NamedTempFile, String> {
    let content =
        std::fs::read_to_string(config_path).map_err(|e| format!("Failed to read config: {e}"))?;
    let stripped = strip_jsonc_comments(&content);
    let mut obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&stripped).map_err(|e| format!("Failed to parse config: {e}"))?;
    obj.remove("build");
    obj.insert(
        "image".to_string(),
        serde_json::Value::String(image_name.to_string()),
    );
    let new_content = serde_json::to_string_pretty(&serde_json::Value::Object(obj))
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    let file = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    std::fs::write(file.path(), new_content.as_bytes())
        .map_err(|e| format!("Failed to write temp config: {e}"))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // --- content_tag ---

    #[test]
    fn content_tag_is_deterministic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("devcontainer.json");
        fs::write(&path, r#"{"image":"test:latest"}"#).unwrap();
        assert_eq!(content_tag(&path), content_tag(&path));
    }

    #[test]
    fn content_tag_differs_for_different_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("devcontainer.json");
        fs::write(&path, r#"{"image":"test:latest"}"#).unwrap();
        let tag1 = content_tag(&path);
        fs::write(&path, r#"{"image":"test:v2"}"#).unwrap();
        let tag2 = content_tag(&path);
        assert_ne!(tag1, tag2);
    }

    #[test]
    fn content_tag_same_content_at_different_paths() {
        let dir = tempdir().unwrap();
        let path1 = dir.path().join("a.json");
        let path2 = dir.path().join("b.json");
        let content = r#"{"image":"test:latest"}"#;
        fs::write(&path1, content).unwrap();
        fs::write(&path2, content).unwrap();
        assert_eq!(content_tag(&path1), content_tag(&path2));
    }

    #[test]
    fn content_tag_format() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("devcontainer.json");
        fs::write(&path, r#"{"image":"test:latest"}"#).unwrap();
        let tag = content_tag(&path);
        assert!(
            tag.starts_with("dcx-base:"),
            "tag must start with dcx-base:, got: {tag}"
        );
        let suffix = tag.strip_prefix("dcx-base:").unwrap();
        assert_eq!(suffix.len(), 8, "hash must be 8 chars, got: {tag}");
        assert!(
            suffix.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must be lowercase hex, got: {tag}"
        );
    }

    // --- has_build_dockerfile ---

    #[test]
    fn has_build_dockerfile_true_when_present() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("devcontainer.json");
        fs::write(
            &path,
            r#"{"name":"test","build":{"dockerfile":"Dockerfile"}}"#,
        )
        .unwrap();
        assert!(has_build_dockerfile(&path));
    }

    #[test]
    fn has_build_dockerfile_false_when_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("devcontainer.json");
        fs::write(&path, r#"{"name":"test","image":"ubuntu:22.04"}"#).unwrap();
        assert!(!has_build_dockerfile(&path));
    }

    // --- expand_local_env ---

    #[test]
    fn expand_local_env_uses_default_when_var_unset() {
        // Use a var name that is definitely not set in the environment.
        let result = expand_local_env("${localEnv:DCX_TEST_UNSET_ZXQY:my-default}");
        assert_eq!(result, "my-default");
    }

    #[test]
    fn expand_local_env_uses_env_when_var_set() {
        // SAFETY: single-threaded test context; no other threads read this var.
        unsafe { std::env::set_var("DCX_TEST_SET_ZXQY", "actual-value") };
        let result = expand_local_env("${localEnv:DCX_TEST_SET_ZXQY:fallback}");
        unsafe { std::env::remove_var("DCX_TEST_SET_ZXQY") };
        assert_eq!(result, "actual-value");
    }
}
