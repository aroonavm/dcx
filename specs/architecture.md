# Architecture: `dcx` — Dynamic Workspace Mounting Wrapper for Colima Devcontainers

## Problem

Colima mounts host directories into its VM at startup time via `colima.yaml` config. There is no CLI or API to add new mounts to a running instance after initialization.

When `devcontainer up --workspace-folder /path/to/project` runs, the devcontainer CLI passes `/path/to/project` as a Docker bind-mount source. Docker runs inside the Colima VM, so the path must exist in the VM's filesystem. If it's not in a mounted directory, Docker fails:

```
docker: Error response from daemon: invalid mount config for type "bind":
  bind source path does not exist: /path/to/project
```

Broadly mounting `$HOME` or `~/Documents` is a security concern — a compromised VM or container could read/write the entire mount tree. This is especially important when running AI coding agents inside devcontainers: an agent in project-A could read/write project-B if both are under a shared mount.

## Alternatives Considered

| Approach | Why not |
|----------|---------|
| **Mount `$HOME` or `~/projects` broadly in Colima** | Simplest fix, but exposes all projects to every container. When running AI agents autonomously, this means a rogue process in one workspace can access all others. |
| **`initializeCommand` in devcontainer.json** | Runs on host before container creation — correct timing for bindfs. But VS Code still passes the original workspace path as Docker bind mount source. The dynamically computed relay path can't be injected into the static `workspaceMount` JSON property. |
| **Docker volumes instead of bind mounts** | No bidirectional host file sync. Edits inside the container aren't reflected on the host filesystem, breaking standard development workflows. |

## Solution

Use `bindfs` (a FUSE userspace bind-mount tool, no root required) to project the workspace directory into a path that Colima already mounts into the VM. A wrapper script (`dcx`) automates this:

1. A single directory `~/.colima-mounts` is permanently mounted in the VM via `colima.yaml`.
2. Before running `devcontainer up`, the wrapper creates a `bindfs` mount: `bindfs --no-allow-other /actual/project/path ~/.colima-mounts/<name>-<hash>`
3. The wrapper rewrites `--workspace-folder` to point at the bindfs mount path.
4. Colima mounts `~/.colima-mounts` into the VM, making the bindfs content accessible there.
5. Docker in the VM can bind-mount from the path because it exists under `~/.colima-mounts`.
6. On teardown (`dcx down`), the wrapper stops the container and unmounts bindfs.

Multiple workspaces can be mounted simultaneously — each gets its own `dcx-` prefixed mount point. Only the mounted project directories are exposed, only while needed.

## Data Flow

```
Host:   /home/user/Documents/myproject/          (actual files)
             |
             | bindfs (FUSE, userspace, no root)
             v
Host:   ~/.colima-mounts/dcx-myproject-a1b2c3d4/    (mirror mount point)
             |
             | Colima mount (configured in colima.yaml)
             v
VM:     ~/.colima-mounts/dcx-myproject-a1b2c3d4/    (mounted in VM)
             |
             | Docker bind mount
             v
Container: /workspace/                            (devcontainer workspace)
```

Reads and writes flow bidirectionally through this chain. File ownership (UID/GID) is preserved as-is through the entire chain — no remapping is performed. devcontainer handles user mapping via `remoteUser` in `devcontainer.json`.

## Architecture & Design

### Language: Rust (Single Binary)

**Why Rust?**
- Single binary: no shell dependencies or environment setup quirks
- Maintainability: clear, readable code; easy for others to understand and contribute
- Error handling: explicit error types with helpful messages
- Simplicity: stdlib handles paths, argument parsing, and subprocesses well
- Cross-platform: same codebase for Linux and macOS

### Core Design

Simple, straightforward approach:
1. Parse subcommand and arguments using `clap` (with shell completion generation via `clap_complete`)
2. Validate prerequisites (`bindfs` installed, Docker/Colima running, workspace exists)
3. Resolve workspace path to absolute path
4. Compute mount point using deterministic hash-based naming
5. Execute operations (mount/unmount/delegate) via subprocess
6. Rewrite `--workspace-folder` argument and delegate to real `devcontainer` CLI

Subprocess calls are direct and explicit — any errors from subprocess failures are clear to readers.

### Design Philosophy

