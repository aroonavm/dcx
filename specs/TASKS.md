# dcx Implementation Roadmap

> **Specs are the source of truth.** Read the relevant spec section before implementing each phase.
> Follow TDD: write failing tests first, then implement. See `testing.md` for the test pyramid.
> Run `make check` before considering any phase done.

---

## Phase 0: Project Scaffolding

- `cargo init --name dcx`
- Dependencies: `clap` (derive), `clap_complete`, `sha2`
- Dev-dependencies: `assert_cmd`, `predicates`, `assert_fs`
- Verify `make check` passes

---

## Phase 1: Pure Logic (Sans-IO)

Pure functions with zero external dependencies. TDD each with `#[test]`.

See: `architecture.md` § "Mount Discovery via Naming Convention"

| Function | What to test |
|----------|-------------|
| Path sanitization | non-alphanumeric → `-`, max 30 chars, empty, unicode |
| Hash computation | SHA256, first 8 hex chars, deterministic |
| Mount name computation | `dcx-<name>-<hash>` from absolute path |
| Mount table parsing (Linux) | Parse `/proc/mounts` for bindfs entries |
| Mount table parsing (macOS) | Parse `mount` output for bindfs entries |
| Mount source lookup | Given entries + mount point → source path or None |
| Mount categorization | active / orphaned / stale / empty classification |
| Exit codes | Constants matching spec exit code table |
| Output formatting | Status table, doctor checks, clean summary |
| Path validation | Detect `dcx-` managed paths (recursive mount guard) |
| Relay dir resolution | `~/.colima-mounts` expansion |

---

## Phase 2: Platform Abstraction + Subprocess Helpers

Platform-specific code and subprocess wrappers. These touch the OS and real commands.

See: `architecture.md` § "Platform Notes"

**Platform abstraction:**
- Unmount command: `fusermount -u` (Linux) vs `umount` (macOS)
- Mount table reading: `/proc/mounts` (Linux) vs `mount` command (macOS)
- Install hints per platform (for `dcx doctor`)

**Subprocess helpers:**
- Command runner (capture output + streaming variants + dry-run)
- Docker availability check (`docker info`)
- Workspace path resolution (canonicalize, default to cwd, validate exists)
- Devcontainer config detection (`.devcontainer/devcontainer.json` or `.devcontainer.json`)

---

## Phase 3: CLI Parsing + Pass-through

See: `architecture.md` § "Subcommand Specifications", "Usage Examples"

- Define clap structs: `up`, `exec`, `down`, `clean`, `status`, `doctor`
- Each subcommand's arguments per spec (e.g. `--workspace-folder`, `--dry-run`, `--yes`, `--all`)
- Pass-through: unknown subcommands forward to `devcontainer`
- Integration tests (Layer 2): `--help`, `--version`, argument parsing, unknown subcommands

---

## Phase 4: `dcx doctor` + `dcx status`

See: `architecture.md` § "dcx doctor" and "dcx status" behavior sections

| Command | Key behaviors |
|---------|-------------|
| `dcx doctor` | Run all prerequisite checks, format results, exit code 0/1 |
| `dcx status` | Scan relay dir → categorize → query containers → format table |

Integration tests: exit codes, output format, empty states ("No active workspaces.", "All checks passed.")

---

## Phase 5: `dcx up`

See: `architecture.md` § "dcx up" behavior section, "Edge Cases", "Permissions"

This is the most complex command. Implement incrementally:
1. Happy path: validate → mount → rewrite path → delegate to `devcontainer up`
2. `--dry-run`: print plan, exit 0, no side effects
3. Idempotent reuse: detect existing healthy mount, verify source matches
4. Stale mount recovery: detect unhealthy mount, unmount, remount fresh
5. Hash collision detection: existing mount with different source → fail with collision error
6. Non-owned directory warning: check ownership, prompt, `--yes` to skip, exit 4 on abort
7. Rollback on failure: if `devcontainer up` fails, unmount + remove dir + print "Mount rolled back."

