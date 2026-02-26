# Dynamic Network Control Feature

## Overview
Replace `--open` flag with flexible `--network restricted|minimal|open|host` option for runtime firewall control.
- `minimal` is default (implicit, no flag needed)
- Network mode persisted in mount metadata, displayed in `dcx status` and `dcx exec` header

## Phase 1: Core Implementation

### 1. Add network mode module
- **File**: `src/network_mode.rs` (new)
- **Content**:
  - `enum NetworkMode { Restricted, Minimal, Host, Open }`
  - `impl FromStr for NetworkMode`
  - `impl Display for NetworkMode`
  - Parsing tests (pure functions)

### 2. Update CLI definition
- **File**: `src/cli.rs`
- **Changes**:
  - Replace `open: bool` flag with `#[arg(long, value_enum)] network: Option<NetworkMode>`
  - Default to `Minimal` if not specified
  - Update help text to document all four modes
  - Validation: ensure value is one of the four

### 3. Update main.rs
- **File**: `src/main.rs`
- **Changes**:
  - Pass `network` mode to `up::run_up()` instead of `open` flag
  - Remove unsafe `set_var` for `FIREWALL_OPEN`

### 4. Add metadata storage module
- **File**: `src/metadata.rs` (new)
- **Content**:
  - `fn write_network_mode(mount_dir: &Path, mode: NetworkMode) -> io::Result<()>`
  - `fn read_network_mode(mount_dir: &Path) -> io::Result<Option<NetworkMode>>`
  - Metadata stored in `.dcx-network-mode` file (plain text, single line)
  - Tests for read/write/missing file scenarios

### 5. Update `dcx up` implementation
- **File**: `src/up.rs`
- **Changes**:
  - Accept `network: NetworkMode` parameter
  - Write network mode to metadata file when creating mount
  - Set env vars based on mode:
    - `restricted` → `FIREWALL_RESTRICTED=true`
    - `minimal` → no env var (default behavior in script)
    - `host` → `FIREWALL_HOST=true`
    - `open` → `FIREWALL_OPEN=true`
  - Pass env var to devcontainer via `env::set_var()` before spawning

### 6. Update firewall script
- **File**: `.devcontainer/full/init-firewall.sh`
- **Changes**:
  - Handle `FIREWALL_RESTRICTED=true` → completely block all traffic (skip all rule setup)
  - Handle `FIREWALL_HOST=true` → allow all traffic to host network only
  - Keep current logic as default (`FIREWALL_MINIMAL` or absent)
  - Keep `FIREWALL_OPEN=true` as-is
  - Update `--open` flag handling to map to `FIREWALL_OPEN=true` for backwards compat

### 7. Update `dcx status`
- **File**: `src/status.rs` and `src/format.rs`
- **Changes**:
  - Query network mode from metadata for each mount
  - Add "network" column to status table
  - Display as: `minimal`, `restricted`, `host`, `open`, or `unknown` if missing

### 8. Update `dcx exec`
- **File**: `src/exec.rs`
- **Changes**:
  - Query network mode from metadata before executing
  - Print header: `→ Network mode: {mode}` to stderr before running command
  - Or: include in progress message

## Phase 2: Documentation

### 9. Update specs
- **File**: `specs/architecture.md`
- **Changes**:
  - Update `dcx up` command definition: replace `--open` with `--network` flag spec
  - Document all four modes with examples
  - Document metadata persistence approach
  - Update exit code section if needed
  - Update progress output section if adding new messages

## Phase 3: Testing

### 10. Unit tests
- **File**: `src/network_mode.rs` tests
  - Parse all four modes correctly
  - FromStr handles invalid input gracefully

- **File**: `src/metadata.rs` tests
  - Write and read network mode correctly
  - Handle missing file
  - Handle corrupted file gracefully (returns error or default)

### 11. Integration tests
- **Directory**: `tests/`
- **Tests**:
  - `dcx up --network minimal` creates correct metadata
  - `dcx up --network restricted` creates correct metadata
  - `dcx up --network host` creates correct metadata
  - `dcx up --network open` creates correct metadata
  - `dcx status` displays correct network mode
  - `dcx exec` prints network mode header
  - Metadata survives container restart


## Key Decisions

1. **Metadata storage**: Plain text file `.dcx-network-mode` in mount directory
   - Rationale: Simple, human-readable, survives container restarts, no Docker API needed

2. **Environment variables**: Four env vars, one per mode
   - `FIREWALL_RESTRICTED` (block all)
   - `FIREWALL_MINIMAL` (default dev tools, GitHub, npm, etc.)
   - `FIREWALL_HOST` (host network only)
   - `FIREWALL_OPEN` (unrestricted)

3. **Default**: `minimal` (implicit in code, no `--network` flag needed)
   - Rationale: Secure by default, matches current behavior

4. **Removed**: `--open` flag completely (no backwards compatibility)
   - Users migrate to `--network open` instead

5. **Output**: Show network mode in status table and exec header
   - Allows users to verify they're in the right mode

## Testing Criteria

All phases complete when:
1. `make check` passes
2. All new code has tests (TDD)
3. Specs updated and in sync with code
4. Manual test: `dcx up --network open`, `dcx up --network restricted`, `dcx status` shows modes correctly
5. Changes committed
