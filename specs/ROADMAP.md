# Development Roadmap {#roadmap}

> **How to use:** This documents each phase of `dcx` development. Each phase links to:
> - **Behavior spec:** architecture.md (what should the system do?)
> - **Implementation plan:** impl/phase-*.md (how do we build it?)
> - **Test strategy:** See testing.md for overall approach

---

## Status Overview {#status}

| Phase | Status | Impl Plan | Last Updated |
|-------|--------|-----------|--------------|
| 0–3 | ✅ Complete | — | Foundation complete |
| 4–6 | ✅ Complete | — | Primary commands (status, up, exec, down) |
| 7–8 | ✅ Complete | — | Advanced cleanup (dcx clean, signals) |
| 9 | ✅ Complete | — | E2E tests, completions |
| 10 | ✅ Complete | — | Container lifecycle fixes |
| 11 | ✅ Complete | — | Image lifecycle, --include-base-image |
| 12 | ✅ Complete | [impl/phase-12-clean-ux.md](impl/phase-12-clean-ux.md) | 2026-02-16 |
| 13+ | 📋 TBD | — | Future phases |

---

## Phase 0: Project Scaffolding ✅

- `cargo init --name dcx`
- Dependencies: `clap`, `clap_complete`, `sha2`
- Dev: `assert_cmd`, `predicates`, `assert_fs`
- Verify: `make check` passes

---

## Phase 1: Pure Logic (Sans-IO) ✅

**Core:** Pure functions with zero external dependencies. Unit tests only.

| What | Tests |
|------|-------|
| Path sanitization | non-alphanumeric → `-`, max 30 chars, empty, unicode |
| Hash computation | SHA256, first 8 hex chars, deterministic |
| Mount name | `dcx-<name>-<hash>` from absolute path |
| Mount table parsing (Linux) | Parse `/proc/mounts` for bindfs entries |
| Mount table parsing (macOS) | Parse `mount` output for bindfs entries |
| Mount source lookup | Entry + mount point → source or None |
| Mount categorization | active / orphaned / stale / empty classification |
| Exit codes | Constants match spec |
| Output formatting | Status table, doctor checks, clean summary |
| Path validation | Detect `dcx-` managed paths (recursive guard) |
| Relay dir resolution | `~/.colima-mounts` expansion |

---

## Phase 2: Platform Abstraction + Subprocess Helpers ✅

**Core:** OS-specific code and subprocess wrappers.

**Platform abstraction:**
- Unmount: `fusermount -u` (Linux) vs `umount` (macOS)
- Mount table: `/proc/mounts` (Linux) vs `mount` (macOS)
- Install hints per platform

**Subprocess helpers:**
- Command runner (capture + streaming)
- Docker availability check (`docker info`)
- Workspace path resolution (canonicalize, default cwd, validate)
- Devcontainer config detection (`.devcontainer/devcontainer.json` or `.devcontainer.json`)

---

## Phase 3: CLI Parsing + Pass-through ✅

**Core:** Clap structs for all managed subcommands.

- Define: `up`, `exec`, `down`, `clean`, `status`, `doctor`
- Each subcommand's arguments (e.g. `--workspace-folder`, `--dry-run`, `--yes`, `--all`)
- Pass-through: unknown subcommands forward to `devcontainer`
- Integration tests: `--help`, `--version`, parsing, pass-through

---

## Phase 4: `dcx doctor` + `dcx status` ✅

