#![allow(dead_code)]

use clap::CommandFactory;
use clap_complete::Shell;
use std::io;

/// Generate shell completion script to stdout and return exit code 0.
pub fn run_completions(shell: Shell) -> i32 {
    let mut cmd = crate::cli::Cli::command();
    clap_complete::generate(shell, &mut cmd, "dcx", &mut io::stdout());
    crate::exit_codes::SUCCESS
}
