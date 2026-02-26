#!/usr/bin/env bash
# Common helpers for dcx E2E tests.
# Source this file at the top of each test script: source "$(dirname "$0")/setup.sh"

set -euo pipefail

# Locate the dcx binary: prefer the debug build next to this file.
DCX=${DCX:-$(cd "$(dirname "$0")/../.." && pwd)/target/debug/dcx}

if [ ! -x "$DCX" ]; then
    echo "ERROR: dcx binary not found at $DCX — run 'cargo build' first"
    exit 1
fi

# --- Skip guards ---

# Call at the top of any test that requires the full stack.
require_e2e_deps() {
    for cmd in colima docker bindfs devcontainer; do
        if ! command -v "$cmd" &>/dev/null; then
            echo "SKIP ALL: '$cmd' not found — E2E tests require Colima + Docker + bindfs"
            exit 0
        fi
    done
    if ! colima status &>/dev/null 2>&1; then
        echo "SKIP ALL: Colima is not running — start it with 'colima start'"
        exit 0
    fi
    if ! docker info &>/dev/null 2>&1; then
        echo "SKIP ALL: Docker is not available"
        exit 0
    fi
}

# Call at the top of tests that only need Docker + devcontainer (no Colima/bindfs).
require_docker_deps() {
    for cmd in docker devcontainer; do
        if ! command -v "$cmd" &>/dev/null; then
            echo "SKIP ALL: '$cmd' not found"
            exit 0
        fi
    done
    if ! docker info &>/dev/null 2>&1; then
        echo "SKIP ALL: Docker is not available"
        exit 0
    fi
}

# --- Test counters ---

PASS=0
FAIL=0

pass() {
    echo "  PASS: $1"
    ((PASS += 1))
}

fail() {
    echo "  FAIL: $1"
    ((FAIL += 1))
    # Print a stack trace to help locate the failing test.
    local i=0
    while caller $i; do ((i += 1)); done
}

summary() {
    echo ""
    echo "Results: ${PASS} passed, ${FAIL} failed"
    if [ "${FAIL}" -gt 0 ]; then
        exit 1
    fi
}

# --- Assertion helpers ---

assert_exit() {
    local label="$1" expected="$2" actual="$3"
    if [ "$actual" -eq "$expected" ]; then
        pass "$label (exit $actual)"
    else
        fail "$label — expected exit $expected, got $actual"
    fi
}

assert_contains() {
    local label="$1" haystack="$2" needle="$3"
    if [[ "$haystack" == *"$needle"* ]]; then
        pass "$label (contains '$needle')"
    else
        fail "$label — expected to contain '$needle', got: $haystack"
    fi
}

assert_not_contains() {
    local label="$1" haystack="$2" needle="$3"
    if [[ "$haystack" != *"$needle"* ]]; then
        pass "$label (does not contain '$needle')"
    else
        fail "$label — expected NOT to contain '$needle', got: $haystack"
    fi
}

assert_dir_exists() {
    local label="$1" dir="$2"
    if [ -d "$dir" ]; then
        pass "$label (dir exists: $dir)"
    else
        fail "$label — directory does not exist: $dir"
    fi
}

assert_dir_missing() {
    local label="$1" dir="$2"
    if [ ! -d "$dir" ]; then
        pass "$label (dir absent: $dir)"
    else
        fail "$label — directory should not exist: $dir"
    fi
}

# --- Workspace helpers ---

# Track workspaces created during this test session so we can clean them up individually.
# This avoids using dcx clean --all which would affect other concurrent workspaces.
declare -a TRACKED_WORKSPACES=()

# Create a minimal devcontainer workspace in a new temp dir.
# Prints the path to the created workspace.
# Automatically tracks the workspace for cleanup.
make_workspace() {
    local tmpdir
    tmpdir=$(mktemp -d)
    mkdir -p "$tmpdir/.devcontainer"
    cat >"$tmpdir/.devcontainer/devcontainer.json" <<'EOF'
{
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
}
EOF
    TRACKED_WORKSPACES+=("$tmpdir")
    echo "$tmpdir"
}

# Check whether a path is currently FUSE-mounted (Linux + macOS).
is_mounted() {
    local path="$1"
    if [ -f /proc/mounts ]; then
        grep -q " $path " /proc/mounts 2>/dev/null
    else
        mount | grep -q " $path " 2>/dev/null
    fi
}

# Clean up only the workspaces created by this test session.
# Uses dcx down --workspace-folder for each tracked workspace (not dcx clean).
# dcx down is safe: it only stops the container and unmounts the relay for the
# specific workspace. It does NOT scan for orphaned mounts, does NOT run global
# Docker image/container cleanup, and cannot affect other workspaces.
e2e_cleanup() {
    local ws
    for ws in "${TRACKED_WORKSPACES[@]}"; do
        "$DCX" down --workspace-folder "$ws" 2>/dev/null || true
    done
    TRACKED_WORKSPACES=()
}
