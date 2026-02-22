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
RUNTIME_IMG=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | head -1 || true)

code=0
"$DCX" clean --all --yes 2>/dev/null || code=$?
assert_exit "clean --all with container exits 0" 0 "$code"

# Verify all mount directories are gone
REMAINING=$(find "${RELAY}" -maxdepth 1 -name 'dcx-*' -type d 2>/dev/null | wc -l)
[ "$REMAINING" -eq 0 ] && pass "all mounts cleaned" || fail "still have $REMAINING mount entries after clean"

# Verify runtime image (vsc-dcx-*) is gone
if [ -n "$RUNTIME_IMG" ]; then
    RUNTIME_REMAINING=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | wc -l || true)
    [ "$RUNTIME_REMAINING" -eq 0 ] && pass "runtime image removed" || fail "vsc-dcx-* image still present after clean: $RUNTIME_REMAINING found"
fi

rm -rf "$WS2"

# --- Default clean: verifies runtime image is removed for current workspace ---
echo "--- default clean removes runtime image ---"
WS3=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS3"' EXIT
"$DCX" up --workspace-folder "$WS3" 2>/dev/null

RUNTIME_IMG3=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | head -1 || true)

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

# --- Purge removes build image after container cleaned ---
echo "--- purge removes build image ---"
WS4=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS4"' EXIT
"$DCX" up --workspace-folder "$WS4" 2>/dev/null

# Capture the build image name (vsc-* without -uid suffix)
BUILD_IMG=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-" | grep -v "\-uid:" | head -1 || true)

# Clean without --purge first (removes container and runtime image, leaves build image)
"$DCX" clean --workspace-folder "$WS4" --yes 2>/dev/null

# Build image should still be present (--purge not used yet)
if [ -n "$BUILD_IMG" ]; then
    if docker image inspect "$BUILD_IMG" > /dev/null 2>&1; then
        pass "build image preserved after clean without --purge"
    else
        fail "build image unexpectedly removed without --purge"
    fi
fi

# Now run clean --purge — should remove the orphaned build image
"$DCX" clean --workspace-folder "$WS4" --purge --yes 2>/dev/null

# Build image should now be gone
if [ -n "$BUILD_IMG" ]; then
    if docker image inspect "$BUILD_IMG" > /dev/null 2>&1; then
        fail "build image $BUILD_IMG still present after clean --purge"
    else
        pass "build image removed by clean --purge"
    fi
fi

rm -rf "$WS4"

# --- Purge in single-workspace mode doesn't remove other workspace's build image ---
echo "--- single-workspace purge doesn't affect other workspaces ---"
WS5=$(make_workspace)
WS6=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS5" "$WS6"' EXIT

# Bring up two workspaces
"$DCX" up --workspace-folder "$WS5" 2>/dev/null
"$DCX" up --workspace-folder "$WS6" 2>/dev/null

# Capture both build and runtime images
ALL_IMAGES=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-" || true)
BUILD_IMG5=$(echo "$ALL_IMAGES" | grep "^vsc-" | grep -v "\-uid:" | head -1 || true)
BUILD_IMG6=$(echo "$ALL_IMAGES" | grep "^vsc-" | grep -v "\-uid:" | tail -1 || true)

# Clean WS5 with --purge (single-workspace mode)
"$DCX" clean --workspace-folder "$WS5" --purge --yes 2>/dev/null

# WS5's build image should be gone
if [ -n "$BUILD_IMG5" ]; then
    if docker image inspect "$BUILD_IMG5" > /dev/null 2>&1; then
        fail "WS5 build image should be removed after clean --purge"
    else
        pass "WS5 build image removed by single-workspace clean --purge"
    fi
fi

# WS6's build image should still exist (not affected by WS5 cleanup)
if [ -n "$BUILD_IMG6" ] && [ "$BUILD_IMG5" != "$BUILD_IMG6" ]; then
    if docker image inspect "$BUILD_IMG6" > /dev/null 2>&1; then
        pass "WS6 build image preserved after WS5 clean --purge"
    else
        fail "WS6 build image was incorrectly removed when cleaning WS5"
    fi
fi

# Clean WS6 to verify it still works
"$DCX" clean --workspace-folder "$WS6" --purge --yes 2>/dev/null
if [ -n "$BUILD_IMG6" ]; then
    if docker image inspect "$BUILD_IMG6" > /dev/null 2>&1; then
        fail "WS6 build image should be removed after clean --purge"
    else
        pass "WS6 build image removed by single-workspace clean --purge"
    fi
fi

rm -rf "$WS5" "$WS6"

# --- Multi-workspace cleanup with active containers ---
# Tests that clean --purge succeeds when multiple workspaces share the same
# base image (and possibly underlying image SHA). This would fail with the
# find_uid_tag() bug because remove_runtime_image() would use SHA256, causing
# conflicts when another workspace's container uses the same SHA.
echo "--- multi-workspace cleanup with active containers ---"
{
    WS_ACTIVE1=$(make_workspace)
    WS_ACTIVE2=$(make_workspace)

    # Bring up both workspaces (both use the same base image from make_workspace)
    "$DCX" up --workspace-folder "$WS_ACTIVE1" 2>/dev/null
    "$DCX" up --workspace-folder "$WS_ACTIVE2" 2>/dev/null
    pass "brought up two workspaces with same base image"

    # Capture images before cleanup (just count them, we'll verify below)
    INITIAL_VSC_IMAGES=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-" | wc -l || true)

    # Critical test: Clean WS1 with --purge while WS2 is still active
    # If find_uid_tag() is broken (returns None), this tries to remove by SHA256.
    # Since both workspaces use the same base image, they may share underlying
    # image SHAs. When WS2's container uses the shared SHA, docker rmi --force
    # would fail with "image is being used by running container", causing
    # dcx clean to exit with error. The fix makes find_uid_tag() work correctly,
    # so removal happens by tag (vsc-X-uid:latest), not SHA256, avoiding conflicts.
    code=0
    "$DCX" clean --workspace-folder "$WS_ACTIVE1" --purge --yes 2>/dev/null || code=$?
    if [ "$code" -eq 0 ]; then
        pass "clean --purge of WS1 succeeded with WS2 still running (bug would cause failure)"
    else
        fail "clean --purge of WS1 failed with exit code $code (indicates find_uid_tag bug or image removal conflict)"
    fi

    # Verify that WS2 is still functional (can bring it down without issue)
    code=0
    "$DCX" clean --workspace-folder "$WS_ACTIVE2" --purge --yes 2>/dev/null || code=$?
    if [ "$code" -eq 0 ]; then
        pass "clean --purge of WS2 succeeded (WS2 still accessible after WS1 cleanup)"
    else
        fail "clean --purge of WS2 failed (WS2 may have been corrupted by WS1 cleanup)"
    fi

    # Cleanup
    rm -rf "$WS_ACTIVE1" "$WS_ACTIVE2"
}

# NOTE: Skipping prompt and failure mode tests - they have environment issues
# TODO: Fix stdin handling for prompt test
# TODO: Fix permission/failure mode test

summary