- **Simple over sophisticated**: direct subprocess calls, no trait abstractions
- **Fail fast**: on errors; no graceful recovery or retries (exception: `dcx clean` continues on individual failures to maximize cleanup)
- **Test pyramid**: unit tests for pure logic, `assert_cmd` integration tests against the binary, shell E2E for full infrastructure (see [testing.md](testing.md))
- **Code clarity**: obvious to readers, not clever

### Acceptable Trade-offs

- Concurrent `dcx up` and `dcx down` for the same workspace may conflict; users can retry
- No locking between commands; simplicity and clarity prioritized

### Mount Discovery via Naming Convention

`dcx` does not use a state file. Instead, it uses a `dcx-` prefix on all mount directories:

- Mount naming: `dcx-<workspace-name>-<hash>` where `<workspace-name>` is the last component of the absolute path, sanitized (non-alphanumeric characters replaced with `-`, max 30 characters). `<hash>` is first 8 hex chars of SHA256 of absolute path (e.g., `dcx-myproject-a1b2c3d4`)
- To discover active mounts: scan `~/.colima-mounts/` for entries matching the `dcx-` prefix
- To check mount health: verify the mount point is accessible (e.g., `ls` succeeds)
- To query mount source paths: parse mount table (`/proc/mounts` on Linux, `mount` command output on macOS)

This makes the filesystem the single source of truth — no state to corrupt or go stale.

### bindfs Options

`dcx` invokes bindfs with `--no-allow-other`. No other options are used.

- **`--no-allow-other`:** Restricts mount access to the current user. This is sufficient because Colima bridges host→VM via sshfs (which has its own `allow_other`), so Docker in the VM can read/write through the mount without the host FUSE mount needing `allow_other`. This avoids requiring `user_allow_other` in `/etc/fuse.conf`.
- **No ownership remapping:** File UID/GID is preserved as-is through bindfs. devcontainer handles user mapping via `remoteUser` in `devcontainer.json`.
- **No symlink options:** Default symlink behavior — relative symlinks within the workspace work; absolute symlinks pointing outside the workspace will be dangling inside the container.

### Relay Directory Auto-Creation

`dcx` automatically creates `~/.colima-mounts/` if it doesn't exist, using system default permissions (respects umask). No manual setup required. The home directory's own permissions naturally protect the relay in multi-user environments.

## Subcommand Specifications

### `dcx up` — Start devcontainer

**Usage:**
```bash
dcx up                                    # Uses current directory
dcx up --workspace-folder /path/to/project
dcx up --workspace-folder=./local/path
dcx up --dry-run                          # Show what would happen without doing it
dcx up --yes                              # Skip confirmation for non-owned directories
```

**Behavior:**
1. Validate Docker/Colima is available (`docker info` succeeds); fail fast with "Docker is not available. Is Colima running?" (exit code 1)
2. Resolve workspace path to absolute path (handle symlinks, `.`, `..`)
3. If workspace path starts with `~/.colima-mounts/dcx-`, fail with "Cannot use a dcx-managed mount point as a workspace. Use the original workspace path instead." (exit code 2)
4. Check for devcontainer configuration (`.devcontainer/devcontainer.json` or `.devcontainer.json`); fail fast with "No devcontainer configuration found in <path>." (exit code 2)
5. Compute mount point using hash-based naming scheme
6. **If `--dry-run`:** print what would happen and exit (no filesystem changes):
   ```
   Would mount: /home/user/myproject → ~/.colima-mounts/dcx-myproject-a1b2c3d4
   Would run: devcontainer up --workspace-folder ~/.colima-mounts/dcx-myproject-a1b2c3d4
   ```
7. Auto-create `~/.colima-mounts/` if it doesn't exist (system default permissions)
8. If mount point already exists:
   - Verify mount is healthy (test accessibility, e.g., `ls` the mount point)
   - If healthy:
     - Query mount table to detect actual source path (`/proc/mounts` on Linux, `mount` output on macOS)
     - Compare against current workspace path (must match exactly)
     - If sources match: reuse the existing mount (idempotent)
     - If sources differ: fail with hash collision error (see Edge Cases section)
   - If unhealthy (stale): unmount, remount with `bindfs`
9. If mount point doesn't exist:
   - Create directory
   - Mount with `bindfs --no-allow-other` (source: workspace, target: mount point)
10. Rewrite `--workspace-folder` argument to point to mount point
11. Delegate to real `devcontainer up` and wait for completion (output flows through to stderr/stdout)
12. If `devcontainer up` fails: print "Mount rolled back." to stderr, unmount bindfs and remove mount directory (atomicity)

