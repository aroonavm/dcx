#![allow(dead_code)]

use std::path::Path;

use crate::cmd;

/// Return `true` if Docker (or Colima) is running and reachable.
///
/// Runs `docker info` and considers exit code 0 as "available".
pub fn is_docker_available() -> bool {
    match cmd::run_capture("docker", &["info"]) {
        Ok(out) => out.status == 0,
        Err(_) => false,
    }
}

/// Query `docker ps` for a running container associated with `mount_point`.
///
/// Returns the short container ID if a running container is found, or `None` otherwise.
/// Takes only the first line of output (handles multi-ID edge cases).
pub fn query_container(mount_point: &Path) -> Option<String> {
    let label = format!("label=devcontainer.local_folder={}", mount_point.display());
    let out =
        cmd::run_capture("docker", &["ps", "--filter", &label, "--format", "{{.ID}}"]).ok()?;
    let id = out.stdout.lines().next().unwrap_or("").trim().to_string();
    if id.is_empty() { None } else { Some(id) }
}

/// Query `docker ps -a` for any container (running or stopped) associated with `mount_point`.
///
/// Returns the short container ID if a container is found, or `None` otherwise.
/// Takes only the first line of output (handles multi-ID edge cases).
pub fn query_container_any(mount_point: &Path) -> Option<String> {
    let label = format!("label=devcontainer.local_folder={}", mount_point.display());
    let out = cmd::run_capture(
        "docker",
        &["ps", "-a", "--filter", &label, "--format", "{{.ID}}"],
    )
    .ok()?;
    let id = out.stdout.lines().next().unwrap_or("").trim().to_string();
    if id.is_empty() { None } else { Some(id) }
}

/// Stop a running container associated with `mount_point` using `docker stop`.
///
/// Returns `Ok(())` if the container was stopped or if no running container is found (idempotent).
/// Returns `Err(message)` if the stop command fails.
pub fn stop_container(mount_point: &Path) -> Result<(), String> {
    if let Some(container_id) = query_container(mount_point) {
        let out = cmd::run_capture("docker", &["stop", &container_id])?;
        if out.status != 0 {
            return Err(format!("Failed to stop container: {}", out.stderr.trim()));
        }
    }
    // Idempotent: no error if no running container found
    Ok(())
}

/// Remove a container by ID using `docker rm`.
///
/// Returns `Err(message)` if the remove command fails.
pub fn remove_container(container_id: &str) -> Result<(), String> {
    let out = cmd::run_capture("docker", &["rm", container_id])?;
    if out.status != 0 {
        return Err(format!("Failed to remove container: {}", out.stderr.trim()));
    }
    Ok(())
}

/// Get the image ID from a container by inspecting it.
///
/// Returns `Err(message)` if the inspect command fails.
pub fn get_image_id(container_id: &str) -> Result<String, String> {
    let out = cmd::run_capture("docker", &["inspect", "--format={{.Image}}", container_id])?;
    if out.status != 0 {
        return Err(format!(
            "Failed to inspect container: {}",
            out.stderr.trim()
        ));
    }
    let image_id = out.stdout.trim().to_string();
    if image_id.is_empty() {
        return Err("Could not determine image ID from container".to_string());
    }
    Ok(image_id)
}

/// From a list of repo tags, find the one that represents the runtime (`-uid`) image.
///
/// Devcontainer names runtime images with a `-uid` suffix (e.g. `vsc-myproject-hash-uid`).
/// Handles both bare repository names and `repository:tag` strings as produced by
/// `docker image inspect --format={{range .RepoTags}}{{.}}\n{{end}}`.
pub fn find_uid_tag<'a>(tags: &[&'a str]) -> Option<&'a str> {
    tags.iter().copied().find(|tag| {
        let repo = tag.split(':').next().unwrap_or(tag);
        repo.ends_with("-uid")
    })
}

