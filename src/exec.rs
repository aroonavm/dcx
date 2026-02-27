#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process;

use crate::cmd;
use crate::docker;
use crate::exit_codes;
use crate::mount_table;
use crate::naming::{is_dcx_managed_path, mount_name, relay_dir};
use crate::platform;
use crate::progress;
use crate::workspace::{find_devcontainer_config, resolve_workspace};

// ── RAII TempFile ─────────────────────────────────────────────────────────

/// RAII guard for a temporary file. Automatically deletes on drop.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    /// Create a new temp file and return its path.
    fn new() -> Result<Self, String> {
        let path = PathBuf::from(format!("/tmp/dcx-override-{}.json", process::id()));
        Ok(TempFile { path })
    }

    /// Get the path to the temp file.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// ── JSON Helpers ──────────────────────────────────────────────────────────

/// Escape a string for JSON by replacing special characters.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Generate a merged override-config by injecting workspaceFolder and workspaceMount
/// into the base devcontainer.json before the final `}`.
fn generate_merged_override_config(
    base_jsonc: &str,
    relay_path: &Path,
    workspace: &Path,
) -> String {
    let clean = docker::strip_jsonc_comments(base_jsonc);
    let clean = clean.trim();
    match clean.rfind('}') {
        None => generate_override_config(relay_path, workspace),
        Some(last_brace) => {
            let before = clean[..last_brace].trim_end();
            let needs_comma = !before.is_empty() && !before.ends_with(',') && before != "{";
            let relay_str = json_escape(&relay_path.to_string_lossy());
            let ws_str = json_escape(&workspace.to_string_lossy());
            format!(
                "{}{}\n  \"workspaceMount\": \"source={},target={},type=bind,consistency=delegated\",\n  \"workspaceFolder\": \"{}\"\n}}\n",
                before,
                if needs_comma { ",\n" } else { "\n" },
                relay_str,
                ws_str,
                ws_str
            )
        }
    }
}

/// Generate the override-config JSON that remaps workspaceFolder and workspaceMount
/// to the original workspace path (standalone, 2-field form for fallback).
fn generate_override_config(relay_path: &Path, original_path: &Path) -> String {
    let relay_str = json_escape(&relay_path.to_string_lossy());
    let original_str = json_escape(&original_path.to_string_lossy());
    format!(
        "{{\n  \"workspaceMount\": \"source={},target={},type=bind,consistency=delegated\",\n  \"workspaceFolder\": \"{}\"\n}}\n",
        relay_str, original_str, original_str
    )
}

// ── Pure functions ────────────────────────────────────────────────────────────

/// Error message when no dcx mount exists for the workspace.
pub fn no_mount_error(workspace: &Path) -> String {
    format!(
        "No mount found for {}. Run `dcx up` first.",
        workspace.display()
    )
}

/// Error message when the mount point exists in the table but is inaccessible.
pub fn stale_mount_error() -> &'static str {
    "Mount is stale. Run `dcx up` to remount."
}

/// Choose the right error when no active mount is found.
///
/// If the relay directory still exists (created by a previous `dcx up`), the mount
/// became stale (FUSE died, system rebooted, etc.). If the directory is absent,
/// `dcx up` was never run for this workspace.
pub fn mount_not_found_error(workspace: &Path, mount_dir_exists: bool) -> String {
    if mount_dir_exists {
        stale_mount_error().to_string()
    } else {
        no_mount_error(workspace)
    }
}

