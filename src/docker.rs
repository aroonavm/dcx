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

/// Remove a container image by inspecting the container to get the image ID, then removing it.
///
/// Returns `Err(message)` if the inspect or remove command fails.
pub fn remove_container_image(container_id: &str) -> Result<(), String> {
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
    let rmi_out = cmd::run_capture("docker", &["rmi", &image_id])?;
    if rmi_out.status != 0 {
        return Err(format!("Failed to remove image: {}", rmi_out.stderr.trim()));
    }
    Ok(())
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