/// Get a reference for the runtime (`-uid`) image of a container.
///
/// Prefers the repo tag (e.g. `vsc-myproject-hash-uid`) over the raw SHA256 so that
/// `docker rmi <tag>` removes only the runtime tag and does not accidentally delete
/// the build image when both share the same underlying SHA256 (which happens when
/// devcontainer's UID remapping produces no new layers).
///
/// Falls back to the raw image SHA256 if no `-uid` tag is found.
/// Must be called BEFORE the container is removed.
pub fn get_runtime_image_ref(container_id: &str) -> Result<String, String> {
    let image_id = get_image_id(container_id)?;

    let out = cmd::run_capture(
        "docker",
        &[
            "image",
            "inspect",
            "--format={{range .RepoTags}}{{.}}\n{{end}}",
            &image_id,
        ],
    )?;

    if out.status == 0 {
        let tags: Vec<&str> = out
            .stdout
            .lines()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect();
        if let Some(uid_tag) = find_uid_tag(&tags) {
            return Ok(uid_tag.to_string());
        }
    }

    // No -uid tag found; fall back to the SHA256 (keeps existing behaviour for edge cases)
    Ok(image_id)
}

/// Remove the runtime image by reference.
///
/// When `image_ref` is a repo tag (e.g. `vsc-name-hash-uid`), removes without
/// `--force` so that only that tag is removed.  If the build image shares the
/// same underlying SHA256 and has its own tag, it is preserved.
///
/// When `image_ref` is a SHA256 (fallback), uses `--force` for compatibility.
pub fn remove_runtime_image(image_ref: &str) -> Result<(), String> {
    let out = if image_ref.starts_with("sha256:") {
        cmd::run_capture("docker", &["rmi", "--force", image_ref])?
    } else {
        cmd::run_capture("docker", &["rmi", image_ref])?
    };
    if out.status != 0 {
        return Err(format!(
            "Failed to remove runtime image: {}",
            out.stderr.trim()
        ));
    }
    Ok(())
}

/// Remove a container image by ID using `docker rmi`.
///
/// Uses `--force` to handle tagged images (e.g. `vsc-dcx-*-uid`) which would
/// otherwise fail removal without it.
/// Returns `Err(message)` if the remove command fails.
pub fn remove_image(image_id: &str) -> Result<(), String> {
    let out = cmd::run_capture("docker", &["rmi", "--force", image_id])?;
    if out.status != 0 {
        return Err(format!("Failed to remove image: {}", out.stderr.trim()));
    }
    Ok(())
}

/// Read the build image name from a devcontainer configuration.
///
/// If `config` is `Some`, reads directly from that path. Otherwise checks
/// `.devcontainer/devcontainer.json` then `.devcontainer.json` at the workspace root.
/// Extracts the top-level `"image"` field value. Returns `None` if the file is not found,
/// the field is absent, or parsing fails.
pub fn get_base_image_name(
    workspace: &std::path::Path,
    config: Option<&std::path::Path>,
) -> Option<String> {
    if let Some(path) = config {
        let content = std::fs::read_to_string(path).ok()?;
        return extract_image_field(&content);
    }
    let candidates = [
        workspace.join(".devcontainer").join("devcontainer.json"),
        workspace.join(".devcontainer.json"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Some(name) = extract_image_field(&content)
        {
            return Some(name);
        }
    }
    None
}

/// Strip JSONC-style `//` and `/* */` comments from content, preserving string literals.
///
/// devcontainer.json uses JSONC format which allows comments. This ensures comment
/// content is not mistaken for real JSON keys or values.
fn strip_jsonc_comments(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '\\' {
                // Escaped character — emit next char as-is (don't end string on `\"`)
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => {
                    in_string = true;
                    result.push(c);
                }
                '/' => match chars.peek() {
                    Some('/') => {
                        // Line comment — skip to end of line, preserve the newline
                        chars.next();
                        for c2 in chars.by_ref() {
                            if c2 == '\n' {
                                result.push('\n');
                                break;
                            }
                        }
                    }
                    Some('*') => {
                        // Block comment — skip until `*/`, preserve newlines for line numbers
                        chars.next();
                        loop {
                            match chars.next() {
                                Some('*') if chars.peek() == Some(&'/') => {
                                    chars.next();
                                    break;
                                }
                                Some('\n') => result.push('\n'),
                                None => break,
                                _ => {}
                            }
                        }
                    }
                    _ => result.push(c),
                },
                _ => result.push(c),
            }
        }
    }
    result
}

