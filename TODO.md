# Fix: `dcx exec` lands in wrong directory without override-config

## Context
After fixing `dcx up` to merge override-config, `dcx exec` still landed in `/workspace`
instead of the original workspace path. Root cause: override-config was only passed to
`dcx up`, not to `dcx exec`.

## Solution
Apply the same override-config approach to `dcx exec`:
- Generate merged override-config (workspace remapping)
- Pass to `devcontainer exec` via `--override-config` flag
- User now lands in correct directory

## Changes Completed
1. [x] src/exec.rs — Add TempFile, json_escape, generate_merged_override_config
2. [x] src/exec.rs — Update build_exec_args to accept override_config_path
3. [x] src/exec.rs — Update run_exec to generate and pass override-config
4. [x] src/exec.rs — Add 2 new unit tests for override-config handling
5. [x] Run make check — All 200 tests pass (198 unit + 32 CLI)

## Testing
✓ 12 exec unit tests (including 2 new)
✓ 198 total unit tests
✓ 32 CLI integration tests
✓ All tests pass
