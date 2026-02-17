# Docker Integration

## Container Operations

### Query Container
```rust
fn find_container_by_label(label: &str, value: &str) -> Result<Option<ContainerId>>
```
Find container by `devcontainer.local_folder` label. Returns None if not found (idempotent).

### Stop Container
```rust
fn stop_container(container_id: &str) -> Result<()>
```
Stop running container. Safe to call on stopped container (returns success). Uses `docker stop` with timeout.

### Remove Container
```rust
fn remove_container(container_id: &str, force: bool) -> Result<()>
```
Remove container. With `force=true`, kills running container. With `force=false`, fails if running.

## Image Operations

### Query Image
```rust
fn get_image_id(image_name: &str) -> Result<Option<ImageId>>
```
Get image ID from name. Returns None if not found (idempotent).

### Remove Image
```rust
fn remove_image(image_id: &str, force: bool) -> Result<()>
```
Remove image. With `force=false`, fails if other images depend on it. With `force=true`, removes regardless.

### Build Image Name Detection
```rust
fn get_build_image_name(devcontainer_path: &str) -> Result<Option<String>>
```
Read `image` field from devcontainer.json. Returns None if not specified or file not found.

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
