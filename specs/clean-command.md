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
    state: String,                      // "running", "orphaned", "stale", "empty"
    container_id: Option<String>,
    runtime_image_id: Option<String>,
    build_image_name: Option<String>,   // only when purge=true
    volumes: Vec<String>,               // only when purge=true
    is_mounted: bool,
}
```

**Functions:**
- `scan_one(mount_point, purge) → CleanPlan` — Read-only. Queries mount table, containers, images. When `purge=true`, also queries build image name and container volumes.
- `execute_one(plan) → Result<(state, action)>` — Executes cleanup based on plan.

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
- Add `scan_one()` function
- Add `execute_one()` function
- Update `run_clean()` signature: `(purge: bool, dry_run: bool)` instead of `include_base_image`
- Add dry-run logic: if `--dry-run`, scan → format → print → exit 0

### Dry-run Formatting
**src/format.rs** — Add:
- `format_dry_run(plans: &[CleanPlan]) → String` — Preview showing what would be cleaned

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
