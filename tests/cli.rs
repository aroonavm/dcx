use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn dcx() -> Command {
    cargo_bin_cmd!("dcx")
}

// --- --help / --version ---

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

// Subcommand help text is validated by clap; spot-checking one subcommand is sufficient.

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
        .env_remove("DCX_DEVCONTAINER_CONFIG_PATH")
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
fn up_network_flag_is_accepted() {
    // `dcx up --network open --dry-run` must not fail with exit 2 (clap parse error).
    // It may exit 0 (plan printed) or 1 (Docker unavailable).
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
            "--network",
            "open",
            "--dry-run",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let exit_code = out.status.code();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        exit_code == Some(0) || exit_code == Some(1),
        "expected exit 0 or 1, got {exit_code:?}; stderr: {stderr}"
    );
}

#[test]
fn up_dry_run_without_devcontainer_config_exits_nonzero() {
    // --dry-run still validates before printing the plan.
    // exit 1 if Docker is unavailable; exit 2 if Docker is available.
    dcx()
        .env_remove("DCX_DEVCONTAINER_CONFIG_PATH")
        .args(["up", "--dry-run", "--workspace-folder", "/tmp"])
        .assert()
        .failure();
}

#[test]
fn up_nonexistent_relay_path_exits_nonzero() {
    // A relay-style path that does not exist on disk causes workspace resolution
    // to fail (exit 2) or Docker check to fail (exit 1) — either way, non-zero.
    // Note: this does not reach the recursive-mount guard (step 3 in run_up) because
    // the path does not exist and resolve_workspace returns Err first (step 2).
    // The actual recursive-mount guard is exercised by the E2E tests where the relay
    // dir can be made to exist.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let relay_path = format!("{home}/.colima-mounts/dcx-test-a1b2c3d4");
    dcx()
        .args(["up", "--workspace-folder", &relay_path])
        .assert()
        .failure();
}

#[test]
fn up_with_nonexistent_config_exits_nonzero() {
    // --config pointing to a missing file must fail (exit 2 if Docker available, 1 if not).
    use assert_fs::TempDir;
    let workspace = TempDir::new().unwrap();
    dcx()
        .args([
            "up",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
            "--config",
            "/nonexistent/__dcx_test_config__.json",
        ])
        .assert()
        .failure();
}

