# dcx

Dynamic workspace mounting wrapper for Colima devcontainers.

## Quick Start

```bash
# Setup (one time)
sudo apt install bindfs
# Edit ~/.config/colima/default/colima.yaml to mount ~/.colima-mounts
colima stop && colima start
cargo install --path .   # installs dcx to ~/.cargo/bin/

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
