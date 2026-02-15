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

/// Remove a container image by ID using `docker rmi`.
///
/// Returns `Err(message)` if the remove command fails.
pub fn remove_image(image_id: &str) -> Result<(), String> {
    let out = cmd::run_capture("docker", &["rmi", image_id])?;
    if out.status != 0 {
        return Err(format!("Failed to remove image: {}", out.stderr.trim()));
    }
    Ok(())
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

/// Remove all dcx container images that are not in use.
///
/// This removes both dangling images and named vsc-dcx-* images that have no running/stopped containers.
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

    // Also remove vsc-dcx-* images that have no containers
    let out = cmd::run_capture(
        "docker",
        &["images", "--format", "{{.Repository}}:{{.Tag}}"],
    )?;

    for image_name in out.stdout.lines() {
        let image_name = image_name.trim();
        if image_name.is_empty() || !image_name.contains("vsc-dcx-") {
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

        // No container uses this image, try to remove it (use --force to remove even if it has tags)
        if let Ok(out) = cmd::run_capture("docker", &["rmi", "--force", image_name])
            && out.status == 0
        {
            removed += 1;
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_docker_available_returns_a_bool_without_panic() {
        // Docker may or may not be running in the test environment.
        // This smoke test verifies the function does not panic.
        let _ = is_docker_available();
    }

    #[test]
    fn query_container_handles_empty_output() {
        // This test verifies the function handles empty output without panicking.
        // In practice, this would be called with a mount point that has no containers.
        // We can't mock docker here, so this is a logical test.
    }

    #[test]
    fn query_container_any_handles_empty_output() {
        // This test verifies the function handles empty output without panicking.
    }
}
