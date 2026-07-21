// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use esr_disc_patcher_rs::{PatchState, inspect};

use crate::workflow::{self, Operation};

#[derive(Debug, Parser)]
#[command(
    name = "esr-disc-patcher",
    version,
    about = "Inspect, patch, and unpatch ESR-compatible PS2 DVD images"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect an image without modifying it.
    Inspect {
        /// DVD ISO image to inspect.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
    },
    /// Create a patched copy of an image.
    Patch(TransformArgs),
    /// Create an unpatched copy of an image.
    Unpatch(TransformArgs),
}

#[derive(Debug, Args)]
struct TransformArgs {
    /// Source DVD ISO image. This file is never modified.
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// Destination image path.
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// Suppress progress and success messages.
    #[arg(long)]
    quiet: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandStatus {
    Success,
    Failure,
}

pub(crate) fn execute(cli: Cli) -> Result<CommandStatus, CliError> {
    match cli.command {
        Command::Inspect { input } => inspect_image(input),
        Command::Patch(arguments) => transform_image(arguments, Operation::Patch),
        Command::Unpatch(arguments) => transform_image(arguments, Operation::Unpatch),
    }
}

fn inspect_image(input: PathBuf) -> Result<CommandStatus, CliError> {
    let mut image = workflow::open_regular_input(&input)?;
    let info = inspect(&mut image).map_err(|source| CliError::InspectInput {
        path: input,
        source,
    })?;

    let (text, status) = match info.state {
        PatchState::Unpatched => ("unpatched", CommandStatus::Success),
        PatchState::Patched => ("patched", CommandStatus::Success),
        PatchState::Inconsistent => ("inconsistent", CommandStatus::Failure),
    };
    writeln!(io::stdout().lock(), "{text}").map_err(CliError::WriteOutput)?;
    Ok(status)
}

fn transform_image(
    arguments: TransformArgs,
    operation: Operation,
) -> Result<CommandStatus, CliError> {
    let output = workflow::transform(
        &arguments.input,
        arguments.output.as_deref(),
        operation,
        arguments.quiet,
    )?;

    if !arguments.quiet {
        eprintln!(
            "Created {} image: {}",
            operation.past_tense(),
            output.display()
        );
    }
    Ok(CommandStatus::Success)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error("cannot open input {path:?}: {source}")]
    OpenInput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot read metadata for input {path:?}: {source}")]
    InputMetadata {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("input is not a regular file: {path:?}")]
    InputNotRegular { path: PathBuf },

    #[error("cannot inspect input {path:?}: {source}")]
    InspectInput {
        path: PathBuf,
        #[source]
        source: esr_disc_patcher_rs::Error,
    },

    #[error("cannot {operation} image: {source}")]
    TransformImage {
        operation: &'static str,
        #[source]
        source: esr_disc_patcher_rs::Error,
    },

    #[error("image is already patched")]
    AlreadyPatched,

    #[error("image is not patched")]
    NotPatched,

    #[error("image contains an incomplete or inconsistent ESR patch")]
    InconsistentState,

    #[error("cannot derive an output name from input path {path:?}")]
    InvalidInputName { path: PathBuf },

    #[error("output path aliases the input image: {path:?}")]
    OutputAliasesInput { path: PathBuf },

    #[error("output path already exists: {path:?}")]
    OutputExists { path: PathBuf },

    #[error("cannot inspect output path {path:?}: {source}")]
    InspectOutput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot create a temporary output in {directory:?}: {source}")]
    CreateTemporary {
        directory: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot rewind input {path:?}: {source}")]
    RewindInput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot copy input data: {0}")]
    CopyInput(#[source] io::Error),

    #[error(
        "input length changed while it was being copied (expected {expected} bytes, copied {copied})"
    )]
    InputLengthChanged { expected: u64, copied: u64 },

    #[error("cannot flush temporary output: {0}")]
    FlushTemporary(#[source] io::Error),

    #[error("cannot preserve source permissions on temporary output: {0}")]
    PreservePermissions(#[source] io::Error),

    #[error("cannot synchronize temporary output: {0}")]
    SynchronizeTemporary(#[source] io::Error),

    #[error("cannot publish output {path:?}: {source}")]
    PublishOutput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("cannot write command output: {0}")]
    WriteOutput(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_each_documented_command() {
        assert!(Cli::try_parse_from(["tool", "inspect", "game.iso"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "tool",
                "patch",
                "game.iso",
                "--output",
                "patched.iso",
                "--quiet",
            ])
            .is_ok()
        );
        assert!(Cli::try_parse_from(["tool", "unpatch", "game.iso", "-o", "out.iso"]).is_ok());
    }
}
