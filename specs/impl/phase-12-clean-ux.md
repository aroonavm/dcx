# Phase 12: Improve `dcx clean` UX — `--purge`, `--dry-run`, Volume Cleanup {#phase-12}

## Behavior Specification

See [architecture.md § Command: dcx clean](../architecture.md#command-dcx-clean) for complete behavior spec.

**Changes:**
- Replace `--include-base-image` → `--purge` (clearer intent: "nuke everything")
- Add `--dry-run` flag: preview cleanup without executing
- Add volume cleanup: remove Docker volumes associated with container (`dcx-shellhistory-*` etc.)
- No backward compatibility for old `--include-base-image` flag

---

## Design Rationale {#design}

### Scan/Execute Split {#scan-execute}

**Why:** Enable `--dry-run` preview without duplicating cleanup logic in two code paths.

**How:** Introduce `CleanPlan` struct that separates read-only scanning from mutation:

```rust
struct CleanPlan {
    mount_point: PathBuf,
    mount_name: String,
    state: String,                      // "running", "orphaned", "stale", "empty dir"
    container_id: Option<String>,
    runtime_image_id: Option<String>,
    build_image_name: Option<String>,   // only when purge=true
    volumes: Vec<String>,               // only when purge=true
    is_mounted: bool,
}
```

**Functions:**
- `scan_one(mount_point, purge) -> CleanPlan` — Read-only. Queries mount table, containers, images. When `purge=true`, also queries build image name and container volumes.
- `execute_one(plan) -> Result<(state, action)>` — Executes: stop container, remove container, remove runtime image, remove build image (if purge), remove volumes (if purge), unmount, remove directory.

### Volume Discovery {#volume-discovery}

**Why:** Must capture volume names BEFORE container removal, or lose reference to which volumes belong to this workspace.

**When container exists (normal case):**
- Call `get_container_volumes(container_id)` BEFORE `docker rm`
- Stores volume names in CleanPlan
- After removal, delete volumes

**When container already gone (external removal):**
- Single-workspace mode: skip volume cleanup silently (best-effort)
- `--all --purge` mode: final sweep removes remaining `dcx-*` volumes as catch-all

This avoids needing to reverse-engineer `devcontainerId` from mount sources.

---

## Implementation Plan {#impl-plan}

### Step 1: CLI Changes

**src/cli.rs** — Replace field in `Clean` struct:
```rust
// Remove:
include_base_image: bool,

// Add:
#[arg(long)]
/// Leave nothing behind: also remove build image and Docker volumes.
purge: bool,

#[arg(long)]
/// Show what would be cleaned without doing it.
dry_run: bool,
```

**src/main.rs** — Update dispatch to pass `purge, dry_run` to `run_clean()` instead of `include_base_image`.

### Step 2: Volume Helpers in src/docker.rs

Three new functions (before tests section):

1. **`list_volumes(name_filter: &str) -> Result<Vec<String>, String>`**
   - `docker volume ls --filter name=<filter> --format {{.Name}}`

2. **`remove_volume(name: &str) -> Result<(), String>`**
   - `docker volume rm <name>`

3. **`get_container_volumes(container_id: &str) -> Result<Vec<String>, String>`**
   - `docker inspect --format '{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}} {{end}}{{end}}' <id>`
   - Filter to `dcx-*` prefix only
   - Call BEFORE container removal to capture names

### Step 3: Refactor src/clean.rs

**Add CleanPlan struct** (after imports, before functions):
```rust
struct CleanPlan {
    mount_point: PathBuf,
    mount_name: String,
    state: String,
    container_id: Option<String>,
    runtime_image_id: Option<String>,
    build_image_name: Option<String>,
    volumes: Vec<String>,
    is_mounted: bool,
}
```

**Add scan_one() function:**
- Input: mount_point, purge flag
- Query mount table, detect state, find container, get image IDs
- When purge: also query build image name and volumes from container
- Return: populated CleanPlan

**Add execute_one() function:**
- Input: CleanPlan
- Execute in order: stop container → rm container → rm runtime image → rm build image (if purge) → rm volumes (if purge) → unmount → rmdir
- Non-fatal errors for volume/build image removal (logged to stderr)
- Return: (state_before, action_taken)

**Update run_clean() signature:**
```rust
pub fn run_clean(
    home: &Path,
    workspace_folder: Option<PathBuf>,
    all: bool,
    yes: bool,
    purge: bool,      // was: include_base_image
    dry_run: bool,    // new
) -> i32
```

**Dry-run logic in run_clean():**

For default mode (no `--all`):
- If dry_run: resolve workspace → compute mount → scan_one() → format_dry_run() → print → exit 0

For --all mode:
- If dry_run: scan all dcx-* mounts → collect CleanPlans → format_dry_run() → print → exit 0

**Replace old clean_one() calls** with execute_one(scan_one(...)) pattern where appropriate.

### Step 4: Dry-run Formatting in src/format.rs

**Add DryRunPlan struct:**
```rust
pub struct DryRunPlan {
    pub mount_name: String,
    pub state: String,
    pub container_id: Option<String>,
    pub runtime_image_id: Option<String>,
    pub build_image_name: Option<String>,
    pub volumes: Vec<String>,
    pub is_mounted: bool,
}
```

**Add format_dry_run() function:**
```rust
pub fn format_dry_run(plans: &[DryRunPlan]) -> String {
    // Returns formatted preview showing what would be cleaned
    // Example output:
    // Would clean:
    //   dcx-myproject-a1b2c3d4  (running)
    //     - Stop and remove container abc123
    //     - Remove runtime image sha256:xyz
    //     - Remove build image dcx-dev:latest  [purge]
    //     - Remove volume dcx-shellhistory-abc  [purge]
    //     - Unmount bindfs
    //     - Remove mount directory
}
```

### Step 5: Tests (TDD)

**Unit tests** (write before implementation):
- `format_dry_run()` with various plan combinations
- `scan_one()` with running container + purge
- `scan_one()` with orphaned mount
- `get_container_volumes()` returns dcx-prefixed volumes only

**Integration tests** (tests/cli.rs):
- `dcx clean --dry-run` exits 0 on empty relay
- `dcx clean --purge` flag is accepted
- `dcx clean --all --purge --dry-run` flags combine correctly
- `--include-base-image` flag is rejected with error

---

## Verification Checklist {#verify}

- [ ] `make check` passes (all tests, clippy, fmt)
- [ ] `dcx clean --dry-run` on empty relay: "Nothing to clean."
- [ ] `dcx clean --dry-run` on mounted workspace: shows plan, exits 0, no changes
- [ ] `dcx clean --purge --yes` removes: container, runtime image, build image, volumes, mount, directory
- [ ] `dcx clean --all --purge --dry-run` shows all workspaces
- [ ] `docker volume ls | grep dcx-` shows no dcx volumes after `--purge`
- [ ] `--include-base-image` is rejected with clap error
- [ ] All new code covered by tests (TDD approach)
- [ ] No clippy warnings
- [ ] Code is formatted correctly
