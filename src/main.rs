mod categorize;
mod clean;
mod cli;
mod cmd;
mod colima;
mod completions;
mod dcx_config;
mod docker;
mod doctor;
mod down;
mod exec;
mod exit_codes;
mod format;
mod logs;
mod mount_table;
mod naming;
mod network_mode;
mod platform;
mod progress;
mod signals;
mod status;
mod sync;
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
            config_dir,
            files,
            dry_run,
            yes,
            network,
            no_cache,
        } => {
            let config_dir = config_dir.or_else(|| {
                std::env::var("DCX_DEVCONTAINER_CONFIG_DIR_PATH")
                    .ok()
                    .map(std::path::PathBuf::from)
            });
            std::process::exit(up::run_up(
                &home_dir(),
                up::UpOptions {
                    workspace_folder,
                    config_dir,
                    extra_files: files,
                    dry_run,
                    yes,
                    cli_network: network,
                    no_cache,
                },
            ));
        }
        cli::Commands::Exec {
            workspace_folder,
            config_dir,
            command,
        } => {
            let config_dir = config_dir.or_else(|| {
                std::env::var("DCX_DEVCONTAINER_CONFIG_DIR_PATH")
                    .ok()
                    .map(std::path::PathBuf::from)
            });
            std::process::exit(exec::run_exec(
                &home_dir(),
                workspace_folder,
                config_dir,
                command,
            ));
        }
        cli::Commands::Down { workspace_folder } => {
            std::process::exit(down::run_down(&home_dir(), workspace_folder));
        }
        cli::Commands::Logs {
            workspace_folder,
            follow,
            since,
            until,
            tail,
        } => {
            std::process::exit(logs::run_logs(
                &home_dir(),
                workspace_folder.as_deref(),
                follow,
                since.as_deref(),
                until.as_deref(),
                tail.as_deref(),
            ));
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
        cli::Commands::SyncDaemon {
            sources,
            stagings,
            pid_file,
        } => {
            let sync_pairs = sources
                .into_iter()
                .zip(stagings)
                .map(|(source, staging)| sync::SyncPair { source, staging })
                .collect();
            sync::run_sync_daemon(sync_pairs, pid_file);
        }
        cli::Commands::External(args) => {
            let code =
                cmd::run_stream("devcontainer", &args).unwrap_or(exit_codes::PREREQ_NOT_FOUND);
            std::process::exit(code);
        }
    }
}
