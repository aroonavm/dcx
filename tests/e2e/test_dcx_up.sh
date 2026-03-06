#!/usr/bin/env bash
# E2E tests for `dcx up`.
# Requires: Colima running, Docker, bindfs, devcontainer CLI.

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx up ==="

RELAY="$HOME/.colima-mounts"

# --- Happy path (all defaults: no dcx_config.yaml → empty up.files, no --network flag → minimal,
#     no --file flags; auto-detects .devcontainer/devcontainer.json without --config-dir) ---
echo "--- happy path (defaults) ---"
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

code=0
"$DCX" up --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "up exits 0" 0 "$code"

WS_RELAY=$(relay_dir_for "$WS")
assert_dir_exists "mount directory created in relay" "$WS_RELAY"
is_mounted "$WS_RELAY" && pass "mount is active in mount table" || fail "mount not in mount table"

# --- Default network mode (minimal) ---
echo "--- default network mode (minimal) ---"
# Verify container has dcx.network-mode=minimal when no --network flag is passed
CONTAINER_ID=$(docker ps -a --filter "label=devcontainer.local_folder=${WS_RELAY}" --format "{{.ID}}" 2>/dev/null | head -1)
if [ -n "$CONTAINER_ID" ]; then
    NETWORK_MODE=$(docker inspect --format='{{index .Config.Labels "dcx.network-mode"}}' "$CONTAINER_ID" 2>/dev/null || echo "")
    if [ "$NETWORK_MODE" = "minimal" ]; then
        pass "container has network mode minimal (default)"
    else
        fail "container has network mode '$NETWORK_MODE', expected 'minimal' (default)"
    fi
else
    fail "no container found to check default network mode"
fi

# --- Idempotent reuse ---
echo "--- idempotent reuse ---"
code=0
"$DCX" up --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "second up exits 0" 0 "$code"
# The same relay dir must still exist and be mounted — idempotent, no duplicate entry.
assert_dir_exists "relay dir unchanged after second up" "$(relay_dir_for "$WS")"
is_mounted "$(relay_dir_for "$WS")" && pass "relay mount healthy after second up" || fail "relay mount gone after idempotent up"

# --- Dry-run: no side effects ---
echo "--- dry-run ---"
WS2=$(make_workspace)
dry_out=$("$DCX" up --dry-run --workspace-folder "$WS2" 2>/dev/null)
dry_code=$?
assert_exit "dry-run exits 0" 0 "$dry_code"
assert_contains "dry-run shows Would mount" "$dry_out" "Would mount:"
assert_contains "dry-run shows Would run" "$dry_out" "Would run:"
# Dry-run must not create a relay dir for WS2.
assert_dir_missing "dry-run creates no mount" "$(relay_dir_for "$WS2")"
# Verify --no-cache is forwarded as --build-no-cache in the devcontainer command
no_cache_out=$("$DCX" up --dry-run --no-cache --workspace-folder "$WS2" 2>/dev/null)
assert_contains "dry-run --no-cache shows --build-no-cache" "$no_cache_out" "--build-no-cache"
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

fail_code=0
fail_out=$("$DCX" up --workspace-folder "$WS3" 2>&1) || fail_code=$?
assert_exit "rollback: up exits 1" 1 "$fail_code"
assert_contains "rollback prints message" "$fail_out" "Mount rolled back."
# Rollback must remove the relay dir for WS3 — no leftover mount.
assert_dir_missing "rollback: no leftover mount" "$(relay_dir_for "$WS3")"
# Rollback must also remove the staging dir if it was created
staging=$(relay_dir_for "$WS3" | sed 's/dcx-/\.dcx-/')."-files"
if [ -d "$staging" ]; then
    fail "rollback: staging dir not cleaned up: $staging"
else
    pass "rollback: staging dir cleaned"
fi
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
    # YAML up.yes: true skips prompt without --yes CLI flag
    cat > "$WS_ROOT/.devcontainer/dcx_config.yaml" <<'EOF'
up:
  yes: true
EOF
    code=0
    "$DCX" up --workspace-folder "$WS_ROOT" 2>/dev/null || code=$?
    [ "$code" -ne 4 ] && pass "dcx_config.yaml up.yes:true skips prompt" || fail "dcx_config.yaml up.yes:true still prompted (exit 4)"
    "$DCX" down --workspace-folder "$WS_ROOT" 2>/dev/null || true
    rm -f "$WS_ROOT/.devcontainer/dcx_config.yaml"
    sudo chown "$(id -u):$(id -g)" "$WS_ROOT"
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

# --- --workspace-folder default (current directory) ---
echo "--- --workspace-folder default (current dir) ---"
WS5=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS" "$WS5"' EXIT
code=0
# Run without --workspace-folder: dcx must use $PWD as workspace
(cd "$WS5" && "$DCX" up 2>/dev/null) || code=$?
assert_exit "up without --workspace-folder exits 0 from workspace dir" 0 "$code"
is_mounted "$(relay_dir_for "$WS5")" && pass "relay mount created using cwd as workspace" || fail "relay mount not created from cwd"
"$DCX" down --workspace-folder "$WS5" 2>/dev/null
rm -rf "$WS5"

summary
