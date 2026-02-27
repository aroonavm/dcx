#!/usr/bin/env bash
# E2E tests for edge cases documented in architecture.md.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== edge cases ==="

RELAY="$HOME/.colima-mounts"

# --- Hash stability (same workspace → same mount name) ---
echo "--- hash stability ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

"$DCX" up --workspace-folder "$WS" 2>/dev/null
MOUNT1=$(basename "$(relay_dir_for "$WS")")

"$DCX" down --workspace-folder "$WS" 2>/dev/null
"$DCX" up --workspace-folder "$WS" 2>/dev/null
MOUNT2=$(basename "$(relay_dir_for "$WS")")

[ "$MOUNT1" = "$MOUNT2" ] && pass "hash is stable across up/down/up cycles" || fail "mount name changed: $MOUNT1 → $MOUNT2"

# --- Workspace path with spaces ---
echo "--- path with spaces ---"
WS_SPACES=$(mktemp -d -t "dcx e2e XXXXXX")
echo "$WS_SPACES" >> "$_CLEANUP_LIST"
trap 'e2e_cleanup; rm -rf "$WS" "$WS_SPACES"' EXIT
mkdir -p "$WS_SPACES/.devcontainer"
cat >"$WS_SPACES/.devcontainer/devcontainer.json" <<'EOF'
{
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
}
EOF
code=0
"$DCX" up --workspace-folder "$WS_SPACES" 2>/dev/null || code=$?
assert_exit "up handles path with spaces" 0 "$code"
"$DCX" down --workspace-folder "$WS_SPACES" 2>/dev/null
rm -rf "$WS_SPACES"

# --- Sanitized mount name ---
echo "--- mount name sanitization ---"
WS3=$(mktemp -d -t "my.project.XXXXXX")
echo "$WS3" >> "$_CLEANUP_LIST"
trap 'e2e_cleanup; rm -rf "$WS" "$WS3"' EXIT
mkdir -p "$WS3/.devcontainer"
cat >"$WS3/.devcontainer/devcontainer.json" <<'EOF'
{
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
}
EOF
"$DCX" up --workspace-folder "$WS3" 2>/dev/null
# Compute the expected mount name for WS3 (mirrors Rust logic in naming.rs).
MOUNT3=$(basename "$(relay_dir_for "$WS3")")
# The basename of WS3 starts with "my.project." — dot should become hyphen.
[[ "$MOUNT3" == dcx-my-project* ]] && pass "dots sanitized to hyphens in mount name" || fail "expected dcx-my-project* but got ${MOUNT3:-<none>}"
"$DCX" down --workspace-folder "$WS3" 2>/dev/null
rm -rf "$WS3"

# --- Stale mount recovery ---
echo "--- stale mount recovery ---"
WS4=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS4"' EXIT
"$DCX" up --workspace-folder "$WS4" 2>/dev/null
MOUNT_DIR4=$(relay_dir_for "$WS4")
# Simulate stale state: take WS4 down (removes mount + dir), then recreate
# the empty directory. Models a FUSE mount that died without cleanup.
"$DCX" down --workspace-folder "$WS4" 2>/dev/null || true
mkdir -p "$MOUNT_DIR4"
# Now dcx up should recover (leftover dir, not mounted → remount fresh).
code=0
"$DCX" up --workspace-folder "$WS4" 2>/dev/null || code=$?
assert_exit "up recovers from stale mount" 0 "$code"
is_mounted "$MOUNT_DIR4" && pass "mount is healthy after stale recovery" || fail "mount still unhealthy after recovery"

# --- Shell completion is valid bash syntax ---
echo "--- bash completion validity ---"
code=0
bash -c "source <($DCX completions bash)" 2>/dev/null || code=$?
assert_exit "bash completions are valid bash" 0 "$code"

summary
