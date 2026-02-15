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
assert_contains "clean prints Nothing to clean" "$out" "Nothing to clean."

# --- Empty dir removed (default clean skips unrelated mounts) ---
echo "--- empty directory removed ---"
mkdir -p "${RELAY}/dcx-empty-test-00000000"
out=$("$DCX" clean 2>/dev/null)
code=$?
assert_exit "clean exits 0 with unrelated mount" 0 "$code"
assert_contains "clean output says nothing to clean" "$out" "Nothing to clean."
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
MOUNT_DIR=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1)

# Run clean from current directory (test dir, not WS) - should find nothing to clean
out=$("$DCX" clean 2>/dev/null)
code=$?
assert_exit "clean from wrong dir exits 0" 0 "$code"
assert_contains "clean from wrong dir prints nothing to clean" "$out" "Nothing to clean."
# Mount should still exist (wasn't cleaned because we were in wrong directory)
assert_dir_exists "mount unchanged when clean targets wrong workspace" "$MOUNT_DIR"

# --- Orphaned mount cleanup ---
echo "--- orphaned mount cleanup ---"
# Stop the container to create an orphaned mount.
MOUNT_DIR2=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1)
CONTAINER=$(docker ps --filter "label=devcontainer.local_folder=$MOUNT_DIR2" --format "{{.ID}}" 2>/dev/null | head -1)
[ -n "$CONTAINER" ] && docker stop "$CONTAINER" >/dev/null 2>&1 || true

# Clean the specific workspace (not current directory)
out=$("$DCX" clean --workspace-folder "$WS" 2>/dev/null)
code=$?
assert_exit "clean orphan exits 0" 0 "$code"
assert_contains "clean output shows cleaned" "$out" "cleaned"
assert_dir_missing "orphaned mount removed" "$MOUNT_DIR2"

# --- --all --yes cleans everything ---
echo "--- --all --yes ---"
WS2=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS2"' EXIT
"$DCX" up --workspace-folder "$WS2" 2>/dev/null
code=0
"$DCX" clean --all --yes 2>/dev/null || code=$?
assert_exit "clean --all --yes exits 0" 0 "$code"
REMAINING=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | wc -l)
[ "$REMAINING" -eq 0 ] && pass "relay is empty after --all --yes" || fail "relay still has $REMAINING entries"
rm -rf "$WS2"

# --- --all prompts, N aborts ---
echo "--- --all prompts and N aborts ---"
WS3=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS3"' EXIT
"$DCX" up --workspace-folder "$WS3" 2>/dev/null
code=0
echo "n" | "$DCX" clean --all 2>/dev/null || code=$?
assert_exit "clean --all with N exits 4" 4 "$code"
MOUNT_DIR3=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1)
assert_dir_exists "mount left after abort" "$MOUNT_DIR3"
e2e_cleanup
rm -rf "$WS3"

# --- Continue on failure (--all mode) ---
echo "--- continue on failure ---"
mkdir -p "${RELAY}/dcx-ok-00000000"
mkdir -p "${RELAY}/dcx-locked-00000000"
chmod 000 "${RELAY}/dcx-locked-00000000"
out=$("$DCX" clean --all 2>&1) || true
# Clean --all should handle the locked dir and still remove the ok one.
assert_dir_missing "ok dir removed despite sibling failure" "${RELAY}/dcx-ok-00000000"
chmod 755 "${RELAY}/dcx-locked-00000000"
rm -rf "${RELAY}/dcx-locked-00000000"

summary