**Spec:** [architecture.md § Command: dcx doctor](architecture.md#command-dcx-doctor), [§ Command: dcx status](architecture.md#command-dcx-status)

| Command | Behavior |
|---------|----------|
| `dcx doctor` | Run all prerequisite checks, format results, exit code 0/1 |
| `dcx status` | Scan relay → categorize → query containers → format table |

Integration tests: exit codes, output format, empty states.

---

## Phase 5: `dcx up` ✅

**Spec:** [architecture.md § Command: dcx up](architecture.md#command-dcx-up)

Incrementally:
1. Happy path: validate → mount → rewrite path → delegate
2. `--dry-run`: print plan, exit 0
3. Idempotent reuse: detect healthy mount, verify source matches
4. Stale mount recovery: detect unhealthy, unmount, remount
5. Hash collision detection: existing mount with different source → fail
6. Non-owned directory warning: check ownership, prompt, `--yes` to skip
7. Rollback on failure: if `devcontainer up` fails, unmount + remove + message

Integration tests per step: missing workspace, missing config, recursive guard, dry-run.

---

## Phase 6: `dcx exec` + `dcx down` ✅

**Spec:** [architecture.md § Command: dcx exec](architecture.md#command-dcx-exec), [§ Command: dcx down](architecture.md#command-dcx-down)

| Command | Behavior |
|---------|----------|
| `dcx exec` | Verify mount + healthy → rewrite path → delegate |
| `dcx down` | `docker stop` → unmount → remove dir. Idempotent. |

Integration tests: no mount found, workspace doesn't exist, recursive guard.

---

## Phase 7: `dcx clean` ✅

**Spec:** [architecture.md § Command: dcx clean](architecture.md#command-dcx-clean)

- Default: clean current workspace only (full cleanup)
- `--all`: clean all dcx-managed workspaces
- Both: prompt if running containers (unless `--yes`)
- Summary output per spec

Integration tests: "Nothing to clean", confirmation prompt, `--yes` bypass, continue-on-failure.

---

## Phase 8: Signal Handling + Progress Output ✅

**Spec:** [architecture.md § Standards & Requirements](architecture.md#standards)

**Signal handling:**
- `dcx up`: SIGINT → rollback (fully up or fully down)
- `dcx down`: SIGINT during unmount → finish unmount before exit
- `dcx exec`: forward SIGINT to child
- `dcx clean`: finish current unmount, then exit

**Progress output:** All commands print `→ <action>...` to stderr per spec

---

## Phase 9: E2E Tests + Polish ✅

**Spec:** [guides/testing.md § Layer 3](guides/testing.md)

**E2E shell tests** (requires Colima + Docker + bindfs):
- Full lifecycle per command
- Edge case coverage

**Polish:**
- Shell completions via `clap_complete`
- Audit error messages and exit codes vs. spec
- Cross-platform verification (Linux + macOS)

---

## Phase 10: Fix Container Lifecycle ✅

**Spec:** [architecture.md § Command: dcx down](architecture.md#command-dcx-down), [§ Command: dcx clean](architecture.md#command-dcx-clean)

**Tasks:**
1. Extract Docker helpers → `src/docker.rs`
2. Rewrite `src/down.rs` step 7 (stop container)
3. Update `src/cli.rs` + `src/main.rs` (pass workspace_folder)
4. Redesign `src/clean.rs`: default + `--all` modes
5. Run tests + E2E validation

**Quality:** 140+ unit tests, 25+ integration tests, cross-platform ✅

---

## Phase 11: Fix `dcx clean` Image Lifecycle ✅

**Spec:** [architecture.md § Command: dcx clean § Two-image lifecycle](architecture.md#two-image-lifecycle)

**Tasks:**
1. Add base image helpers to `src/docker.rs` (read image name, remove if safe)
2. Fix `remove_image()` with `--force`
3. Reinstate orphan cleanup as safety net
4. Add `--include-base-image` flag
5. Update E2E tests (verify image removal)

**Quality:** 147 unit tests, 41 integration tests ✅

---

## Phase 12: Improve `dcx clean` UX ✅

**Spec:** [architecture.md § Command: dcx clean](architecture.md#command-dcx-clean)
**Implementation:** [impl/phase-12-clean-ux.md](impl/phase-12-clean-ux.md)

**Changes:**
- Replace `--include-base-image` → `--purge` (clearer intent)
- Add `--dry-run` (preview without executing)
- Add volume cleanup (remove Docker volumes)
- No backward compatibility

**Quality:** 183 unit tests, 30 integration tests, all phases 0–12 complete ✅

---

## Next Phases (Roadmap) {#next}

Possible future improvements:
- **Phase 13:** Remote development (SSH workspaces)
- **Phase 14:** Performance profiling & optimization
- **Phase 15:** Windows support (WSL2)
- **Phase 16:** VS Code integration helpers
- **Phase 17:** Docker Compose support

(Not currently planned; subject to community feedback and use cases.)

---

## Contributing to Roadmap

To propose new phases:
1. Create issue describing use case
2. Propose phase description linking to architecture.md behavior spec
3. Draft impl/phase-N-name.md with high-level plan
4. Discuss scope and priority

---

## Quality Standards {#quality}

**All phases must:**
- ✅ Pass `make check` (tests + clippy + fmt)
- ✅ Follow TDD (tests written first)
- ✅ Include unit + integration tests
- ✅ Have code reviewed before merge
- ✅ Match architecture.md behavior spec exactly
- ✅ Document trade-offs and limitations

**Test pyramid (per testing.md):**
- Layer 1: Unit tests (pure logic, 80% of tests)
- Layer 2: Integration tests (CLI parsing, command behavior)
- Layer 3: E2E tests (full infrastructure, small set)
