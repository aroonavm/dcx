# Clean Command Implementation

See [architecture.md § dcx clean](architecture.md#cmd-clean) for complete behavior spec.

## Design Rationale

### Scan/Execute Split
**Why:** Enable `--dry-run` preview without duplicating cleanup logic.
**How:** Introduce `CleanPlan` struct separating read-only scanning from mutation.

```rust
struct CleanPlan {
    mount_point: PathBuf,
    mount_name: String,
    state: String,                      // "running", "orphaned", "stale", "empty dir"
    container_ids: Vec<String>,         // May contain 0 or more containers
    runtime_image_id: Option<String>,
    has_base_image_tag: bool,           // dcx-base:<mount_name> exists (purge only)
    volumes: Vec<String>,               // only when purge=true
    is_mounted: bool,
}
```

**Functions:**
- `scan_one(mount_point, purge) → CleanPlan` — Read-only. Queries mount table, containers, images. When `purge=true`, also checks for `dcx-base:<mount_name>` tag and container volumes.
- `execute_one(plan) → Result<(state, action)>` — Executes cleanup based on plan.

### Base Image Discovery via Docker Tags
**Problem:** After `dcx clean` removes the mount directory, a subsequent `dcx clean --purge` can't find the base image because the workspace path (needed to read devcontainer.json) is no longer reachable.
**Solution:** During `dcx up`, the base image is tagged as `dcx-base:<mount-name>`. Cleanup uses `docker rmi dcx-base:<mount-name>` which only deletes the underlying image if no other tags reference it.
**Fallback:** `--all --purge` does a final sweep of all `dcx-base:*` tags.

### Volume Discovery Strategy
**Problem:** Must capture volume names BEFORE removing container, else lose reference.
**Solution:** Call `docker inspect` to capture `dcx-*` volumes BEFORE `docker rm`.
**Fallback:** When container already gone (external removal), single-workspace mode skips volume cleanup (best-effort). With `--all --purge`, final sweep removes remaining `dcx-*` volumes.

## Implementation

### CLI Changes
**src/cli.rs** — Replace field:
```rust
#[arg(long)]
purge: bool,      // was: include_base_image

#[arg(long)]
dry_run: bool,    // new
```

**src/main.rs** — Update dispatch to pass `purge, dry_run` to `run_clean()`.

### Volume Helpers
**src/docker.rs** — Add functions (see [docker-helpers.md](docker-helpers.md))

### Refactor src/clean.rs
- Add `CleanPlan` struct
- Add `scan_one()` function: `(mount_point: &Path, purge: bool) → CleanPlan`
- Add `execute_one()` function: `(plan: &CleanPlan) → Result<(), String>`
- Update `run_clean()` signature: `(home: &Path, workspace_folder: Option<PathBuf>, all: bool, yes: bool, purge: bool, dry_run: bool) → i32`
- Add dry-run logic: if `--dry-run`, scan → format → print → exit 0

### Dry-run Formatting
**src/format.rs** — `format_dry_run` function (implemented):
- `format_dry_run(plans: &[DryRunPlan]) → String` — Preview showing what would be cleaned

Example output:
```
Would clean:
  dcx-myproject-a1b2c3d4  (running)
    - Stop and remove container abc123
    - Remove runtime image sha256:xyz
    - Remove build image dcx-dev:latest  [purge]
    - Remove volume dcx-shellhistory-abc  [purge]
    - Unmount bindfs
    - Remove mount directory
```

### Tests (TDD)
**Unit tests:**
- `format_dry_run()` with various plan combinations

**Integration tests:**
- `dcx clean --dry-run` exits 0, shows plan, makes no changes
- `dcx clean --purge` flag is accepted and works
- `dcx clean --all --purge --dry-run` parses and executes correctly
- `--include-base-image` is rejected (no backward compat)
