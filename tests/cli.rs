use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn dcx() -> Command {
    cargo_bin_cmd!("dcx")
}

// --- --help / --version ---

#[test]
fn help_exits_zero() {
    dcx().arg("--help").assert().success();
}

#[test]
fn help_lists_all_managed_subcommands() {
    dcx()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("up"))
        .stdout(predicate::str::contains("exec"))
        .stdout(predicate::str::contains("down"))
        .stdout(predicate::str::contains("clean"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("doctor"));
}

#[test]
fn version_exits_zero() {
    dcx().arg("--version").assert().success();
}

#[test]
fn version_output_contains_binary_name() {
    dcx()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("dcx"));
}

// --- dcx up --help ---

#[test]
fn up_help_shows_workspace_folder() {
    dcx()
        .args(["up", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--workspace-folder"));
}

#[test]
fn up_help_shows_dry_run() {
    dcx()
        .args(["up", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn up_help_shows_yes() {
    dcx()
        .args(["up", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--yes"));
}

// --- dcx exec --help ---

#[test]
fn exec_help_shows_workspace_folder() {
    dcx()
        .args(["exec", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--workspace-folder"));
}

// --- dcx down --help ---

#[test]
fn down_help_shows_workspace_folder() {
    dcx()
        .args(["down", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--workspace-folder"));
}

// --- dcx clean --help ---

#[test]
fn clean_help_shows_all() {
    dcx()
        .args(["clean", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--all"));
}

#[test]
fn clean_help_shows_yes() {
    dcx()
        .args(["clean", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--yes"));
}

// --- dcx up ---

#[test]
fn up_missing_workspace_exits_nonzero() {
    // A workspace path that does not exist must fail.
    // exit 1 if Docker is unavailable; exit 2 if Docker is available.
    dcx()
        .args(["up", "--workspace-folder", "/nonexistent/__dcx_test_path__"])
        .assert()
        .failure();
}

#[test]
fn up_dir_without_devcontainer_config_exits_nonzero() {
    // /tmp exists but has no devcontainer configuration.
    // exit 1 if Docker is unavailable; exit 2 if Docker is available.
    dcx()
        .args(["up", "--workspace-folder", "/tmp"])
        .assert()
        .failure();
}

#[test]
fn up_dry_run_with_valid_workspace_prints_plan() {
    // With a valid workspace+config, --dry-run must print the plan and exit 0.
    // If Docker is unavailable the command exits 1 before reaching dry-run; in
    // that case we only assert the exit code, not the output.
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    let workspace = TempDir::new().unwrap();
    workspace
        .child(".devcontainer/devcontainer.json")
        .touch()
        .unwrap();
    let out = dcx()
        .args([
            "up",
            "--dry-run",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let exit_code = out.status.code();
    if exit_code == Some(0) {
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Would mount:"),
            "dry-run output must contain 'Would mount:', got: {stdout}"
        );
        assert!(
            stdout.contains("Would run:"),
            "dry-run output must contain 'Would run:', got: {stdout}"
        );
        assert!(
            stdout.contains("devcontainer up"),
            "dry-run output must mention 'devcontainer up', got: {stdout}"
        );
    } else {
        // Docker not available — exit 1 is acceptable; nothing to assert about output.
        assert_eq!(
            exit_code,
            Some(1),
            "expected exit 0 (plan printed) or exit 1 (no Docker), got: {exit_code:?}"
        );
    }
}

#[test]
fn up_dry_run_without_devcontainer_config_exits_nonzero() {
    // --dry-run still validates before printing the plan.
    // exit 1 if Docker is unavailable; exit 2 if Docker is available.
    dcx()
        .args(["up", "--dry-run", "--workspace-folder", "/tmp"])
        .assert()
        .failure();
}

#[test]
fn up_recursive_guard_exits_nonzero() {
    // Using a path inside ~/.colima-mounts/dcx-* as the workspace must be rejected.
    // The path doesn't exist so workspace resolution fails first (exit 2) or
    // Docker is unavailable (exit 1) — either way, non-zero.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let relay_path = format!("{home}/.colima-mounts/dcx-test-a1b2c3d4");
    dcx()
        .args(["up", "--workspace-folder", &relay_path])
        .assert()
        .failure();
}

// --- dcx doctor ---

#[test]
fn doctor_exits_zero_or_one_not_two() {
    // In the test environment not all prerequisites will be installed.
    // What matters: dcx doctor must never exit 2 (clap parse error).
    let code = dcx().arg("doctor").output().unwrap().status.code();
    assert_ne!(code, Some(2), "dcx doctor must not exit with a clap error");
}

#[test]
fn doctor_always_prints_checking_prerequisites() {
    // The "Checking prerequisites..." header must appear regardless of check results.
    dcx()
        .arg("doctor")
        .assert()
        .stdout(predicate::str::contains("Checking prerequisites..."));
}

// --- dcx status ---

#[test]
fn status_exits_zero_or_one_not_two() {
    // docker may not be running in CI; exit 0 or 1 are both fine.
    let code = dcx().arg("status").output().unwrap().status.code();
    assert_ne!(code, Some(2), "dcx status must not exit with a clap error");
}

#[test]
fn status_output_is_table_or_no_workspaces() {
    // When docker is not running status exits 1 and prints to stderr.
    // When docker is running with no dcx mounts it prints "No active workspaces."
    // Either way, stdout should not contain a clap error message.
    let out = dcx().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("error:"),
        "dcx status stdout should not contain a clap error: {stdout}"
    );
}

// --- dcx exec ---

#[test]
fn exec_no_mount_exits_nonzero_with_message() {
    // Workspace exists on disk but has no dcx mount.
    // When Docker is available: exit 1 with "No mount found" on stderr.
    // When Docker is unavailable: exit 1 with Docker error on stderr.
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    let workspace = TempDir::new().unwrap();
    workspace
        .child(".devcontainer/devcontainer.json")
        .touch()
        .unwrap();
    let out = dcx()
        .args([
            "exec",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
            "--",
            "true",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "dcx exec with no mount must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("No mount found") || stderr.contains("Docker is not available"),
        "expected 'No mount found' or Docker error on stderr, got: {stderr}"
    );
}

#[test]
fn exec_missing_workspace_exits_nonzero() {
    // A workspace path that does not exist must fail.
    // exit 1 if Docker is unavailable; exit 2 if Docker is available.
    dcx()
        .args([
            "exec",
            "--workspace-folder",
            "/nonexistent/__dcx_test_path__",
            "--",
            "true",
        ])
        .assert()
        .failure();
}

#[test]
fn exec_recursive_guard_exits_nonzero() {
    // Using a path inside ~/.colima-mounts/dcx-* as the workspace must be rejected.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let relay_path = format!("{home}/.colima-mounts/dcx-test-a1b2c3d4");
    dcx()
        .args(["exec", "--workspace-folder", &relay_path, "--", "true"])
        .assert()
        .failure();
}

// --- dcx down ---

#[test]
fn down_missing_workspace_exits_nonzero() {
    // A workspace path that does not exist must fail.
    // exit 1 if Docker is unavailable; exit 2 if Docker is available.
    dcx()
        .args([
            "down",
            "--workspace-folder",
            "/nonexistent/__dcx_test_path__",
        ])
        .assert()
        .failure();
}

#[test]
fn down_recursive_guard_exits_nonzero() {
    // Using a path inside ~/.colima-mounts/dcx-* as the workspace must be rejected.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let relay_path = format!("{home}/.colima-mounts/dcx-test-a1b2c3d4");
    dcx()
        .args(["down", "--workspace-folder", &relay_path])
        .assert()
        .failure();
}

#[test]
fn down_valid_workspace_no_mount_exits_zero_or_one() {
    // /tmp has no dcx mount: exits 0 (Docker available, "Nothing to do")
    // or exits 1 (Docker not available). Must not exit 2 (clap error).
    let code = dcx()
        .args(["down", "--workspace-folder", "/tmp"])
        .output()
        .unwrap()
        .status
        .code();
    assert_ne!(code, Some(2), "dcx down must not exit with a clap error");
}

#[test]
fn down_valid_workspace_no_mount_prints_nothing_to_do_or_docker_error() {
    // When Docker is available and no mount exists, "Nothing to do." must appear on stdout.
    // When Docker is unavailable, stderr gets the Docker error (stdout is empty).
    let out = dcx()
        .args(["down", "--workspace-folder", "/tmp"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("Nothing to do.") || stderr.contains("Docker is not available"),
        "expected 'Nothing to do.' on stdout or Docker error on stderr, got stdout={stdout} stderr={stderr}"
    );
}

// --- dcx clean ---

#[test]
fn clean_exits_zero_or_one_not_two() {
    // Docker may or may not be available. Must not exit 2 (clap parse error).
    let code = dcx().arg("clean").output().unwrap().status.code();
    assert_ne!(code, Some(2), "dcx clean must not exit with a clap error");
}

#[test]
fn clean_nothing_to_clean_with_empty_home() {
    // With an empty HOME (no relay dir), exit 0 (nothing to clean) or exit 1 (no Docker).
    // In either case stdout must not contain a clap error.
    use assert_fs::TempDir;
    let home = TempDir::new().unwrap();
    let out = dcx()
        .env("HOME", home.path())
        .arg("clean")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("error:"),
        "dcx clean stdout must not contain a clap error: {stdout}"
    );
}

#[test]
fn clean_nothing_to_clean_message_when_relay_empty() {
    // When Docker is available and the relay dir is empty, "Nothing to clean." must appear.
    // When Docker is unavailable, stderr gets the error message.
    use assert_fs::TempDir;
    let home = TempDir::new().unwrap();
    let out = dcx()
        .env("HOME", home.path())
        .arg("clean")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("Nothing to clean for") || stderr.contains("Docker is not available"),
        "Expected 'Nothing to clean for' or Docker error, got stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn clean_all_yes_with_empty_relay_prints_nothing_to_clean() {
    // --all --yes with no entries should succeed without a prompt.
    use assert_fs::TempDir;
    let home = TempDir::new().unwrap();
    let out = dcx()
        .env("HOME", home.path())
        .args(["clean", "--all", "--yes"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("Nothing to clean.") || stderr.contains("Docker is not available"),
        "Expected 'Nothing to clean.' or Docker error, got stdout={stdout} stderr={stderr}"
    );
}

// --- Progress output ---

// The progress arrow character (→ U+2192) must appear on stderr when commands
// advance past the initial Docker check. Tests that require Docker use the
// "if exit 0" guard pattern (same as the dry-run test above).

#[test]
fn up_dry_run_emits_progress_to_stderr() {
    // When Docker is available, `dcx up --dry-run` resolves the workspace and
    // prints a `→ Resolving workspace path: ...` step to stderr before the plan.
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    let workspace = TempDir::new().unwrap();
    workspace
        .child(".devcontainer/devcontainer.json")
        .touch()
        .unwrap();
    let out = dcx()
        .args([
            "up",
            "--dry-run",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    if out.status.code() == Some(0) {
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains('\u{2192}'),
            "expected → progress arrow on stderr, got: {stderr}"
        );
        assert!(
            stderr.contains("Resolving workspace path:"),
            "expected 'Resolving workspace path:' in progress output, got: {stderr}"
        );
    }
    // If Docker is unavailable (exit 1), no progress before the docker check — skip.
}

#[test]
fn down_no_mount_emits_progress_to_stderr() {
    // When Docker is available and the workspace exists, `dcx down` prints at least
    // the `→ Resolving workspace path:` step to stderr before "Nothing to do.".
    use assert_fs::TempDir;
    let workspace = TempDir::new().unwrap();
    let out = dcx()
        .args([
            "down",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    if out.status.code() == Some(0) {
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains('\u{2192}'),
            "expected → progress arrow on stderr, got: {stderr}"
        );
        assert!(
            stderr.contains("Resolving workspace path:"),
            "expected 'Resolving workspace path:' in progress output, got: {stderr}"
        );
    }
    // If Docker is unavailable (exit 1), no progress before the docker check — skip.
}

#[test]
fn clean_emits_progress_to_stderr() {
    // When Docker is available, `dcx clean` with an empty relay dir prints
    // `→ Scanning relay directory...` to stderr before "Nothing to clean.".
    use assert_fs::TempDir;
    let home = TempDir::new().unwrap();
    let out = dcx()
        .env("HOME", home.path())
        .arg("clean")
        .output()
        .unwrap();
    if out.status.code() == Some(0) {
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains('\u{2192}'),
            "expected → progress arrow on stderr, got: {stderr}"
        );
        assert!(
            stderr.contains("Scanning relay directory"),
            "expected 'Scanning relay directory' in progress output, got: {stderr}"
        );
    }
    // If Docker is unavailable (exit 1), no progress before the docker check — skip.
}

// --- dcx completions ---

#[test]
fn completions_bash_exits_zero() {
    dcx().args(["completions", "bash"]).assert().success();
}

#[test]
fn completions_zsh_exits_zero() {
    dcx().args(["completions", "zsh"]).assert().success();
}

#[test]
fn completions_fish_exits_zero() {
    dcx().args(["completions", "fish"]).assert().success();
}

#[test]
fn completions_bash_output_is_nonempty() {
    dcx()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn completions_bash_output_mentions_dcx() {
    let out = dcx().args(["completions", "bash"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("dcx"),
        "bash completion output must reference 'dcx', got: {stdout}"
    );
}

#[test]
fn completions_invalid_shell_exits_nonzero() {
    dcx().args(["completions", "tcsh"]).assert().failure();
}

// --- Pass-through ---

#[test]
fn unknown_subcommand_is_not_a_clap_error() {
    // Unknown subcommands are forwarded to `devcontainer`, not rejected by clap.
    // devcontainer is likely not installed in the test env, so we get exit 127
    // (PREREQ_NOT_FOUND). What we must NOT see is exit 2 (clap parse error).
    let output = dcx().arg("__dcx_test_passthrough__").output().unwrap();
    assert_ne!(
        output.status.code(),
        Some(2),
        "unknown subcommand should be passed through, not rejected by clap"
    );
}
