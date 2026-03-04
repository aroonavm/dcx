#!/usr/bin/env bash
# E2E test for sync daemon handling of atomic writes (temp+rename).
# Verifies that the sync daemon correctly tracks files through inode changes.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== sync daemon atomic write test ==="

RELAY="$HOME/.colima-mounts"
WS=$(make_workspace)

# Create a test file on the same filesystem as the relay dir (same as ~/.colima-mounts).
# Using a file inside the relay parent so hardlink succeeds (same filesystem).
TEST_FILE=$(mktemp "$RELAY/.dcx-e2e-sync-XXXXXX")
echo "initial" > "$TEST_FILE"

# Write dcx_config.yaml with sync: true for the test file.
cat > "$WS/.devcontainer/dcx_config.yaml" <<EOF
up:
  files:
    - path: $TEST_FILE
      sync: true
EOF

trap '
    e2e_cleanup
    RELAY_NAME=$(basename "$(relay_dir_for "$WS")")
    rm -rf "${RELAY}/.${RELAY_NAME}-files" 2>/dev/null || true
    rm -f "$TEST_FILE"
    rm -rf "$WS"
' EXIT

# dcx up (spawns sync daemon watching parent dir, filtering by filename)
code=0
"$DCX" up --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "up exits 0" 0 "$code"

# Compute staging dir path
RELAY_DIR=$(relay_dir_for "$WS")
RELAY_NAME=$(basename "$RELAY_DIR")
STAGING_DIR="${RELAY}/.${RELAY_NAME}-files"
STAGING_FILE="${STAGING_DIR}/$(basename "$TEST_FILE")"

# Verify staging dir was created
assert_dir_exists "staging dir created" "$STAGING_DIR"

# Verify staging file has initial content
assert_contains "staging file has initial content" "$(cat "$STAGING_FILE")" "initial"

# First atomic write: simulate Claude Code auth file update
echo "update1" > "${TEST_FILE}.tmp"
mv "${TEST_FILE}.tmp" "$TEST_FILE"
sleep 1.5

# Verify staging reflects first atomic write
CONTENT_1=$(cat "$STAGING_FILE")
assert_eq "staging reflects first atomic write" "update1" "$CONTENT_1"

# Second atomic write (THIS VERIFIES THE BUG FIX)
# Without the fix (watching files instead of parent dir), this write would be missed
# because the watch descriptor is on an orphaned inode after the first rename.
echo "update2" > "${TEST_FILE}.tmp"
mv "${TEST_FILE}.tmp" "$TEST_FILE"
sleep 1.5

# Verify staging reflects second atomic write
CONTENT_2=$(cat "$STAGING_FILE")
assert_eq "staging reflects second atomic write (bug fix)" "update2" "$CONTENT_2"

# Test scenario: run dcx up a second time on the same workspace
# This verifies that a new daemon is not spawned (duplicate prevention works)
echo ""
echo "=== testing duplicate daemon prevention ==="

# Record the current daemon PID before re-running up
DAEMON_PID_BEFORE=$(cat "$STAGING_DIR/.sync-daemon.pid" 2>/dev/null || echo "")

# Run dcx up again on the same workspace
code=0
"$DCX" up --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "second up exits 0" 0 "$code"

# Count matching sync daemon processes for this workspace
# We search for the PID file path to uniquely identify daemons for this workspace
DAEMON_PID_AFTER=$(cat "$STAGING_DIR/.sync-daemon.pid" 2>/dev/null || echo "")
DAEMON_COUNT=$(pgrep -c -f "dcx _sync-daemon.*$(basename "$pid_file")" 2>/dev/null || echo "0")

# Assert that the PID didn't change (same daemon is still running)
assert_eq "daemon PID unchanged on second up" "$DAEMON_PID_BEFORE" "$DAEMON_PID_AFTER"

# Assert that only one daemon is running (not duplicated)
assert_eq "no duplicate daemon spawned" "1" "$DAEMON_COUNT"

# Clean up: bring down and verify daemon is killed and staging dir is removed
"$DCX" down --workspace-folder "$WS" 2>/dev/null
assert_dir_missing "staging dir removed after down" "$STAGING_DIR"

summary