/// Extract the top-level `"image"` field value from devcontainer JSON content.
///
/// Strips JSONC comments first so that commented-out `"image"` keys are ignored.
/// Searches for the first `"image"` key followed by a string value.
fn extract_image_field(content: &str) -> Option<String> {
    let stripped = strip_jsonc_comments(content);
    let key = "\"image\"";
    let pos = stripped.find(key)?;
    let after_key =
        stripped[pos + key.len()..].trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    let after_key = after_key.trim_start();
    if !after_key.starts_with('"') {
        return None;
    }
    let inner = &after_key[1..];
    let end = inner.find('"')?;
    let value = inner[..end].trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

/// Check if a Docker image exists locally.
pub fn image_exists(image: &str) -> bool {
    cmd::run_capture("docker", &["image", "inspect", image])
        .map(|out| out.status == 0)
        .unwrap_or(false)
}

/// The Docker repository used for dcx base image tags.
///
/// During `dcx up`, the base image (from devcontainer.json `"image"` field) is tagged
/// as `dcx-base:<mount-name>`. This lets `dcx clean --purge` find and remove base images
/// by convention, without needing to resolve workspace paths.
const BASE_IMAGE_REPO: &str = "dcx-base";

/// Tag a base image with a dcx-managed reference.
///
/// Creates `dcx-base:<mount_name>` as an alias for `base_image`. Removing this tag
/// later only deletes the underlying image if no other tags reference it.
pub fn tag_base_image(base_image: &str, mount_name: &str) -> Result<(), String> {
    let tag = format!("{BASE_IMAGE_REPO}:{mount_name}");
    let out = cmd::run_capture("docker", &["tag", base_image, &tag])?;
    if out.status != 0 {
        return Err(format!("Failed to tag base image: {}", out.stderr.trim()));
    }
    Ok(())
}

/// Remove the dcx base image tag for a mount.
///
/// Runs `docker rmi dcx-base:<mount_name>`. This only removes the tag; the underlying
/// image is deleted only if this was the last reference. Non-fatal if the tag doesn't exist.
pub fn remove_base_image_tag(mount_name: &str) -> Result<(), String> {
    let tag = format!("{BASE_IMAGE_REPO}:{mount_name}");
    let out = cmd::run_capture("docker", &["rmi", &tag])?;
    if out.status != 0 {
        let stderr = out.stderr.trim();
        // Ignore "No such image" — tag was already removed or never created
        if stderr.contains("No such image") {
            return Ok(());
        }
        return Err(format!("Failed to remove base image tag: {stderr}"));
    }
    Ok(())
}

/// Remove all dcx base image tags.
///
/// Lists all `dcx-base:*` images and removes each tag. Returns the count of removed tags.
pub fn clean_all_base_image_tags() -> Result<usize, String> {
    let out = cmd::run_capture(
        "docker",
        &[
            "images",
            BASE_IMAGE_REPO,
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ],
    )?;
    if out.status != 0 {
        return Err(format!(
            "Failed to list base image tags: {}",
            out.stderr.trim()
        ));
    }

    let mut removed = 0;
    for tag in out.stdout.lines() {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        let rm_out = cmd::run_capture("docker", &["rmi", tag])?;
        if rm_out.status == 0 {
            removed += 1;
        }
        // Non-fatal: skip tags that fail to remove
    }
    Ok(removed)
}

/// Find the running devcontainer for a given relay mount point.
///
/// Searches for a running container whose `devcontainer.local_folder` label matches
/// `mount_point`. Returns the container ID, or `None` if no running container is found.
pub fn find_devcontainer_by_workspace(mount_point: &Path) -> Option<String> {
    let mount_str = mount_point.to_string_lossy();
    let filter = format!("label=devcontainer.local_folder={mount_str}");
    let out = cmd::run_capture(
        "docker",
        &["ps", "--filter", &filter, "--format", "{{.ID}}"],
    )
    .ok()?;
    let id = out.stdout.lines().next().unwrap_or("").trim().to_string();
    if id.is_empty() { None } else { Some(id) }
}

/// Find all dcx-managed stopped containers and remove them.
///
/// This finds containers with devcontainer labels matching the naming pattern
/// (vsc-dcx-*) and removes them, even if their mount directories no longer exist.
/// Returns the count of removed containers.
pub fn clean_orphaned_containers() -> Result<usize, String> {
    // Find all stopped dcx containers (using the naming pattern vsc-dcx-*)
    let out = cmd::run_capture(
        "docker",
        &[
            "ps",
            "-a",
            "--filter",
            "status=exited",
            "--format",
            "{{.ID}}",
        ],
    )?;

    let mut removed = 0;
    for container_id in out.stdout.lines() {
        let container_id = container_id.trim();
        if container_id.is_empty() {
            continue;
        }

        // Check if container has devcontainer.local_folder label (dcx-managed)
        let inspect_out = match cmd::run_capture(
            "docker",
            &[
                "inspect",
                "--format={{index .Config.Labels \"devcontainer.local_folder\"}}",
                container_id,
            ],
        ) {
            Ok(out) => out,
            Err(_) => continue,
        };

        let local_folder = inspect_out.stdout.trim();

        // Only remove if it has the devcontainer.local_folder label (starts with / and not empty/no value)
        if !local_folder.is_empty()
            && !local_folder.contains("no value")
            && local_folder.starts_with("/")
        {
            // This is a dcx-managed container, try to remove it
            if remove_container(container_id).is_ok() {
                removed += 1;
            }
        }
    }

    Ok(removed)
}

/// Returns true if `name` is a devcontainer runtime image tag.
///
/// Runtime images are named `vsc-*-uid` (the UID-remapped layer devcontainer
/// creates on top of the build image). Build images (`vsc-*` without the
/// `-uid` suffix) must NOT be treated as orphans during a normal clean — only
/// `--purge` removes them.
///
/// Accepts both bare repository names (`vsc-foo-uid`) and `repository:tag`
/// strings as produced by `docker images --format "{{.Repository}}:{{.Tag}}"`.
pub fn is_runtime_image_tag(name: &str) -> bool {
    let repo = name.split(':').next().unwrap_or(name);
    repo.starts_with("vsc-") && repo.ends_with("-uid")
}

/// Returns true if `name` is a devcontainer build image tag.
///
/// Build images are named `vsc-*` without a `-uid` suffix. They are created
/// by devcontainer as the intermediate build layer and only removed by `--purge`.
///
/// Accepts both bare repository names and `repository:tag` strings.
pub fn is_build_image_tag(name: &str) -> bool {
    let repo = name.split(':').next().unwrap_or(name);
    repo.starts_with("vsc-") && !repo.ends_with("-uid")
}

/// Derive the corresponding runtime image name from a build image name.
///
/// Build image `vsc-X:tag` → runtime image `vsc-X-uid:tag`
/// This assumes the build image name is in the format `vsc-*:tag`.
/// If `name` contains a `:`, appends `-uid` before the tag. Otherwise, appends `-uid` to the name.
fn build_image_to_runtime_image(build_image: &str) -> String {
    if let Some(colon_pos) = build_image.find(':') {
        let repo = &build_image[..colon_pos];
        let tag = &build_image[colon_pos..];
        format!("{}-uid{}", repo, tag)
    } else {
        format!("{}-uid", build_image)
    }
}

/// Remove all devcontainer build images (`vsc-*` without `-uid`) that have no containers.
///
/// Used by `dcx clean --purge --all` as a final sweep to remove orphaned build images
/// whose containers were already removed. Also used by `dcx clean --purge` in single-workspace
/// mode after cleaning up the specific workspace.
///
/// A build image is considered orphaned if:
/// 1. Its corresponding runtime image (`vsc-*-uid`) no longer exists (workspace fully cleaned), AND
/// 2. No containers directly reference this build image
///
/// Skips images whose runtime image still exists (workspace still active) or that have containers.
/// Returns the count of removed images.
pub fn clean_orphaned_build_images() -> Result<usize, String> {
    let out = cmd::run_capture(
        "docker",
        &["images", "--format", "{{.Repository}}:{{.Tag}}"],
    )?;

    let mut removed = 0;
    for image_name in out.stdout.lines() {
        let image_name = image_name.trim();
        if image_name.is_empty() || !is_build_image_tag(image_name) {
            continue;
        }

        // First check: if the corresponding runtime image still exists, skip this build image.
        // The runtime image existing means the workspace is still active.
        let runtime_image = build_image_to_runtime_image(image_name);
        if image_exists(&runtime_image) {
            continue;
        }

        // Second check: if any container (running or stopped) directly references this build image, skip it.
        // This is a fallback in case containers were created directly from the build image.
        let check_out = match cmd::run_capture(
            "docker",
            &[
                "ps",
                "-a",
                "--filter",
                &format!("ancestor={image_name}"),
                "--format",
                "{{.ID}}",
            ],
        ) {
            Ok(out) => out,
            Err(_) => continue,
        };

        if !check_out.stdout.trim().is_empty() {
            continue;
        }

        // Both checks passed: runtime image gone and no containers → safe to remove
        if let Ok(out) = cmd::run_capture("docker", &["rmi", image_name])
            && out.status == 0
        {
            removed += 1;
        }
    }

    Ok(removed)
}

/// Remove all dcx container images that are not in use.
///
/// This removes both dangling images and named vsc-*-uid runtime images that
/// have no running/stopped containers. Build images (vsc-* without -uid) are
/// intentionally skipped — they are Docker cache and only removed by --purge.
/// Returns the count of removed images.
pub fn clean_orphaned_images() -> Result<usize, String> {
    // First remove dangling images (not used by any container)
    let out = cmd::run_capture(
        "docker",
        &["images", "--filter", "dangling=true", "--format", "{{.ID}}"],
    )?;

    let mut removed = 0;
    for image_id in out.stdout.lines() {
        let image_id = image_id.trim();
        if image_id.is_empty() {
            continue;
        }

        // Try to remove the image
        if remove_image(image_id).is_ok() {
            removed += 1;
        }
    }

    // Also remove orphaned vsc-*-uid runtime images (no containers).
    // Build images (vsc-* without -uid) are intentionally skipped here;
    // they are only removed by --purge.
    let out = cmd::run_capture(
        "docker",
        &["images", "--format", "{{.Repository}}:{{.Tag}}"],
    )?;

    for image_name in out.stdout.lines() {
        let image_name = image_name.trim();
        if image_name.is_empty() || !is_runtime_image_tag(image_name) {
            continue;
        }

        // Check if this image is used by any container (running or stopped)
        let check_out = match cmd::run_capture(
            "docker",
            &[
                "ps",
                "-a",
                "--filter",
                &format!("ancestor={image_name}"),
                "--format",
                "{{.ID}}",
            ],
        ) {
            Ok(out) => out,
            Err(_) => continue,
        };

        if !check_out.stdout.trim().is_empty() {
            // Container is using this image, skip it
            continue;
        }

        // No container uses this image; remove by tag (no --force, consistent
        // with remove_runtime_image which also removes by tag only)
        if let Ok(out) = cmd::run_capture("docker", &["rmi", image_name])
            && out.status == 0
        {
            removed += 1;
        }
    }

    Ok(removed)
}

/// List Docker volumes matching a name filter.
///
/// Returns a vector of volume names (one per line) that match the filter.
pub fn list_volumes(name_filter: &str) -> Result<Vec<String>, String> {
    let out = cmd::run_capture(
        "docker",
        &[
            "volume",
            "ls",
            "--filter",
            &format!("name={name_filter}"),
            "--format",
            "{{.Name}}",
        ],
    )?;
    if out.status != 0 {
        return Err(format!("Failed to list volumes: {}", out.stderr.trim()));
    }
    Ok(out
        .stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// Remove a Docker volume by name.
///
/// Returns `Err(message)` if the volume removal fails.
pub fn remove_volume(name: &str) -> Result<(), String> {
    let out = cmd::run_capture("docker", &["volume", "rm", name])?;
    if out.status != 0 {
        return Err(format!(
            "Failed to remove volume {}: {}",
            name,
            out.stderr.trim()
        ));
    }
    Ok(())
}

/// Get volumes associated with a container (by container ID).
///
/// Returns a vector of volume names associated with the container, filtered to `dcx-*` prefix only.
/// This must be called BEFORE the container is removed to capture volume names.
pub fn get_container_volumes(container_id: &str) -> Result<Vec<String>, String> {
    let out = cmd::run_capture(
        "docker",
        &[
            "inspect",
            "--format",
            r#"{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}} {{end}}{{end}}"#,
            container_id,
        ],
    )?;
    if out.status != 0 {
        return Err(format!(
            "Failed to inspect container volumes: {}",
            out.stderr.trim()
        ));
    }
    let volumes: Vec<String> = out
        .stdout
        .split_whitespace()
        .filter(|v| !v.is_empty() && v.starts_with("dcx-"))
        .map(|v| v.to_string())
        .collect();
    Ok(volumes)
}

/// Remove all Docker volumes with the `dcx-` prefix.
///
/// Used by `dcx clean --purge --all` as a final sweep to remove any orphaned
/// dcx-managed volumes whose containers were already removed externally.
/// Returns the count of removed volumes.
pub fn clean_all_dcx_volumes() -> Result<usize, String> {
    let volumes = list_volumes("dcx-")?;
    let mut removed = 0;
    for volume in &volumes {
        if remove_volume(volume).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_image_field ---

    #[test]
    fn extract_image_field_returns_image_name() {
        let json = r#"{ "name": "My Dev", "image": "dcx-dev:latest", "build": {} }"#;
        assert_eq!(
            extract_image_field(json),
            Some("dcx-dev:latest".to_string())
        );
    }

    #[test]
    fn extract_image_field_returns_none_when_absent() {
        let json = r#"{ "name": "My Dev", "build": { "dockerfile": "Dockerfile" } }"#;
        assert_eq!(extract_image_field(json), None);
    }

    #[test]
    fn extract_image_field_handles_whitespace_around_colon() {
        let json = r#"{ "image"  :  "my-image:1.0" }"#;
        assert_eq!(extract_image_field(json), Some("my-image:1.0".to_string()));
    }

    #[test]
    fn extract_image_field_returns_none_for_empty_value() {
        let json = r#"{ "image": "" }"#;
        assert_eq!(extract_image_field(json), None);
    }

    #[test]
    fn extract_image_field_truncates_at_escaped_quote() {
        // The simple scanner doesn't handle escaped quotes — it stops at the first `"`.
        // This documents the known limitation: the value is truncated before the escape.
        let json = r#"{ "image": "my-image:\"tag\"" }"#;
        assert_eq!(extract_image_field(json), Some(r"my-image:\".to_string()));
    }

    #[test]
    fn extract_image_field_ignores_line_comment() {
        let json =
            "{\n  // \"image\": \"commented-out:image\",\n  \"image\": \"real-image:latest\"\n}";
        assert_eq!(
            extract_image_field(json),
            Some("real-image:latest".to_string())
        );
    }

    #[test]
    fn extract_image_field_ignores_block_comment() {
        let json = r#"{ /* "image": "block-commented:image", */ "image": "real-image:1.0" }"#;
        assert_eq!(
            extract_image_field(json),
            Some("real-image:1.0".to_string())
        );
    }

    #[test]
    fn strip_jsonc_comments_removes_line_comments() {
        let input = "{\n  // this is a comment\n  \"key\": \"value\"\n}";
        let result = strip_jsonc_comments(input);
        assert!(!result.contains("this is a comment"));
        assert!(result.contains("\"key\": \"value\""));
    }

    #[test]
    fn strip_jsonc_comments_removes_block_comments() {
        let input = r#"{ /* block comment */ "key": "value" }"#;
        let result = strip_jsonc_comments(input);
        assert!(!result.contains("block comment"));
        assert!(result.contains("\"key\": \"value\""));
    }

    #[test]
    fn strip_jsonc_comments_preserves_comment_syntax_in_strings() {
        let input = r#"{ "key": "http://example.com" }"#;
        let result = strip_jsonc_comments(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_jsonc_comments_handles_unclosed_block_comment() {
        // An unclosed /* comment reaching EOF must not panic; remaining content is dropped.
        let input = "before /* unclosed";
        let result = strip_jsonc_comments(input);
        assert_eq!(result, "before ");
    }

    #[test]
    fn strip_jsonc_comments_handles_unclosed_string() {
        // An unclosed string reaching EOF: characters are emitted as-is (no comments to strip).
        let input = r#"{ "key": "no close"#;
        let result = strip_jsonc_comments(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_jsonc_comments_escaped_quote_does_not_end_string() {
        // \" inside a string must not close it; the next / should NOT start a comment.
        let input = r#"{ "key": "val\"//not a comment" }"#;
        let result = strip_jsonc_comments(input);
        // The // is inside a string and must survive unchanged.
        assert!(result.contains("//not a comment"), "got: {result}");
    }

    // --- find_uid_tag ---

    #[test]
    fn find_uid_tag_returns_uid_tag() {
        let tags = vec!["vsc-myproject-a1b2c3d4", "vsc-myproject-a1b2c3d4-uid"];
        assert_eq!(find_uid_tag(&tags), Some("vsc-myproject-a1b2c3d4-uid"));
    }

    #[test]
    fn find_uid_tag_returns_none_when_no_uid_tag() {
        let tags = vec!["vsc-myproject-a1b2c3d4"];
        assert_eq!(find_uid_tag(&tags), None);
    }

    #[test]
    fn find_uid_tag_returns_none_for_empty_list() {
        assert_eq!(find_uid_tag(&[]), None);
    }

    #[test]
    fn find_uid_tag_ignores_tags_containing_uid_in_middle() {
        // Only suffix match counts
        let tags = vec!["vsc-myuid-project-a1b2c3d4"];
        assert_eq!(find_uid_tag(&tags), None);
    }

    #[test]
    fn find_uid_tag_matches_full_docker_tag_format() {
        // docker image inspect --format={{range .RepoTags}}{{.}}\n{{end}} produces "repo:tag" strings
        let tags = vec!["vsc-foo-a1b2c3d4-uid:latest"];
        assert_eq!(find_uid_tag(&tags), Some("vsc-foo-a1b2c3d4-uid:latest"));
    }

    #[test]
    fn find_uid_tag_rejects_build_image_with_tag_suffix() {
        // Build image with :tag suffix should be rejected (no -uid in repo name)
        let tags = vec!["vsc-foo-a1b2c3d4:latest"];
        assert_eq!(find_uid_tag(&tags), None);
    }

    #[test]
    fn find_uid_tag_matches_non_latest_tag() {
        // Should work with any docker tag, not just :latest
        let tags = vec!["vsc-foo-a1b2c3d4-uid:20240101"];
        assert_eq!(find_uid_tag(&tags), Some("vsc-foo-a1b2c3d4-uid:20240101"));
    }

    // --- is_runtime_image_tag ---

    #[test]
    fn is_runtime_image_tag_matches_standard_runtime_image() {
        assert!(is_runtime_image_tag("vsc-dcx-a1b2c3d4-uid"));
    }

    #[test]
    fn is_runtime_image_tag_matches_any_project_name() {
        assert!(is_runtime_image_tag("vsc-epsilon-kms-a1b2c3d4-uid"));
    }

    #[test]
    fn is_runtime_image_tag_rejects_build_image() {
        // Build image: vsc-* without -uid suffix must NOT match
        assert!(!is_runtime_image_tag("vsc-dcx-a1b2c3d4"));
    }

    #[test]
    fn is_runtime_image_tag_rejects_build_image_any_project() {
        assert!(!is_runtime_image_tag("vsc-epsilon-kms-a1b2c3d4"));
    }

    #[test]
    fn is_runtime_image_tag_rejects_non_vsc_prefix() {
        assert!(!is_runtime_image_tag("myapp-container-uid"));
    }

    #[test]
    fn is_runtime_image_tag_rejects_empty_string() {
        assert!(!is_runtime_image_tag(""));
    }

    #[test]
    fn is_runtime_image_tag_rejects_uid_in_middle() {
        // "-uid" must be a suffix, not appear in the middle
        assert!(!is_runtime_image_tag("vsc-uid-project-a1b2c3d4"));
    }

    #[test]
    fn is_runtime_image_tag_matches_with_docker_tag_suffix() {
        // docker images --format "{{.Repository}}:{{.Tag}}" produces "repo:latest"
        // is_runtime_image_tag must still match when the docker tag (:latest) is appended
        assert!(is_runtime_image_tag("vsc-dcx-a1b2c3d4-uid:latest"));
    }

    #[test]
    fn is_runtime_image_tag_rejects_build_image_with_docker_tag_suffix() {
        // Build image with :latest suffix should still be rejected
        assert!(!is_runtime_image_tag("vsc-dcx-a1b2c3d4:latest"));
    }

    // --- is_build_image_tag ---

    #[test]
    fn is_build_image_tag_matches_build_image() {
        assert!(is_build_image_tag("vsc-dcx-a1b2c3d4"));
    }

    #[test]
    fn is_build_image_tag_matches_build_image_with_docker_tag_suffix() {
        assert!(is_build_image_tag("vsc-dcx-a1b2c3d4:latest"));
    }

    #[test]
    fn is_build_image_tag_rejects_runtime_image() {
        assert!(!is_build_image_tag("vsc-dcx-a1b2c3d4-uid"));
    }

    #[test]
    fn is_build_image_tag_rejects_runtime_image_with_docker_tag_suffix() {
        assert!(!is_build_image_tag("vsc-dcx-a1b2c3d4-uid:latest"));
    }

    #[test]
    fn is_build_image_tag_rejects_non_vsc_prefix() {
        assert!(!is_build_image_tag("myapp-a1b2c3d4"));
    }

    // --- build_image_to_runtime_image ---

    #[test]
    fn build_image_to_runtime_image_adds_uid_suffix() {
        assert_eq!(
            build_image_to_runtime_image("vsc-dcx-a1b2c3d4"),
            "vsc-dcx-a1b2c3d4-uid"
        );
    }

    #[test]
    fn build_image_to_runtime_image_preserves_tag() {
        assert_eq!(
            build_image_to_runtime_image("vsc-dcx-a1b2c3d4:latest"),
            "vsc-dcx-a1b2c3d4-uid:latest"
        );
    }

    #[test]
    fn build_image_to_runtime_image_handles_complex_names() {
        assert_eq!(
            build_image_to_runtime_image("vsc-my-project-xyz-a1b2c3d4:latest"),
            "vsc-my-project-xyz-a1b2c3d4-uid:latest"
        );
    }

    #[test]
    fn build_image_to_runtime_image_handles_custom_tag() {
        assert_eq!(
            build_image_to_runtime_image("vsc-dcx-a1b2c3d4:dev"),
            "vsc-dcx-a1b2c3d4-uid:dev"
        );
    }

    // --- get_base_image_name ---

    #[test]
    fn get_base_image_name_reads_devcontainer_json() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let dc_dir = dir.path().join(".devcontainer");
        fs::create_dir(&dc_dir).unwrap();
        fs::write(
            dc_dir.join("devcontainer.json"),
            r#"{"image":"test-image:latest"}"#,
        )
        .unwrap();
        assert_eq!(
            get_base_image_name(dir.path(), None),
            Some("test-image:latest".to_string())
        );
    }

    #[test]
    fn get_base_image_name_falls_back_to_root_devcontainer_json() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".devcontainer.json"),
            r#"{"image":"root-image:v2"}"#,
        )
        .unwrap();
        assert_eq!(
            get_base_image_name(dir.path(), None),
            Some("root-image:v2".to_string())
        );
    }

    #[test]
    fn get_base_image_name_returns_none_when_no_config() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(get_base_image_name(dir.path(), None), None);
    }

    #[test]
    fn get_base_image_name_returns_none_when_no_image_field() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let dc_dir = dir.path().join(".devcontainer");
        fs::create_dir(&dc_dir).unwrap();
        fs::write(
            dc_dir.join("devcontainer.json"),
            r#"{"name":"My Dev","build":{}}"#,
        )
        .unwrap();
        assert_eq!(get_base_image_name(dir.path(), None), None);
    }

    #[test]
    fn get_base_image_name_uses_explicit_config_path() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("custom.json");
        fs::write(&config_path, r#"{"image":"custom-image:v3"}"#).unwrap();
        assert_eq!(
            get_base_image_name(dir.path(), Some(&config_path)),
            Some("custom-image:v3".to_string())
        );
    }

    #[test]
    fn get_base_image_name_explicit_config_ignores_workspace_default() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        // Put one image in the workspace default location...
        let dc_dir = dir.path().join(".devcontainer");
        fs::create_dir(&dc_dir).unwrap();
        fs::write(
            dc_dir.join("devcontainer.json"),
            r#"{"image":"workspace-image:latest"}"#,
        )
        .unwrap();
        // ...and a different one in the explicit config.
        let config_path = dir.path().join("full").join("devcontainer.json");
        fs::create_dir(dir.path().join("full")).unwrap();
        fs::write(&config_path, r#"{"image":"full-image:latest"}"#).unwrap();
        assert_eq!(
            get_base_image_name(dir.path(), Some(&config_path)),
            Some("full-image:latest".to_string())
        );
    }
}
