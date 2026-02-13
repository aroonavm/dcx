#!/usr/bin/env bash
# E2E tests for `dcx exec`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx exec ==="

RELAY="$HOME/.colima-mounts"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

# Bring up the workspace first.
"$DCX" up --workspace-folder "$WS" 2>/dev/null

# --- Happy path: run a command ---
echo "--- happy path ---"
out=$("$DCX" exec --workspace-folder "$WS" -- echo hello 2>/dev/null)
code=$?
assert_exit "exec exits 0" 0 "$code"
assert_contains "exec stdout contains hello" "$out" "hello"

# --- Exit code pass-through ---
echo "--- exit code passthrough ---"
code=0
"$DCX" exec --workspace-folder "$WS" -- sh -c 'exit 42' 2>/dev/null || code=$?
assert_exit "exec passes exit code 42" 42 "$code"

# --- No mount: fails with correct message ---
echo "--- no mount error ---"
WS2=$(make_workspace)
err=$("$DCX" exec --workspace-folder "$WS2" -- true 2>&1) || true
code=$?
[ "$code" -ne 0 ] && pass "exec without mount exits non-zero" || fail "exec without mount should fail"
assert_contains "exec without mount shows error" "$err" "No mount found"
rm -rf "$WS2"

# --- Recursive mount guard ---
echo "--- recursive mount guard ---"
code=0
"$DCX" exec --workspace-folder "${RELAY}/dcx-test-00000000" -- true 2>/dev/null || code=$?
[ "$code" -ne 0 ] && pass "recursive guard exits non-zero" || fail "recursive guard should fail"

# --- Stale mount ---
echo "--- stale mount ---"
MOUNT_DIR=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1)
# Simulate stale FUSE: unmount without removing the directory.
if [ -f /proc/mounts ]; then
    fusermount -u "$MOUNT_DIR" 2>/dev/null || true
else
    umount "$MOUNT_DIR" 2>/dev/null || true
fi
code=0
err=$("$DCX" exec --workspace-folder "$WS" -- true 2>&1) || code=$?
[ "$code" -ne 0 ] && pass "exec with stale mount exits non-zero" || fail "exec with stale mount should fail"
assert_contains "exec with stale mount shows stale error" "$err" "Mount is stale"
# Remount so subsequent tests can proceed.
"$DCX" up --workspace-folder "$WS" 2>/dev/null

# --- Progress output on stderr ---
echo "--- progress output ---"
stderr_out=$("$DCX" exec --workspace-folder "$WS" -- true 2>&1 >/dev/null) || true
assert_contains "exec shows resolving step" "$stderr_out" "→ Resolving workspace path:"

summary