/// Build the argument list for `devcontainer exec`.
///
/// Passes `--container-id` (reliable container lookup, bypasses config-hash
/// resolution), `--workspace-folder` (original workspace path), optionally
/// `--override-config` to remap workspace paths, and optionally `--config` to
/// specify the devcontainer config.
pub fn build_exec_args(
    container_id: &str,
    workspace_path: &Path,
    config: Option<&Path>,
    override_config_path: Option<&Path>,
    command: &[String],
) -> Vec<String> {
    let mut args = vec![
        "exec".to_string(),
        "--container-id".to_string(),
        container_id.to_string(),
        "--workspace-folder".to_string(),
        workspace_path.to_string_lossy().into_owned(),
    ];
    if let Some(oc) = override_config_path {
        args.push("--override-config".to_string());
        args.push(oc.to_string_lossy().into_owned());
    }
    if let Some(cfg) = config {
        args.push("--config".to_string());
        args.push(cfg.to_string_lossy().into_owned());
    }
    if !command.is_empty() {
        args.push("--".to_string());
        for c in command {
            args.push(c.clone());
        }
    }
    args
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run `dcx exec`.
///
/// Returns the exit code that `main` should pass to `std::process::exit`.
pub fn run_exec(
    home: &Path,
    workspace_folder: Option<PathBuf>,
    config: Option<PathBuf>,
    command: Vec<String>,
) -> i32 {
    // 1. Validate Docker/Colima is available.
    if !docker::is_docker_available() {
        eprintln!("Docker is not available. Is Colima running?");
        return exit_codes::RUNTIME_ERROR;
    }

    // 2. Resolve workspace path to absolute canonical path.
    let workspace = match resolve_workspace(workspace_folder.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return exit_codes::USAGE_ERROR;
        }
    };
    progress::step(&format!(
        "Resolving workspace path: {}",
        workspace.display()
    ));

    // 3. Recursive mount guard — block nested dcx mounts.
    let relay = relay_dir(home);
    if is_dcx_managed_path(&workspace, &relay) {
        eprintln!(
            "Cannot use a dcx-managed mount point as a workspace. \
             Use the original workspace path instead."
        );
        return exit_codes::USAGE_ERROR;
    }

    // 4. Verify mount exists in the mount table.
    let name = mount_name(&workspace);
    let mount_point = relay.join(&name);
    let table = platform::read_mount_table().unwrap_or_default();
    let source_in_table = mount_table::find_mount_source(&table, &mount_point);

    if source_in_table.is_none() {
        // Mount directory existing means dcx up was run before but the mount went away.
        eprintln!(
            "{}",
            mount_not_found_error(&workspace, mount_point.exists())
        );
        return exit_codes::RUNTIME_ERROR;
    }

    // 5. Verify mount is healthy (accessible). In table but not accessible = zombie FUSE.
    if !mount_point.exists() {
        eprintln!("{}", stale_mount_error());
        return exit_codes::RUNTIME_ERROR;
    }

    // 6. Find the running container by its devcontainer.local_folder label.
    //    Using --container-id bypasses devcontainer's config-hash-based lookup entirely,
    //    which is more reliable than relying on devcontainer to resolve the config.
    let container_id = docker::find_devcontainer_by_workspace(&mount_point);
    let Some(container_id) = container_id else {
        eprintln!("No running devcontainer found for this workspace. Run `dcx up` first.");
        return exit_codes::RUNTIME_ERROR;
    };

    // 7. Print network mode if available
    if let Some(network_mode) = docker::read_network_mode(&container_id) {
        progress::step(&format!("Network: {}", network_mode));
    }

    // 8. Generate override-config to remap workspaceFolder and workspaceMount to the original path.
    // This ensures devcontainer exec applies the workspace remapping, so the user lands in
    // the correct directory.
    let override_config = match TempFile::new() {
        Ok(temp_file) => {
            // Try to read the base devcontainer.json and generate a merged config
            let base_config_path = config
                .clone()
                .or_else(|| find_devcontainer_config(&workspace));
            let json_content = if let Some(ref path) = base_config_path {
                match std::fs::read_to_string(path) {
                    Ok(base) => generate_merged_override_config(&base, &mount_point, &workspace),
                    Err(e) => {
                        eprintln!(
                            "Warning: Could not read base config at {}, falling back to standalone mode: {e}",
                            path.display()
                        );
                        generate_override_config(&mount_point, &workspace)
                    }
                }
            } else {
                generate_override_config(&mount_point, &workspace)
            };

            if let Err(e) = std::fs::write(temp_file.path(), &json_content) {
                eprintln!("Failed to write override config: {e}");
                return exit_codes::RUNTIME_ERROR;
            }
            Some(temp_file)
        }
        Err(e) => {
            eprintln!("Failed to create temp file: {e}");
            return exit_codes::RUNTIME_ERROR;
        }
    };

    // 9. Delegate to `devcontainer exec`.
    // Pass --container-id for reliable container lookup, --workspace-folder pointing
    // to the original workspace path, --override-config to remap workspace paths, and optionally
    // --config. SIGINT is forwarded naturally to the child (same process group).
    progress::step("Running exec in container...");

    let args = build_exec_args(
        &container_id,
        &workspace,
        config.as_deref(),
        override_config.as_ref().map(|t| t.path()),
        &command,
    );
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let code = cmd::run_stream("devcontainer", &args_str).unwrap_or(exit_codes::PREREQ_NOT_FOUND);
    // Drop override_config to clean up temp file before returning
    drop(override_config);
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- no_mount_error ---

    #[test]
    fn no_mount_error_message() {
        let ws = Path::new("/home/user/myproject");
        let msg = no_mount_error(ws);
        assert!(msg.contains("No mount found"), "got: {msg}");
        assert!(msg.contains("/home/user/myproject"), "got: {msg}");
        assert!(msg.contains("dcx up"), "got: {msg}");
    }

    // --- stale_mount_error ---

    #[test]
    fn stale_mount_error_message() {
        let msg = stale_mount_error();
        assert!(msg.contains("stale"), "got: {msg}");
        assert!(msg.contains("dcx up"), "got: {msg}");
        assert!(msg.contains("remount"), "got: {msg}");
    }

    // --- build_exec_args ---

    #[test]
    fn exec_args_includes_container_id() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, None, None, &[]);
        let ci = args.iter().position(|a| a == "--container-id").unwrap();
        assert_eq!(args[ci + 1], "abc123");
    }

    #[test]
    fn exec_args_includes_workspace_folder() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, None, None, &[]);
        let wi = args.iter().position(|a| a == "--workspace-folder").unwrap();
        assert_eq!(args[wi + 1], "/home/user/myproject");
    }

    #[test]
    fn exec_args_appends_command_after_separator() {
        let ws = Path::new("/home/user/myproject");
        let cmd = vec!["bash".to_string(), "-c".to_string(), "echo hi".to_string()];
        let args = build_exec_args("abc123", ws, None, None, &cmd);
        let sep = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[sep + 1], "bash");
        assert_eq!(args[sep + 2], "-c");
        assert_eq!(args[sep + 3], "echo hi");
    }

    #[test]
    fn exec_args_no_separator_when_command_empty() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, None, None, &[]);
        assert!(!args.contains(&"--".to_string()));
    }

    #[test]
    fn exec_args_includes_config_when_provided() {
        let ws = Path::new("/home/user/myproject");
        let cfg = Path::new("/home/user/project/.devcontainer/devcontainer.json");
        let args = build_exec_args("abc123", ws, Some(cfg), None, &[]);
        let ci = args.iter().position(|a| a == "--config").unwrap();
        assert_eq!(args[ci + 1], cfg.to_string_lossy().as_ref());
    }

    #[test]
    fn exec_args_no_config_flag_when_absent() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, None, None, &[]);
        assert!(!args.contains(&"--config".to_string()));
    }

    #[test]
    fn exec_args_includes_override_config_when_provided() {
        let ws = Path::new("/home/user/myproject");
        let oc = Path::new("/tmp/dcx-override-12345.json");
        let args = build_exec_args("abc123", ws, None, Some(oc), &[]);
        let oci = args.iter().position(|a| a == "--override-config").unwrap();
        assert_eq!(args[oci + 1], oc.to_string_lossy().as_ref());
    }

    #[test]
    fn exec_args_no_override_config_flag_when_absent() {
        let ws = Path::new("/home/user/myproject");
        let args = build_exec_args("abc123", ws, None, None, &[]);
        assert!(!args.contains(&"--override-config".to_string()));
    }

    // --- mount_not_found_error ---

    #[test]
    fn mount_not_found_error_stale_when_dir_exists() {
        // If the relay directory still exists but the mount is gone, it's a stale state
        let ws = Path::new("/home/user/myproject");
        let msg = mount_not_found_error(ws, true);
        assert!(msg.contains("stale"), "got: {msg}");
    }

    #[test]
    fn mount_not_found_error_no_mount_when_dir_absent() {
        // If the relay directory never existed, no mount was ever set up
        let ws = Path::new("/home/user/myproject");
        let msg = mount_not_found_error(ws, false);
        assert!(msg.contains("No mount found"), "got: {msg}");
    }
}