Integration tests for each: missing workspace (exit 2), missing config (exit 2), recursive mount guard (exit 2), dry-run output.

---

## Phase 6: `dcx exec` + `dcx down`

See: `architecture.md` § "dcx exec" and "dcx down" behavior sections

| Command | Key behaviors |
|---------|-------------|
| `dcx exec` | Verify mount exists + healthy → rewrite path → delegate |
| `dcx down` | `docker stop` container → unmount → remove dir. Idempotent for missing mounts. |

Integration tests: no mount found, workspace doesn't exist, recursive mount guard, "Nothing to do" case.

---

## Phase 7: `dcx clean`

See: `architecture.md` § "dcx clean" behavior section

- Default mode: clean current workspace only (full cleanup: stop + rm + rmi + unmount + rmdir)
- `--all` mode: clean all dcx-managed workspaces (full cleanup for each)
- Both modes prompt if running containers will be stopped, `--yes` to skip
- Summary output format per spec

Integration tests: "Nothing to clean.", confirmation prompt, `--yes` bypass, continue-on-failure behavior.

---

## Phase 8: Signal Handling + Progress Output

See: `architecture.md` § each subcommand's "Signal handling" section, "Progress Output"

**Signal handling:**
- `dcx up`: SIGINT → rollback mount (fully up or fully down)
- `dcx down`: SIGINT during unmount → finish unmount before exit
- `dcx exec`: forward SIGINT to child process
- `dcx clean`: finish current unmount, then exit

**Progress output:**
- All commands print `→ <action>...` to stderr per spec

---

## Phase 9: E2E Tests + Polish ✅

See: `testing.md` § "Layer 3"

**E2E shell tests** (requires Colima + Docker + bindfs):
Full lifecycle tests per command + edge cases.

**Polish:**
- Shell completions via `clap_complete`
- Audit error messages and exit codes against `architecture.md` § "Exit Codes"
- Cross-platform verification (Linux + macOS)

---

## Phase 10: Fix container lifecycle (`dcx down` + `dcx clean`) ✅

See: `architecture.md` § "dcx down", "dcx clean". `failure-recovery.md` § "When to Use dcx clean".

### Task 1: Update specs ✅

### Task 2: Extract Docker helpers into `src/docker.rs` ✅

Moved `query_container()` from `src/status.rs` to `src/docker.rs`.
Updated `src/status.rs` and `src/clean.rs` imports.

Added to `src/docker.rs`:

| Function | Signature | Behavior |
|----------|-----------|----------|
| `query_container` | `(mount_point: &Path) -> Option<String>` | Running containers only (`docker ps`). |
| `query_container_any` | `(mount_point: &Path) -> Option<String>` | Includes stopped (`docker ps -a`). |
| `stop_container` | `(mount_point: &Path) -> Result<(), String>` | `query_container` → `docker stop`. Idempotent. |
| `remove_container` | `(container_id: &str) -> Result<(), String>` | `docker rm`. |
| `remove_container_image` | `(container_id: &str) -> Result<(), String>` | `docker inspect` → `docker rmi`. |

All query functions: take first line only (handle multi-ID output).

### Task 3: Rewrite `src/down.rs` step 7 ✅

Replaced inline 30-line docker ps/stop match block with:
```rust
if let Err(e) = docker::stop_container(&mount_point) {
    eprintln!("{e}");
    return exit_codes::RUNTIME_ERROR;
}
```

Fixed SIGINT comment: `docker stop` uses `run_capture`, signal not forwarded.
Check `interrupted` flag after call returns.

### Task 4: Update `src/cli.rs` + `src/main.rs` ✅

Added `workspace_folder: Option<PathBuf>` to `Clean` struct.
Updated `--all` help text: `"Clean all dcx-managed workspaces (default: current workspace only)"`.
Updated `main.rs` dispatch to pass `workspace_folder`.
New signature: `run_clean(home, workspace_folder, all, yes)`.

