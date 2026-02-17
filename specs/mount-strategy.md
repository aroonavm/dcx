# Mount Strategy Implementation

See [architecture.md § Design](architecture.md#design) for complete behavior spec.

## Core Design

### Mount Naming Convention
`dcx-<name>-<hash>` pattern:
- `<name>`: sanitized last path component (alphanumeric + `-`, max 30 chars)
- `<hash>`: first 8 hex chars of SHA256(absolute_path)
- **Filesystem is the source of truth** (no state files, no corruption risk)

### bindfs Configuration
- `--no-allow-other`: Restricts to current user (Colima handles VM-level `allow_other`)
- No ownership remapping: UID/GID preserved (devcontainer handles user mapping)
- Default symlinks: relative symlinks work, absolute outside workspace are dangling

### Relay Directory
`~/.colima-mounts/` auto-created with system default permissions (respects umask).

## Implementation Patterns

### Hash Collision Detection
When two paths hash to same 8-char hash (~1 in 4B odds):
1. Query mount table to detect actual mounted source
2. Compare to current path
3. Fail with collision error if mismatch (tell user to `dcx clean`)

### Stale Mount Recovery
FUSE mounts don't survive reboots. When mount directory exists but not mounted:
1. Detect via mount table query
2. Unmount stale bindfs
3. Remount fresh

### Idempotent Mount
When mount already exists (same source):
1. Verify health via mount table
2. Verify source matches current path
3. Reuse if healthy, recover if stale
4. Skip mounting step if already mounted

## Platform-Specific Calls

| Operation | Linux | macOS |
|-----------|-------|-------|
| Query mounts | Parse `/proc/mounts` | Run `mount` command |
| Unmount | `fusermount -u <path>` | `umount <path>` |
| Mount | `bindfs --no-allow-other <source> <target>` | `bindfs --no-allow-other <source> <target>` |

## Edge Cases

**Recursive mount guard:** Reject workspace paths starting with `~/.colima-mounts/dcx-` (prevent nesting)

**Workspace deleted while mounted:** Bindfs mount becomes invalid. Next interaction fails with message: "Workspace directory does not exist. Use `dcx clean` to remove stale mounts."

**Colima restart:** Host bindfs mounts survive. VM recreated, `~/.colima-mounts` re-mounted from host per `colima.yaml`. Containers lost but `dcx up` recovers: detects healthy mount, reuses it, starts new container.

**Non-user-owned directories:** Warn user with permissions implications. `--yes` skips prompt. Container runs as current user (not root), read/write depends on directory perms.
