#!/usr/bin/env bash
# E2E tests for file staging via dcx_config.yaml and --file flag.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx file staging ==="

RELAY="$HOME/.colima-mounts"

# --- File staging via dcx_config.yaml ---
echo "--- file staging via dcx_config.yaml ---"
WS=$(make_workspace)

# Create a test file on the same filesystem as the relay dir (same as ~/.colima-mounts).
# Using a file inside the relay parent so hardlink succeeds (same filesystem).
TEST_FILE=$(mktemp "$RELAY/.dcx-e2e-testfile-XXXXXX")
echo "host content" > "$TEST_FILE"

# Write dcx_config.yaml into the workspace's .devcontainer dir.
cat > "$WS/.devcontainer/dcx_config.yaml" <<EOF
up:
  files:
    - path: $TEST_FILE
EOF

trap '
    e2e_cleanup
    RELAY_NAME=$(basename "$(relay_dir_for "$WS")")
    rm -rf "${RELAY}/.${RELAY_NAME}-files" 2>/dev/null || true
    rm -f "$TEST_FILE"
    rm -rf "$WS"
' EXIT

code=0
"$DCX" up --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "up exits 0 with dcx_config.yaml file" 0 "$code"

# Verify the file is visible inside the container at its original path.
CONTENT=$("$DCX" exec --workspace-folder "$WS" -- cat "$TEST_FILE" 2>/dev/null)
assert_eq "file is readable inside container" "host content" "$CONTENT"

# Verify hardlink bidirectionality: write inside container → read on host.
"$DCX" exec --workspace-folder "$WS" -- bash -c "echo 'container write' > '$TEST_FILE'" 2>/dev/null
HOST_CONTENT=$(cat "$TEST_FILE")
assert_eq "write inside container propagates to host file" "container write" "$HOST_CONTENT"

# Verify staging dir exists alongside relay dir.
RELAY_DIR=$(relay_dir_for "$WS")
RELAY_NAME=$(basename "$RELAY_DIR")
STAGING_DIR="${RELAY}/.${RELAY_NAME}-files"
assert_dir_exists "staging dir exists" "$STAGING_DIR"

# Bring down and verify staging dir is cleaned up.
"$DCX" down --workspace-folder "$WS" 2>/dev/null
assert_dir_missing "staging dir removed after dcx down" "$STAGING_DIR"

echo ""
echo "=== file staging via --file flag ==="

WS2=$(make_workspace)
TEST_FILE2=$(mktemp "$RELAY/.dcx-e2e-testfile2-XXXXXX")
echo "flag content" > "$TEST_FILE2"

trap '
    e2e_cleanup
    RELAY_NAME2=$(basename "$(relay_dir_for "$WS2")")
    rm -rf "${RELAY}/.${RELAY_NAME2}-files" 2>/dev/null || true
    rm -f "$TEST_FILE2"
    rm -rf "$WS2"
' EXIT

code2=0
"$DCX" up --workspace-folder "$WS2" --file "$TEST_FILE2" 2>/dev/null || code2=$?
assert_exit "up exits 0 with --file flag" 0 "$code2"

# Verify the file is visible inside the container.
CONTENT2=$("$DCX" exec --workspace-folder "$WS2" -- cat "$TEST_FILE2" 2>/dev/null)
assert_eq "flagged file is readable inside container" "flag content" "$CONTENT2"

"$DCX" down --workspace-folder "$WS2" 2>/dev/null

summary
