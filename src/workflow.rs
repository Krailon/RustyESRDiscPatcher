// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsString;
use std::fs::{self, File, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use esr_disc_patcher_rs::{PatchState, inspect, patch, unpatch};
use indicatif::{ProgressBar, ProgressStyle};
use same_file::Handle;
use tempfile::{Builder, NamedTempFile};

use crate::cli::CliError;

const TEMPORARY_PREFIX: &str = ".esr-disc-patcher-";
const TEMPORARY_SUFFIX: &str = ".tmp";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Patch,
    Unpatch,
}

impl Operation {
    fn suffix(self) -> &'static str {
        match self {
            Self::Patch => "_patched",
            Self::Unpatch => "_unpatched",
        }
    }

    fn infinitive(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Unpatch => "unpatch",
        }
    }

    pub(crate) fn past_tense(self) -> &'static str {
        match self {
            Self::Patch => "patched",
            Self::Unpatch => "unpatched",
        }
    }

    fn phase_message(self) -> &'static str {
        match self {
            Self::Patch => "Applying ESR patch...",
            Self::Unpatch => "Removing ESR patch...",
        }
    }

    fn validate_state(self, state: PatchState) -> Result<(), CliError> {
        match (self, state) {
            (Self::Patch, PatchState::Unpatched) | (Self::Unpatch, PatchState::Patched) => Ok(()),
            (Self::Patch, PatchState::Patched) => Err(CliError::AlreadyPatched),
            (Self::Unpatch, PatchState::Unpatched) => Err(CliError::NotPatched),
            (_, PatchState::Inconsistent) => Err(CliError::InconsistentState),
        }
    }

    fn apply(self, image: &mut File) -> Result<(), CliError> {
        let result = match self {
            Self::Patch => patch(image),
            Self::Unpatch => unpatch(image),
        };
        result.map_err(|source| CliError::TransformImage {
            operation: self.infinitive(),
            source,
        })
    }
}

