// SPDX-License-Identifier: GPL-3.0-or-later

#![forbid(unsafe_code)]

mod cli;
mod workflow;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, CommandStatus};

fn main() -> ExitCode {
    match cli::execute(Cli::parse()) {
        Ok(CommandStatus::Success) => ExitCode::SUCCESS,
        Ok(CommandStatus::Failure) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("esr-disc-patcher: {error}");
            ExitCode::FAILURE
        }
    }
}
