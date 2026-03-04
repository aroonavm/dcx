# Docker Integration

## Container Operations

### Query Container
```rust
fn query_container(mount_point: &Path) -> Option<String>
fn query_container_any(mount_point: &Path) -> Vec<String>
```
Query containers by `devcontainer.local_folder` label. `query_container` returns the first match; `query_container_any` returns all matching containers. Returns empty if none found (idempotent).

### Stop Container
```rust
fn stop_container(mount_point: &Path) -> Result<(), String>
```
Stop running container by mount point. Queries the container by `devcontainer.local_folder` label, then stops it. Safe to call when no container exists (returns success). Uses `docker stop` with timeout.

### Remove Container
```rust
fn remove_container(container_id: &str) -> Result<(), String>
```
Remove container by ID. Always uses `docker rm --force` to remove even if running. Idempotent — fails silently if container not found.

## Image Operations

### Query Image
```rust
fn get_image_id(container_id: &str) -> Result<String, String>
fn get_runtime_image_ref(container_id: &str) -> Result<String, String>
```
Inspect a container by ID to get its image ID or image reference. Used to find which image a container is running from. Returns error if container not found or image cannot be determined.

### Remove Image
```rust
fn remove_image(image_id: &str) -> Result<(), String>
fn remove_runtime_image(image_id: &str) -> Result<(), String>
```
Remove Docker image by ID or reference. Always uses `--force` flag. `remove_image` removes by ID; `remove_runtime_image` removes by image reference (sha256:... or repo:tag). Idempotent — fails silently if image not found.

### Build Image Name Detection
```rust
fn get_base_image_name(workspace: &Path, config: Option<&Path>) -> Option<String>
```
Read `image` field from devcontainer.json. Searches for the config file in: explicit `config` path (if provided), then `.devcontainer/devcontainer.json` in the workspace, then `.devcontainer.json` at the workspace root. Returns None if not specified in any found config or no config exists.

### Base Image Tagging
During `dcx up`, the base image is tagged as `dcx-base:<mount-name>` so that `dcx clean --purge` can find and remove it by convention — no need to resolve workspace paths.

```rust
fn tag_base_image(base_image: &str, mount_name: &str) -> Result<()>
fn remove_base_image_tag(mount_name: &str) -> Result<()>
fn clean_all_base_image_tags() -> Result<usize>
fn image_exists(image: &str) -> bool
```

- `tag_base_image`: Creates `dcx-base:<mount_name>` alias. Called after successful `devcontainer up`.
- `remove_base_image_tag`: Removes the tag. Only deletes the underlying image if no other tags reference it. Ignores "No such image" errors.
- `clean_all_base_image_tags`: Lists all `dcx-base:*` tags and removes them. Used by `--all --purge` as a final sweep.
- `image_exists`: Checks if a Docker image exists locally via `docker image inspect`.

## Volume Operations

### Get Container Volumes
```rust
fn get_container_volumes(container_id: &str) -> Result<Vec<String>>
```
Query `docker inspect` for `dcx-*` volumes attached to container. Call BEFORE container removal to avoid losing reference.

### Remove Volume
```rust
fn remove_volume(name: &str) -> Result<()>
```
Remove Docker volume. Non-fatal on failure — log but don't fail command. Called after container removal when names still known.

### List Volumes
```rust
fn list_volumes(name_filter: &str) -> Result<Vec<String>>
```
List volumes matching a name prefix filter via `docker volume ls --filter name=<filter>`.

### Clean Orphaned Build Images
```rust
fn clean_orphaned_build_images() -> Result<usize>
```
Remove all `vsc-*` build images (no `-uid` suffix) that have no running containers and whose corresponding runtime image (with `-uid` suffix) no longer has running containers. Used by `--all --purge` as a final sweep to catch build images whose containers were already removed externally. Returns count removed. Non-fatal per image.

### Clean All DCX Volumes
```rust
fn clean_all_dcx_volumes() -> Result<usize>
```
Remove all volumes with `dcx-` prefix. Used by `--all --purge` as a final sweep to catch volumes whose containers were already removed externally. Returns count removed. Non-fatal per volume.

## Error Handling Patterns

**Idempotent operations:**
- Query operations (container, image, volume list) return None/empty if not found
- Remove operations succeed if target already gone
- No retry logic; fail fast with clear messages

**Non-fatal failures:**
- Volume removal non-fatal (may be in use, logged as warning)
- Image removal fails if other containers depend (user must `docker rmi --force`)

**Docker connectivity:**
- All docker calls fail with exit code 1 if Docker not available
- Clear error message: "Docker daemon not available"
