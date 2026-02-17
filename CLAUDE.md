# dcx — Dynamic Workspace Mounting Wrapper for Colima Devcontainers

## What this is

A Rust CLI tool (`dcx`) that wraps `devcontainer` to manage bindfs mounts for Colima.
Read `specs/README.md` before making any changes — the specs are the source of truth.

## Conventions

- **TDD:** Write failing tests first, then implement. Red → Green → Refactor.
- **No mocks:** Classicist testing. Unit tests use real inputs. Integration tests run the real binary. E2E tests use real infrastructure.
- **Sans-IO:** Pure logic (hashing, parsing, formatting) lives in pure functions with no subprocess calls or filesystem access. These go in dedicated modules and are unit-tested.
- **Fail fast:** Errors exit immediately with clear messages. No retries (except `dcx clean` which continues on individual failures).
- **Simple over clever:** Direct subprocess calls, no trait abstractions, no unnecessary generics.

## Specification Structure

**Core Files (Never duplicate content across these):**
- `specs/architecture.md` — WHAT: all command behaviors, edge cases, exit codes, platform notes
- `specs/mount-strategy.md` — HOW: bindfs mounting implementation details
- `specs/clean-command.md` — HOW: clean command logic and volume handling
- `specs/docker-helpers.md` — HOW: docker wrapper functions
- `specs/guides/` — User documentation (setup, troubleshooting)
- `AGENTS.md` (root) — Developer reference only: build, test, code patterns (NOT status)
- `IMPLEMENTATION_PLAN.md` — Work tracking (created/updated per phase)

**Pattern:** Reference behavior in `specs/architecture.md` from HOW specs instead of duplicating. Example: "See [architecture.md § dcx clean](specs/architecture.md#cmd-clean) for behavior spec."

**When adding features:**
1. Update `specs/architecture.md` (WHAT: new behavior)
2. Create or update `specs/topic-name.md` (HOW: implementation)
3. Update `AGENTS.md` if patterns/conventions change
4. Update `IMPLEMENTATION_PLAN.md` with progress
5. Keep `specs/guides/` current with user-facing changes

## Commands

- `make check` — run all checks (test + lint + format). Run before considering any phase done.
- `cargo test` — unit + integration tests only
- `cargo build` — compile

## Definition of done (per phase)

A phase is complete when:
1. `make check` passes
2. All new code has tests written first (TDD)
3. Changes are committed

## Current status
After each phase is over, check that the current state of the code exactly follows the spec. Then update this status and commit the code.

Phase: 12 ✅ Complete
Last completed: Phase 12 — Improve dcx clean UX (--purge, --dry-run, volume cleanup)
Improvements:
- Replaced --include-base-image with --purge (cleaner UX)
- Added --dry-run to preview cleanup without executing
- Added volume helper functions to docker.rs
- New CleanPlan struct separates scan from execute phases
- All tests pass (183 unit + 30 integration)
