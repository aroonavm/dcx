# TODO

## Bug: `~/.claude.json` goes stale in container — causes re-onboarding on every session

### Context

`.devcontainer/full/dcx_config.yaml` lists `~/.claude.json` under `up.files`:

```yaml
up:
  files:
    - path: ~/.gitconfig
    - path: ~/.claude.json
```

`dcx up` processes `up.files` via `stage_file` (`src/up.rs`), which creates a **hard link**
from `~/.claude.json` into the relay staging dir under `~/.colima-mounts/.dcx-<id>-files/`.
That staged path is then bind-mounted into the container at the original path.

### Root Cause

Hard links share an inode. Atomic writes (used by Claude Code and most apps) work by writing
to a temp file then calling `rename()` over the target. `rename()` installs a new inode at
`~/.claude.json`. The hard link in the staging dir still points to the **old inode** — the
container now sees stale content and will never see subsequent updates.

Proven by:
```
# After dcx down && dcx up — inodes already differ again after one Claude Code write:
ls -i ~/.claude.json
ls -i ~/.colima-mounts/.dcx-dcx-e007842c-files/.claude.json
# → different inode numbers, different content
```

`~/.claude.json` contains `hasCompletedOnboarding: true` on the host. The stale staged copy
has `hasCompletedOnboarding: false`, causing Claude Code in the container to show the
onboarding/theme prompt on every session start.

### Constraint

Colima does not support mounting individual files — only directories. So `~/.claude.json`
cannot be added to `colima.yaml` mounts the way `~/.claude/` is.

### What needs fixing

Find an alternative to hard-link staging for `~/.claude.json` (and any other file in
`up.files`) that survives atomic writes, so the container always sees the current version
of the file from the host.

---

## Implementation Plan: Live Sync (Option B)

### Architecture

- **Copy, not hardlink**: fs::copy overwrites staging file in-place; inode stable for VirtioFS propagation
- **Sync daemon**: Watches inotify/FSEvents with 1s polling fallback; SHA256 debounce; two-way sync (host ↔ staging ↔ container)
- **Target override**: For `sync: true` files, mount at `/home/<remoteUser>/<filename>`
- **Daemon lifecycle**: Spawned by `dcx up` as orphan; killed by `dcx down` via SIGTERM

### Implementation Tasks (TDD order)

#### Phase 1: Core Utilities & Data Structures

- [x] 1.1 — Unit tests: parse_remote_user (with comments, missing field)
- [x] 1.2 — Unit tests: container_home (regular user, root)
- [x] 1.3 — Implement parse_remote_user() and container_home() in src/up.rs
- [x] 1.4 — Unit tests: sha256_file (known content, missing file)
- [x] 1.5 — Create src/sync.rs with SyncPair struct and sha256_file()
- [x] 1.6 — Unit tests: file mount deserialization (with sync, without sync)
- [x] 1.7 — Extend src/dcx_config.rs: add sync field to FileMount
- [x] 1.8 — Update call sites in src/up.rs to use FileMount struct

#### Phase 2: Sync Daemon

- [x] 2.1 — Unit tests: sync loop behavior (no trigger, source change, staging change, loop prevention)
- [x] 2.2 — Implement run_sync_daemon() in src/sync.rs with poll loop and SIGTERM handler

#### Phase 3: CLI & Main

- [x] 3.1 — Add hidden _sync-daemon subcommand to src/cli.rs
- [x] 3.2 — Implement route in src/main.rs for SyncDaemon command
- [x] 3.3 — Implement parse_sync_pairs() helper

#### Phase 4: Staging & Daemon Spawn (src/up.rs)

- [x] 4.1 — Update explicit-files staging: fs::copy for sync: true files, record SyncPair
- [x] 4.2 — Implement mount target override for synced files
- [x] 4.3 — Spawn sync daemon before devcontainer up call
- [ ] 4.4 — Integration test: dcx up --dry-run with sync: true
- [ ] 4.5 — Integration test: dcx up --dry-run without remoteUser

#### Phase 5: Daemon Lifecycle (src/down.rs)

- [x] 5.1 — Read PID file and send SIGTERM in src/down.rs

#### Phase 6: Configuration & Documentation

- [x] 6.1 — Add sync: true to .devcontainer/full/dcx_config.yaml
- [ ] 6.2 — Update specs/architecture.md
- [ ] 6.3 — Update specs/guides/setup.md

#### Phase 7: Verification (make check + E2E)

- [x] 7.1 — make check passes (348 tests pass, no clippy errors, formatting OK)
- [ ] 7.2 — End-to-end: verify daemon running, live sync works, dcx down kills it
