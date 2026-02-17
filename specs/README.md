# Specifications — `dcx` {#readme}

## What is `dcx`?

Wrapper for `devcontainer` that solves: Colima mounts are static (set in `colima.yaml`), but `devcontainer up` needs dynamic paths. Broadly mounting `$HOME` is a security risk (agent in project-A can access project-B). **Solution:** Use `bindfs` to project only the workspace into a pre-mounted relay directory (`~/.colima-mounts`).

## Quick Navigation {#navigation}

| I want to... | Read this |
|--------------|-----------|
| **Understand how dcx works** | [architecture.md](architecture.md) — Problem, solution, all command specs |
| **Implement a feature** | [architecture.md](architecture.md) + relevant HOW spec (mount-strategy, clean-command, docker-helpers) |
| **Install dcx** | [guides/setup.md](guides/setup.md) — Prerequisites, setup steps |
| **Fix a problem** | [guides/failure-recovery.md](guides/failure-recovery.md) — Common errors |
| **Write tests** | [guides/testing.md](guides/testing.md) — Test strategy + pyramid |
| **Start development** | Read [../AGENTS.md](../AGENTS.md) and [../TODO.md](../TODO.md) |

## Key Facts

- **Single binary:** Rust (no shell deps, cross-platform Linux + macOS)
- **7 commands:** up, exec, down, clean, status, doctor + pass-through
- **Multi-workspace:** Each mount isolated (agent A can't access project B)
- **No state files:** Filesystem is source of truth (naming convention)
- **Idempotent:** Safe to call commands multiple times
- **Phase 12 complete:** --purge, --dry-run, volume cleanup ✅

## File Structure

```
.
├── AGENTS.md              ← Developer reference (build, test, conventions)
├── TODO.md ← Work tracking (phases completed, current status)
└── specs/
    ├── README.md          ← You are here
    ├── architecture.md    ← WHAT (behaviors, commands, edge cases)
    ├── mount-strategy.md  ← HOW (bindfs implementation)
    ├── clean-command.md   ← HOW (clean logic, volumes)
    ├── docker-helpers.md  ← HOW (docker wrappers)
    └── guides/            ← User documentation
        ├── setup.md
        ├── failure-recovery.md
        └── testing.md
```

**Pattern:** `architecture.md` describes WHAT. Implementation specs describe HOW (referencing behavior, no duplication). Work tracking is in `TODO.md`, not in specs.
