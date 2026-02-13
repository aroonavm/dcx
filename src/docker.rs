#![allow(dead_code)]

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_docker_available_returns_a_bool_without_panic() {
        // Docker may or may not be running in the test environment.
        // This smoke test verifies the function does not panic.
        let _ = is_docker_available();
    }
}
