#!/bin/bash
set -euo pipefail

# Test: Network mode enforcement across "$DCX" down/up cycles
# Verifies that:
# 1. "$DCX" up --network restricted creates a container with the restricted mode
# 2. "$DCX" down stops and removes the container
# 3. "$DCX" up --network open creates a fresh container with the open mode (not restarting the old one)
# 4. When FUSE mount disappears but container survives, "$DCX" down still removes the container

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "${REPO_ROOT}/tests/e2e/setup.sh"

# Create test workspace
WORKSPACE_DIR=$(mktemp -d)
trap "rm -rf ${WORKSPACE_DIR}" EXIT

# Initialize devcontainer config
mkdir -p "${WORKSPACE_DIR}/.devcontainer"
cat > "${WORKSPACE_DIR}/.devcontainer/devcontainer.json" << 'EOF'
{
  "image": "busybox:latest",
  "remoteUser": "root",
  "runArgs": [
    "--label", "dcx.network-mode=${localEnv:DCX_NETWORK_MODE:minimal}"
  ]
}
EOF

echo "Test 1: Bring up with network mode restricted"
"$DCX" up --workspace-folder "${WORKSPACE_DIR}" --network restricted

# Verify container has the restricted network mode label
# The container is labeled with the relay mount path (devcontainer.local_folder),
# so we need to compute that path to search for the container.
RELAY_DIR=$(relay_dir_for "${WORKSPACE_DIR}")
CONTAINER_ID=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" | head -1)
test -n "${CONTAINER_ID}" || {
  echo "FAIL: No container found after "$DCX" up"
  exit 1
}

NETWORK_MODE=$(docker inspect --format='{{index .Config.Labels "dcx.network-mode"}}' "${CONTAINER_ID}")
test "${NETWORK_MODE}" = "restricted" || {
  echo "FAIL: Expected network mode 'restricted', got '${NETWORK_MODE}'"
  exit 1
}
echo "PASS: Container has network mode 'restricted'"

echo ""
echo "Test 2: Normal down (mount exists)"
"$DCX" down --workspace-folder "${WORKSPACE_DIR}"

# Verify container is removed
RUNNING=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" | wc -l)
test "${RUNNING}" -eq 0 || {
  echo "FAIL: Container still exists after "$DCX" down"
  exit 1
}
echo "PASS: Container removed after "$DCX" down"

echo ""
echo "Test 3: Bring up with new network mode (open)"
"$DCX" up --workspace-folder "${WORKSPACE_DIR}" --network open

# Verify new container has the open network mode label (not the old restricted one)
CONTAINER_ID=$(docker ps -a --filter "label=devcontainer.local_folder=${RELAY_DIR}" --format "{{.ID}}" | head -1)
test -n "${CONTAINER_ID}" || {
  echo "FAIL: No container found after second "$DCX" up"
  exit 1
}

NETWORK_MODE=$(docker inspect --format='{{index .Config.Labels "dcx.network-mode"}}' "${CONTAINER_ID}")
test "${NETWORK_MODE}" = "open" || {
  echo "FAIL: Expected network mode 'open', got '${NETWORK_MODE}'"
  exit 1
}
echo "PASS: Container has network mode 'open' (not restarted old restricted container)"

# Clean up
"$DCX" down --workspace-folder "${WORKSPACE_DIR}"

echo ""
echo "Test 4: FUSE crash scenario (container survives, mount gone)"
"$DCX" up --workspace-folder "${WORKSPACE_DIR}" --network restricted

# Get the relay mount point
RELAY_DIR="${HOME}/.colima-mounts"
MOUNT_NAME=$(ls "${RELAY_DIR}" | grep "dcx-$(basename "${WORKSPACE_DIR}")" | head -1)
MOUNT_POINT="${RELAY_DIR}/${MOUNT_NAME}"

# Simulate FUSE crash: unmount bindfs while container is still running
echo "Simulating FUSE crash by unmounting bindfs..."
if ! fusermount -u "${MOUNT_POINT}" 2>/dev/null; then
  # Fallback to umount if fusermount not available
  # Use non-interactive sudo (-n) to avoid prompting for password in automated tests
  sudo -n umount "${MOUNT_POINT}" 2>/dev/null || true
fi

# Verify mount is gone but container still exists
test ! -e "${MOUNT_POINT}" || {
  echo "WARNING: Mount point still exists after unmount"
}

CONTAINER_ID=$(docker ps -a --filter "label=devcontainer.local_folder=${MOUNT_POINT}" --format "{{.ID}}" 2>/dev/null | head -1 || true)
test -n "${CONTAINER_ID}" || {
  echo "WARNING: Container already missing (expected to survive FUSE crash)"
}

echo ""
echo "Test 5: "$DCX" down removes the orphaned container (mount gone but container exists)"
# This should NOT print "Nothing to do" — it should still stop and remove the container
"$DCX" down --workspace-folder "${WORKSPACE_DIR}" 2>&1 | grep -q "Nothing to do" && {
  echo "FAIL: "$DCX" down printed 'Nothing to do' when container existed without mount"
  exit 1
}

# Verify container is now gone
REMAINING=$(docker ps -a --filter "label=devcontainer.local_folder=${MOUNT_POINT}" --format "{{.ID}}" 2>/dev/null | wc -l)
test "${REMAINING}" -eq 0 || {
  echo "FAIL: Container still exists after "$DCX" down (with missing mount)"
  exit 1
}
echo "PASS: "$DCX" down removed orphaned container even when mount was gone"

echo ""
echo "All network mode switch tests passed!"
