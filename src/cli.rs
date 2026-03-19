use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::network_mode::NetworkMode;

#[derive(Parser)]
#[command(
    name = "dcx",
    version,
    about = "Dynamic workspace mounting wrapper for Colima devcontainers",
    long_about = "dcx wraps `devcontainer` to manage bindfs mounts for Colima.\n\n\
                  Managed subcommands: up, exec, down, logs, clean, status, doctor\n\
                  All other subcommands are forwarded to `devcontainer` unchanged."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create bindfs mount and start devcontainer
    Up {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,

        /// Directory containing devcontainer.json (default: auto-detected)
        #[arg(long, value_name = "DIR")]
        config_dir: Option<PathBuf>,

        /// Host file path to stage into the container (may be repeated).
        /// Alternatively, declare files in .devcontainer/dcx_config.yaml with a 'files:' list
        #[arg(long = "file", value_name = "PATH")]
        files: Vec<PathBuf>,

        /// Print what would happen without doing it
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompts (e.g. for non-owned directories)
        #[arg(long)]
        yes: bool,

        /// Network isolation level (default: minimal)
        ///
        /// - restricted: no network access (block all traffic)
        /// - minimal: dev tools only (GitHub, npm, Anthropic) [default]
        /// - host: allow host network only
        /// - open: unrestricted access
        #[arg(long, value_name = "MODE")]
        network: Option<NetworkMode>,

        /// Build the container image without using Docker cache
        #[arg(long)]
        no_cache: bool,
    },

    /// Run a command inside the devcontainer
    Exec {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,

        /// Directory containing devcontainer.json (default: auto-detected)
        #[arg(long, value_name = "DIR")]
        config_dir: Option<PathBuf>,

        /// Command and arguments to run inside the container
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "CMD"
        )]
        command: Vec<String>,
    },

    /// Stop container and unmount workspace
    Down {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,
    },

    /// View or stream logs from the container for a workspace
    /// Mirrors `docker logs` — see `docker logs --help` for flag details.
    Logs {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,

        /// Follow log output (stream new lines as they arrive)
        #[arg(long, short = 'f')]
        follow: bool,

        /// Show logs since timestamp or duration (e.g. 2024-01-01T00:00:00Z, 10m, now)
        #[arg(long, value_name = "VALUE")]
        since: Option<String>,

        /// Show logs before timestamp or duration
        #[arg(long, value_name = "VALUE")]
        until: Option<String>,

        /// Number of lines to show from the end of the logs (e.g. 20, all)
        #[arg(long, value_name = "VALUE")]
        tail: Option<String>,
    },

    /// Clean up dcx-managed mounts
    Clean {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,

        /// Clean all dcx-managed workspaces (default: current workspace only)
        #[arg(long)]
        all: bool,

        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,

        /// Leave nothing behind: also remove the build image and Docker volumes
        #[arg(long)]
        purge: bool,

        /// Show what would be cleaned without doing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Show status of all dcx-managed workspaces
    Status,

    /// Validate prerequisites (bindfs, devcontainer, Docker, Colima)
    Doctor,

    /// Manage Colima autostart on system boot
    Autostart {
        #[command(subcommand)]
        action: AutostartAction,
    },

    #[command(
        about = "Generate shell completion script (bash, zsh, fish, powershell, elvish)",
        long_about = "Generates a completion script for your shell to enable tab-completion of dcx commands.\n\n\
                      EXAMPLES:\n\
                      \n\
                      # Generate bash completions and install system-wide\n\
                      dcx completions bash | sudo tee /etc/bash_completion.d/dcx\n\
                      \n\
                      # Generate zsh completions and install system-wide\n\
                      dcx completions zsh | sudo tee /usr/share/zsh/site-functions/_dcx\n\
                      \n\
                      # Generate fish completions and install in user directory\n\
                      dcx completions fish | tee ~/.config/fish/completions/dcx.fish\n\
                      \n\
                      After installation, restart your shell or source the file to enable completions.\n\
                      Then use Tab to auto-complete: `dcx <TAB>` suggests up, down, exec, etc."
    )]
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Internal sync daemon (not for direct user invocation)
    #[command(name = "_sync-daemon", hide = true)]
    SyncDaemon {
        /// Source (host) file paths (repeatable, paired with --staging)
        #[arg(long = "source", required = true)]
        sources: Vec<PathBuf>,

        /// Staging file paths (repeatable, paired with --source)
        #[arg(long = "staging", required = true)]
        stagings: Vec<PathBuf>,

        /// PID file path
        #[arg(long = "pid-file", required = true)]
        pid_file: PathBuf,
    },

    /// Forward to devcontainer CLI (any unrecognised subcommand)
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
pub enum AutostartAction {
    /// Configure Colima to start on boot and start it now if not running
    Enable,
    /// Remove Colima autostart configuration
    Disable,
    /// Show current autostart status
    Status,
}
