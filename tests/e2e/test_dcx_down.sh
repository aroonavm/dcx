#!/usr/bin/env bash
# E2E tests for `dcx down`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx down ==="

RELAY="$HOME/.colima-mounts"

# --- Happy path: up → down ---
echo "--- happy path ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

"$DCX" up --workspace-folder "$WS" 2>/dev/null
MOUNT_DIR=$(relay_dir_for "$WS")

code=0
"$DCX" down --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "down exits 0" 0 "$code"
assert_dir_missing "mount directory removed after down" "$MOUNT_DIR"
! is_mounted "$MOUNT_DIR" && pass "mount not in mount table after down" || fail "mount still in mount table after down"

# Container should be fully removed (not just stopped)
RELAY_DIR=$(relay_dir_for "$WS")
container_after=$(docker ps -a \
    --filter "label=devcontainer.local_folder=$RELAY_DIR" \
    --format "{{.ID}}" 2>/dev/null || true)
[ -z "$container_after" ] && pass "container fully removed after down" \
    || fail "container still in docker ps -a after down: $container_after"

# --- Idempotent: no mount found ---
echo "--- idempotent no-mount ---"
WS2=$(make_workspace)
out=$("$DCX" down --workspace-folder "$WS2" 2>/dev/null)
code=$?
assert_exit "down with no mount exits 0" 0 "$code"
assert_contains "down with no mount prints Nothing to do" "$out" "Nothing to do."
rm -rf "$WS2"

# --- Idempotent: second down after first ---
echo "--- idempotent second down ---"
WS3=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS3"' EXIT
"$DCX" up --workspace-folder "$WS3" 2>/dev/null
"$DCX" down --workspace-folder "$WS3" 2>/dev/null
out=$("$DCX" down --workspace-folder "$WS3" 2>/dev/null)
code=$?
assert_exit "second down exits 0" 0 "$code"
assert_contains "second down says Nothing to do" "$out" "Nothing to do."
rm -rf "$WS3"

# --- Progress output on stderr ---
echo "--- progress output ---"
WS4=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS4"' EXIT
"$DCX" up --workspace-folder "$WS4" 2>/dev/null
stderr_out=$("$DCX" down --workspace-folder "$WS4" 2>&1 >/dev/null)
assert_contains "down shows resolving step" "$stderr_out" "→ Resolving workspace path:"
assert_contains "down shows stopping step" "$stderr_out" "→ Stopping devcontainer..."
assert_contains "down shows unmounting step" "$stderr_out" "→ Unmounting"
assert_contains "down shows done step" "$stderr_out" "→ Done."
rm -rf "$WS4"

# --- Missing workspace ---
echo "--- missing workspace ---"
code=0
"$DCX" down --workspace-folder "/nonexistent/__dcx_e2e_test__" 2>/dev/null || code=$?
[ "$code" -ne 0 ] && pass "down with missing workspace exits non-zero" || fail "down with missing workspace should fail"

summary
