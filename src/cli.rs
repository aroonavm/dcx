use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "dcx",
    version,
    about = "Dynamic workspace mounting wrapper for Colima devcontainers",
    long_about = "dcx wraps `devcontainer` to manage bindfs mounts for Colima.\n\n\
                  Managed subcommands: up, exec, down, clean, status, doctor\n\
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

        /// Print what would happen without doing it
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompts (e.g. for non-owned directories)
        #[arg(long)]
        yes: bool,
    },

    /// Run a command inside the devcontainer
    Exec {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,

        /// Command and arguments to run inside the container
        #[arg(last = true, value_name = "CMD")]
        command: Vec<String>,
    },

    /// Stop container and unmount workspace
    Down {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,
    },

    /// Clean up dcx-managed mounts
    Clean {
        /// Remove everything including active mounts (default: skip active)
        #[arg(long)]
        all: bool,

        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },

    /// Show status of all dcx-managed workspaces
    Status,

    /// Validate prerequisites (bindfs, devcontainer, Docker, Colima)
    Doctor,

    /// Generate shell completion script (bash, zsh, fish, powershell, elvish)
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Forward to devcontainer CLI (any unrecognised subcommand)
    #[command(external_subcommand)]
    External(Vec<String>),
}
