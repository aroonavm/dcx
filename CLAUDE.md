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
After each phase is over, update this status and commit the code.

Phase: 3
Last completed: Phase 2 — platform abstraction + subprocess helpers
Next: Phase 3 — CLI parsing + pass-through (see `specs/TASKS.md`)
