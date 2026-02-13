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
| `dcx down` | Delegate `devcontainer down` → unmount → remove dir. Idempotent for missing mounts. |

Integration tests: no mount found, workspace doesn't exist, recursive mount guard, "Nothing to do" case.

---

## Phase 7: `dcx clean`

See: `architecture.md` § "dcx clean" behavior section

- Safe mode (default): skip active mounts, clean orphaned/stale/empty, continue on failure
- `--all` mode: prompt if active containers found, `--yes` to skip, stop + unmount everything
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

## Phase 9: E2E Tests + Polish

See: `testing.md` § "Layer 3"

**E2E shell tests** (requires Colima + Docker + bindfs):
Full lifecycle tests per command + edge cases.

**Polish:**
- Shell completions via `clap_complete`
- Audit error messages and exit codes against `architecture.md` § "Exit Codes"
- Cross-platform verification (Linux + macOS)
