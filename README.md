# dcx

Dynamic workspace mounting wrapper for Colima devcontainers.

## Quick Start

```bash
# Setup (one time)
sudo apt install bindfs
mkdir -p ~/.colima-mounts
# Edit ~/.config/colima/default/colima.yaml to mount ~/colima-mounts
colima stop && colima start
cp dcx ~/.local/bin/dcx && chmod +x ~/.local/bin/dcx

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

## Building

```bash
cargo build --release
```
