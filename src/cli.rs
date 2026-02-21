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

        /// Path to devcontainer.json config file (default: auto-detected)
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Print what would happen without doing it
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompts (e.g. for non-owned directories)
        #[arg(long)]
        yes: bool,

        /// Disable container network firewall (passes FIREWALL_OPEN=true to the container)
        #[arg(long)]
        open: bool,
    },

    /// Run a command inside the devcontainer
    Exec {
        /// Workspace folder path (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,

        /// Path to devcontainer.json config file (default: auto-detected)
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

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

    /// Forward to devcontainer CLI (any unrecognised subcommand)
    #[command(external_subcommand)]
    External(Vec<String>),
}