#[test]
fn up_dry_run_with_explicit_config_shows_config_in_plan() {
    // --config must appear in the dry-run plan output.
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    let workspace = TempDir::new().unwrap();
    let config = workspace.child("custom/devcontainer.json");
    config.touch().unwrap();
    let out = dcx()
        .args([
            "up",
            "--dry-run",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
            "--config",
            config.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    if out.status.code() == Some(0) {
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("--config"),
            "dry-run output must contain '--config', got: {stdout}"
        );
    } else {
        assert_eq!(
            out.status.code(),
            Some(1),
            "expected exit 0 (plan printed) or exit 1 (no Docker), got: {:?}",
            out.status.code()
        );
    }
}

#[test]
fn up_dry_run_uses_env_var_config() {
    // When DCX_DEVCONTAINER_CONFIG_PATH is set and no --config is passed,
    // the env var path should appear in the dry-run output.
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    let workspace = TempDir::new().unwrap();
    let config = workspace.child("custom/devcontainer.json");
    config.touch().unwrap();
    let out = dcx()
        .env(
            "DCX_DEVCONTAINER_CONFIG_PATH",
            config.path().to_str().unwrap(),
        )
        .args([
            "up",
            "--dry-run",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    if out.status.code() == Some(0) {
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("--config"),
            "dry-run output must contain '--config' when env var is set, got: {stdout}"
        );
        assert!(
            stdout.contains(config.path().to_str().unwrap()),
            "dry-run output must contain the config path from env var, got: {stdout}"
        );
    } else {
        assert_eq!(
            out.status.code(),
            Some(1),
            "expected exit 0 (plan printed) or exit 1 (no Docker), got: {:?}",
            out.status.code()
        );
    }
}

#[test]
fn up_config_flag_overrides_env_var() {
    // When both DCX_DEVCONTAINER_CONFIG_PATH and --config are provided,
    // --config must take precedence.
    use assert_fs::TempDir;
    use assert_fs::prelude::*;
    let workspace = TempDir::new().unwrap();
    let env_config = workspace.child("env/devcontainer.json");
    env_config.touch().unwrap();
    let flag_config = workspace.child("flag/devcontainer.json");
    flag_config.touch().unwrap();
    let out = dcx()
        .env(
            "DCX_DEVCONTAINER_CONFIG_PATH",
            env_config.path().to_str().unwrap(),
        )
        .args([
            "up",
            "--dry-run",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
            "--config",
            flag_config.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    if out.status.code() == Some(0) {
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(flag_config.path().to_str().unwrap()),
            "dry-run output must contain the --config path (flag), got: {stdout}"
        );
        assert!(
            !stdout.contains(env_config.path().to_str().unwrap()),
            "dry-run output must not contain the env var path, got: {stdout}"
        );
    } else {
        assert_eq!(
            out.status.code(),
            Some(1),
            "expected exit 0 (plan printed) or exit 1 (no Docker), got: {:?}",
            out.status.code()
        );
    }
}

#[test]
fn up_env_var_config_nonexistent_exits_nonzero() {
    // When DCX_DEVCONTAINER_CONFIG_PATH points to a nonexistent file and no --config
    // is provided, the command must fail (exit 2 if Docker available, 1 if not).
    use assert_fs::TempDir;
    let workspace = TempDir::new().unwrap();
    dcx()
        .env(
            "DCX_DEVCONTAINER_CONFIG_PATH",
            "/nonexistent/__dcx_test_env_config__.json",
        )
        .args([
            "up",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn exec_env_var_config_nonexistent_exits_nonzero() {
    // When DCX_DEVCONTAINER_CONFIG_PATH points to a nonexistent file and no --config
    // is provided, `dcx exec` must fail (exit 2 if Docker available, 1 if not).
    use assert_fs::TempDir;
    let workspace = TempDir::new().unwrap();
    dcx()
        .env(
            "DCX_DEVCONTAINER_CONFIG_PATH",
            "/nonexistent/__dcx_test_env_config__.json",
        )
        .args([
            "exec",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
            "true",
        ])
        .assert()
        .failure();
}

// --- dcx doctor ---

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
fn exec_multiple_args_are_accepted_by_arg_parser() {
    // `dcx exec -- cmd arg1 arg2` must not fail due to argument parsing.
    // The actual exec will fail (no mount / no Docker) but the error must not
    // be a clap "unexpected argument" error (exit 2).
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
            "echo",
            "hello",
            "world",
        ])
        .output()
        .unwrap();
    assert_ne!(
        out.status.code(),
        Some(2),
        "multiple exec args should not cause a clap parse error"
    );
}

#[test]
fn exec_with_nonexistent_config_exits_nonzero() {
    // --config pointing to a missing file must fail (exit 2 if Docker available, 1 if not).
    use assert_fs::TempDir;
    let workspace = TempDir::new().unwrap();
    dcx()
        .args([
            "exec",
            "--workspace-folder",
            workspace.path().to_str().unwrap(),
            "--config",
            "/nonexistent/__dcx_test_config__.json",
            "true",
        ])
        .assert()
        .failure();
}

// --- dcx down ---

#[test]
fn down_missing_workspace_exits_nonzero() {
    // Nonexistent workspace path must fail.
    // exit 1 if Docker is unavailable; exit 2 if Docker is available (USAGE_ERROR).
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
fn clean_flags_are_accepted() {
    // --purge, --yes, and --all must all be recognised flags (not rejected by clap).
    use assert_fs::TempDir;
    for flags in [
        vec!["clean", "--purge", "--dry-run"],
        vec!["clean", "--yes"],
        vec!["clean", "--all", "--dry-run"],
    ] {
        let home = TempDir::new().unwrap();
        let out = dcx()
            .env("HOME", home.path())
            .args(&flags)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("error: unexpected argument"),
            "{flags:?} should not be rejected as unknown, got stderr: {stderr}"
        );
    }
}

#[test]
fn clean_dry_run_empty_relay_exits_success() {
    // --dry-run with empty relay should exit 0
    use assert_fs::TempDir;
    let home = TempDir::new().unwrap();
    let out = dcx()
        .env("HOME", home.path())
        .args(["clean", "--dry-run"])
        .output()
        .unwrap();
    // Exit success OR Docker not available error
    assert!(
        out.status.success()
            || String::from_utf8_lossy(&out.stderr).contains("Docker is not available"),
        "Exit code: {:?}",
        out.status
    );
}

#[test]
fn clean_include_base_image_flag_is_rejected() {
    // Old --include-base-image flag must not be accepted (no backward compat)
    use assert_fs::TempDir;
    let home = TempDir::new().unwrap();
    let out = dcx()
        .env("HOME", home.path())
        .args(["clean", "--include-base-image"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && stderr.contains("error"),
        "Flag should be rejected. stderr: {stderr}"
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
fn completions_all_supported_shells_exit_zero() {
    for shell in ["bash", "zsh", "fish"] {
        dcx().args(["completions", shell]).assert().success();
    }
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
