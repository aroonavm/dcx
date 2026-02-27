#!/usr/bin/env bash
# Common helpers for dcx E2E tests.
# Source this file at the top of each test script: source "$(dirname "$0")/setup.sh"

set -euo pipefail

# Prevent the host's DCX_DEVCONTAINER_CONFIG_PATH from leaking into tests.
# Tests that need to test env-var behaviour set it explicitly themselves.
unset DCX_DEVCONTAINER_CONFIG_PATH || true

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

assert_eq() {
    local label="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        pass "$label"
    else
        fail "$label — expected '$expected', got '$actual'"
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
# Uses a temp file (persists across subshells) instead of a bash array (lost in subshells).
_CLEANUP_LIST=$(mktemp)

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
    echo "$tmpdir" >> "$_CLEANUP_LIST"
    echo "$tmpdir"
}

# Compute the expected relay directory path for a given workspace.
# Mirrors the Rust logic in naming.rs exactly so tests can reference specific
# relay dirs without depending on ls ordering or pre-existing relay entries.
#
# Usage: relay_dir=$(relay_dir_for "/path/to/workspace")
relay_dir_for() {
    local ws_abs
    ws_abs=$(realpath "$1" 2>/dev/null || echo "$1")
    local last_component
    last_component=$(basename "$ws_abs")
    # Sanitize: replace non-alphanumeric (except '-') with '-', take first 30 chars.
    # Matches: name.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' }).take(30)
    local sanitized
    sanitized=$(printf '%s' "$last_component" | tr -c 'a-zA-Z0-9-' '-' | head -c 30)
    # Hash: first 8 hex chars of SHA256 of the absolute path.
    # Matches: sha2::Sha256::digest(workspace.to_string_lossy().as_bytes())[..8]
    local hash
    hash=$(printf '%s' "$ws_abs" | sha256sum | head -c 8)
    echo "$HOME/.colima-mounts/dcx-${sanitized}-${hash}"
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
# Operates directly on the relay dir (computed via relay_dir_for) so it works
# even when the workspace dir has already been deleted.
# Stops+removes containers via devcontainer.local_folder label, unmounts the relay
# FUSE mount, removes the relay dir, and removes the runtime image (vsc-*-uid).
e2e_cleanup() {
    [ -f "$_CLEANUP_LIST" ] || return
    local ws
    while IFS= read -r ws; do
        [ -n "$ws" ] || continue
        local relay_dir relay_name
        relay_dir=$(relay_dir_for "$ws")
        relay_name=$(basename "$relay_dir" | tr '[:upper:]' '[:lower:]')

        # Stop and remove containers via devcontainer.local_folder label
        local cid
        while IFS= read -r cid; do
            [ -n "$cid" ] || continue
            docker stop "$cid" 2>/dev/null || true
            docker rm "$cid" 2>/dev/null || true
        done < <(docker ps -a \
            --filter "label=devcontainer.local_folder=$relay_dir" \
            --format "{{.ID}}" 2>/dev/null || true)

        # Unmount and remove the relay dir
        if is_mounted "$relay_dir"; then
            fusermount -u "$relay_dir" 2>/dev/null || true
        fi
        [ -d "$relay_dir" ] && rm -rf "$relay_dir" 2>/dev/null || true

        # Remove runtime image (vsc-*-uid)
        docker images --format "{{.Repository}}:{{.Tag}}" \
            | grep "^vsc-${relay_name}-" \
            | grep -- '-uid' \
            | xargs -r docker rmi 2>/dev/null || true
    done < "$_CLEANUP_LIST"
    > "$_CLEANUP_LIST"
}
