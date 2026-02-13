#!/usr/bin/env bash
# E2E tests for `dcx up`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx up ==="

RELAY="$HOME/.colima-mounts"

# --- Happy path ---
echo "--- happy path ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

code=0
"$DCX" up --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "up exits 0" 0 "$code"

MOUNT_DIR=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | head -1)
[ -n "$MOUNT_DIR" ] && pass "mount directory created in relay" || fail "no dcx-* entry in relay"
is_mounted "$MOUNT_DIR" && pass "mount is active in mount table" || fail "mount not in mount table"

# --- Idempotent reuse ---
echo "--- idempotent reuse ---"
code=0
"$DCX" up --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "second up exits 0" 0 "$code"

MOUNT_COUNT=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | wc -l)
[ "$MOUNT_COUNT" -eq 1 ] && pass "only one dcx-* entry after two ups" || fail "expected 1 dcx-* entry, got $MOUNT_COUNT"

# --- Dry-run: no side effects ---
echo "--- dry-run ---"
WS2=$(make_workspace)
dry_out=$("$DCX" up --dry-run --workspace-folder "$WS2" 2>/dev/null)
dry_code=$?
assert_exit "dry-run exits 0" 0 "$dry_code"
assert_contains "dry-run shows Would mount" "$dry_out" "Would mount:"
assert_contains "dry-run shows Would run" "$dry_out" "Would run:"
# No mount should have been created for WS2
MOUNT_COUNT_AFTER=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | wc -l)
[ "$MOUNT_COUNT_AFTER" -eq 1 ] && pass "dry-run creates no mount" || fail "dry-run created a mount"
rm -rf "$WS2"

# --- Rollback on devcontainer failure ---
echo "--- rollback on failure ---"
WS3=$(make_workspace)
# Replace the image with a non-existent one to force devcontainer up to fail.
cat >"$WS3/.devcontainer/devcontainer.json" <<'EOF'
{
    "image": "dcx-e2e-nonexistent-image:0.0.0"
}
EOF

MOUNT_COUNT_BEFORE=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | wc -l)
fail_out=$("$DCX" up --workspace-folder "$WS3" 2>&1) || true
fail_code=$?
assert_exit "rollback: up exits 1" 1 "$fail_code"
assert_contains "rollback prints message" "$fail_out" "Mount rolled back."
MOUNT_COUNT_AFTER2=$(ls -d "${RELAY}"/dcx-* 2>/dev/null | wc -l)
[ "$MOUNT_COUNT_AFTER2" -eq "$MOUNT_COUNT_BEFORE" ] && \
    pass "rollback: no leftover mount" || fail "rollback left a mount behind"
rm -rf "$WS3"

# --- Recursive mount guard exits 2 ---
echo "--- recursive mount guard ---"
code=0
"$DCX" up --workspace-folder "${RELAY}/dcx-test-00000000" 2>/dev/null || code=$?
assert_exit "recursive guard exits 2" 2 "$code"

# --- Missing workspace exits 2 ---
echo "--- missing workspace exits 2 ---"
code=0
"$DCX" up --workspace-folder "/nonexistent/__dcx_e2e__" 2>/dev/null || code=$?
assert_exit "missing workspace exits 2" 2 "$code"

# --- Missing devcontainer config exits 2 ---
echo "--- missing devcontainer config exits 2 ---"
WS_NOCONF=$(mktemp -d)
trap 'e2e_cleanup; rm -rf "$WS" "$WS_NOCONF"' EXIT
code=0
"$DCX" up --workspace-folder "$WS_NOCONF" 2>/dev/null || code=$?
assert_exit "missing config exits 2" 2 "$code"
rm -rf "$WS_NOCONF"

# --- Non-owned directory warning ---
echo "--- non-owned directory warning ---"
if ! sudo -n true 2>/dev/null; then
    echo "SKIP: sudo not available for non-owned directory test"
else
    WS_ROOT=$(make_workspace)
    sudo chown 0:0 "$WS_ROOT"
    # Echo N: should abort with exit 4
    code=0
    echo "n" | "$DCX" up --workspace-folder "$WS_ROOT" 2>/dev/null || code=$?
    assert_exit "up non-owned dir N aborts with exit 4" 4 "$code"
    # --yes: should skip prompt (may fail for other reasons, but not exit 4)
    code=0
    "$DCX" up --workspace-folder "$WS_ROOT" --yes 2>/dev/null || code=$?
    [ "$code" -ne 4 ] && pass "up non-owned dir --yes skips prompt" || fail "up --yes still returned 4"
    sudo chown "$(id -u):$(id -g)" "$WS_ROOT"
    e2e_cleanup
    rm -rf "$WS_ROOT"
fi

# --- Progress output on stderr ---
echo "--- progress output ---"
WS4=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS4"' EXIT
stderr_out=$("$DCX" up --workspace-folder "$WS4" 2>&1 >/dev/null) || true
assert_contains "up shows resolving step" "$stderr_out" "→ Resolving workspace path:"
assert_contains "up shows mounting step" "$stderr_out" "→ Mounting workspace to"
assert_contains "up shows devcontainer step" "$stderr_out" "→ Starting devcontainer..."
assert_contains "up shows done step" "$stderr_out" "→ Done."
e2e_cleanup
rm -rf "$WS4"

summary
