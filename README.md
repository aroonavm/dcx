# dcx

Dynamic workspace mounting wrapper for Colima devcontainers.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/aroonavm/dcx/main/install.sh | sh
```

Installs to `/usr/local/bin` by default. Override with `DCX_BIN_DIR=~/.local/bin`.

**Prerequisites:** `bindfs`, `devcontainer` CLI, and on macOS: Colima.

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

```bash
# Shared dev environment: same devcontainer.json, multiple workspaces
# First workspace: builds base image once from devcontainer.json
dcx up --config ~/.dcx/devcontainer.json --workspace-folder ~/project-a

# Second workspace: reuses the same base image (instant, no rebuild)
dcx up --config ~/.dcx/devcontainer.json --workspace-folder ~/project-b

# --open controls the firewall per container:
dcx up --config ~/.dcx/devcontainer.json --workspace-folder ~/project-c --open
# project-c has no network restrictions; project-a and project-b retain theirs.

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
