# Specifications — `dcx` Workspace Mounting for Colima

## Quick Links

| Document | Purpose |
|----------|---------|
| **[Architecture](architecture.md)** | Problem, solution, design, subcommands, usage examples |
| **[Setup](setup.md)** | Installation and one-time configuration |
| **[Troubleshooting](failure-recovery.md)** | Common errors and recovery steps |
| **[Testing](testing.md)** | Testing strategy and approach |
| **[Phase 12: dcx clean UX](dcx-clean-ux.md)** | Improve `dcx clean` with `--purge`, `--dry-run`, and volume cleanup |

---

## What is `dcx`?

`dcx` is a wrapper for `devcontainer` that solves this problem:

**Problem:** Colima mounts are static (configured in `colima.yaml` at startup). But `devcontainer up` needs to mount dynamic workspace paths that don't exist in the VM yet. Broadly mounting `$HOME` exposes all projects to every container — a security risk, especially when running AI agents autonomously.

**Solution:** Use `bindfs` to project only the workspace directory into a pre-mounted relay directory (`~/.colima-mounts`). This exposes minimal surface area while keeping the mount dynamic. Each workspace is isolated — an agent in project-A cannot access project-B.

## Key Points

- **Single responsibility:** Wrap `devcontainer up/exec/down` to manage workspace mounting
- **Simple design:** Rust binary, direct subprocess calls, fail-fast error handling
- **Multi-workspace:** Multiple workspaces can be mounted simultaneously; each gets its own `dcx-` prefixed mount
- **7 commands:** 6 core subcommands (`dcx up`, `dcx exec`, `dcx down`, `dcx clean`, `dcx status`, `dcx doctor`) plus `dcx completions` for shell completion — everything else passes through to `devcontainer`
- **CLI-first:** Wraps the `devcontainer` CLI; VS Code "Reopen in Container" is not supported (use `dcx up` + "Attach to Running Container" instead)
- **Idempotent:** Safe to call `dcx up` multiple times; verifies mount health and reuses if valid
- **Self-managing:** `dcx` auto-creates the relay directory and tracks mounts via naming convention — no state files
- **No locking:** Avoid concurrent `dcx up` + `dcx down` for same workspace (limitation for simplicity)

## v1.0 Scope

**In:** Linux and macOS, multiple simultaneous workspaces, warning before mounting non-owned directories, recovery from stale mounts, auto-creation of relay directory, `--dry-run` for `dcx up` and `dcx clean`, shell completions via `clap`
**Out:** Windows, read-only mounts, concurrent operations on same workspace, automatic Colima setup, VS Code "Reopen in Container" integration

See [Architecture](architecture.md) for design details and [Known Limitations](architecture.md#known-limitations) for scope constraints.
