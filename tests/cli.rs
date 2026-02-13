use assert_cmd::Command;
use predicates::prelude::*;

fn dcx() -> Command {
    Command::cargo_bin("dcx").unwrap()
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
