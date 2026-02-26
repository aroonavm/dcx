#!/usr/bin/env bash
# E2E tests for stale mount recovery.
# Tests the scenario where a mount exists with fstype 'fuse' (interrupted/orphaned state).
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx stale mount recovery ==="

RELAY="$HOME/.colima-mounts"
mkdir -p "$RELAY"

# --- Stale mount is detected and cleaned ---
echo "--- stale mount detection and cleanup ---"

# Create a test workspace
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

# Manually create a stale mount with fstype 'fuse' by:
# 1. Creating the mount directory
# 2. Creating the mount using bindfs
# 3. Simulating interruption by directly accessing it
MOUNT_DIR="${RELAY}/dcx-stale-test-00000000"
mkdir -p "$MOUNT_DIR"

# Mount the workspace using bindfs
echo "  Creating stale mount..."
if ! bindfs --no-allow-other "$WS" "$MOUNT_DIR" >/dev/null 2>&1; then
    fail "stale mount setup: bindfs failed"
    rm -rf "$WS"
    exit 1
fi

# Verify the mount exists and is accessible
if ! is_mounted "$MOUNT_DIR"; then
    fail "stale mount setup: mount not created"
    rm -rf "$WS"
    exit 1
fi
pass "stale mount created"

# --- dcx status detects the stale mount ---
echo "  Checking if dcx status sees the stale mount..."
out=$("$DCX" status 2>/dev/null)
code=$?
assert_exit "stale mount status exits 0" 0 "$code"

# The mount should show as "orphaned" (mounted but no running container)
if [[ "$out" == *"orphaned"* ]]; then
    pass "stale mount shows as orphaned in status"
else
    fail "stale mount not shown as orphaned — output: $out"
fi

# Verify the mount directory path is visible in status
if [[ "$out" == *"dcx-stale-test-00000000"* ]]; then
    pass "stale mount path visible in status"
else
    fail "stale mount path not in status output"
fi

# --- dcx clean removes the stale mount ---
echo "  Cleaning stale mount..."
out=$("$DCX" clean 2>/dev/null)
code=$?
assert_exit "clean exits 0" 0 "$code"

# Verify the mount was removed
assert_dir_missing "stale mount removed" "$MOUNT_DIR"
assert_not_contains "no error message" "$out" "ERROR"

# --- Status is empty after cleanup ---
echo "  Verifying status is empty after cleanup..."
out=$("$DCX" status 2>/dev/null)
assert_contains "status shows no active workspaces" "$out" "No active workspaces."

summary
