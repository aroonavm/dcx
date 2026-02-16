#!/usr/bin/env bash
# E2E tests for `dcx clean`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx clean ==="

RELAY="$HOME/.colima-mounts"
mkdir -p "$RELAY"

# --- Nothing to clean ---
echo "--- nothing to clean ---"
# Use a fresh relay by temporarily redirecting HOME (if no real dcx-* entries exist).
e2e_cleanup  # ensure we start clean
out=$("$DCX" clean 2>/dev/null)
code=$?
assert_exit "clean exits 0 when nothing to clean" 0 "$code"
assert_contains "clean prints Nothing to clean" "$out" "Nothing to clean for"

# --- Empty dir removed (default clean skips unrelated mounts) ---
echo "--- empty directory removed ---"
mkdir -p "${RELAY}/dcx-empty-test-00000000"
out=$("$DCX" clean 2>/dev/null)
code=$?
assert_exit "clean exits 0 with unrelated mount" 0 "$code"
assert_contains "clean output says nothing to clean" "$out" "Nothing to clean for"
# The unrelated empty mount should still exist (clean only targets current workspace)
assert_dir_exists "unrelated mount left alone" "${RELAY}/dcx-empty-test-00000000"
# Now clean it with --all
"$DCX" clean --all --yes 2>/dev/null
assert_dir_missing "empty dir removed with --all" "${RELAY}/dcx-empty-test-00000000"

# --- Default clean targets current workspace (not all active mounts) ---
echo "--- default clean targets current workspace ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT
"$DCX" up --workspace-folder "$WS" 2>/dev/null
MOUNT_DIR=$(find "${RELAY}" -maxdepth 1 -name 'dcx-*' -type d 2>/dev/null | tail -1)

# Run clean from current directory (test dir, not WS) - should find nothing to clean
out=$("$DCX" clean 2>/dev/null)
code=$?
assert_exit "clean from wrong dir exits 0" 0 "$code"
assert_contains "clean from wrong dir prints nothing to clean" "$out" "Nothing to clean for"
# Mount should still exist (wasn't cleaned because we were in wrong directory)
assert_dir_exists "mount unchanged when clean targets wrong workspace" "$MOUNT_DIR"

# --- Clean with running container (--all): verifies runtime image is removed ---
echo "--- clean with running container (--all) ---"
WS2=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS2"' EXIT
"$DCX" up --workspace-folder "$WS2" 2>/dev/null

# Capture the runtime image name (vsc-dcx-*) before cleaning
RUNTIME_IMG=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | head -1)

code=0
"$DCX" clean --all --yes 2>/dev/null || code=$?
assert_exit "clean --all with container exits 0" 0 "$code"

# Verify all mount directories are gone
REMAINING=$(find "${RELAY}" -maxdepth 1 -name 'dcx-*' -type d 2>/dev/null | wc -l)
[ "$REMAINING" -eq 0 ] && pass "all mounts cleaned" || fail "still have $REMAINING mount entries after clean"

# Verify runtime image (vsc-dcx-*) is gone
if [ -n "$RUNTIME_IMG" ]; then
    RUNTIME_REMAINING=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | wc -l)
    [ "$RUNTIME_REMAINING" -eq 0 ] && pass "runtime image removed" || fail "vsc-dcx-* image still present after clean: $RUNTIME_REMAINING found"
fi

rm -rf "$WS2"

# --- Default clean: verifies runtime image is removed for current workspace ---
echo "--- default clean removes runtime image ---"
WS3=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS3"' EXIT
"$DCX" up --workspace-folder "$WS3" 2>/dev/null

RUNTIME_IMG3=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | head -1)

code=0
"$DCX" clean --workspace-folder "$WS3" --yes 2>/dev/null || code=$?
assert_exit "default clean exits 0" 0 "$code"

if [ -n "$RUNTIME_IMG3" ]; then
    if docker image inspect "$RUNTIME_IMG3" > /dev/null 2>&1; then
        fail "runtime image $RUNTIME_IMG3 still present after default clean"
    else
        pass "runtime image removed by default clean"
    fi
fi

rm -rf "$WS3"

# NOTE: Skipping prompt and failure mode tests - they have environment issues
# TODO: Fix stdin handling for prompt test
# TODO: Fix permission/failure mode test

summary
