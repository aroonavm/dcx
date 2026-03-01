# `dcx_config.yaml` Configuration Reference

`dcx_config.yaml` allows you to configure `dcx up` defaults per workspace, avoiding the need to repeat flags on every invocation.

## File Location & Discovery

`dcx` searches for configuration in this order:

1. **Explicit:** `--config-dir DIR` → `DIR/dcx_config.yaml`
2. **Environment:** `$DCX_DEVCONTAINER_CONFIG_DIR_PATH` → same as above
3. **Alongside devcontainer.json:** `.devcontainer/dcx_config.yaml` (alongside your devcontainer.json if auto-detected)
4. **Workspace root:** `dcx_config.yaml` or `.devcontainer/dcx_config.yaml` if auto-discovered

If no file is found, all settings default to CLI / built-in defaults.

## Schema

```yaml
# .devcontainer/dcx_config.yaml

up:
  network: open                 # (string, optional) network mode: restricted/minimal/host/open
  yes: true                     # (bool, optional) skip confirmation prompts
  files:                        # (list, optional) files to stage into container
    - path: ~/.gitconfig
    - path: ~/.claude.json
```

### Supported Keys

| Key | Type | CLI Equiv | Default | Notes |
|-----|------|-----------|---------|-------|
| `up.network` | string | `--network` | `minimal` | One of: `restricted`, `minimal`, `host`, `open`. Invalid values logged with warning, uses default. |
| `up.yes` | bool | `--yes` | `false` | Skip confirmation prompts for non-owned directories. |
| `up.files` | list | `--file` (repeatable) | empty | Paths to stage into container. Tilde (`~`) expanded at runtime. |

### Unsupported Options

These are not configurable in `dcx_config.yaml` because they are runtime-specific or circular:

- `workspace-folder` — Context-dependent per invocation
- `config-dir` — Circular (config can't specify its own location)
- `dry-run` — Preview flag, not a project default
- Other commands (`exec`, `down`, `clean`, `status`, `doctor`) — No configuration keys defined for these commands yet

## Merge Behavior

### Network

When both YAML and CLI specify `--network`:
- **YAML wins** (if valid)
- A **warning** is printed showing the override
- If YAML value is invalid, it logs a warning and falls back to `minimal`

Examples:
```bash
# dcx_config.yaml has: up.network: open
# Running: dcx up --network minimal
# Result: opens network, prints warning about override

# dcx_config.yaml has: up.network: invalid_mode
# Running: dcx up
# Result: prints warning, uses minimal (default)
```

### Yes

Values are **OR-combined**: if either YAML or CLI sets it true, prompts are skipped.

```bash
# dcx_config.yaml has: up.yes: true
# Running: dcx up --yes
# Result: yes=true (no redundancy)

# dcx_config.yaml has: up.yes: true
# Running: dcx up
# Result: yes=true (YAML is used)
```

### Files

**Additive:** CLI `--file` paths are prepended, then YAML `files` are appended.

```bash
# dcx_config.yaml has files: [~/.gitconfig, ~/.claude.json]
# Running: dcx up --file ~/.ssh/config
# Result: stages ~/.ssh/config, then ~/.gitconfig, then ~/.claude.json
```

## Full Annotated Example

```yaml
# .devcontainer/dcx_config.yaml
#
# Project-wide defaults for `dcx up`. Eliminates need to pass --network,
# --yes, or --file on every invocation.

up:
  # Network isolation level. Default: minimal (dev tools only).
  # Choose based on container's needs:
  #   restricted — No network (fully offline)
  #   minimal    — GitHub, npm, Anthropic only (safest for untrusted code)
  #   host       — Direct access to host network
  #   open       — Unrestricted internet (most permissive)
  network: minimal

  # Skip confirmation prompts for non-owned directories.
  # Useful in CI/CD or when you trust all workspaces in a mount.
  yes: false

  # Files to copy into container at their original paths.
  # Paths are tilde-expanded at runtime using $HOME.
  # If a file doesn't exist on the host, it's skipped with a warning.
  files:
    - path: ~/.gitconfig        # Git configuration (will use GIT_CONFIG_GLOBAL env var if in mounts)
    - path: ~/.ssh/config       # SSH config (if you have one)
    - path: ~/.gitignore        # Global gitignore
```

## Troubleshooting

**File not being staged:**
- Check that the path exists on your host: `ls -la <path>`
- Verify file path is under `dcx_config.yaml` as `up.files` (not at root level `files:`)
- Look for "Warning: --file X does not exist" message

**Network mode not being applied:**
- Check `dcx_config.yaml` is in a discoverable location (see "File Location & Discovery")
- Run `dcx up --dry-run` to see final merged settings in the plan
- Watch for "Warning: dcx_config.yaml up.network:" messages indicating parse errors

**Command not recognized:**
- Ensure the config is under the `up:` section (not at root)
- Check spelling: `network` (not `net-mode`), `yes` (not `skip-prompts`), `files` (not `file`)

## See Also

- [architecture.md](architecture.md) — Full `dcx up` specification and merge steps
- [guides/setup.md](guides/setup.md) — Setup instructions and examples
