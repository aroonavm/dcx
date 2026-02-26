#!/usr/bin/env bash
# E2E tests for `dcx status`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx status ==="

RELAY="$HOME/.colima-mounts"

# --- Empty state ---
echo "--- empty state ---"
out=$("$DCX" status 2>/dev/null)
code=$?
assert_exit "status exits 0 with no mounts" 0 "$code"
assert_contains "status says no active workspaces" "$out" "No active workspaces."

# --- Running container shows as running ---
echo "--- running container ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT
"$DCX" up --workspace-folder "$WS" 2>/dev/null

out=$("$DCX" status 2>/dev/null)
code=$?
assert_exit "status exits 0 with running container" 0 "$code"
assert_contains "status shows WORKSPACE header" "$out" "WORKSPACE"
assert_contains "status shows MOUNT header" "$out" "MOUNT"
assert_contains "status shows CONTAINER header" "$out" "CONTAINER"
assert_contains "status shows STATE header" "$out" "STATE"
assert_contains "status shows workspace path" "$out" "$WS"
assert_contains "status shows running state" "$out" "running"

# --- Orphaned mount shows as orphaned ---
echo "--- orphaned mount ---"
MOUNT_DIR=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1)
CONTAINER=$(docker ps --filter "label=devcontainer.local_folder=$MOUNT_DIR" --format "{{.ID}}" 2>/dev/null | head -1)
[ -n "$CONTAINER" ] && docker stop "$CONTAINER" >/dev/null 2>&1 || true

out=$("$DCX" status 2>/dev/null)
assert_contains "status shows orphaned" "$out" "orphaned"

# --- Multiple workspaces ---
echo "--- multiple workspaces ---"
WS2=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS2"' EXIT
"$DCX" up --workspace-folder "$WS2" 2>/dev/null

out=$("$DCX" status 2>/dev/null)
assert_contains "status shows first workspace" "$out" "$WS"
assert_contains "status shows second workspace" "$out" "$WS2"
RUNNING_COUNT=$(echo "$out" | grep -c "running" || true)
[ "$RUNNING_COUNT" -ge 1 ] && pass "at least one running workspace shown" || fail "expected at least one running workspace"

summary
