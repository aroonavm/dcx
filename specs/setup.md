# Setup Guide

## Prerequisites

- **bindfs:** FUSE userspace bind-mount tool (no root required)
- **Colima:** VM running with Docker
- **devcontainer CLI:** Already installed
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

## Troubleshooting Setup

### bindfs command not found
```bash
# Linux
sudo apt install bindfs

# macOS
brew install macfuse && brew install bindfs
```

### mount: operation not permitted
- Check filesystem supports FUSE (most do)
- Check user can mount FUSE (usually yes for regular users)
- **macOS:** Ensure macFUSE kernel extension is allowed in System Settings > Privacy & Security (requires reboot)
- Check `~/.colima-mounts` directory exists (dcx auto-creates it, but verify permissions)

### colima ssh — ls ~/.colima-mounts fails

**Problem:** Mount base directory is not accessible in VM

**Solution:**
1. Verify `colima.yaml` has the mount entry (see platform-specific paths above)
2. Restart Colima: `colima stop && colima start`
3. Check Colima is fully running: `docker info`

### dcx command not found

**Problem:** `dcx` binary not in PATH

**Solution:**
```bash
# Verify binary exists
ls -la ~/.local/bin/dcx

# Verify PATH includes ~/.local/bin
echo $PATH

# If not in PATH, add to shell config
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### dcx up fails on first try

Run the setup validation command:
```bash
dcx doctor  # Checks all prerequisites and reports what's missing/broken
```

Follow the fix instructions for any failing checks.

---

See [`architecture.md`](architecture.md) for how to use `dcx` after setup.
