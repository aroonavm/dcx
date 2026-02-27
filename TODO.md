# Implementation: Auto-inject Colima host mounts into dcx containers ✓

## Overview
Auto-discover and inject Colima host mounts (excluding ~/.colima-mounts) into containers. Mounts placed at original host paths with env vars for git and claude config.

## Completed Tasks

### Phase 1: Dependencies & New Module ✓
- [x] 1. Updated `Cargo.toml` — added serde + serde_yaml
- [x] 2. Created `src/colima.rs` — pure functions for YAML parsing, mount filtering, tilde expansion
  - [x] Serde types (private): ColimaConfig, ColimaMountRaw
  - [x] Public type: ColimaMount
  - [x] Pure functions: colima_config_path, parse_colima_mounts, filter_relay_mounts, expand_tilde
  - [x] Unit tests (14 tests, all passing)
- [x] 3. Registered `mod colima;` in `src/main.rs`

### Phase 2: Mount Injection Functions ✓
- [x] 4. Added pure functions to `src/up.rs`:
  - [x] build_mount_entry — formats mount strings with source == target
  - [x] build_env_overrides — extracts GIT_CONFIG_GLOBAL and CLAUDE_CONFIG_DIR
  - [x] inject_mounts — injects quoted mount entries into JSON arrays
  - [x] inject_env_vars — injects env vars into containerEnv, skips existing keys
  - [x] Unit tests (11 tests, all passing)
- [x] 5. Modified `generate_merged_override_config` signature to accept extra_mounts and extra_env
- [x] 6. Updated all 6 call sites in src/up.rs with new params (&[], &[])

### Phase 3: Integration ✓
- [x] 7. Wired up in `run_up` — reads colima.yaml, parses, filters, builds entries/env, injects
- [x] 8. Computed colima mounts before override-config generation
- [x] 9. Implemented graceful degradation (missing colima.yaml, missing host paths)

### Phase 4: Project Config ✓
- [x] 10. Updated `.devcontainer/full/devcontainer.json`:
  - [x] Changed ~/.claude mount target from /home/rust/.claude to ${localEnv:HOME}/.claude
  - [x] Updated CLAUDE_CONFIG_DIR from /home/rust/.claude to ${localEnv:HOME}/.claude
  - [x] Added ~/.gitconfig mount at ${localEnv:HOME}/.gitconfig (readonly)
  - [x] Added GIT_CONFIG_GLOBAL to containerEnv
- [x] 11. Updated `.devcontainer/slim/devcontainer.json`:
  - [x] Added mounts array with ~/.claude and ~/.gitconfig
  - [x] Added containerEnv with CLAUDE_CONFIG_DIR and GIT_CONFIG_GLOBAL

### Phase 5: Documentation & Testing ✓
- [x] 12. Updated `specs/architecture.md` step 13 with mount injection details
- [x] 13. Updated `specs/guides/setup.md`:
  - [x] Added ~/.gitconfig to colima.yaml example
  - [x] Added note about auto-injection
- [x] 14. Verified `make check` passes (226 unit + 32 CLI = 258 tests)
- [x] 15. Ready to commit

## Test Results
- 14 colima module tests: ✓ PASS
- 11 mount injection tests: ✓ PASS
- 6 existing merged_override_config tests: ✓ PASS (updated to use new signature)
- 32 CLI integration tests: ✓ PASS
- **Total: 258 tests PASS, 0 FAIL**
- clippy: ✓ PASS
- rustfmt: ✓ PASS

## Key Implementation Details
- Mount entries are properly JSON-quoted when injected into arrays
- Environment variables skip injection if key already exists (respects project config)
- Graceful degradation: missing colima.yaml returns empty (no error)
- Host paths that don't exist are skipped (no error)
- Mounts use source == target (original host path) to avoid remapping container home
