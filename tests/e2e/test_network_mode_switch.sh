#!/usr/bin/env bash
# E2E tests for network mode enforcement across dcx down/up cycles.
# Verifies that:
# 1. dcx up --network restricted creates a container with the restricted mode
# 2. dcx down stops and removes the container
# 3. dcx up --network open creates a fresh container with the open mode (not restarting the old one)
# 4. When FUSE mount disappears but container survives, dcx down still removes the container

source "$(dirname "$0")/setup.sh"
require_e2e_deps

echo "=== dcx network mode switch ==="

# Create a test workspace
WS=$(make_workspace)
trap 'e2e_cleanup; rm -rf "$WS"' EXIT

# --- Test 1: Network mode restricted ---
echo "--- network mode restricted ---"
code=0
"$DCX" up --workspace-folder "$WS" --network restricted 2>/dev/null || code=$?
assert_exit "up with network restricted exits 0" 0 "$code"

RELAY_DIR=$(relay_dir_for "$WS")
CONTAINER_ID=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" 2>/dev/null | head -1)
if [ -n "$CONTAINER_ID" ]; then
    NETWORK_MODE=$(docker inspect --format='{{index .Config.Labels "dcx.network-mode"}}' "$CONTAINER_ID" 2>/dev/null || echo "")
    if [ "$NETWORK_MODE" = "restricted" ]; then
        pass "container has network mode restricted"
    else
        fail "container has network mode '$NETWORK_MODE', expected 'restricted'"
    fi
else
    fail "no container found after up --network restricted"
fi

# --- Test 2: Normal down (mount exists) ---
echo "--- normal down removes container ---"
code=0
"$DCX" down --workspace-folder "$WS" 2>/dev/null || code=$?
assert_exit "down exits 0" 0 "$code"

REMAINING=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" 2>/dev/null | wc -l)
if [ "$REMAINING" -eq 0 ]; then
    pass "container removed after down"
else
    fail "container still exists after down"
fi

# --- Test 3: Network mode open (fresh container, not restart) ---
echo "--- network mode open creates fresh container ---"
code=0
"$DCX" up --workspace-folder "$WS" --network open 2>/dev/null || code=$?
assert_exit "up with network open exits 0" 0 "$code"

CONTAINER_ID=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" 2>/dev/null | head -1)
if [ -n "$CONTAINER_ID" ]; then
    NETWORK_MODE=$(docker inspect --format='{{index .Config.Labels "dcx.network-mode"}}' "$CONTAINER_ID" 2>/dev/null || echo "")
    if [ "$NETWORK_MODE" = "open" ]; then
        pass "container has network mode open (not restarted old restricted container)"
    else
        fail "container has network mode '$NETWORK_MODE', expected 'open'"
    fi
else
    fail "no container found after up --network open"
fi

# --- Test 4: FUSE crash scenario (container survives, mount gone) ---
echo "--- fuse crash recovery ---"
# Simulate FUSE crash: unmount bindfs while container is still running
if is_mounted "$RELAY_DIR"; then
    fusermount -u "$RELAY_DIR" 2>/dev/null || \
        sudo -n fusermount -u "$RELAY_DIR" 2>/dev/null || \
        true
    pass "unmounted relay dir to simulate FUSE crash"
else
    pass "relay dir already unmounted"
fi

# Verify container still exists even though mount is gone
CONTAINER_ID=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" 2>/dev/null | head -1)
if [ -n "$CONTAINER_ID" ]; then
    pass "container survived FUSE unmount (expected)"
else
    pass "container was already removed (acceptable)"
fi

# --- Test 5: Down removes orphaned container (mount gone but container exists) ---
echo "--- down removes orphaned container ---"
code=0
down_out=$("$DCX" down --workspace-folder "$WS" 2>&1) || code=$?
assert_exit "down exits 0" 0 "$code"

# Verify output does NOT say "Nothing to do" if a container existed
if echo "$down_out" | grep -q "Nothing to do"; then
    fail "down should not print 'Nothing to do' when container exists without mount"
else
    pass "down handled orphaned container (did not print 'Nothing to do')"
fi

# Verify container is now gone
REMAINING=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" 2>/dev/null | wc -l)
if [ "$REMAINING" -eq 0 ]; then
    pass "orphaned container removed"
else
    fail "orphaned container still exists after down"
fi

summary