**Signal handling:** If user sends Ctrl+C (SIGINT) after bindfs mount but before `devcontainer up` completes, `dcx` traps the signal and rolls back the mount. The system is always in a clean state: fully up or fully down.

**Mount point naming:** `dcx-<workspace-name>-<hash>` where `<hash>` is the first 8 hex characters of SHA256 of the absolute workspace path. Example: `/home/user/Documents/myproject` → `dcx-myproject-a1b2c3d4`

**Workspace validation:**
- Must exist on host filesystem
- If not owned by current user, warn and ask for confirmation (show read/write implications). Use `--yes` flag to skip the prompt.

### `dcx exec` — Run command in devcontainer

**Usage:**
```bash
dcx exec --workspace-folder . -- bash
dcx exec -- npm test              # Uses current directory
```

**Behavior:**
1. Validate Docker/Colima is available; fail fast with "Docker is not available. Is Colima running?" (exit code 1)
2. Resolve workspace path
3. If workspace path starts with `~/.colima-mounts/dcx-`, fail with "Cannot use a dcx-managed mount point as a workspace. Use the original workspace path instead." (exit code 2)
4. Verify mount exists (created by `dcx up`); fail with "No mount found for <path>. Run `dcx up` first."
5. Verify mount is healthy (accessible). If unhealthy, fail with "Mount is stale. Run `dcx up` to remount."
6. Rewrite `--workspace-folder` argument
7. Delegate to real `devcontainer exec`

**Signal handling:** SIGINT is forwarded directly to the `devcontainer exec` child process. No special handling needed — `dcx` performs no mount operations during `exec`.

### `dcx down` — Stop container and unmount

**Usage:**
```bash
dcx down                          # Uses current directory
dcx down --workspace-folder /path/to/project
```

**Behavior:**
1. Validate Docker/Colima is available; fail fast with "Docker is not available. Is Colima running?" (exit code 1)
2. Resolve workspace path
3. If workspace directory doesn't exist, fail with "Workspace directory does not exist. Use `dcx clean` to remove stale mounts." (exit code non-zero)
4. If workspace path starts with `~/.colima-mounts/dcx-`, fail with "Cannot use a dcx-managed mount point as a workspace. Use the original workspace path instead." (exit code 2)
5. Compute mount point from workspace path (same hash as `dcx up`)
6. If no mount found: print "No mount found for <path>. Nothing to do." and exit (exit code 0)
7. Stop the container via `docker stop`, finding it by the `devcontainer.local_folder=<rewritten-path>` label. If no running container is found, continue (idempotent). The container is left stopped (not removed) — `dcx clean` handles full removal.
8. Unmount `bindfs` (`fusermount -u` on Linux, `umount` on macOS)
9. Remove mount directory

**Signal handling:** If SIGINT arrives during step 7 (container stop), `docker stop` runs to completion (captured, not streamed). Check the interrupted flag after step 7 completes and bail before unmount if set. If SIGINT arrives during step 8 (unmount in progress), log "Signal received, finishing unmount..." and complete the unmount before exiting.

**Idempotent for missing mounts:** Safe to call multiple times when mount was never created. If no mount found, prints informational message and exits cleanly (exit code 0). If workspace directory is deleted, fails with error — use `dcx clean` to recover.

### `dcx clean` — Full cleanup of container, image, and mount

**Usage:**
```bash
dcx clean                # Clean current directory's devcontainer
dcx clean --yes          # Clean current directory, skip confirmation
dcx clean --all          # Clean ALL dcx-managed devcontainers
dcx clean --all --yes    # Clean all, skip confirmation
```

**Behavior:**

1. Validate Docker/Colima is available; fail fast with "Docker is not available. Is Colima running?" (exit code 1)

**Without `--all` (default — current workspace):**

2. Resolve workspace path (current directory or `--workspace-folder`)
3. Compute mount point from workspace path (same hash as `dcx up`)
4. If no mount found and no stopped container found: print "Nothing to clean for <path>." and exit (exit code 0)
5. If a running container is found: prompt for confirmation. `--yes` skips the prompt.
   ```
   ⚠ Active container will be stopped:
     /home/user/myproject  →  dcx-myproject-a1b2c3d4  (container: abc123)

   Continue? [y/N]
   ```
