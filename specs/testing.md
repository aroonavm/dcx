# Testing Strategy

## Overview

`dcx` uses **integration testing** via shell scripts. No unit tests or mocking layers.

## Integration Test Structure

Create shell script tests in `tests/` directory:

```
tests/
  ├── setup.sh              # Common setup/teardown
  ├── test_dcx_up.sh         # Test: dcx up with various workspaces
  ├── test_dcx_exec.sh       # Test: dcx exec
  ├── test_dcx_down.sh       # Test: dcx down and cleanup
  ├── test_dcx_clean.sh      # Test: dcx clean with stale mounts
  ├── test_dcx_status.sh     # Test: dcx status output
  ├── test_dcx_doctor.sh     # Test: dcx doctor checks
  ├── test_error_cases.sh   # Test: missing bindfs, no Docker, etc.
  └── test_edge_cases.sh    # Test: symlinks, relative paths, etc.
```

## Test Scenarios

### 1. Happy Path: `dcx up` → `dcx exec` → `dcx down`

```bash
# Setup workspace
WORKSPACE=$(mktemp -d)
cd $WORKSPACE
git init  # Initialize git repo for devcontainer detection

# Test: dcx up
dcx up --workspace-folder .
assert mount point exists
assert container is running

# Test: dcx exec
dcx exec -- echo "hello from container"
assert output is "hello from container"

# Test: dcx down
dcx down
assert container is stopped
assert mount point is unmounted
```

### 2. Idempotency: Multiple `dcx up` calls

```bash
# First call
dcx up
MOUNT1=$(find ~/.colima-mounts -maxdepth 1 -type d -name "*")

# Second call (same workspace)
dcx up
MOUNT2=$(find ~/.colima-mounts -maxdepth 1 -type d -name "*")

assert MOUNT1 == MOUNT2  # Same mount reused
assert count of mount points == 1  # No duplicates
```

### 3. Error Cases

```bash
# bindfs not installed
uninstall bindfs
dcx up 2>&1 | grep "bindfs not installed"
assert exit code 127
reinstall bindfs

# Workspace doesn't exist
dcx up --workspace-folder /nonexistent 2>&1 | grep "not found"
assert exit code 2

# Colima/Docker not running (fail fast before bindfs)
stop colima
dcx up 2>&1 | grep "Docker is not available"
assert exit code 1
dcx exec 2>&1 | grep "Docker is not available"
assert exit code 1
dcx down 2>&1 | grep "Docker is not available"
assert exit code 1
start colima

# No devcontainer configuration
WORKSPACE=$(mktemp -d)
dcx up --workspace-folder $WORKSPACE 2>&1 | grep "No devcontainer configuration"
assert exit code 2
rm -rf $WORKSPACE
```

### 4. Multi-Workspace: Simultaneous Mounts

```bash
# Mount workspace A
WORKSPACE_A=$(mktemp -d)
cd $WORKSPACE_A && git init
dcx up --workspace-folder $WORKSPACE_A
assert mount dcx-*A* exists

# Mount workspace B (while A is still mounted)
WORKSPACE_B=$(mktemp -d)
cd $WORKSPACE_B && git init
dcx up --workspace-folder $WORKSPACE_B
assert mount dcx-*B* exists

# Both mounts should coexist
MOUNT_COUNT=$(ls ~/.colima-mounts/ | grep '^dcx-' | wc -l)
assert MOUNT_COUNT == 2

# dcx down only affects the targeted workspace
dcx down --workspace-folder $WORKSPACE_A
assert mount dcx-*A* removed
assert mount dcx-*B* still exists

dcx down --workspace-folder $WORKSPACE_B
```

### 5. Stale Mount Recovery via `dcx up`

```bash
# Create mount, then simulate stale state
dcx up
MOUNT=$(ls -d ~/.colima-mounts/dcx-*)

# Simulate stale mount (unmount without cleanup)
fusermount -u $MOUNT

# dcx up should detect unhealthy mount, remount automatically
dcx up
assert mount is healthy (ls $MOUNT succeeds)
assert container is running
```

### 8. `dcx clean` (default) Leaves Active Containers

```bash
# Start two workspaces
WORKSPACE_A=$(mktemp -d)
WORKSPACE_B=$(mktemp -d)
cd $WORKSPACE_A && git init && dcx up
cd $WORKSPACE_B && git init && dcx up

# Create a stale mount (simulate)
mkdir ~/.colima-mounts/dcx-stale-12345678

# dcx clean (no --all) should only remove the stale mount
dcx clean
assert dcx-stale-12345678 removed
assert both active mounts still exist
assert both containers still running

# dcx clean --all should stop both containers and unmount both
dcx clean --all --yes
assert no dcx-* entries in ~/.colima-mounts/
assert no running containers for either workspace
```

### 9. `dcx up` Rollback on `devcontainer up` Failure

```bash
# Setup workspace with broken devcontainer.json
WORKSPACE=$(mktemp -d)
cd $WORKSPACE && git init
echo '{ invalid json }' > .devcontainer/devcontainer.json

# dcx up should fail and rollback the mount
dcx up 2>&1
assert exit code != 0
assert no dcx-* entries in ~/.colima-mounts/  # mount rolled back
```

