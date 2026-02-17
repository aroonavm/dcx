# Developer Guide — dcx

## Before Making Changes

Read `specs/README.md` — the specs are the source of truth for behavior and design.

## Specifications

- **Behavior:** [specs/architecture.md](specs/architecture.md) — all commands, behaviors, edge cases, exit codes
- **Implementation:** [specs/mount-strategy.md](specs/mount-strategy.md), [specs/clean-command.md](specs/clean-command.md), [specs/docker-helpers.md](specs/docker-helpers.md)
- **User Guides:** [specs/guides/](specs/guides/)

**Principle:** Specs may describe planned features that don't yet exist in code. Assume NOT implemented.

## Build & Test

- `make check` — run all checks (test + lint + format). Run before submitting work.
- `cargo test` — unit + integration tests only
- `cargo build` — compile release binary

## Code Conventions

### Testing
- **TDD:** Write failing tests first, then implement. Red → Green → Refactor.
- **No mocks:** Classicist testing. Unit tests use real inputs. Integration tests run the real binary.
- **Test pyramid:** 80% unit, 15% integration, 5% E2E

### Code Organization
- **Sans-IO:** Pure logic (hashing, parsing, formatting) lives in pure functions with no subprocess calls or filesystem access. Unit-tested separately.
- **Fail fast:** Errors exit immediately with clear messages. No retries (except `dcx clean` which continues on individual failures).
- **Simple over clever:** Direct subprocess calls, no trait abstractions, no unnecessary generics.

### Error Handling
- Exit codes: 0 (success), 1 (runtime error), 2 (usage error), 4 (user abort), 127 (prerequisite missing)
- Clear error messages sent to stderr
- No silent failures — all errors logged

### Subprocess Calls
- Explicit, direct calls via `std::process::Command`
- Fail-fast: exit with error code if subprocess fails
- No retries or recovery logic

## Workspace & Commands

### File Structure
```
dcx/
├── specs/
│   ├── architecture.md         ← WHAT (all behaviors)
│   ├── mount-strategy.md       ← HOW (implementation)
│   ├── clean-command.md        ← HOW (implementation)
│   ├── docker-helpers.md       ← HOW (implementation)
│   ├── guides/                 ← User documentation
│   └── README.md               ← Navigation guide
├── src/
│   ├── main.rs                 ← CLI entry point
│   ├── cli.rs                  ← Argument parsing
│   ├── mount.rs                ← bindfs mounting
│   ├── clean.rs                ← clean command logic
│   ├── docker.rs               ← Docker subprocess calls
│   ├── format.rs               ← Output formatting
│   ├── hash.rs                 ← Pure hash computation
│   ├── parse.rs                ← Pure parsing logic
│   └── *.rs                    ← Other modules
├── Makefile                    ← Build automation
├── Cargo.toml                  ← Dependencies
├── AGENTS.md                   ← This file
└── CLAUDE.md                   ← Project conventions
```

### Primary Commands
- `dcx up` — Create mount, start container
- `dcx exec` — Run commands in container
- `dcx down` — Stop container, unmount, remove directory
- `dcx clean` — Full cleanup (container, images, volumes, mount)
- `dcx status` — Show mounted workspaces
- `dcx doctor` — Check prerequisites

## Work in Progress

Current focus: See `IMPLEMENTATION_PLAN.md`

## When Adding Features

1. Update [specs/architecture.md](specs/architecture.md) (WHAT)
2. Create or update implementation spec (HOW)
3. Update user guides if needed
4. Write tests first (TDD)
5. Implement following spec
6. Update `IMPLEMENTATION_PLAN.md`
7. Run `make check`
8. Commit all spec + code changes together
