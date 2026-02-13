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

Phase: 5
Last completed: Phase 4 — dcx doctor + dcx status
Next: Phase 5 — dcx up (see `specs/TASKS.md`)
