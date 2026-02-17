# Specifications — `dcx` {#readme}

## What is `dcx`?

Wrapper for `devcontainer` that solves: Colima mounts are static (set in `colima.yaml`), but `devcontainer up` needs dynamic paths. Broadly mounting `$HOME` is a security risk (agent in project-A can access project-B). **Solution:** Use `bindfs` to project only the workspace into a pre-mounted relay directory (`~/.colima-mounts`).

## Quick Navigation {#navigation}

| I want to... | Read this |
|--------------|-----------|
| **Understand how dcx works** | [architecture.md](architecture.md) — Problem, solution, all command specs |
| **See development roadmap** | [ROADMAP.md](ROADMAP.md) — Phases 0–12 status + future plans |
| **Implement a feature** | [ROADMAP.md](ROADMAP.md) + [impl/phase-N.md](impl/) — What + how |
| **Install dcx** | [guides/setup.md](guides/setup.md) — Prerequisites, setup steps |
| **Fix a problem** | [guides/failure-recovery.md](guides/failure-recovery.md) — Common errors |
| **Write tests** | [guides/testing.md](guides/testing.md) — Test strategy + pyramid |

## Key Facts

- **Single binary:** Rust (no shell deps, cross-platform Linux + macOS)
- **7 commands:** up, exec, down, clean, status, doctor + pass-through
- **Multi-workspace:** Each mount isolated (agent A can't access project B)
- **No state files:** Filesystem is source of truth (naming convention)
- **Idempotent:** Safe to call commands multiple times
- **Phase 12 complete:** --purge, --dry-run, volume cleanup ✅

## File Structure

```
specs/
├── README.md              ← you are here
├── architecture.md        ← behavior spec (AUTHORITATIVE)
├── ROADMAP.md             ← phases, status, standards
├── guides/                ← user documentation
│   ├── setup.md
│   ├── failure-recovery.md
│   └── testing.md
└── impl/                  ← implementation plans
    └── phase-12-clean-ux.md
```

**Principle:** `architecture.md` describes WHAT should happen. `impl/` describes HOW to build it. No duplication.
