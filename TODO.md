# Fix: `dcx up` crashes with "missing image/dockerFile/dockerComposeFile"

## Root Cause
`devcontainer --override-config` **replaces** the entire base config, not merges. The temp file with only `workspaceMount` and `workspaceFolder` lacks required image source fields.

## Implementation
1. [x] `src/docker.rs` — Make `strip_jsonc_comments` public
2. [x] `src/up.rs` — Add `generate_merged_override_config` function
3. [x] `src/up.rs` — Update `run_up` call site to read base config and use merged approach
4. [x] Add unit tests for `generate_merged_override_config` (6 tests)
5. [x] Run `make check` — all 196 unit + 32 integration tests pass

## Changes Summary
- **src/docker.rs** (line 232): Made `strip_jsonc_comments` public for reuse
- **src/up.rs** (lines 67-97): Added `generate_merged_override_config` that:
  - Strips JSONC comments from base config
  - Finds final `}` and injects workspaceMount + workspaceFolder before it
  - Falls back to standalone 2-field form if base cannot be parsed
- **src/up.rs** (lines 475-495): Updated `run_up` to:
  - Read the base devcontainer.json (from --config or found in workspace)
  - Call `generate_merged_override_config` with base content
  - Fall back to standalone form with warning if read fails
- **src/up.rs** (lines 731-846): Added 6 unit tests covering:
  - Preservation of original fields
  - Injection of workspace fields
  - Comma separator insertion
  - JSONC comment stripping
  - Fallback on empty/invalid base
  - JSON escaping of special characters
