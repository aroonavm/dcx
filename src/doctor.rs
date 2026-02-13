#![allow(dead_code)]

use std::path::Path;

use crate::cmd;
use crate::exit_codes;
use crate::format::DoctorCheck;
use crate::naming::relay_dir;
use crate::platform;

/// Extract the first version-like token (`MAJOR.MINOR[.PATCH...]`) from `output`.
///
/// Strips a leading `v` and trailing punctuation before matching. Returns `None`
/// if no token with at least two dot-separated numeric parts is found.
pub fn parse_version_str(output: &str) -> Option<String> {
    for word in output.split_whitespace() {
        let w = word
            .trim_start_matches('v')
            .trim_end_matches([',', ';', '.'].as_slice());
        let parts: Vec<&str> = w.split('.').collect();
        if parts.len() >= 2
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return Some(w.to_string());
        }
    }
    None
}

fn which(prog: &str) -> bool {
    cmd::run_capture("which", &[prog])
        .map(|out| out.status == 0)
        .unwrap_or(false)
}

pub fn check_bindfs() -> DoctorCheck {
    if !which("bindfs") {
        return DoctorCheck {
            name: "bindfs installed".to_string(),
            passed: false,
            detail: Some(platform::bindfs_install_hint().to_string()),
        };
    }
    let version = cmd::run_capture("bindfs", &["--version"])
        .ok()
        .and_then(|out| parse_version_str(&out.stdout).or_else(|| parse_version_str(&out.stderr)));
    DoctorCheck {
        name: "bindfs installed".to_string(),
        passed: true,
        detail: version,
    }
}

pub fn check_devcontainer() -> DoctorCheck {
    if !which("devcontainer") {
        return DoctorCheck {
            name: "devcontainer CLI installed".to_string(),
            passed: false,
            detail: Some(platform::devcontainer_install_hint().to_string()),
        };
    }
    let version = cmd::run_capture("devcontainer", &["--version"])
        .ok()
        .and_then(|out| parse_version_str(&out.stdout).or_else(|| parse_version_str(&out.stderr)));
    DoctorCheck {
        name: "devcontainer CLI installed".to_string(),
        passed: true,
        detail: version,
    }
}

pub fn check_docker() -> DoctorCheck {
    let result = cmd::run_capture("docker", &["info", "--format", "{{.ServerVersion}}"]);
    match result {
        Ok(out) if out.status == 0 => {
            let version = parse_version_str(&out.stdout);
            DoctorCheck {
                name: "Docker available".to_string(),
                passed: true,
                detail: version,
            }
        }
        _ => DoctorCheck {
            name: "Docker available".to_string(),
            passed: false,
            detail: Some("Is Docker/Colima running?".to_string()),
        },
    }
}

pub fn check_colima() -> DoctorCheck {
    let result = cmd::run_capture("colima", &["status"]);
    match result {
        Ok(out) if out.status == 0 => {
            let version = parse_version_str(&out.stdout).or_else(|| parse_version_str(&out.stderr));
            DoctorCheck {
                name: "Colima running".to_string(),
                passed: true,
                detail: version,
            }
        }
        _ => DoctorCheck {
            name: "Colima running".to_string(),
            passed: false,
            detail: Some("Run: colima start".to_string()),
        },
    }
}

pub fn check_unmount_tool() -> DoctorCheck {
    let prog = platform::unmount_prog();
    DoctorCheck {
        name: format!("{prog} installed"),
        passed: which(prog),
        detail: None,
    }
}

pub fn check_relay_exists(home: &Path) -> DoctorCheck {
    let relay = relay_dir(home);
    let exists = relay.is_dir();
    DoctorCheck {
        name: "~/.colima-mounts exists on host".to_string(),
        passed: exists,
        detail: if exists {
            None
        } else {
            Some(format!("Run: mkdir -p {}", relay.display()))
        },
    }
}

pub fn check_relay_in_vm() -> DoctorCheck {
    // First verify the directory is visible inside the VM.
    let ls_ok = cmd::run_capture("colima", &["ssh", "--", "ls", "~/.colima-mounts"])
        .map(|out| out.status == 0)
        .unwrap_or(false);

    if !ls_ok {
        return DoctorCheck {
            name: "~/.colima-mounts mounted in VM (writable)".to_string(),
            passed: false,
            detail: Some(
                "Add ~/.colima-mounts to Colima mounts in colima.yaml and run: colima start"
                    .to_string(),
            ),
        };
    }

    // Verify the mount is writable by creating and removing a test file.
    let writable = cmd::run_capture(
        "colima",
        &[
            "ssh",
            "--",
            "sh",
            "-c",
            "touch ~/.colima-mounts/.dcx-write-test && rm ~/.colima-mounts/.dcx-write-test",
        ],
    )
    .map(|out| out.status == 0)
    .unwrap_or(false);

    DoctorCheck {
        name: "~/.colima-mounts mounted in VM (writable)".to_string(),
        passed: writable,
        detail: if writable {
            None
        } else {
            Some("Check Colima mount permissions for ~/.colima-mounts".to_string())
        },
    }
}

/// Run all prerequisite checks, print the report, and return an exit code.
///
/// Returns `exit_codes::SUCCESS` (0) if all checks pass, `exit_codes::RUNTIME_ERROR` (1)
/// if any check fails.
pub fn run_doctor(home: &Path) -> i32 {
    let checks = vec![
        check_bindfs(),
        check_devcontainer(),
        check_docker(),
        check_colima(),
        check_unmount_tool(),
        check_relay_exists(home),
        check_relay_in_vm(),
    ];
    let all_passed = checks.iter().all(|c| c.passed);
    let report = crate::format::format_doctor_report(&checks);
    println!("{report}");
    if all_passed {
        exit_codes::SUCCESS
    } else {
        exit_codes::RUNTIME_ERROR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_version_str ---

    #[test]
    fn parse_version_basic_semver() {
        assert_eq!(parse_version_str("1.17.2"), Some("1.17.2".to_string()));
    }

    #[test]
    fn parse_version_with_v_prefix() {
        assert_eq!(parse_version_str("v0.71.0"), Some("0.71.0".to_string()));
    }

    #[test]
    fn parse_version_embedded_in_text() {
        assert_eq!(
            parse_version_str("bindfs 1.17.2\n"),
            Some("1.17.2".to_string())
        );
    }

    #[test]
    fn parse_version_two_part_version() {
        assert_eq!(parse_version_str("Docker 27.1"), Some("27.1".to_string()));
    }

    #[test]
    fn parse_version_empty_input() {
        assert_eq!(parse_version_str(""), None);
    }

    #[test]
    fn parse_version_no_version_in_text() {
        assert_eq!(parse_version_str("not a version"), None);
    }

    #[test]
    fn parse_version_returns_first_match() {
        assert_eq!(
            parse_version_str("version 1.0.0 and 2.0.0"),
            Some("1.0.0".to_string())
        );
    }

    #[test]
    fn parse_version_ignores_single_number() {
        // A lone number with no dots is not a version string.
        assert_eq!(parse_version_str("42"), None);
    }

    #[test]
    fn parse_version_four_part_version() {
        assert_eq!(
            parse_version_str("Docker Engine 20.10.17.5"),
            Some("20.10.17.5".to_string())
        );
    }

    #[test]
    fn parse_version_trailing_comma_stripped() {
        assert_eq!(
            parse_version_str("colima version 0.8.1,"),
            Some("0.8.1".to_string())
        );
    }
}
