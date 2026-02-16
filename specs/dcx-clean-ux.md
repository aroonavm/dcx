# Phase 12: Improve `dcx clean` UX — `--purge`, `--dry-run`, volume cleanup

## Context

`dcx clean --include-base-image` is the only way to do a full cleanup but the flag name is clunky and doesn't convey "nuke everything." Docker volumes (shell history, etc.) created by devcontainer are never cleaned by any dcx command. There's also no way to preview what would be cleaned.

**Goal:** Replace `--include-base-image` with `--purge` (which also cleans volumes), add `--dry-run`, and remove backward compatibility for the old flag.

## New CLI

```
dcx clean                        # mount + container + runtime image
dcx clean --purge                # same + build image + docker volumes
dcx clean --all                  # all workspaces at default level
dcx clean --all --purge          # all workspaces, full nuke
dcx clean --dry-run              # preview (combinable with all above)
dcx clean --yes                  # skip confirmation (unchanged)
```

## Files to modify

| File | Change |
|------|--------|
| `specs/architecture.md` | Update dcx clean spec (~lines 205-307) |
| `specs/TASKS.md` | Add Phase 12 |
| `src/cli.rs` | Replace `include_base_image` with `purge`, add `dry_run` |
| `src/main.rs` | Update dispatch (lines 51-63) |
| `src/docker.rs` | Add volume functions: `list_volumes()`, `remove_volume()`, `get_container_volumes()` |
| `src/clean.rs` | Refactor into scan/execute phases, add dry-run + purge logic |
| `src/format.rs` | Add `format_dry_run()` for dry-run output |
| `CLAUDE.md` | Update status to Phase 12 |

## Implementation steps

### Step 1: Update specs

Update `specs/architecture.md` dcx clean section:
- Replace `--include-base-image` with `--purge` in usage and behavior
- `--purge` adds: build image removal + Docker volume removal
- Add `--dry-run` to usage: previews without executing, combinable with all flags
- Add note on volume lifecycle: devcontainer creates `dcx-shellhistory-<devcontainerId>` volumes; `--purge` removes volumes associated with the workspace's container

### Step 2: CLI + dispatch changes

**`src/cli.rs`** — Replace `include_base_image` field:
```rust
/// Leave nothing behind: also remove the build image and Docker volumes.
#[arg(long)]
purge: bool,

/// Show what would be cleaned without doing it.
#[arg(long)]
dry_run: bool,
```

**`src/main.rs`** — Update destructuring and `run_clean()` call to pass `purge` + `dry_run` instead of `include_base_image`.

### Step 3: Volume helpers in `src/docker.rs`

Three new functions:

1. **`list_volumes(name_filter: &str) -> Result<Vec<String>, String>`**
   - `docker volume ls --filter name=<filter> --format {{.Name}}`

2. **`remove_volume(name: &str) -> Result<(), String>`**
   - `docker volume rm <name>`

3. **`get_container_volumes(container_id: &str) -> Result<Vec<String>, String>`**
   - `docker inspect --format '{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}} {{end}}{{end}}' <id>`
   - Filter results to `dcx-*` prefix only
   - Called BEFORE container removal to capture volume names

### Step 4: Refactor `clean.rs` — scan/execute split

Introduce a `CleanPlan` struct to separate observation from action:

```rust
struct CleanPlan {
    mount_point: PathBuf,
    mount_name: String,
    state: String,                    // "running", "orphaned", "stale", "empty dir"
    container_id: Option<String>,
    runtime_image_id: Option<String>,
    build_image_name: Option<String>,  // populated when purge=true
    volumes: Vec<String>,             // populated when purge=true
    is_mounted: bool,
}
```

**`scan_one(mount_point, purge) -> CleanPlan`** — Read-only. Queries mount table, container state, image IDs. When `purge`, also queries build image name and container volumes. Does NOT mutate.

**`execute_one(plan) -> Result<(String, String), String>`** — Executes: stop container, get volumes (from plan), remove container, remove runtime image, remove build image (if purge), remove volumes (if purge), unmount, remove dir.

**`run_clean()` flow:**
1. Scan all relevant mounts → `Vec<CleanPlan>`
2. If `--dry-run`: format and print plans, exit 0
3. If not dry-run: prompt if needed, then execute each plan
4. Orphaned mount/image sweeps (existing logic, unchanged)
5. When `--all --purge`: final sweep of remaining `dcx-*` volumes

### Step 5: Dry-run formatting in `src/format.rs`

Add `format_dry_run(plans: &[CleanPlan]) -> String`:

```
Would clean:
  dcx-myproject-a1b2c3d4  (running)
    - Stop and remove container abc123
    - Remove runtime image sha256:def456
    - Remove build image dcx-dev:latest          [purge]
    - Remove volume dcx-shellhistory-xyz789     [purge]
    - Unmount bindfs
    - Remove mount directory

  dcx-other-e5f6g7h8  (orphaned)
    - Unmount bindfs
    - Remove mount directory
```

Unit-testable: takes `CleanPlan` structs, returns string.

### Step 6: Tests (TDD)

**Unit tests (write first):**
- `format_dry_run` with various plan combinations (running+purge, orphaned, empty, stale)
- `confirm_prompt` still works with new flow

**Integration tests (`tests/cli.rs`):**
- `dcx clean --dry-run` exits 0 with empty relay
- `dcx clean --purge` flag is accepted
- `dcx clean --all --purge --dry-run` parses correctly
- `--include-base-image` is rejected (no backward compat)

### Step 7: `make check` + commit

Run `make check`, update `CLAUDE.md` status to Phase 12, commit.

## Volume discovery: key design decision

When the container still exists (normal case for `dcx clean`), inspect it for volume names BEFORE removing it. This avoids needing to reverse-engineer the devcontainerId.

When the container is already gone (removed externally):
- **Single-workspace mode:** Cannot determine volumes; skip volume cleanup, no warning (best-effort)
- **`--all --purge` mode:** Final sweep removes all remaining `dcx-*` volumes as a catch-all

## Edge cases

- **`--dry-run --yes`**: `--yes` silently ignored (nothing to confirm)
- **Volume in use**: `docker volume rm` fails if container still exists; non-fatal (logged)
- **Base image shared**: Only removed if no containers depend on it (existing logic, unchanged)

## Verification

1. `make check` passes
2. `dcx clean --dry-run` on a running workspace shows the plan without cleaning
3. `dcx clean --purge --yes` removes container, runtime image, build image, and volumes
4. `dcx clean --all --purge --dry-run` shows all workspaces
5. `docker volume ls | grep dcx-` shows no dcx volumes after `--purge`
