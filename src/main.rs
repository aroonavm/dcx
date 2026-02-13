mod categorize;
mod cli;
mod cmd;
mod docker;
mod exit_codes;
mod format;
mod mount_table;
mod naming;
mod platform;
mod workspace;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Commands::Up { .. } => {
            eprintln!("dcx up: not yet implemented");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        }
        cli::Commands::Exec { .. } => {
            eprintln!("dcx exec: not yet implemented");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        }
        cli::Commands::Down { .. } => {
            eprintln!("dcx down: not yet implemented");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        }
        cli::Commands::Clean { .. } => {
            eprintln!("dcx clean: not yet implemented");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        }
        cli::Commands::Status => {
            eprintln!("dcx status: not yet implemented");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        }
        cli::Commands::Doctor => {
            eprintln!("dcx doctor: not yet implemented");
            std::process::exit(exit_codes::RUNTIME_ERROR);
        }
        cli::Commands::External(args) => {
            let code =
                cmd::run_stream("devcontainer", &args).unwrap_or(exit_codes::PREREQ_NOT_FOUND);
            std::process::exit(code);
        }
    }
}
