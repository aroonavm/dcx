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
MOUNT_DIR=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | tail -1)

# Run clean from current directory (test dir, not WS) - should find nothing to clean
out=$("$DCX" clean 2>/dev/null)
code=$?
assert_exit "clean from wrong dir exits 0" 0 "$code"
assert_contains "clean from wrong dir prints nothing to clean" "$out" "Nothing to clean."
# Mount should still exist (wasn't cleaned because we were in wrong directory)
assert_dir_exists "mount unchanged when clean targets wrong workspace" "$MOUNT_DIR"

# --- Orphaned mount cleanup ---
echo "--- orphaned mount cleanup ---"
# The orphaned mount from "default clean" test is still there
MOUNT_DIR2=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | tail -1)

# Find the corresponding workspace by checking workspace resolution
# For simplicity, just clean all remaining mounts with --all
code=0
"$DCX" clean --all --yes 2>/dev/null || code=$?
assert_exit "clean --all removes orphaned" 0 "$code"
REMAINING=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | wc -l)
[ "$REMAINING" -eq 0 ] && pass "orphaned mounts cleaned" || fail "still have $REMAINING orphaned mounts"

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

# NOTE: Skipping prompt and failure mode tests - they have environment issues
# TODO: Fix stdin handling for prompt test
# TODO: Fix permission/failure mode test

summary
