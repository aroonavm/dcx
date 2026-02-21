# Architecture: `dcx` — Dynamic Workspace Mounting for Colima {#top}

## Problem & Solution {#problem-solution}

**Problem:** Colima mounts are static (set in `colima.yaml` at startup). `devcontainer up` needs dynamic workspace paths that don't exist in the VM yet. Broadly mounting `$HOME` exposes all projects to every container — risky when running AI agents autonomously (agent in project-A could access project-B).

**Solution:** Use `bindfs` (FUSE userspace tool, no root required) to project workspace into a pre-mounted relay directory (`~/.colima-mounts`). Only needed directories exposed, only while in use. Multiple workspaces mount simultaneously, each isolated.

**Alternatives rejected:** Mount `$HOME` broadly (security risk), `initializeCommand` in devcontainer.json (can't inject relay path into static `workspaceMount`), Docker volumes (no bidirectional host sync).

---

## Design {#design}

**Data Flow:**
```
Host /home/user/myproject ──[bindfs]──> Host ~/.colima-mounts/dcx-myproject-a1b2c3d4
                                              ↓ [Colima mount]
                                         VM ~/.colima-mounts/dcx-myproject-a1b2c3d4
                                              ↓ [Docker bind]
                                         Container /workspace
```

**Technical Stack:**
- Language: Rust (single binary, no shell deps, cross-platform)
- Subprocess calls: direct & explicit (fail-fast, clear errors)
- Testing: unit tests (pure logic), integration tests (CLI), E2E shell tests
- Philosophy: simple > sophisticated, fail fast, code clarity

**Mount Naming:** `dcx-<name>-<hash>`
- `<name>`: sanitized last path component (alphanumeric + `-`, max 30 chars)
- `<hash>`: first 8 hex chars of SHA256(absolute_path)
- Example: `/home/user/myproject` → `dcx-myproject-a1b2c3d4`
- **Filesystem is the source of truth** (no state files, no corruption risk)

**bindfs Options:**
- `--no-allow-other`: Restricts to current user (sufficient because Colima bridges host→VM via sshfs with own `allow_other`)
- No ownership remapping: UID/GID preserved (devcontainer handles user mapping via `remoteUser`)
- Default symlinks: relative symlinks work, absolute outside workspace are dangling

**Relay Directory:** `~/.colima-mounts/` auto-created with system default permissions (respects umask).

---

## Commands {#commands}

### `dcx up` {#cmd-up}

**Usage:**
```bash
dcx up [--workspace-folder PATH] [--config PATH] [--dry-run] [--yes] [--open]
```

**Flags:**
- `--workspace-folder PATH` — workspace directory (default: current dir)
- `--config PATH` — explicit path to `devcontainer.json`; skips auto-detection; forwarded to `devcontainer up`
- `--open` — sets `FIREWALL_OPEN=true` in the dcx process environment, which devcontainer forwards into the container via `containerEnv ${localEnv:FIREWALL_OPEN}`; `init-firewall.sh` checks this var and runs with all traffic allowed

**Behavior:**
1. Validate Docker available; fail with exit 1 if not
2. Resolve workspace path; fail exit 2 if missing
3. Resolve `--config` to absolute path; fail exit 2 if provided but not found
4. Guard against recursive mounts (path starts with `~/.colima-mounts/dcx-`)
5. Verify devcontainer config exists (`.devcontainer/devcontainer.json` or `.devcontainer.json`); skip if `--config` provided
6. Compute mount point hash
7. If `--open`: set `FIREWALL_OPEN=true` in host env before spawning devcontainer
8. If `--dry-run`: print plan (including `--config` if provided), exit 0
9. Auto-create `~/.colima-mounts/` (system defaults)
10. If mount exists: verify health + source matches (idempotent), else recover from stale
11. If mount missing: create + mount with `bindfs --no-allow-other`
12. If workspace not owned by user: warn + prompt (skip with `--yes`)
13. Rewrite `--workspace-folder` → mount point; forward `--config` if provided
14. Delegate to `devcontainer up`
15. On failure: rollback (unmount + remove dir), exit 1
16. On SIGINT: rollback before exit

---

### `dcx exec` {#cmd-exec}

**Usage:**
```bash
dcx exec [--workspace-folder PATH] [--config PATH] COMMAND [ARGS...]
```

**Flags:**
- `--workspace-folder PATH` — workspace directory (default: current dir)
- `--config PATH` — explicit path to `devcontainer.json`; forwarded to `devcontainer exec`

**Behavior:**
1. Validate Docker available; fail exit 1
2. Resolve workspace path
3. Resolve `--config` to absolute path; fail exit 2 if provided but not found
4. Guard: reject `~/.colima-mounts/dcx-*` paths
5. Verify mount exists + healthy
6. Find running container by `devcontainer.local_folder` label on the relay mount point
7. Delegate to `devcontainer exec` with both `--container-id` (reliable container lookup) and `--workspace-folder` pointing to the relay mount point (so devcontainer reads the config and sets the remote working directory); forward `--config` if provided
8. The user's shell lands in the `workspaceFolder` inside the container (e.g. `/workspaces/<name>`), not the container's home directory
9. Forward SIGINT to child process

---

### `dcx down` {#cmd-down}

**Usage:**
```bash
dcx down [--workspace-folder PATH]
```

**Behavior:**
1. Validate Docker; fail exit 1
2. Resolve workspace; fail exit 2 if missing or is a managed path
3. Compute mount point
4. If no mount: print "nothing to do", exit 0 (idempotent)
5. Stop running container (find by `devcontainer.local_folder` label)
6. Unmount bindfs
7. Remove mount directory
8. On SIGINT during unmount: complete unmount before exit

---

### `dcx clean` {#cmd-clean}

**Usage:**
```bash
dcx clean [--workspace-folder PATH] [--all] [--purge] [--dry-run] [--yes]
```

**Two-Image Lifecycle:**
- **Build image** (e.g., `dcx-dev:latest`): from workspace Dockerfile, read from `image` field in devcontainer.json. Expensive build, preserved by default as Docker cache.
- **Runtime image** (e.g., `vsc-dcx-<hash>-uid`): thin UID-adjusted layer on build image, created by devcontainer CLI. Cheap rebuild.

**Behavior (default mode — current workspace):**
1. Validate Docker; fail exit 1
2. If `--dry-run`: scan all resources, print plan, exit 0
3. Resolve workspace path
4. Compute mount point
5. Find container (running or stopped)
6. If nothing found: print "Nothing to clean", exit 0
7. If running container: prompt unless `--yes`
8. Cleanup sequence:
   - Stop running container
   - If `--purge` + container: capture volume names BEFORE removal
   - Remove container + runtime image (`--force`)
   - If `--purge`: read build image name from devcontainer.json, remove if safe (`docker rmi` without `--force` fails if other images depend on it)
   - Remove captured volumes (if any)
   - Unmount bindfs
   - Remove mount directory
9. Scan for orphaned mounts (mounted but no container): unmount + remove
10. Clean orphaned `vsc-dcx-*` images (runtime images without containers)
11. Print summary + exit 0 (or 1 if failures)

**Behavior (--all mode):**
- Same, but iterate all `dcx-*` mounts, continue on individual failures
- If `--purge`: after per-mount cleanup, deduplicate + remove all build images, sweep remaining `dcx-*` volumes
- Print summary count

---

## Standards {#standards}

### Exit Codes

| Code | Meaning | Examples |
|------|---------|----------|
| 0 | Success | `dcx up` completed; `dcx down` found nothing |
| 1 | Runtime error | Mount/unmount failed; `devcontainer up` failed |
| 2 | Usage error | Workspace missing; invalid args |
| 4 | User aborted | User answered "N" to prompt |
| 127 | Prerequisite not found | `bindfs` not installed |
| N | Pass-through | Pass-through commands return child code |

### Progress Output

All commands print steps to stderr: `→ <action>...` (U+2192 arrow)

### Workspace Validation

- Must exist on host filesystem
- If not owned by current user: prompt with read/write implications, skip with `--yes`

### Docker Volumes

Devcontainer creates volumes like `dcx-shellhistory-<devcontainerId>`. Only `--purge` removes them. Must capture names BEFORE removing container (else lost reference).

---

## Edge Cases {#edge-cases}

**Recursive mount guard:** Reject workspace paths starting with `~/.colima-mounts/dcx-` (prevent nesting)

**Stale mount:** FUSE mounts don't survive reboots. Directory exists but not mounted. `dcx up` detects + recovers (unmount stale, remount fresh).

**Hash collision:** Two paths with same 8-char hash (~1 in 4B odds). `dcx up` queries mount table to detect actual source, compares to current path, fails with collision error if mismatch (tells user to `dcx clean`).

**Non-user-owned directories:** Warn user with permissions implications. `--yes` skips prompt. Container runs as current user (not root), read/write depends on directory perms.

**Workspace deleted while mounted:** Bindfs mount becomes invalid. `dcx up`, `dcx down`, and `dcx exec` fail with: "Workspace directory does not exist. Use `dcx clean` to remove stale mounts." `dcx clean` itself reports "Workspace directory does not exist." without the self-referential hint.

**Colima restart:** Host bindfs mounts survive. VM is recreated, `~/.colima-mounts` re-mounted from host per `colima.yaml`. Containers lost (VM recreated) but `dcx up` recovers: detects healthy mount, reuses it, starts new container.

**External container/image removal:** Container/image removed outside dcx. `dcx clean` handles gracefully (idempotent). Volume names unrecoverable in single-workspace mode (skipped), but `--all --purge` final sweep removes remaining `dcx-*` volumes.

---

## Platform Support {#platform}

| Operation | Linux | macOS |
|-----------|-------|-------|
| Query mounts | `/proc/mounts` | `mount` command |
| Unmount | `fusermount -u` | `umount` |
| Install bindfs | `sudo apt install bindfs` | `brew install bindfs` |

---

## Known Limitations {#limitations}

- **No Windows support** (FUSE/bindfs Windows story unclear)
- **No read-only mounts** (all mounts are read-write)
- **No concurrent ops on same workspace** (avoid `dcx up` + `dcx down` simultaneously; users retry)
- **No automatic Colima setup** (users edit `colima.yaml` manually)
- **VS Code "Reopen in Container" unsupported** (no way to intercept VS Code's bundled devcontainer CLI). Workaround: `dcx up` + "Attach to Running Container"
- **Custom `workspaceMount` in devcontainer.json:** If project specifies custom `workspaceMount` pointing to original path, it overrides dcx's rewrite → container fails. Solution: remove or adjust `workspaceMount` to use relay path.