6. Full cleanup for this workspace:
   - Stop running container if any (`docker stop`)
   - Remove container — running or stopped (`docker rm`)
   - Remove container's image (`docker rmi`)
   - Unmount bindfs (`fusermount -u` on Linux, `umount` on macOS) if mounted
   - Remove mount directory if it exists
7. Print result:
   ```
   Cleaned /home/user/myproject:
     dcx-myproject-a1b2c3d4  was: running  → stopped, removed
   ```

**With `--all`:**

2. Iterate all `dcx-*` entries in `~/.colima-mounts/`
3. For each entry, find associated containers (running or stopped) via the `devcontainer.local_folder` label
4. If any running containers found: prompt for confirmation listing all. `--yes` skips the prompt.
   ```
   ⚠ 2 active containers will be stopped:
     - /home/user/project-a  →  dcx-project-a-a1b2c3d4  (container: abc123)
     - /home/user/project-b  →  dcx-project-b-e5f6g7h8  (container: def456)

   Continue? [y/N]
   ```
5. For each entry, full cleanup:
   - Stop running container if any (`docker stop`)
   - Remove container — running or stopped (`docker rm`)
   - Remove container's image (`docker rmi`)
   - Unmount bindfs if mounted
   - Remove mount directory
6. **Continue on failure:** If any individual mount fails, log the error and continue with remaining mounts
7. Print summary:
   ```
   Cleaned 4 mounts:
     /home/user/project-a  →  dcx-project-a-a1b2c3d4    was: running     → stopped, removed
     /home/user/project-b  →  dcx-project-b-e5f6g7h8    was: orphaned    → removed
     dcx-project-c-i9j0k1l2                              was: stale       → removed
     dcx-old-thing-m3n4o5p6                               was: empty dir   → removed
   ```
   For stale/orphaned mounts, host path may not be recoverable — show mount directory name only.

**Common to both modes:**
- If any failures occurred, print them at the end and exit with non-zero code
- If no entries found to clean: print "Nothing to clean." and exit (exit code 0)
- Container/image removal failures are logged but do not block mount cleanup (continue best-effort)

**Signal handling:** If SIGINT arrives while cleanup is in progress, log "Signal received, finishing current cleanup..." and complete the current entry's cleanup before exiting. Remaining entries (in `--all` mode) are left for the user to re-run `dcx clean --all`.

**Design rationale:** `dcx clean` targets a single workspace for precise cleanup. `dcx clean --all` is the recovery tool for full resets. Both modes do full cleanup: stop container, remove container, remove image, unmount, remove directory. Both use regular `umount` (not lazy) to ensure deterministic cleanup.

### `dcx status` — Show mounted workspaces

**Usage:**
```bash
dcx status
```

**Behavior:**
1. Validate Docker/Colima is available; fail fast with "Docker is not available. Is Colima running?" (exit code 1)
2. Scan `~/.colima-mounts/` for all `dcx-*` entries
3. For each entry:
   - Check if it's a healthy mount (accessible via `ls`)
   - For healthy mounts: query mount table to resolve original host path
   - Check if a running container exists for this mount
   - Query containers via `docker ps --filter label=devcontainer.local_folder=<rewritten-path>` to match containers to mounts
4. Print table:
   ```
   WORKSPACE                    MOUNT                        CONTAINER    STATE
   /home/user/project-a         dcx-project-a-a1b2c3d4        abc123       running
   /home/user/project-b         dcx-project-b-e5f6g7h8        def456       running
   (unknown)                    dcx-project-c-i9j0k1l2        (none)       stale mount
   ```
   For stale mounts where host path can't be resolved, show `(unknown)`.
5. If no `dcx-*` entries found: print "No active workspaces."

### `dcx doctor` — Validate setup

**Usage:**
```bash
dcx doctor
```

**Behavior:**
1. Run all prerequisite checks without side effects (no mounts created, no containers started)
2. Print results:
   ```
   Checking prerequisites...
     ✓ bindfs installed (1.17.2)
     ✓ devcontainer CLI installed (0.71.0)
     ✓ Docker available (27.1.1)
     ✓ Colima running (0.8.1)
     ✓ ~/.colima-mounts exists on host
     ✓ ~/.colima-mounts mounted in VM (writable)

   All checks passed.
   ```
3. On failure, print the fix:
   ```
     ✗ bindfs not installed
       Fix: sudo apt install bindfs
   ```

**Checks performed:**

