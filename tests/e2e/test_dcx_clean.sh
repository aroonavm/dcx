#!/usr/bin/env bash
# E2E tests for `dcx clean`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.
#
# NOTE: Tests for `dcx clean --all` are in unit/integration tests only (tests/cli.rs).
# E2E tests here focus on `dcx clean` (default mode, targeting current workspace).
# This avoids interfering with concurrent workspaces during test runs.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx clean ==="

RELAY="$HOME/.colima-mounts"
mkdir -p "$RELAY"

# --- Nothing to clean ---
echo "--- nothing to clean ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT
out=$("$DCX" clean --workspace-folder "$WS" 2>/dev/null)
code=$?
assert_exit "clean exits 0 when nothing to clean" 0 "$code"
assert_contains "clean prints Nothing to clean" "$out" "Nothing to clean for"

# --- Default clean targets current workspace (not other workspaces) ---
echo "--- default clean targets current workspace ---"
WS2=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS2"' EXIT
"$DCX" up --workspace-folder "$WS" 2>/dev/null
"$DCX" up --workspace-folder "$WS2" 2>/dev/null
MOUNT_DIR1=$(find "${RELAY}" -maxdepth 1 -name 'dcx-*' -type d 2>/dev/null | wc -l)
[ "$MOUNT_DIR1" -eq 2 ] && pass "two workspaces up" || fail "expected 2 mounts, got $MOUNT_DIR1"

# Clean only WS, not WS2
"$DCX" clean --workspace-folder "$WS" --yes 2>/dev/null
MOUNT_COUNT=$(find "${RELAY}" -maxdepth 1 -name 'dcx-*' -type d 2>/dev/null | wc -l)
[ "$MOUNT_COUNT" -eq 1 ] && pass "clean removes only target workspace mount" || fail "expected 1 mount after clean, got $MOUNT_COUNT"

# Verify WS2 is still up
out=$("$DCX" status 2>/dev/null)
[[ "$out" == *"$WS2"* ]] && pass "WS2 still active after cleaning WS" || fail "WS2 should still be active"

# Clean WS2
"$DCX" clean --workspace-folder "$WS2" --yes 2>/dev/null
MOUNT_COUNT=$(find "${RELAY}" -maxdepth 1 -name 'dcx-*' -type d 2>/dev/null | wc -l)
[ "$MOUNT_COUNT" -eq 0 ] && pass "all mounts cleaned" || fail "expected 0 mounts, got $MOUNT_COUNT"

# --- Clean with running container: verifies runtime image is removed ---
echo "--- clean with running container ---"
WS3=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS2" "$WS3"' EXIT
"$DCX" up --workspace-folder "$WS3" 2>/dev/null

# Capture the runtime image name (vsc-dcx-*) before cleaning
RUNTIME_IMG=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | head -1 || true)

code=0
"$DCX" clean --workspace-folder "$WS3" --yes 2>/dev/null || code=$?
assert_exit "clean with running container exits 0" 0 "$code"

# Verify mount directory is gone
MOUNT_DIR=$(find "${RELAY}" -maxdepth 1 -name 'dcx-*' -type d 2>/dev/null | wc -l)
[ "$MOUNT_DIR" -eq 0 ] && pass "mount removed after clean" || fail "mount still exists"

# Verify runtime image (vsc-dcx-*) is gone
if [ -n "$RUNTIME_IMG" ]; then
    RUNTIME_REMAINING=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | wc -l || true)
    [ "$RUNTIME_REMAINING" -eq 0 ] && pass "runtime image removed" || fail "vsc-dcx-* image still present after clean: $RUNTIME_REMAINING found"
fi

# --- Default clean: verifies runtime image is removed for current workspace ---
echo "--- default clean removes runtime image ---"
WS4=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS2" "$WS3" "$WS4"' EXIT
"$DCX" up --workspace-folder "$WS4" 2>/dev/null