pub(crate) fn open_regular_input(path: &Path) -> Result<File, CliError> {
    let file = File::open(path).map_err(|source| CliError::OpenInput {
        path: path.to_owned(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| CliError::InputMetadata {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(CliError::InputNotRegular {
            path: path.to_owned(),
        });
    }
    Ok(file)
}

pub(crate) fn transform(
    input_path: &Path,
    requested_output: Option<&Path>,
    operation: Operation,
    quiet: bool,
) -> Result<PathBuf, CliError> {
    if !quiet {
        eprintln!("Validating input: {}", input_path.display());
    }

    let mut input = open_regular_input(input_path)?;
    let metadata = input.metadata().map_err(|source| CliError::InputMetadata {
        path: input_path.to_owned(),
        source,
    })?;
    let info = inspect(&mut input).map_err(|source| CliError::InspectInput {
        path: input_path.to_owned(),
        source,
    })?;
    operation.validate_state(info.state)?;

    let output = match requested_output {
        Some(path) => path.to_owned(),
        None => default_output_path(input_path, operation)?,
    };
    ensure_destination_available(&input, &output)?;

    input
        .seek(SeekFrom::Start(0))
        .map_err(|source| CliError::RewindInput {
            path: input_path.to_owned(),
            source,
        })?;
    write_output(
        &mut input,
        metadata.len(),
        metadata.permissions(),
        &output,
        quiet,
        operation,
        |image| operation.apply(image),
    )?;
    Ok(output)
}

fn default_output_path(input: &Path, operation: Operation) -> Result<PathBuf, CliError> {
    let stem = input
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| CliError::InvalidInputName {
            path: input.to_owned(),
        })?;

    let mut name = OsString::from(stem);
    name.push(operation.suffix());
    match input.extension().filter(|extension| !extension.is_empty()) {
        Some(extension) => {
            name.push(".");
            name.push(extension);
        }
        None => name.push(".iso"),
    }
    Ok(input.with_file_name(name))
}

fn ensure_destination_available(input: &File, output: &Path) -> Result<(), CliError> {
    match fs::symlink_metadata(output) {
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(CliError::InspectOutput {
                path: output.to_owned(),
                source,
            });
        }
    }

    match fs::metadata(output) {
        Ok(metadata) if metadata.is_file() => {
            let input_handle =
                Handle::from_file(
                    input
                        .try_clone()
                        .map_err(|source| CliError::InspectOutput {
                            path: output.to_owned(),
                            source,
                        })?,
                )
                .map_err(|source| CliError::InspectOutput {
                    path: output.to_owned(),
                    source,
                })?;
            let output_handle =
                Handle::from_path(output).map_err(|source| CliError::InspectOutput {
                    path: output.to_owned(),
                    source,
                })?;
            if input_handle == output_handle {
                Err(CliError::OutputAliasesInput {
                    path: output.to_owned(),
                })
            } else {
                Err(CliError::OutputExists {
                    path: output.to_owned(),
                })
            }
        }
        Ok(_) => Err(CliError::OutputExists {
            path: output.to_owned(),
        }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Err(CliError::OutputExists {
            path: output.to_owned(),
        }),
        Err(source) => Err(CliError::InspectOutput {
            path: output.to_owned(),
            source,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn write_output<R, F>(
    input: &mut R,
    expected_length: u64,
    permissions: Permissions,
    output: &Path,
    quiet: bool,
    operation: Operation,
    transform: F,
) -> Result<(), CliError>
where
    R: Read + ?Sized,
    F: FnOnce(&mut File) -> Result<(), CliError>,
{
    let directory = output_directory(output);
    let mut temporary = Builder::new()
        .prefix(TEMPORARY_PREFIX)
        .suffix(TEMPORARY_SUFFIX)
        .tempfile_in(directory)
        .map_err(|source| CliError::CreateTemporary {
            directory: directory.to_owned(),
            source,
        })?;

    let progress = copy_progress(expected_length, quiet);
    let copy_result = {
        let mut reader = progress.wrap_read(input);
        io::copy(&mut reader, temporary.as_file_mut())
    };
    let copied = match copy_result {
        Ok(copied) => {
            progress.finish_and_clear();
            copied
        }
        Err(source) => {
            progress.finish_and_clear();
            return Err(CliError::CopyInput(source));
        }
    };
    if copied != expected_length {
        return Err(CliError::InputLengthChanged {
            expected: expected_length,
            copied,
        });
    }

    temporary
        .as_file_mut()
        .flush()
        .map_err(CliError::FlushTemporary)?;
    if !quiet {
        eprintln!("{}", operation.phase_message());
    }
    transform(temporary.as_file_mut())?;
    temporary
        .as_file()
        .set_permissions(permissions)
        .map_err(CliError::PreservePermissions)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(CliError::SynchronizeTemporary)?;
    publish(temporary, output)
}

fn copy_progress(length: u64, quiet: bool) -> ProgressBar {
    if quiet {
        return ProgressBar::hidden();
    }
    let style = ProgressStyle::with_template("Copying [{wide_bar}] {bytes}/{total_bytes} ({eta})")
        .expect("static progress template is valid")
        .progress_chars("=> ");
    ProgressBar::new(length).with_style(style)
}

fn publish(temporary: NamedTempFile, output: &Path) -> Result<(), CliError> {
    match temporary.persist_noclobber(output) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            Err(CliError::OutputExists {
                path: output.to_owned(),
            })
        }
        // Windows may report a no-clobber destination collision as a different error kind.
        Err(error) => match fs::symlink_metadata(output) {
            Ok(_) => Err(CliError::OutputExists {
                path: output.to_owned(),
            }),
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                Err(CliError::PublishOutput {
                    path: output.to_owned(),
                    source: error.error,
                })
            }
            Err(source) => Err(CliError::InspectOutput {
                path: output.to_owned(),
                source,
            }),
        },
    }
}

fn output_directory(output: &Path) -> &Path {
    output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn default_names_preserve_parent_and_extension() {
        assert_eq!(
            default_output_path(Path::new("images/Game.ISO"), Operation::Patch).unwrap(),
            Path::new("images/Game_patched.ISO")
        );
        assert_eq!(
            default_output_path(Path::new("game"), Operation::Unpatch).unwrap(),
            Path::new("game_unpatched.iso")
        );
        assert_eq!(
            default_output_path(Path::new("game."), Operation::Patch).unwrap(),
            Path::new("game_patched.iso")
        );
    }

    #[test]
    fn output_directory_uses_current_directory_for_bare_names() {
        assert_eq!(output_directory(Path::new("game.iso")), Path::new("."));
        assert_eq!(
            output_directory(Path::new("images/game.iso")),
            Path::new("images")
        );
    }

    #[test]
    fn progress_reader_accounts_for_every_copied_byte() {
        let progress = copy_progress(8, true);
        let mut input = Cursor::new(vec![1_u8; 8]);
        let mut output = Vec::new();
        {
            let mut reader = progress.wrap_read(&mut input);
            assert_eq!(io::copy(&mut reader, &mut output).unwrap(), 8);
        }
        assert_eq!(progress.position(), 8);
        assert_eq!(output, vec![1_u8; 8]);
    }

    #[test]
    fn copy_failure_removes_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output.iso");
        let mut input = FailingReader;
        let permissions = directory.path().metadata().unwrap().permissions();

        assert!(matches!(
            write_output(
                &mut input,
                8,
                permissions,
                &output,
                true,
                Operation::Patch,
                |_| unreachable!(),
            ),
            Err(CliError::CopyInput(_))
        ));
        assert_no_output_or_temporary(directory.path(), &output);
    }

    #[test]
    fn transform_failure_removes_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output.iso");
        let mut input = Cursor::new(vec![1_u8; 8]);
        let permissions = directory.path().metadata().unwrap().permissions();

        assert!(matches!(
            write_output(
                &mut input,
                8,
                permissions,
                &output,
                true,
                Operation::Patch,
                |image| Operation::Patch.apply(image),
            ),
            Err(CliError::TransformImage { .. })
        ));
        assert_no_output_or_temporary(directory.path(), &output);
    }

    #[test]
    fn persistence_race_does_not_overwrite_destination_or_leave_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output.iso");
        let mut input = Cursor::new(vec![1_u8; 8]);
        let permissions = directory.path().metadata().unwrap().permissions();

        assert!(matches!(
            write_output(
                &mut input,
                8,
                permissions,
                &output,
                true,
                Operation::Patch,
                |_| {
                    fs::write(&output, b"existing").unwrap();
                    Ok(())
                },
            ),
            Err(CliError::OutputExists { .. })
        ));
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        assert_no_temporary(directory.path());
    }

    #[test]
    fn short_copy_is_rejected_and_cleaned_up() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output.iso");
        let mut input = Cursor::new(vec![1_u8; 7]);
        let permissions = directory.path().metadata().unwrap().permissions();

        assert!(matches!(
            write_output(
                &mut input,
                8,
                permissions,
                &output,
                true,
                Operation::Patch,
                |_| unreachable!(),
            ),
            Err(CliError::InputLengthChanged {
                expected: 8,
                copied: 7,
            })
        ));
        assert_no_output_or_temporary(directory.path(), &output);
    }

    fn assert_no_output_or_temporary(directory: &Path, output: &Path) {
        assert!(!output.exists());
        assert_no_temporary(directory);
    }

    fn assert_no_temporary(directory: &Path) {
        let leftovers: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().starts_with(TEMPORARY_PREFIX))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files remain: {leftovers:?}"
        );
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected copy failure"))
        }
    }
}
