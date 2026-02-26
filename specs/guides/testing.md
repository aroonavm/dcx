# Testing Strategy

## Overview

`dcx` uses a **Test Pyramid** approach with **Sans-IO / Classicist** principles:

- **Sans-IO:** Pure logic is separated from subprocess calls, making it directly testable without infrastructure.
- **Test Pyramid:** Many fast unit tests for pure logic, some integration tests against the binary, few E2E shell tests for the full infrastructure.
- **Classicist:** No mocks anywhere. Tests use real inputs, real binaries, and real infrastructure. The specs (`architecture.md`, `failure-recovery.md`) are the test specification — no separate test spec files.

## Layer 1: Rust Unit Tests (`cargo test`, no infrastructure)

Fast `#[test]` functions for pure logic. These run in milliseconds with zero external dependencies.

**Sans-IO boundary — keep these as pure functions (no subprocess calls, no filesystem access):**

- Mount name computation: absolute path → `dcx-<name>-<hash>`
- Path sanitization: non-alphanumeric → `-`, max 30 characters
- Hash computation: SHA256 of absolute path, first 8 hex chars
- Mount table parsing: `/proc/mounts` text (Linux) and `mount` output (macOS) → structured data
- Mount categorization: entry → active / orphaned / stale / empty
- Exit code mapping
- Output formatting: status table, doctor checks, clean summary

**Location:** Inline `#[cfg(test)]` modules next to the code they test.

## Layer 2: Rust Integration Tests (`cargo test`, no infrastructure)

Tests that build the `dcx` binary and run it as a subprocess against controlled temp directories. Covers argument parsing, validation, error messages, exit codes, and any behavior that happens before subprocess calls to external tools.

**Location:** `tests/` directory (Rust integration test convention).

**Coverage:** Includes testing of `dcx clean --all` behavior in isolated temporary home directories, ensuring `--all` semantics are verified without affecting real system state.

**Crates:**
- `assert_cmd` — run binary, assert on stdout/stderr/exit code
- `predicates` — expressive assertions (contains, matches, etc.)
- `assert_fs` — temp directories and file fixtures

## Layer 3: Shell E2E Tests (requires Colima + Docker + bindfs)

Shell scripts that test the full mount → container → cleanup lifecycle. These are slow and require the full environment. Covers all behaviors that involve real bindfs mounts, Docker containers, and Colima interaction.

**Isolation Guarantee:** E2E tests NEVER call `dcx clean`. Cleanup uses `dcx down --workspace-folder` per tracked workspace only.

**Why not `dcx clean`:** `dcx clean --workspace-folder` has global side effects beyond the target workspace — it scans ALL relay mounts for orphans and runs global Docker image cleanup. These can delete containers and mounts belonging to real workspaces running on the same system. `dcx down` is safe: it only stops the container and unmounts the relay for the specific workspace, nothing else.

**`dcx clean` is tested exclusively in Layer 2** (Rust integration tests) with isolated temporary HOME directories.

**Environment isolation rules:**
- `setup.sh` unconditionally unsets `DCX_DEVCONTAINER_CONFIG_PATH` so the host shell's config path cannot leak into tests and cause negative tests (tests that expect `dcx up` to fail) to unexpectedly succeed. Tests that specifically test env-var behaviour must set it explicitly.
- Tests never use `ls -d "${RELAY}"/dcx-* | head/tail/wc` to reason about relay state, because pre-existing user workspaces pollute the count. Instead, use `relay_dir_for "$WS"` (defined in `setup.sh`) to compute the exact relay directory for a specific workspace. This helper mirrors `naming.rs` exactly (sanitize + SHA256 hash).
- Assertions about "No active workspaces" are replaced with workspace-specific checks (e.g., assert the relay dir for `$WS` is absent).

**Guard:** `require_e2e_deps` — skips if Colima, Docker, bindfs, or devcontainer CLI is missing.

**Structure:**

```
tests/
  ├── e2e/
  │   ├── setup.sh              # Common setup/teardown helpers (uses dcx down for cleanup)
  │   ├── test_dcx_up.sh
  │   ├── test_dcx_exec.sh
  │   ├── test_dcx_down.sh
  │   ├── test_dcx_status.sh
  │   ├── test_dcx_doctor.sh
  │   ├── test_edge_cases.sh
  │   └── test_stale_mounts.sh
  └── ... (Rust integration tests at tests/*.rs)
```

## Layer 3b: Docker-only E2E Tests (no Colima/bindfs)

Shell scripts that test dcx's argument forwarding to `devcontainer` using Docker alone. These run on any machine with Docker and the devcontainer CLI — no Colima or bindfs required.

**Guard:** `require_docker_deps` — skips if Docker or devcontainer CLI is missing.

**What they cover:**
- Unknown subcommands forwarded to devcontainer with correct args
- `dcx up --dry-run` works without bindfs
- Exit code propagation from devcontainer
- `dcx doctor` reports missing dependencies without crashing

**Structure:**

```
tests/
  └── e2e/
      ├── Dockerfile.test          # Minimal test image (rust + git + node + devcontainer CLI, no bindfs)
      └── test_passthrough.sh      # Docker-only pass-through tests
```
