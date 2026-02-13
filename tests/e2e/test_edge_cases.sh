#!/usr/bin/env bash
# E2E tests for edge cases documented in architecture.md.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== edge cases ==="

RELAY="$HOME/.colima-mounts"
e2e_cleanup

# --- Hash stability (same workspace → same mount name) ---
echo "--- hash stability ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

"$DCX" up --workspace-folder "$WS" 2>/dev/null
MOUNT1=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1 | xargs basename)

"$DCX" down --workspace-folder "$WS" 2>/dev/null
"$DCX" up --workspace-folder "$WS" 2>/dev/null
MOUNT2=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1 | xargs basename)

[ "$MOUNT1" = "$MOUNT2" ] && pass "hash is stable across up/down/up cycles" || fail "mount name changed: $MOUNT1 → $MOUNT2"

# --- Workspace path with spaces ---
echo "--- path with spaces ---"
WS_SPACES=$(mktemp -d -t "dcx e2e XXXXXX")
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
trap 'e2e_cleanup; rm -rf "$WS" "$WS3"' EXIT
mkdir -p "$WS3/.devcontainer"
cat >"$WS3/.devcontainer/devcontainer.json" <<'EOF'
{
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
}
EOF
"$DCX" up --workspace-folder "$WS3" 2>/dev/null
MOUNT3=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | tail -1 | xargs basename)
# The basename of WS3 starts with "my.project." — dot should become hyphen.
[[ "$MOUNT3" == dcx-my-project* ]] && pass "dots sanitized to hyphens in mount name" || fail "expected dcx-my-project* but got $MOUNT3"
"$DCX" down --workspace-folder "$WS3" 2>/dev/null
rm -rf "$WS3"

# --- Stale mount recovery ---
echo "--- stale mount recovery ---"
WS4=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS4"' EXIT
"$DCX" up --workspace-folder "$WS4" 2>/dev/null
MOUNT_DIR4=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | tail -1)
# Simulate stale FUSE: unmount manually without removing dir.
if [ -f /proc/mounts ]; then
    fusermount -u "$MOUNT_DIR4" 2>/dev/null || true
else
    umount "$MOUNT_DIR4" 2>/dev/null || true
fi
# Now dcx up should recover (detect stale, remount).
code=0
"$DCX" up --workspace-folder "$WS4" 2>/dev/null || code=$?
assert_exit "up recovers from stale mount" 0 "$code"
is_mounted "$MOUNT_DIR4" && pass "mount is healthy after stale recovery" || fail "mount still unhealthy after recovery"
e2e_cleanup
rm -rf "$WS4"

# --- Shell completion is valid bash syntax ---
echo "--- bash completion validity ---"
code=0
bash -c "source <($DCX completions bash)" 2>/dev/null || code=$?
assert_exit "bash completions are valid bash" 0 "$code"

# --- Pass-through exit code ---
echo "--- pass-through exit code ---"
code=0
"$DCX" __dcx_nonexistent_e2e__ 2>/dev/null || code=$?
[ "$code" -ne 2 ] && pass "pass-through does not exit 2 (clap error)" || fail "pass-through must not exit 2"

summary