| Check | Method |
|-------|--------|
| bindfs installed | `which bindfs` → exit code 0; parse version from `bindfs --version` |
| devcontainer CLI | `which devcontainer` → exit code 0; parse version |
| Docker available | `docker info` → exit code 0; parse version |
| Colima running | `colima status` → exit code 0; parse version |
| Unmount tool available | `which fusermount` (Linux) or `which umount` (macOS) → exit code 0 |
| ~/.colima-mounts exists | `stat ~/.colima-mounts` → is a directory |
| ~/.colima-mounts mounted in VM | `colima ssh -- ls ~/.colima-mounts` succeeds; then `colima ssh -- touch ~/.colima-mounts/.dcx-write-test && rm ~/.colima-mounts/.dcx-write-test` to verify writable |

Exit code 0 if all checks pass, non-zero if any fail.

## Progress Output

All commands print step-by-step progress to stderr:

```bash
$ dcx up
→ Resolving workspace path: /home/user/myproject
→ Mounting workspace to ~/.colima-mounts/dcx-myproject-a1b2c3d4...
→ Starting devcontainer...
→ Done.

$ dcx down
→ Resolving workspace path: /home/user/myproject
→ Stopping devcontainer...
→ Unmounting ~/.colima-mounts/dcx-myproject-a1b2c3d4...
→ Done.
```

## Usage Examples

The `dcx` wrapper augments certain `devcontainer` commands with mount management. All other commands are forwarded to `devcontainer` transparently:

```bash
# These commands use dcx wrapper (bindfs mount is created/managed)
dcx up                              # Create mount, start container
dcx exec -- npm test                # Run command in container
dcx down                            # Stop container, cleanup mount
dcx clean                           # Full cleanup for current workspace (stop, rm, rmi, unmount)
dcx clean --all                     # Full cleanup for ALL dcx-managed workspaces
dcx status                          # Show mounted workspaces and container state
dcx doctor                          # Validate full setup (no side effects)

# dcx's own commands
dcx --help                          # Show dcx help (not devcontainer's)
dcx --version                       # Show dcx version

# These commands forward to devcontainer (no mounting involved)
dcx build                           # Forwards to: devcontainer build
dcx features list                   # Forwards to: devcontainer features list
# ... any other devcontainer subcommand
```

**`dcx --help` and `dcx --version`:** These show `dcx`'s own help and version, not devcontainer's. `dcx --help` lists the 6 managed subcommands and explains the tool's purpose. To see devcontainer's help, use `devcontainer --help` directly.

**Pass-through behavior:** `dcx` maintains a list of known subcommands (`up`, `exec`, `down`, `clean`, `status`, `doctor`). Any subcommand not in this list is forwarded directly to `devcontainer` with all arguments unchanged. This makes `dcx` a transparent drop-in replacement for `devcontainer` — users can use `dcx` for everything.

## Edge Cases

**Workspace already under a Colima mount:** If workspace path starts with `~/.colima-mounts/dcx-`, fail with "Cannot use a dcx-managed mount point as a workspace. Use the original workspace path instead." (exit code 2). This prevents nesting devcontainers inside dcx mounts, which would break if the underlying mount gets cleaned up.

**Stale mount after reboot:** FUSE mounts don't survive host reboots. Mount directory exists but isn't mounted. `dcx up` detects the unhealthy mount, unmounts it, and remounts fresh. `dcx clean` also handles this.

**Hash collision:** Two paths producing same 8-char hash (~1 in 4 billion odds). When `dcx up` finds an existing mount point:
1. Query mount table to detect the actual source path of the existing mount
2. Compare against the current workspace path (absolute, canonicalized)
3. If mismatch detected, fail with clear error showing both paths:
   ```
   ✗ Mount point already exists but points to wrong source!
     Expected: /home/bob/project-bar
     Found:    /home/alice/project-foo

     Hash collision detected (both hash to a1b2c3d4).
     This is extremely rare (~1 in 4 billion).
     Run `dcx clean` to reset and retry.
   ```
4. Log detailed debug info (hash values, mount point name, source paths) to stderr for troubleshooting
5. If mount is healthy and sources match, reuse it (normal idempotent case)

**Container volume identity:** devcontainer CLI computes `devcontainerId` from workspace folder path. Since `dcx` rewrites the path, volumes from non-wrapper runs won't be reused (one-time issue when switching to wrapper).

**Workspace deleted while mounted:** If the source directory is deleted after mounting, the bindfs mount becomes invalid. On next interaction (`dcx exec`, `dcx up`, `dcx down`), `dcx` detects the missing workspace and errors: "Workspace directory does not exist. Use `dcx clean` to remove stale mounts."

