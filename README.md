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

## Documentation

- **[Setup Guide](specs/setup.md)** — Installation & configuration
- **[Architecture](specs/architecture.md)** — How it works, commands, usage
- **[Troubleshooting](specs/failure-recovery.md)** — Error recovery
- **[Full Index](specs/README.md)** — All documentation

## Development Containers

Three Dockerfiles are provided for different use cases:

| File | Base | Purpose |
|------|------|---------|
| `.devcontainer/Dockerfile` | `rust:slim-bookworm` | Slim dev image (git + bindfs). Default for contributors. |
| `.devcontainer/Dockerfile.full` | `node:latest` | Full image (Claude Code, zsh, git-delta). For a more complete dev environment. |
| `tests/e2e/Dockerfile.test` | `rust:slim-bookworm` | Minimal test image (git + node + devcontainer CLI, no bindfs). |

```bash
# Slim image (default) — start with either:
dcx up --workspace-folder .
# Or run `devcontainer up --workspace-folder .`

# Full image — for the complete dev environment:
dcx up --workspace-folder . --config .devcontainer/devcontainer.full.json
# Or run `devcontainer up --workspace-folder . --config .devcontainer/devcontainer.full.json`
```

## Building

```bash
cargo build --release
cargo install --path .
```

This installs `dcx` to `~/.cargo/bin/`. Ensure `~/.cargo/bin` is in your `PATH` (add `export PATH="$HOME/.cargo/bin:$PATH"` to your shell rc file if needed).

## Releasing

Tag a commit to trigger the release workflow, which builds binaries for Linux and macOS (x86\_64 + aarch64) and attaches them to the GitHub release:

```bash
git tag v0.1.0
git push origin v0.1.0
```
