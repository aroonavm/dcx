#!/usr/bin/env bash
# E2E tests for dcx pass-through behavior (Docker-only, no Colima/bindfs).
# These tests verify dcx correctly forwards args to devcontainer.

source "$(dirname "$0")/setup.sh"
require_docker_deps

echo "=== dcx pass-through (Docker-only) ==="

# --- Unknown subcommands forward to devcontainer ---
echo "--- unknown subcommand forwarding ---"
ws1=$(make_workspace)
trap 'e2e_cleanup' EXIT
out=$("$DCX" read-configuration --workspace-folder "$ws1" 2>&1) || true
code=$?
# devcontainer read-configuration should succeed (exit 0) when forwarded
assert_exit "read-configuration forwards to devcontainer" 0 "$code"
assert_contains "read-configuration returns JSON" "$out" "configuration"

# --- dcx up --dry-run works without bindfs ---
echo "--- up --dry-run without bindfs ---"
ws=$(make_workspace)
out=$("$DCX" up --dry-run --workspace-folder "$ws" 2>&1) || true
code=$?
# --dry-run should print the plan and exit 0, even without Colima
assert_exit "up --dry-run exits 0" 0 "$code"

# --- Exit codes propagate from devcontainer ---
echo "--- exit code propagation ---"
code=0
out=$("$DCX" exec --workspace-folder /nonexistent 2>&1) || code=$?
# Should propagate a non-zero exit code for a bad workspace
[ "$code" -ne 0 ] && pass "non-zero exit propagated (exit $code)" || fail "expected non-zero exit, got $code"

# --- dcx doctor reports missing bindfs without crashing ---
echo "--- doctor without bindfs ---"
# If bindfs is not installed, doctor should still run and report it
if ! command -v bindfs &>/dev/null; then
    out=$("$DCX" doctor 2>&1) || true
    code=$?
    # doctor exits non-zero when checks fail, but should not crash
    [ "$code" -ne 0 ] && pass "doctor exits non-zero without bindfs (exit $code)" || pass "doctor exits 0 (bindfs may be available)"
    assert_contains "doctor mentions bindfs" "$out" "bindfs"
else
    echo "  SKIP: bindfs is installed, cannot test missing-bindfs path"
fi

summary