### Task 5: Redesign `src/clean.rs` per spec ✅

**Default mode (no `--all`):**
- Resolve workspace path
- Compute mount point
- Find container (running or stopped)
- Prompt if running container found (unless `--yes`)
- Full cleanup: stop → rm container → rmi image → unmount → rmdir
- Print result. "Nothing to clean." if nothing exists.

**`--all` mode:**
- Scan all `dcx-*` entries
- Find running containers for confirmation
- Prompt if running containers found (unless `--yes`)
- Full cleanup for each entry: stop → rm → rmi → unmount → rmdir
- Continue on failure, print summary

Rewrote `clean_one` to always do full cleanup.

### Task 6: Run `make check` + E2E test ✅

- `make check` passes (147 unit + 41 integration tests, clippy, fmt)
- Ready for E2E validation

---

## Phase 11: Fix `dcx clean` image lifecycle + `--include-base-image`

See: `architecture.md` § "dcx clean" — two-image lifecycle, `--include-base-image` behavior, base image detection.

### Task 1: Update specs ✅

Done. See `architecture.md` § "dcx clean".

### Task 2: Fix `remove_image()` in `docker.rs`

Add `--force` to `docker rmi`. Consistent with `clean_orphaned_images()` which already uses `--force`.

### Task 3: Add base image helpers to `docker.rs`

| Function | Signature | Behavior |
|----------|-----------|----------|
| `get_base_image_name` | `(workspace: &Path) -> Option<String>` | Reads `image` field from `.devcontainer/devcontainer.json` or `.devcontainer.json`. Returns `None` if absent or unreadable. |
| `remove_base_image` | `(image_name: &str) -> Result<(), String>` | `docker ps -a --filter ancestor=<image>` check; if clear, `docker rmi <name>` (no `--force`). |

Unit tests: field present, field absent, file missing, whitespace variant (`"image" : "name"`).

### Task 4: Fix default-mode orphan image cleanup in `clean.rs`

Reinstate `clean_orphaned_images()` call at end of default mode (safety net for externally-removed containers). Non-fatal; push errors to `errors` vec.

### Task 5: Add `--include-base-image` to CLI, `clean_one()`, `run_clean()`

- `src/cli.rs`: add `include_base_image: bool` to `Clean` struct.
- `src/main.rs`: pass through to `run_clean()`.
- `src/clean.rs`: `clean_one(mount_point, container_id, include_base_image)` — before unmounting, read mount table to get workspace source, call `get_base_image_name()`, then `remove_base_image()` after runtime image deleted. Non-fatal on failure.
- `--all` mode: collect base image names inside `clean_one()` per-entry; deduplication is natural (second deletion attempt for shared image gets "not found" → ignored).

### Task 6: Update E2E tests (`tests/e2e/test_dcx_clean.sh`)

- After `dcx clean --all --yes`: assert no `vsc-dcx-*` images remain.
- After `dcx clean --all --yes --include-base-image`: assert build image also gone.
- After default `dcx clean` with running container: assert `vsc-dcx-*` image removed.

### Task 7: Run `make check` + E2E validation

---

## Project Status

**Phase 0-10 Complete.**

- ✅ Phase 0-3: Core architecture and CLI foundation
- ✅ Phase 4-6: Primary commands (`dcx doctor`, `dcx status`, `dcx up`, `dcx exec`, `dcx down`)
- ✅ Phase 7-8: Advanced cleanup (`dcx clean`), signal handling, progress output
- ✅ Phase 9: E2E tests, shell completions, error message audit
- ✅ Phase 10: Container lifecycle fixes, orphaned cleanup, spec compliance
- 🔄 Phase 11: Fix `dcx clean` image lifecycle + `--include-base-image` (in progress)

**Quality Metrics (Phase 10):**
- 140 unit tests pass
- 25 integration tests pass
- Cross-platform support (Linux + macOS) confirmed
