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

**Crates:**
- `assert_cmd` — run binary, assert on stdout/stderr/exit code
- `predicates` — expressive assertions (contains, matches, etc.)
- `assert_fs` — temp directories and file fixtures

## Layer 3: Shell E2E Tests (requires Colima + Docker + bindfs)

Shell scripts that test the full mount → container → cleanup lifecycle. These are slow and require the full environment. Covers all behaviors that involve real bindfs mounts, Docker containers, and Colima interaction.

**Guard:** `require_e2e_deps` — skips if Colima, Docker, bindfs, or devcontainer CLI is missing.

**Structure:**

```
tests/
  ├── e2e/
  │   ├── setup.sh              # Common setup/teardown helpers
  │   ├── test_dcx_up.sh
  │   ├── test_dcx_exec.sh
  │   ├── test_dcx_down.sh
  │   ├── test_dcx_clean.sh
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
