# dcx

Dynamic workspace mounting wrapper for Colima devcontainers.

## Why dcx?

**Problem:** Colima mounts are static (configured at startup). Running `devcontainer up` requires workspaces to already exist in the VM, but they're dynamic local paths. Mounting `$HOME` broadly exposes all projects to every container — a security risk, especially when running autonomous agents.

**Solution:** `dcx` dynamically mounts only the workspace you need, only while in use. It uses `bindfs` (a FUSE userspace tool) to project your workspace into a pre-mounted relay directory (`~/.colima-mounts`), then starts the devcontainer there.

**Benefits:**
- **Security:** Only the active workspace is mounted; other projects remain private
- **Performance:** No need to mount `$HOME` broadly; multiple isolated workspaces can coexist
- **Convenience:** One command (`dcx up`) handles mount creation, devcontainer startup, and cleanup

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/aroonavm/dcx/main/install.sh | sh
```

Installs to `/usr/local/bin` by default. Override with `DCX_BIN_DIR=~/.local/bin`.

**Prerequisites:** `bindfs`, [`devcontainer` CLI](https://code.visualstudio.com/docs/devcontainers/devcontainer-cli), and [Colima](https://colima.run/).

## Quick Start

```bash
# Setup (one time)
sudo apt install bindfs        # Linux — see specs/setup.md for macOS
# Edit ~/.config/colima/default/colima.yaml to mount ~/.colima-mounts
colima stop && colima start

# Usage
dcx up                                       # Start devcontainer with mount
dcx exec --workspace-folder . /bin/zsh       # Run command in container
dcx down                                     # Stop container and cleanup
```

### Environment Variables

Set `DCX_DEVCONTAINER_CONFIG_PATH` to avoid passing `--config` on every invocation:

```bash
export DCX_DEVCONTAINER_CONFIG_PATH=~/.dcx/devcontainer.json
dcx up --workspace-folder ~/project-a       # Uses env var config
dcx up --workspace-folder ~/project-b       # Uses env var config
dcx up --workspace-folder ~/project-c --config /other/config.json  # Flag overrides env var
```

### Network Isolation

Control network access per container with `--network`:

```bash
# Default: minimal — dev tools only (GitHub, npm, Anthropic APIs, VSCode, Sentry)
dcx up --workspace-folder ~/my-project

# Restricted: no external network access
dcx up --workspace-folder ~/my-project --network restricted

# Host-only: connect to services on host machine (localhost:*)
dcx up --workspace-folder ~/my-project --network host

# Open: unrestricted access (use for local dev only)
dcx up --workspace-folder ~/my-project --network open
```

Each container gets its own network mode. Useful for:
- **Security:** Autonomous agents with restricted network (can't exfiltrate data)
- **Sandboxing:** Different projects with different trust levels
- **Testing:** Host-only mode to test against local services

```bash
# Example: Shared dev environment with mixed trust levels
dcx up --config ~/.dcx/devcontainer.json --workspace-folder ~/trusted-project --network minimal
dcx up --config ~/.dcx/devcontainer.json --workspace-folder ~/untrusted-agent --network restricted
dcx up --config ~/.dcx/devcontainer.json --workspace-folder ~/testing-project --network host

# The base image is shared: same devcontainer.json content → same image.
# Changing devcontainer.json (e.g. bumping a package version) triggers a new build.
```

## Documentation

- **[Setup Guide](specs/setup.md)** — Installation & configuration
- **[Architecture](specs/architecture.md)** — How it works, commands, usage
- **[Troubleshooting](specs/failure-recovery.md)** — Error recovery
- **[Full Index](specs/README.md)** — All documentation

## Development Containers

Three Dockerfiles are provided for different use cases:

| File | Base | Purpose |
|------|------|---------|
| `.devcontainer/slim/Dockerfile` | `rust:slim-bookworm` | Slim dev image (git + bindfs). Default for contributors. |
| `.devcontainer/full/Dockerfile` | `node:latest` | Full image (Claude Code, zsh, git-delta). For a more complete dev environment. |
| `tests/e2e/Dockerfile.test` | `rust:slim-bookworm` | Minimal test image (git + node + devcontainer CLI, no bindfs). |

```bash
# Slim image (default) — start with:
dcx up --workspace-folder . --config .devcontainer/slim/devcontainer.json

# Full image — for the complete dev environment:
dcx up --workspace-folder . --config .devcontainer/full/devcontainer.json
```

## Building

```bash
cargo build --release
cargo install --path .
```

This installs `dcx` to `~/.cargo/bin/`. Ensure `~/.cargo/bin` is in your `PATH` (add `export PATH="$HOME/.cargo/bin:$PATH"` to your shell rc file if needed).

## Releasing

The release process builds binaries for all platforms (Linux x86_64/aarch64, macOS x86_64/aarch64) and publishes them as GitHub Release assets, along with a Debian package.

### Release Checklist

1. **Update version** in `Cargo.toml`:
   ```bash
   # Edit Cargo.toml, change version = "0.1.0" to version = "0.1.2"
   ```

2. **Update Cargo.lock** by building:
   ```bash
   cargo build --release
   ```

3. **Commit both files**:
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "Bump version to 0.1.2"
   ```

4. **Create a git tag** (tag must match version in Cargo.toml):
   ```bash
   git tag v0.1.2
   ```

5. **Push commits and tag** (in two steps to avoid race conditions):
   ```bash
   git push origin main
   git push origin v0.1.2
   ```

The tag push triggers three workflows:
- `validate-release.yml` — Checks that tag, Cargo.toml, and Cargo.lock all match
- `release.yml` — Builds binaries for 4 platforms + Debian package + creates GitHub Release
- If validation fails, release is skipped (CI shows error)

### Installation Methods

Once the GitHub Release is published, users can install from:

```bash
# Install script (auto-detects platform, downloads binary)
curl -fsSL https://raw.githubusercontent.com/aroonavm/dcx/main/install.sh | sh

# Debian/Ubuntu
sudo dpkg -i dcx_<version>_amd64.deb

# Manual (download from releases, extract, move to PATH)
tar -xzf dcx-v<version>-<platform>.tar.gz
sudo mv dcx /usr/local/bin/
```
