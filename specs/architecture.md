# Architecture: `dcx` — Dynamic Workspace Mounting for Colima {#top}

## Problem & Solution {#problem-solution}

**Problem:** Colima mounts are static (set in `colima.yaml` at startup). `devcontainer up` needs dynamic workspace paths that don't exist in the VM yet. Broadly mounting `$HOME` exposes all projects to every container — risky when running AI agents autonomously (agent in project-A could access project-B).

**Solution:** Use `bindfs` (FUSE userspace tool, no root required) to project workspace into a pre-mounted relay directory (`~/.colima-mounts`). Only needed directories exposed, only while in use. Multiple workspaces mount simultaneously, each isolated.

**Alternatives rejected:** Mount `$HOME` broadly (security risk), `initializeCommand` in devcontainer.json (can't inject relay path into static `workspaceMount`), Docker volumes (no bidirectional host sync).

### Why bindfs over system mounts?

**FUSE** (Filesystem in Userspace) is a kernel module that lets user programs implement custom filesystems without modifying kernel code. **bindfs** is a FUSE tool that creates bind mounts (re-exposing a directory at another path) with custom behavior.

**Why bindfs for dcx:**
- **No root required** — FUSE runs as the user, so `dcx` doesn't need `sudo` for mount operations (Colima bridges privileges through sshfs)
- **Dynamic creation/destruction** — Mounts can be created on demand and cleaned up immediately without manual mount table management
- **Per-user isolation** — `--no-allow-other` restricts access to the current user, preventing container access to other users' projects
- **Survives host reboot** — FUSE mounts don't survive VM reboot, but `dcx up` re-mounts them automatically
- **Simpler than standard mounts** — Standard `mount --bind` would require `sudo` and permanent mount table entries, making cleanup harder

Compared to system mounts (e.g., `mount -o bind`), bindfs gives dcx the flexibility to dynamically isolate workspaces without privilege escalation or persistent kernel state.

---

## Design {#design}

**Data Flow:**
```
Host /home/user/myproject ──[bindfs]──> Host ~/.colima-mounts/dcx-myproject-a1b2c3d4
                                              ↓ [Colima mount]
                                         VM ~/.colima-mounts/dcx-myproject-a1b2c3d4
                                              ↓ [Docker bind mount]
                                         Container /home/user/myproject
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
dcx up [--workspace-folder PATH] [--config-dir DIR] [--file PATH]... [--network MODE] [--no-cache] [--dry-run] [--yes]
```

**Flags:**
- `--workspace-folder PATH` — workspace directory (default: current dir)
- `--config-dir DIR` — directory containing `devcontainer.json` (and optionally `dcx_config.yaml`); skips auto-detection; resolves `devcontainer.json` from within and forwards it to `devcontainer up`. Overridden by `DCX_DEVCONTAINER_CONFIG_DIR_PATH` if both are set (flag wins).
- `--file PATH` — host file path to stage into the container (may be repeated); see file staging below
- `--network MODE` — network isolation level (default: `minimal`)
- `--no-cache` — build the container image without using Docker cache (passed as `--build-no-cache` to `devcontainer up`)
  - `restricted` — no network access; block all external traffic
  - `minimal` — dev tools only (GitHub, npm, Anthropic APIs, VSCode, Sentry) [default]
  - `host` — allow host network only
  - `open` — unrestricted access; all traffic allowed

**Behavior:**
1. Validate Docker available; fail with exit 1 if not
2. Resolve workspace path; fail exit 2 if missing
3. Resolve `--config-dir` to absolute path; verify it is a directory containing `devcontainer.json`; fail exit 2 if not found or missing `devcontainer.json`
4. Guard against recursive mounts (path starts with `~/.colima-mounts/dcx-`)
5. Verify devcontainer config exists (`.devcontainer/devcontainer.json` or `.devcontainer.json`); skip if `--config-dir` provided
6. Compute mount point hash
7. Set `DCX_NETWORK_MODE=<mode>` in host env before spawning devcontainer (devcontainer forwards it via `containerEnv`; `postStartCommand` uses `sudo --preserve-env=DCX_NETWORK_MODE` so the firewall script sees the mode)
8. If `--dry-run`: print plan (including resolved `devcontainer.json` path if `--config-dir` provided), exit 0
9. Auto-create `~/.colima-mounts/` (system defaults)
10. If mount exists: verify health + source matches (idempotent), else recover from stale
11. If mount missing: create + mount with `bindfs --no-allow-other`
12. If workspace not owned by user: warn + prompt (skip with `--yes`)
13. Discover mounts from `colima.yaml`: read colima config, extract mounts, filter out `~/.colima-mounts`, expand tilde paths, and check which host paths exist. For directory mounts, build bind mount entries (source == target == original host path). For file mounts, stage via hardlink into `~/.colima-mounts/.dcx-<name>-files/` (see file staging below). Build environment variable overrides for well-known apps (git, claude). Merge config settings (network, yes, files) from `dcx_config.yaml` using discovery order (see [dcx_config.md](dcx_config.md)). Also process files from CLI `--file` flags via the same file staging mechanism. Create override-config JSON mapping `workspaceMount` and `workspaceFolder` to the original workspace path, plus the discovered mounts and env vars. Pass `--workspace-folder` → mount point (relay path) and `--override-config` → override JSON. Forward `--config` (resolved `devcontainer.json`) if provided.
14. Network mode enforcement: check if any existing containers have a mismatched `dcx.network-mode` label. If found, stop and remove them so `devcontainer up` creates a fresh container with the requested mode. Handles containers that survived `dcx down` for any reason (e.g., FUSE mount disappeared but container remained).
15. Delegate to `devcontainer up` (devcontainer stamps container with label `dcx.network-mode=<mode>`)
16. On failure: rollback (unmount + remove dir), exit 1
17. On SIGINT: rollback before exit

**File staging:**

Colima cannot mount individual files into the VM — only directories. To make individual host files (e.g., `~/.gitconfig`, `~/.claude.json`) accessible inside containers, dcx stages them:

1. Compute staging directory: `~/.colima-mounts/.dcx-<name>-files/` (dot-prefixed to avoid scan_relay pickup)
2. **For standard files** (`sync: false`, default):
   - Hardlink the file into the staging directory (same inode → writes inside container propagate to host)
   - If hardlink fails (EXDEV, cross-filesystem): fall back to `std::fs::copy` with a readonly mount and a warning
   - Inject the staged path as a bind mount (source=staged, target=original host path)

3. **For synced files** (`sync: true`):
   - Copy the file into the staging directory via `fs::copy` (overwrites content in-place, stable inode)
   - Mount at `/home/<remoteUser>/<filename>` (read from `devcontainer.json` `remoteUser` field)
   - Spawn background sync daemon (orphan process) before `devcontainer up`
   - Daemon watches **parent directories** of source and staging files using inotify (Linux) / FSEvents (macOS), filtering events by filename — handles atomic writes (temp+rename) correctly
   - Uses SHA256-based debouncing to detect actual content changes (avoids spurious syncs)
   - Writes use atomic temp+rename (never truncates destination mid-write)
   - Staging→source sync is guarded: empty staging file cannot overwrite non-empty source (prevents data loss from container writing stripped configs)
   - Falls back to 1-second polling if file watcher unavailable
   - On `dcx down`: kill daemon via SIGTERM; cleanup staging directory
   - On rollback (failed `dcx up`): kill daemon via SIGTERM before removing staging directory (prevents orphaned daemons)

**Files can be declared three ways:**
- Colima mounts (`colima.yaml`): if a mount entry resolves to a file, it is staged (standard mode)
- Per-project config: `dcx_config.yaml` alongside `devcontainer.json` with `up.files:` list (standard or synced, see [dcx_config.md](dcx_config.md))
- Ad-hoc: `dcx up --file PATH` (always standard mode)

**When to use standard vs synced:**

- **Standard** (`sync: false`): Static config files (git, ssh), rarely updated after container starts
- **Synced** (`sync: true`): Auth files updated atomically by host apps (Claude Code auth, Docker credentials), need real-time propagation

**Examples:**

`dcx_config.yaml` (per-project, nested `up.files:` key):
```yaml
up:
  network: minimal
  files:
    - path: ~/.gitconfig
    - path: ~/.ssh/config
    - path: ~/.claude.json
      sync: true            # Live-sync auth file (inotify/FSEvents with 1s fallback)
```

CLI flag (ad-hoc, singular `--file`, repeatable; always standard):
```bash
dcx up --file ~/.gitconfig --file ~/.ssh/config
```

See [dcx_config.md](dcx_config.md) for full configuration reference, merge behavior, and discovery rules.

---

### `dcx exec` {#cmd-exec}

**Usage:**
```bash
dcx exec [--workspace-folder PATH] [--config-dir DIR] COMMAND [ARGS...]
```

**Flags:**
- `--workspace-folder PATH` — workspace directory (default: current dir)
- `--config-dir DIR` — directory containing `devcontainer.json`; validated but not forwarded (container was already configured by `dcx up`). Overridden by `DCX_DEVCONTAINER_CONFIG_DIR_PATH` if both are set (flag wins).

**Behavior:**
1. Validate Docker available; fail exit 1
2. Resolve workspace path
3. Validate `--config-dir` if provided: resolve to absolute path, verify it is a directory containing `devcontainer.json`; fail exit 2 if not found or missing `devcontainer.json`
4. Guard: reject `~/.colima-mounts/dcx-*` paths
5. Verify mount exists + healthy
6. Find running container by `devcontainer.local_folder` label on the relay mount point
7. Print network mode (read from container label `dcx.network-mode`)
8. Delegate to `docker exec` with `-i` (stdin open) and `-t` (pseudo-TTY) flags when appropriate. `-i` is always passed for input passthrough. `-t` is added when stdin is a terminal (interactive sessions), omitted for piped input. Uses docker directly instead of devcontainer exec to avoid config resolution issues and lifecycle hook re-execution that caused concurrent session conflicts. The container's default user (set to `remoteUser` by devcontainer during creation) is inherited automatically. Command format: `docker exec -i [-t] -w <original_workspace_path> <container_id>`
9. The user's shell lands in the original workspace path (e.g., `/home/user/myproject`)
10. Forward SIGINT to child process (same process group)

---

### `dcx down` {#cmd-down}

**Usage:**
```bash
dcx down [--workspace-folder PATH]
```

**Behavior:**
1. Validate Docker; fail exit 1
2. Resolve workspace; fail exit 2 if missing
3. Guard against recursive mounts: fail if path is under `~/.colima-mounts/dcx-*` (a managed path)
4. Compute mount point
5. If no mount AND no container: print "nothing to do", exit 0 (idempotent). Handles FUSE mount disappearing while container survives.
6. Stop and remove container (find by `devcontainer.local_folder` label; `docker stop` then `docker rm`)
7. Kill sync daemon via SIGTERM (if PID file exists in staging dir)
8. Unmount bindfs
9. Remove mount directory
10. Remove staging directory `~/.colima-mounts/.dcx-<name>-files/` if it exists (non-fatal)
9. On SIGINT during unmount: complete unmount before exit

---

### `dcx logs` {#cmd-logs}

**Usage:**
```bash
dcx logs [--workspace-folder PATH] [--follow] [--since VALUE] [--until VALUE] [--tail VALUE]
```

**Flags:**
- `--workspace-folder PATH` — workspace directory (default: current dir)
- `--follow` — stream logs (equivalent to `docker logs --follow`)
- `--since VALUE` — show logs since timestamp or relative duration (e.g., `2024-01-01T00:00:00Z`, `10m`, `now`)
- `--until VALUE` — show logs before timestamp or duration
- `--tail VALUE` — number of lines to show from end of logs (e.g., `20`, `all`)

**Behavior:**
1. Validate Docker available; fail exit 1
2. Resolve workspace path; fail exit 2 if missing
3. Compute mount point
4. Find container (running or stopped) by `devcontainer.local_folder` label on the relay mount point
5. If no container found: error message, exit 1
6. Build `docker logs` args: always include `--timestamps`, pass through all provided flags (--follow, --since, --until, --tail) verbatim
7. Stream output from `docker logs --timestamps ...` directly to terminal; Ctrl+C exits cleanly
8. Forward docker's exit code

**Notes:**
- Mirrors `docker logs` behavior exactly — all validation/filtering by Docker
- Works with both running and stopped containers
- Output includes RFC3339 timestamps added by Docker (one per line)
- No dcx-specific log file writing; all output goes to terminal

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
   - Remove container + runtime image (by repo tag, not `--force`, to avoid removing build image)
   - If `--purge`: attempt to remove `dcx-base:<mount_name>` tag (alias created during `dcx up` for `"image"` field configs; no-op for `"build"` configs)
   - Remove captured volumes (if any)
   - Kill sync daemon via SIGTERM (if PID file exists in staging dir)
   - Unmount bindfs
   - Remove mount directory
   - Remove staging directory (non-fatal)
9. Scan for orphaned mounts (mounted but no container): unmount + remove
10. Clean orphaned `vsc-*-uid` runtime images (runtime images without containers)
11. If `--purge`: clean orphaned `vsc-*` build images (no `-uid` suffix) without containers — handles `"build"` configs and the two-step `dcx clean` then `dcx clean --purge` workflow
12. Print summary + exit 0 (or 1 if failures)

**Behavior (--all mode):**
- Same, but iterate all `dcx-*` mounts, continue on individual failures
- If `--purge`: after per-mount cleanup, deduplicate + remove all build images, sweep remaining `dcx-*` volumes
- Print summary count

---

### `dcx status` {#cmd-status}

**Usage:**
```bash
dcx status
```

**Behavior:**
1. Query all `dcx-*` mounts in `~/.colima-mounts/`
2. For each mount, determine status:
   - `running` — mount exists and is accessible, container running
   - `orphaned` — mount exists and is accessible, no container
   - `stale mount` — mount directory exists but is not accessible (unmounted)
   - `empty dir` — mount directory doesn't exist, no container
3. Print a formatted table with mount name, status, daemon status (running/stopped), and container ID (if applicable)
4. Exit 0 (always succeeds, even if no mounts exist)

---

### `dcx doctor` {#cmd-doctor}

**Usage:**
```bash
dcx doctor
```

**Behavior:**
1. Check prerequisites and report status:
   - Docker available (running)
   - `devcontainer` CLI installed
   - `bindfs` installed
   - Relay directory `~/.colima-mounts/` accessible
2. Print a report with each check's status (✓ pass, ✗ fail)
3. If any check fails, suggest remediation steps (e.g., install command)
4. Exit 0 if all checks pass, 1 if any check fails

---

### `dcx autostart` {#cmd-autostart}

**Usage:**
```bash
dcx autostart enable   # Write service file and start Colima if not running
dcx autostart disable  # Remove autostart configuration
dcx autostart status   # Show current autostart status
```

**Behavior:**

**Enable:**
1. Locate Colima binary via `which colima`; fail exit 127 if not found
2. Compute service file path:
   - Linux: `~/.config/systemd/user/colima.service`
   - macOS: `~/Library/LaunchAgents/io.colima.autostart.plist`
3. Create parent directories if needed
4. Generate platform-specific service content:
   - Linux: systemd user unit with `ExecStart=colima start`, `ExecStop=colima stop`, `WantedBy=default.target`
   - macOS: launchd plist with `ProgramArguments=[colima, start]`, `RunAtLoad=true`
5. Write service file
6. Activate service:
   - Linux: `systemctl --user daemon-reload` → `systemctl --user enable colima` → `systemctl --user start colima`
   - macOS: `launchctl load <service-path>`
7. Print confirmation with service file location
8. Exit 0 on success, 1 on any error

**Disable:**
1. Compute service file path
2. If file doesn't exist: print "Autostart is not configured", exit 0
3. Deactivate service:
   - Linux: `systemctl --user stop colima` → `systemctl --user disable colima`
   - macOS: `launchctl unload <service-path>` (ignore errors)
4. Delete service file
5. Print confirmation
6. Exit 0 on success, 1 on file removal error

**Status:**
1. Compute service file path
2. Print whether autostart is configured (file exists / does not exist)
3. If configured, print service file path
4. Check live service state:
   - Linux: `systemctl --user is-enabled colima` and `systemctl --user is-active colima`
   - macOS: `launchctl list io.colima.autostart` (exit code 0 = loaded)
5. Print enabled and active status
6. Exit 0 always

---

### `dcx completions` {#cmd-completions}

**Usage:**
```bash
dcx completions SHELL
```

**Arguments:**
- `SHELL` — shell type: `bash`, `fish`, `zsh`, `powershell`

**Behavior:**
1. Generate shell completion script for the specified shell
2. Print to stdout
3. User pipes to shell rc file: `dcx completions bash >> ~/.bashrc`
4. Exit 0

---

## Environment Variables {#env-vars}

| Variable | Used by | Description |
|---|---|---|
| `DCX_DEVCONTAINER_CONFIG_DIR_PATH` | `up`, `exec` | Default directory containing `devcontainer.json`. Overridden by `--config-dir` if both are set. |
| `DCX_NETWORK_MODE` | `init-firewall.sh` (internal) | Set by `dcx up` before spawning devcontainer; forwarded to container via `containerEnv`. Controls firewall rules: `restricted`, `minimal`, `host`, or `open`. |

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
- **Custom `workspaceMount` in devcontainer.json:** dcx injects `workspaceMount` via `--override-config`, which takes precedence over the project's `devcontainer.json`. If a project specifies its own `workspaceMount`, it will be overridden by dcx.
