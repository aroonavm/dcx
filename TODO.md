# TODO

## Current Work: Fix dcx clean --purge regression

**Problem**: The fix for removing build images (commit 82dce70) introduced a regression. In single-workspace mode, `dcx clean --purge` calls `clean_orphaned_build_images()` which is a global sweep. This can remove build images from other active workspaces because Docker's `--filter ancestor=<build-image>` doesn't reliably detect containers created from derived runtime images.

**Root Cause**: `clean_orphaned_build_images()` checks if a build image has containers using:
```
docker ps -a --filter ancestor=<build-image> --format {{.ID}}
```
This filter doesn't traverse image layers reliably. A container created from runtime image `vsc-B-uid:latest` (child of `vsc-B:latest`) may not be found when filtering by `ancestor=vsc-B:latest`.

**Fix Strategy**:
1. Change `clean_orphaned_build_images()` to use a safer orphan test
2. A build image is orphaned only if its corresponding runtime image no longer exists
3. The invariant that makes this safe:
   - `clean_orphaned_images()` always runs before `clean_orphaned_build_images()`
   - After `clean_orphaned_images()` completes, every surviving `vsc-X-uid:latest` image has an active container
   - Therefore: if `vsc-X-uid:latest` exists → workspace X is active → keep build image X
   - If `vsc-X-uid:latest` is gone → workspace X is fully cleaned → safe to remove build image X

### Tasks

- [ ] 1. Modify `src/docker.rs::clean_orphaned_build_images()`
- [ ] 2. Add unit tests
- [ ] 3. Add/update e2e test
- [ ] 4. Run `make check`

### Files to Change
- `src/docker.rs` — `clean_orphaned_build_images()` (lines ~459-500)
- `tests/e2e/test_dcx_clean.sh` — Add/update test scenario
