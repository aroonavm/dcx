mod categorize;
mod clean;
mod cli;
mod cmd;
mod completions;
mod docker;
mod doctor;
mod down;
mod exec;
mod exit_codes;
mod format;
mod mount_table;
mod naming;
mod network_mode;
mod platform;
mod progress;
mod signals;
mod status;
mod up;
mod workspace;

use clap::Parser;

fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("HOME environment variable is not set");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        })
}

fn main() {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Commands::Up {
            workspace_folder,
            config,
            dry_run,
            yes,
            network,
        } => {
            let config = config.or_else(|| {
                std::env::var("DCX_DEVCONTAINER_CONFIG_PATH")
                    .ok()
                    .map(std::path::PathBuf::from)
            });
            // SAFETY: single-threaded at this point; set before spawning devcontainer
            unsafe {
                std::env::set_var("DCX_NETWORK_MODE", network.to_string());
            }
            std::process::exit(up::run_up(
                &home_dir(),
                workspace_folder,
                config,
                dry_run,
                yes,
            ));
        }
        cli::Commands::Exec {
            workspace_folder,
            config,
            command,
        } => {
            let config = config.or_else(|| {
                std::env::var("DCX_DEVCONTAINER_CONFIG_PATH")
                    .ok()
                    .map(std::path::PathBuf::from)
            });
            std::process::exit(exec::run_exec(
                &home_dir(),
                workspace_folder,
                config,
                command,
            ));
        }
        cli::Commands::Down { workspace_folder } => {
            std::process::exit(down::run_down(&home_dir(), workspace_folder));
        }
        cli::Commands::Clean {
            workspace_folder,
            all,
            yes,
            purge,
            dry_run,
        } => {
            std::process::exit(clean::run_clean(
                &home_dir(),
                workspace_folder,
                all,
                yes,
                purge,
                dry_run,
            ));
        }
        cli::Commands::Status => {
            std::process::exit(status::run_status(&home_dir()));
        }
        cli::Commands::Doctor => {
            std::process::exit(doctor::run_doctor(&home_dir()));
        }
        cli::Commands::Completions { shell } => {
            std::process::exit(completions::run_completions(shell));
        }
        cli::Commands::External(args) => {
            let code =
                cmd::run_stream("devcontainer", &args).unwrap_or(exit_codes::PREREQ_NOT_FOUND);
            std::process::exit(code);
        }
    }
}