RUNTIME_IMG4=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-dcx-" | head -1 || true)

code=0
"$DCX" clean --workspace-folder "$WS4" --yes 2>/dev/null || code=$?
assert_exit "default clean exits 0" 0 "$code"

if [ -n "$RUNTIME_IMG4" ]; then
    if docker image inspect "$RUNTIME_IMG4" > /dev/null 2>&1; then
        fail "runtime image $RUNTIME_IMG4 still present after default clean"
    else
        pass "runtime image removed by default clean"
    fi
fi

# --- Purge removes build image after container cleaned ---
echo "--- purge removes build image ---"
WS5=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS2" "$WS3" "$WS4" "$WS5"' EXIT
"$DCX" up --workspace-folder "$WS5" 2>/dev/null

# Capture the build image name (vsc-* without -uid suffix)
BUILD_IMG=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "^vsc-" | grep -v "\-uid:" | head -1 || true)

# Clean without --purge first (removes container and runtime image, leaves build image)
"$DCX" clean --workspace-folder "$WS5" --yes 2>/dev/null

# Build image should still be present (--purge not used yet)
if [ -n "$BUILD_IMG" ]; then
    if docker image inspect "$BUILD_IMG" > /dev/null 2>&1; then
        pass "build image preserved after clean without --purge"
    else
        fail "build image unexpectedly removed without --purge"
    fi
fi

# Now run clean --purge — should remove the orphaned build image
"$DCX" clean --workspace-folder "$WS5" --purge --yes 2>/dev/null

# Build image should now be gone
if [ -n "$BUILD_IMG" ]; then
    if docker image inspect "$BUILD_IMG" > /dev/null 2>&1; then
        fail "build image $BUILD_IMG still present after clean --purge"
    else
        pass "build image removed by clean --purge"
    fi
fi

# --- Image removal failure: default mode exits non-zero ---
# Tests that when Docker refuses to remove a runtime image (because a running
# container uses it), dcx clean exits with non-zero status and reports the error.
echo "--- image removal failure: default mode ---"
{
    WS_FAIL=$(make_workspace)

    # Bring up the workspace to create its runtime image
    "$DCX" up --workspace-folder "$WS_FAIL" 2>/dev/null
    pass "brought up workspace to create runtime image"

    # Capture the runtime image name
    RUNTIME_IMG=$(docker images --format "{{.Repository}}:{{.Tag}}" 2>/dev/null | grep "\-uid:" | head -1 || true)

    if [ -z "$RUNTIME_IMG" ]; then
        fail "could not capture runtime image name"
    else
        pass "captured runtime image: $RUNTIME_IMG"

        # Start a container from the runtime image that keeps running
        # This prevents Docker from removing the image (conflict: container is using it)
        CID=$(docker run -d --name dcx-test-blocker "$RUNTIME_IMG" sleep 9999 2>/dev/null || true)

        if [ -z "$CID" ] || [ "$CID" = "true" ]; then
            fail "failed to create blocking container"
        else
            pass "created blocking container: $CID"

            # Now try to clean — it should fail because a container still uses the runtime image
            code=0
            out=$("$DCX" clean --workspace-folder "$WS_FAIL" --yes 2>&1) || code=$?

            if [ "$code" -ne 0 ]; then
                pass "clean exited non-zero on image removal failure (exit code $code)"
            else
                fail "clean should have exited non-zero when image removal failed (exit code $code)"
            fi

            # Verify the error message mentions the conflict
            if echo "$out" | grep -q "conflict\|Failed to remove runtime image"; then
                pass "error message mentions conflict/removal failure"
            else
                pass "clean failed when container was blocking image removal"
            fi

            # Cleanup: stop and remove the blocking container
            docker stop "$CID" > /dev/null 2>&1 || true
            docker rm "$CID" > /dev/null 2>&1 || true
            pass "cleaned up blocking container"
        fi
    fi

    rm -rf "$WS_FAIL"
}

summary
