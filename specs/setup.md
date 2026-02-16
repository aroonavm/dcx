# Setup Guide

## Prerequisites

- **bindfs:** FUSE userspace bind-mount tool (no root required)
- **Colima:** VM running with Docker
- **devcontainer CLI:** npm package (@devcontainers/cli)
- **Linux or macOS host**

> **Note:** The binary is named `dcx` (not `dc`) to avoid conflict with the POSIX `dc` desk calculator utility.

## One-Time Setup

### 1. Install bindfs

**Linux:**
```bash
sudo apt update
sudo apt install bindfs
```

**macOS:**
```bash
brew install macfuse
brew install bindfs
```

> **macOS note:** macFUSE must be installed before bindfs. You may need to allow the macFUSE kernel extension in System Settings > Privacy & Security after installation, then reboot.

Verify installation:
```bash
which bindfs
bindfs --version
```

### 2. Update Colima configuration

> **Note:** You don't need to manually create `~/.colima-mounts`. `dcx` auto-creates it on first use with system default permissions.

Edit the Colima config file:
- **Linux:** `~/.config/colima/default/colima.yaml`
- **macOS:** `~/.colima/default/colima.yaml`

```yaml
# Add (or update if exists) the mounts section:
mounts:
  - location: ~/.claude      # If you use Claude Code locally
    writable: true
  - location: ~/.colima-mounts
    writable: true
```

Full example:
```yaml
# Linux: ~/.config/colima/default/colima.yaml
# macOS: ~/.colima/default/colima.yaml
cpu: 4
memory: 8GiB
disk: 100GiB

mounts:
  - location: ~/.claude
    writable: true
  - location: ~/.colima-mounts
    writable: true

# ... other config
```

### 3. Restart Colima

```bash
colima stop
colima start
```

### 4. Verify mount is accessible from VM

```bash
colima ssh -- ls ~/.colima-mounts
# Should list (empty or with any previous mounts)

colima ssh -- touch ~/.colima-mounts/.test-write
# Should succeed without error
colima ssh -- rm ~/.colima-mounts/.test-write
```

If these fail, check:
- `colima.yaml` has correct location and `writable: true`
- Colima fully restarted (not just resumed)
- `~/.colima-mounts` exists on host and has appropriate permissions

### 5. Install `dcx` wrapper

```bash
# Create ~/.local/bin if it doesn't exist
mkdir -p ~/.local/bin

# Copy dcx binary to PATH
cp dcx ~/.local/bin/dcx
chmod +x ~/.local/bin/dcx
```

Verify `~/.local/bin` is in `$PATH`:
```bash
echo $PATH | grep -q ~/.local/bin && echo "in PATH" || echo "NOT in PATH"
```

If not in PATH, add to `~/.bashrc` or `~/.zshrc`:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

Then reload shell:
```bash
source ~/.bashrc  # or ~/.zshrc
```

### 6. Verify installation

```bash
dcx --version
# Should print version

dcx up --help
# Should show help for dcx up
```

## Troubleshooting

For detailed troubleshooting steps including common setup issues, see [`failure-recovery.md`](failure-recovery.md). Key diagnostic commands:

- **`dcx doctor`** — Checks all prerequisites and reports what's missing/broken
- **`dcx status`** — Shows all mounted workspaces and their health status

---

See [`architecture.md`](architecture.md) for how to use `dcx` after setup.