### 10. `dcx down` with No Mount

```bash
# Run dcx down in a directory that was never mounted
cd /tmp
dcx down 2>&1 | grep "No mount found"
assert exit code == 0  # exits cleanly
```

### 11. `dcx status` Output

```bash
# Start two workspaces
WORKSPACE_A=$(mktemp -d)
WORKSPACE_B=$(mktemp -d)
cd $WORKSPACE_A && git init && dcx up
cd $WORKSPACE_B && git init && dcx up

# dcx status should show both
dcx status | grep "$WORKSPACE_A"
dcx status | grep "$WORKSPACE_B"
dcx status | grep "running"

# Tear down one
dcx down --workspace-folder $WORKSPACE_A

# dcx status should show only B
dcx status | grep -v "$WORKSPACE_A"
dcx status | grep "$WORKSPACE_B"
```

### 12. `dcx doctor` Checks

```bash
# With everything working, all checks pass
dcx doctor
assert exit code == 0
assert output contains "All checks passed"

# With bindfs missing (if possible to simulate)
# dcx doctor should report the failure and fix instruction
```

### 13. `dcx up --dry-run`

```bash
WORKSPACE=$(mktemp -d)
cd $WORKSPACE && git init
mkdir -p .devcontainer
echo '{}' > .devcontainer/devcontainer.json

# dry-run should show what would happen without doing it
dcx up --dry-run 2>&1 | grep "Would mount"
dcx up --dry-run 2>&1 | grep "Would run"
assert exit code == 0
assert no dcx-* entries in ~/.colima-mounts/  # nothing actually created

rm -rf $WORKSPACE
```

### 14. `dcx --help` and `dcx --version`

```bash
# --help shows dcx's own help
dcx --help 2>&1 | grep "dcx"
dcx --help 2>&1 | grep "up"
dcx --help 2>&1 | grep "clean"
assert exit code == 0

# --version shows dcx version
dcx --version 2>&1 | grep "dcx"
assert exit code == 0
```

### 15. Various Workspace Path Forms

```bash
# Absolute path
dcx up --workspace-folder /home/user/project
assert works

# Relative path
cd /home/user && dcx up --workspace-folder ./project
assert works

# Symlink
ln -s /home/user/project /tmp/project-link
dcx up --workspace-folder /tmp/project-link
assert resolves to canonical path

# Current directory
cd /home/user/project && dcx up
assert uses cwd
```

## Manual Testing Checklist

Before release, manually verify:

- [ ] `dcx doctor` passes all checks on fresh setup
- [ ] Install from scratch: add `~/.colima-mounts` to colima.yaml, restart Colima (dcx auto-creates directory)
- [ ] `dcx --help` shows dcx-specific help with all 6 subcommands
- [ ] `dcx --version` shows dcx version
- [ ] `dcx up` fails fast with "Docker is not available" when Colima is stopped
- [ ] `dcx up` fails fast with "No devcontainer configuration" when no .devcontainer exists
- [ ] `dcx up --dry-run` shows what would happen without creating any mounts
- [ ] `dcx up` creates mount, starts container, prints progress
- [ ] `dcx up` for second workspace mounts both simultaneously
- [ ] `dcx up` rolls back mount if `devcontainer up` fails, prints "Mount rolled back." after devcontainer output
- [ ] `dcx up` on stale mount: detects, remounts, starts container
- [ ] Ctrl+C during `dcx up` rolls back the mount cleanly
- [ ] `dcx exec` runs commands in container
- [ ] `dcx exec` without `dcx up` fails with "Run `dcx up` first"
- [ ] Container can read/write files through the mount
- [ ] `dcx down` stops only the targeted workspace, leaves others intact, prints progress
- [ ] `dcx down` with no mount prints "No mount found. Nothing to do."
- [ ] `dcx clean` (default) removes only orphaned/stale/empty mounts, leaves active untouched
- [ ] `dcx clean --all` prompts if active containers, lists names
- [ ] `dcx clean --all` stops active containers, unmounts all `dcx-*` mounts, prints summary
- [ ] `dcx clean` and `dcx clean --all` continue on individual failures, report all at end
- [ ] `dcx status` shows all mounted workspaces, containers, and state
- [ ] Error messages are clear and actionable
- [ ] Works with various devcontainer.json configurations
- [ ] Works with different workspace locations (home, /tmp, symlinks, etc.)
- [ ] Works with git repos (git root detection)
- [ ] Works after Colima restart (stale mount recovery)

## Continuous Integration

Add to CI pipeline (GitHub Actions, etc.):

```yaml
- name: Run integration tests
  run: |
    chmod +x tests/*.sh
    for test in tests/test_*.sh; do
      bash "$test" || exit 1
    done
```

**Prerequisites for CI:**
- Linux environment (Colima/Lima uses QEMU on Linux)
- Docker installed
- Colima running
- `bindfs` installed
