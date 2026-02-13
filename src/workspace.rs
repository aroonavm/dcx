#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Resolve the workspace path.
///
/// - If `given` is `Some`, canonicalize and return it.
/// - If `given` is `None`, use the current working directory.
///
/// Returns `Err` if the path does not exist or cannot be canonicalized.
pub fn resolve_workspace(given: Option<&Path>) -> Result<PathBuf, String> {
    let path = match given {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|e| format!("Cannot determine current directory: {e}"))?,
    };
    path.canonicalize()
        .map_err(|_| format!("Workspace path does not exist: {}", path.display()))
}

/// Detect a devcontainer configuration in `workspace`.
///
/// Checks in order:
/// 1. `.devcontainer/devcontainer.json`
/// 2. `.devcontainer.json`
///
/// Returns the path to the first found configuration, or `None` if neither exists.
pub fn find_devcontainer_config(workspace: &Path) -> Option<PathBuf> {
    let nested = workspace.join(".devcontainer").join("devcontainer.json");
    if nested.exists() {
        return Some(nested);
    }
    let top_level = workspace.join(".devcontainer.json");
    if top_level.exists() {
        return Some(top_level);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    // --- resolve_workspace ---

    #[test]
    fn resolve_workspace_none_returns_current_dir() {
        let resolved = resolve_workspace(None).unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(resolved, cwd);
    }

    #[test]
    fn resolve_workspace_given_existing_path_canonicalizes() {
        let dir = assert_fs::TempDir::new().unwrap();
        let resolved = resolve_workspace(Some(dir.path())).unwrap();
        assert_eq!(resolved, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn resolve_workspace_nonexistent_path_is_err() {
        let result = resolve_workspace(Some(Path::new("/nonexistent/dcx_test_path_xyz")));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_workspace_error_message_contains_path() {
        let path = Path::new("/nonexistent/dcx_test_path_xyz");
        let err = resolve_workspace(Some(path)).unwrap_err();
        assert!(err.contains("nonexistent/dcx_test_path_xyz"), "got: {err}");
    }

    // --- find_devcontainer_config ---

    #[test]
    fn find_devcontainer_config_finds_nested() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child(".devcontainer/devcontainer.json")
            .touch()
            .unwrap();
        let result = find_devcontainer_config(dir.path());
        assert!(result.is_some());
        assert!(result.unwrap().ends_with(".devcontainer/devcontainer.json"));
    }

    #[test]
    fn find_devcontainer_config_finds_top_level() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child(".devcontainer.json").touch().unwrap();
        let result = find_devcontainer_config(dir.path());
        assert!(result.is_some());
        assert!(result.unwrap().ends_with(".devcontainer.json"));
    }

    #[test]
    fn find_devcontainer_config_returns_none_when_absent() {
        let dir = assert_fs::TempDir::new().unwrap();
        let result = find_devcontainer_config(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn find_devcontainer_config_prefers_nested_over_top_level() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child(".devcontainer/devcontainer.json")
            .touch()
            .unwrap();
        dir.child(".devcontainer.json").touch().unwrap();
        let result = find_devcontainer_config(dir.path()).unwrap();
        assert!(
            result.ends_with(".devcontainer/devcontainer.json"),
            "got: {}",
            result.display()
        );
    }
}
