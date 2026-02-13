mod categorize;
mod cli;
mod cmd;
mod docker;
mod doctor;
mod down;
mod exec;
mod exit_codes;
mod format;
mod mount_table;
mod naming;
mod platform;
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
            dry_run,
            yes,
        } => {
            std::process::exit(up::run_up(&home_dir(), workspace_folder, dry_run, yes));
        }
        cli::Commands::Exec {
            workspace_folder,
            command,
        } => {
            std::process::exit(exec::run_exec(&home_dir(), workspace_folder, command));
        }
        cli::Commands::Down { workspace_folder } => {
            std::process::exit(down::run_down(&home_dir(), workspace_folder));
        }
        cli::Commands::Clean { .. } => {
            eprintln!("dcx clean: not yet implemented");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        }
        cli::Commands::Status => {
            std::process::exit(status::run_status(&home_dir()));
        }
        cli::Commands::Doctor => {
            std::process::exit(doctor::run_doctor(&home_dir()));
        }
        cli::Commands::External(args) => {
            let code =
                cmd::run_stream("devcontainer", &args).unwrap_or(exit_codes::PREREQ_NOT_FOUND);
            std::process::exit(code);
        }
    }
}
