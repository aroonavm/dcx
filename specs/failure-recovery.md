# Troubleshooting Guide

## Quick Lookup

| Error Message | Solution |
|---|---|
| `bindfs not installed` | Linux: `sudo apt install bindfs` / macOS: `brew install macfuse && brew install bindfs` |
| `devcontainer not found in $PATH` | Install `devcontainer` CLI |
| `Colima not running or Docker unavailable` | `colima start` |
| `~/.colima-mounts does not exist` | `dcx` auto-creates this; check parent directory permissions |
| `Mount base directory is not writable` | Check `colima.yaml` has the mount entry; restart Colima |
| `Workspace path does not exist` | Check path exists: `ls -la /your/path` |
| `No mount found. Run 'dcx up' first` | Run `dcx up` before `dcx exec` |
| `Device busy` on unmount | Container still running; try `docker kill <id>` then `dcx down` |
| Mount directory exists but empty | Run `dcx clean` to remove stale directory |

---

## Common Issues

### Issue: `bindfs not installed`

```bash
# Linux
sudo apt install bindfs

# macOS
brew install macfuse && brew install bindfs
```

---

### Issue: `Colima not running`

```bash
colima start
```

If Colima is stuck:
```bash
colima stop
colima start
```

---

### Issue: `~/.colima-mounts not mounted in VM`

**What this means:** Your `colima.yaml` doesn't have the mount entry, or Colima wasn't restarted.

```bash
# 1. Check your colima.yaml
# Linux: ~/.config/colima/default/colima.yaml
# macOS: ~/.colima/default/colima.yaml
cat ~/.config/colima/default/colima.yaml  # adjust path for macOS

# Should have this section:
# mounts:
#   - location: ~/.colima-mounts
#     writable: true

# 2. If missing, add it (edit the file manually)

# 3. Restart Colima
colima stop
colima start

# 4. Verify it's mounted
colima ssh -- ls ~/.colima-mounts
```

---

### Issue: Container still running when trying to `dcx down`

```bash
# See running containers
docker ps

# Stop it manually
docker stop <container-id>

# Then retry
dcx down
```

Or force kill:
```bash
docker kill <container-id>
dcx down
```

---

### Issue: Mount fails with "Device busy"

**What this means:** Container or another process still using the mount.

```bash
# Option 1: Stop the container
docker stop <container-id>
dcx down

# Option 2: Force kill it
docker kill <container-id>
dcx down
```

---

### Issue: `dcx exec` says "No mount found"

**What this means:** You ran `dcx exec` without running `dcx up` first.

```bash
# Run this first
dcx up

# Then retry
dcx exec -- npm test
```

---

### Issue: Workspace path doesn't exist

```bash
# Check the path exists
ls -la /your/path

# Make sure you're in the right directory
pwd

# Try with correct path
dcx up --workspace-folder /correct/path
```

---

### Issue: Stale mount after reboot

**What this means:** FUSE mounts don't survive reboot. Directory exists but isn't mounted.

```bash
# Option 1: dcx up detects the stale mount, unmounts, and remounts fresh
dcx up

# Option 2: Clean up all dcx-managed mounts
dcx clean
```

---

### Issue: Multiple mounts exist for same project

```bash
# See what's mounted
ls -la ~/.colima-mounts

# Clean everything up
dcx clean

# Start fresh
dcx up
```

---

## Recovery Checklist

### Before using `dcx up`, verify:
```bash
dcx doctor  # Checks all prerequisites and reports what's missing
```

Or manually:
- [ ] `bindfs` installed: `which bindfs`
- [ ] Colima running: `docker info`
- [ ] Mount base mounted: `colima ssh -- ls ~/.colima-mounts`
- [ ] Workspace exists: `ls -la /your/workspace`

### Check current state:
```bash
dcx status  # Shows all mounted workspaces, containers, and health
```

### If `dcx up` fails:
```bash
# 1. Run dcx doctor to identify setup issues
dcx doctor
# 2. Fix any failing checks
# 3. Retry dcx up
```

### If `dcx down` fails:
```bash
# 1. Find the container
docker ps -a | grep <workspace-name>

# 2. Stop it manually
docker stop <container-id>  # or docker kill

# 3. Manually clean mount
fusermount -u ~/.colima-mounts/workspace-hash   # Linux
umount ~/.colima-mounts/workspace-hash          # macOS
rmdir ~/.colima-mounts/workspace-hash

# 4. Or use dcx clean
dcx clean
```

---

## Nuclear Option: Complete Reset

**Warning:** This destroys all containers and mounts. Use only if nothing else works.

```bash
# Stop and delete Colima VM
colima stop
colima delete

# Clean local mounts
rm -rf ~/.colima-mounts/*

# Start fresh
mkdir -p ~/.colima-mounts
colima start  # Creates new VM

# Restart your project
cd /path/to/project
dcx up
```

---

## Getting More Information

### See what containers exist
```bash
docker ps -a
```

### See what's mounted on host
```bash
mount | grep colima-mounts
```

### See mount details in VM
```bash
colima ssh -- mount | grep colima-mounts
```

### Check if a directory is mounted
```bash
mountpoint ~/.colima-mounts/workspace-hash
# Returns 0 if mounted, 1 if not
```

### See container logs
```bash
docker logs <container-id>
```

### Check Colima VM logs
```bash
colima ssh -- dmesg | tail -20
```

---

## When to Use `dcx clean`

Run `dcx clean` when:
- You want to fully tear down the current workspace's devcontainer (stop, remove container + image, unmount)
- A container crashed and left a mount for this workspace
- You want to force a fresh rebuild on next `dcx up`

Run `dcx clean --all` when:
- You want to tear down **all** `dcx`-managed mounts, containers, and images
- Full reset before a Colima reinstall or major configuration change
- You see stale directories in `~/.colima-mounts/` from old workspaces

**`dcx clean` (default — current workspace):**
- Targets the current directory's devcontainer (or `--workspace-folder`)
- Full cleanup: stop container, `docker rm`, `docker rmi`, unmount, remove directory
- Prompts for confirmation if a running container will be stopped (`--yes` to skip)

**`dcx clean --all`:**
- Targets all `dcx-*` entries in `~/.colima-mounts/`
- Full cleanup for each: stop container, `docker rm`, `docker rmi`, unmount, remove directory
- Prompts listing all running containers that will be stopped (`--yes` to skip)
- Continues on individual failures; reports all failures at the end

---

## Preventing Issues

### Use `dcx down` consistently
```bash
# Good - tells dcx to stop and unmount
dcx down

# Bad - leaves mount still mounted
docker stop <container-id>
```

### Run `dcx clean --all` periodically
```bash
dcx clean --all  # Full cleanup of all dcx-managed mounts, containers, and images
```

### Check your workspace path
```bash
# Before running dcx up, verify:
ls -la /your/workspace
```

---

## Still Stuck?

1. **Run `dcx doctor`** — validates all prerequisites and reports fixes
2. **Run `dcx status`** — see what's currently mounted and running
3. **Check error message** — see Quick Lookup table above
4. **Use `dcx clean`** — removes stale state
5. **Nuclear option** — if all else fails, reset everything (see above)