**Git root detection:** If workspace is inside git repo, devcontainer may mount git root. With bindfs:
- If workspace IS the git root: `.git` inside mount, works correctly ✓
- If workspace is subdirectory: `.git` not in mount, detection fails gracefully ✓

**Symlinks pointing outside the workspace:** Symlinks with relative targets within the workspace resolve correctly through bindfs. Symlinks with absolute targets outside the workspace will be dangling inside the container (the target path doesn't exist in the mount). This is inherent to bind mounts and is a known limitation — not something `dcx` attempts to handle.

**Colima restart while mounts are active:** If Colima restarts, the VM is recreated but host-side bindfs mounts survive. Colima re-mounts `~/.colima-mounts` from the host into the new VM (per `colima.yaml` config), so `dcx-*` mount content remains visible. Containers are lost (VM recreated) but `dcx up` recovers automatically — it detects the healthy mount, reuses it, and starts a new container.

## Permissions: Non-User-Owned Directories

If a user attempts to mount a directory not owned by the current user:

1. **Warning prompt:** Display warning and ask for confirmation
2. **Explanation:** Show that:
   - Directory is owned by `<owner>` (UID)
   - Current user is `<username>` (UID)
   - Mounted to container as `<username>` user (same user, not root)
3. **Read/Write behavior in container:**
   - Read: allowed if directory has readable permissions for "others" or owner's group
   - Write: allowed if directory has writable permissions for "others" or owner's group
   - If permissions don't allow it, operations will fail inside container with permission errors
4. **User decision:** Confirm or abort mount attempt

Example warning:
```
⚠️  Directory /var/shared is owned by root (UID 0)
    Current user is foobar (UID 1000)

    In the container, you'll run as foobar (1000).
    You'll have read/write access only if the directory permissions allow it.

Proceed? [y/N]
```

---

## Exit Codes

| Exit Code | Meaning | Examples |
|-----------|---------|---------|
| 0 | Success | `dcx up` completed; `dcx down` with no mount found ("nothing to do") |
| 1 | Runtime error | Mount failed; unmount failed; `devcontainer up` failed; `dcx doctor` any check failed |
| 2 | Usage / input error | Workspace path doesn't exist; invalid arguments |
| 4 | User aborted | User answered "N" to confirmation prompt |
| 127 | Prerequisite command not found | `bindfs` not installed; `devcontainer` CLI not found |
| N | Pass-through from child process | Pass-through commands (`dcx build`, etc.) return the child's exit code directly |

**Notes:**
- `dcx doctor` returns 0 if all checks pass, 1 if any check fails.
- Pass-through commands forward the child process exit code unchanged.
- When `dcx up` fails during `devcontainer up`, it returns 1 (not the child's exit code) because `dcx` performs rollback — the failure is a `dcx` error, not just a pass-through.

---

## Platform Notes

`dcx` targets Linux and macOS. The core logic is identical; platform differences are limited to:

| Operation | Linux | macOS |
|-----------|-------|-------|
| Query mount sources | `/proc/mounts` | `mount` command output |
| Unmount bindfs | `fusermount -u` | `umount` |
| Install bindfs | `sudo apt install bindfs` | `brew install bindfs` |
| Install devcontainer | `npm install -g @devcontainers/cli` | `npm install -g @devcontainers/cli` |

All examples in this spec use Linux commands. macOS equivalents apply where noted above.

---

## Known Limitations

**CLI-first tool:** `dcx` wraps the `devcontainer` CLI. VS Code's "Reopen in Container" uses its own bundled devcontainer CLI internally — there is no VS Code setting to swap it for a custom binary. `dcx` cannot intercept that path.

**VS Code workflow:** VS Code users can use `dcx up` from the terminal to create the mount and start the container, then use VS Code's "Attach to Running Container" to connect. This is slightly more manual than "Reopen in Container" but works fully.

**Custom `workspaceMount` in devcontainer.json:** If a project's `devcontainer.json` specifies an explicit `workspaceMount` property pointing to the original host path (not the relay path), it overrides `dcx`'s rewritten `--workspace-folder`. `dcx` does not detect or handle this conflict — the container will fail to start with a bind mount error. Remove the custom `workspaceMount` or adjust it to work with the relay path.

---

See [`specs/README.md`](README.md) for links to implementation details and guides.
