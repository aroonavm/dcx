# Testing Strategy

## Overview

`dcx` uses **integration testing** via shell scripts. No unit tests or mocking layers.

## Integration Test Structure

Create shell script tests in `tests/` directory:

```
tests/
  ├── setup.sh              # Common setup/teardown
  ├── test_dcx_up.sh         # Test: dcx up with various workspaces
  ├── test_dcx_exec.sh       # Test: dcx exec
  ├── test_dcx_down.sh       # Test: dcx down and cleanup
  ├── test_dcx_clean.sh      # Test: dcx clean with stale mounts
  ├── test_dcx_status.sh     # Test: dcx status output
  ├── test_dcx_doctor.sh     # Test: dcx doctor checks
  ├── test_error_cases.sh   # Test: missing bindfs, no Docker, etc.
  └── test_edge_cases.sh    # Test: symlinks, relative paths, etc.
```

## Manual Testing Checklist

Before release, manually verify:

- [ ] `dcx doctor` passes all checks on fresh setup
- [ ] Install from scratch: add `~/.colima-mounts` to colima.yaml, restart Colima (dcx auto-creates directory)
- [ ] `dcx --help` shows dcx-specific help with all 6 subcommands
- [ ] `dcx --version` shows dcx version
- [ ] `dcx up` fails fast with "Docker is not available" when Colima is stopped
- [ ] `dcx up` fails fast with "No devcontainer configuration" when no .devcontainer exists
- [ ] `dcx up --dry-run` shows what would happen without creating any mounts
- [ ] `dcx up` creates mount, starts container, prints progress
- [ ] `dcx up` for second workspace mounts both simultaneously
- [ ] `dcx up` rolls back mount if `devcontainer up` fails, prints "Mount rolled back." after devcontainer output
- [ ] `dcx up` on stale mount: detects, remounts, starts container
- [ ] Ctrl+C during `dcx up` rolls back the mount cleanly
- [ ] `dcx exec` runs commands in container
- [ ] `dcx exec` without `dcx up` fails with "Run `dcx up` first"
- [ ] Container can read/write files through the mount
- [ ] `dcx down` stops only the targeted workspace, leaves others intact, prints progress
- [ ] `dcx down` with no mount prints "No mount found. Nothing to do."
- [ ] `dcx clean` (default) removes only orphaned/stale/empty mounts, leaves active untouched
- [ ] `dcx clean --all` prompts if active containers, lists names
- [ ] `dcx clean --all` stops active containers, unmounts all `dcx-*` mounts, prints summary
- [ ] `dcx clean` and `dcx clean --all` continue on individual failures, report all at end
- [ ] `dcx status` shows all mounted workspaces, containers, and state
- [ ] Error messages are clear and actionable
- [ ] Works with various devcontainer.json configurations
- [ ] Works with different workspace locations (home, /tmp, symlinks, etc.)
- [ ] Works with git repos (git root detection)
- [ ] Works after Colima restart (stale mount recovery)

## Continuous Integration

Add to CI pipeline (GitHub Actions, etc.):

```yaml
- name: Run integration tests
  run: |
    chmod +x tests/*.sh
    for test in tests/test_*.sh; do
      bash "$test" || exit 1
    done
```

**Prerequisites for CI:**
- Linux environment (Colima/Lima uses QEMU on Linux)
- Docker installed
- Colima running
- `bindfs` installed
