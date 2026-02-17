# Development Roadmap {#roadmap}

## Status {#status}

| Phase | Status | Key Deliverables |
|-------|--------|------------------|
| 0–3 | ✅ | Foundation: scaffolding, pure logic, CLI parsing |
| 4–6 | ✅ | Commands: doctor, status, up, exec, down |
| 7–8 | ✅ | Advanced: clean, signal handling, progress output |
| 9 | ✅ | E2E tests, shell completions, error audit |
| 10 | ✅ | Container lifecycle: docker helpers, clean redesign |
| 11 | ✅ | Image lifecycle: base image detection, --include-base-image |
| 12 | ✅ | UX: --purge (replace --include-base-image), --dry-run, volumes |

## Implementation Pattern {#pattern}

Each phase has:
1. **Behavior spec** → `architecture.md` (what should happen)
2. **Implementation plan** → `impl/phase-N.md` (how to build)
3. **Tests** → TDD (write tests first)
4. **Quality gate** → `make check` passes

## Phase Descriptions {#phases}

**Phase 0–3:** Core infrastructure
- Rust scaffolding, pure logic (hashing, parsing), CLI parsing

**Phase 4–6:** Primary commands
- `dcx doctor` (prerequisite checks)
- `dcx status` (show mounted workspaces)
- `dcx up` (create mount, start container)
- `dcx exec` (run commands in container)
- `dcx down` (stop container, unmount)

**Phase 7–8:** Advanced features
- `dcx clean` (full cleanup: stop, rm container, rmi image, unmount)
- Signal handling (SIGINT rollback for up, finish unmount for down)
- Progress output (→ steps to stderr)

**Phase 9:** Polish
- E2E tests (full infrastructure)
- Shell completions (bash, zsh, fish)
- Error message audit

**Phase 10:** Container lifecycle
- Docker helpers (stop, remove, query containers)
- Clean redesign (default + --all modes)
- Orphaned cleanup (mounts, containers, images)

**Phase 11:** Image lifecycle
- Build image detection (from devcontainer.json)
- Safe removal (fails if other containers depend)
- `--include-base-image` flag for full cleanup

**Phase 12:** UX improvements
- Replace `--include-base-image` → `--purge` (clearer intent)
- Add `--dry-run` (preview cleanup)
- Add volume cleanup (remove Docker volumes)
- Scan/execute split (separate observation from action)

## Quality Standards {#quality}

All phases must:
- ✅ Pass `make check` (unit + integration tests, clippy, fmt)
- ✅ Follow TDD (tests written first)
- ✅ Match `architecture.md` behavior spec exactly
- ✅ Have code reviewed

Test pyramid:
- **Layer 1:** Unit tests (pure logic) — 80%
- **Layer 2:** Integration tests (CLI, command behavior) — 15%
- **Layer 3:** E2E tests (full infrastructure) — 5%

## Future Phases {#future}

Possible next areas (not planned):
- Remote development (SSH workspaces)
- Performance optimization
- Windows support (WSL2)
- VS Code integration helpers
- Docker Compose support
