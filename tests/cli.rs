// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use esr_disc_patcher_rs::{PatchState, UdfRevision, inspect, patch};

const SECTOR_BYTES: usize = 2_048;
const FIXTURE_SECTORS: usize = 256;
const TEMPORARY_PREFIX: &str = ".esr-disc-patcher-";

#[test]
fn inspect_prints_exact_states_and_uses_failure_for_inconsistent_images() {
    let directory = tempfile::tempdir().unwrap();
    let unpatched_path = directory.path().join("unpatched.iso");
    let patched_path = directory.path().join("patched.iso");
    let inconsistent_path = directory.path().join("inconsistent.iso");
    fs::write(&unpatched_path, fixture(UdfRevision::Nsr03)).unwrap();
    fs::write(&patched_path, patched_fixture()).unwrap();
    let mut inconsistent = fixture(UdfRevision::Nsr03);
    sector_mut(&mut inconsistent, 14)[0] = 1;
    fs::write(&inconsistent_path, inconsistent).unwrap();

    let unpatched = run(["inspect", path(&unpatched_path)]);
    assert_success(&unpatched);
    assert_eq!(unpatched.stdout, b"unpatched\n");
    assert!(unpatched.stderr.is_empty());

    let patched = run(["inspect", path(&patched_path)]);
    assert_success(&patched);
    assert_eq!(patched.stdout, b"patched\n");
    assert!(patched.stderr.is_empty());

    let inconsistent = run(["inspect", path(&inconsistent_path)]);
    assert_eq!(inconsistent.status.code(), Some(1));
    assert_eq!(inconsistent.stdout, b"inconsistent\n");
    assert!(inconsistent.stderr.is_empty());
}

#[test]
fn patch_and_unpatch_default_outputs_are_safe_and_byte_exact() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("Game.ISO");
    let patched_path = directory.path().join("Game_patched.ISO");
    let unpatched_path = directory.path().join("Game_patched_unpatched.ISO");
    let original = fixture(UdfRevision::Nsr02);
    fs::write(&source, &original).unwrap();

    let patched = run(["patch", path(&source)]);
    assert_success(&patched);
    assert!(patched.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&patched.stderr);
    assert!(stderr.contains("Validating input:"));
    assert!(stderr.contains("Applying ESR patch..."));
    assert!(stderr.contains("Created patched image:"));
    assert!(!stderr.contains('\r'));
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(inspect_file(&patched_path), PatchState::Patched);
    assert_no_temporary(directory.path());

    let unpatched = run(["unpatch", path(&patched_path)]);
    assert_success(&unpatched);
    assert!(unpatched.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&unpatched.stderr);
    assert!(stderr.contains("Removing ESR patch..."));
    assert!(stderr.contains("Created unpatched image:"));
    assert_eq!(inspect_file(&patched_path), PatchState::Patched);
    assert_eq!(fs::read(&unpatched_path).unwrap(), original);
    assert_eq!(inspect_file(&unpatched_path), PatchState::Unpatched);
    assert_no_temporary(directory.path());
}

#[test]
fn quiet_explicit_output_suppresses_success_but_not_errors() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.iso");
    let output = directory.path().join("custom.bin");
    fs::write(&source, fixture(UdfRevision::Nsr03)).unwrap();

    let result = run(["patch", path(&source), "--output", path(&output), "--quiet"]);
    assert_success(&result);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
    assert_eq!(inspect_file(&output), PatchState::Patched);

    let rejected = directory.path().join("rejected.iso");
    let result = run([
        "patch",
        path(&output),
        "--output",
        path(&rejected),
        "--quiet",
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("already patched"));
    assert!(!rejected.exists());
    assert_no_temporary(directory.path());
}

#[test]
fn malformed_inputs_fail_before_output_creation() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("invalid.iso");
    let output = directory.path().join("output.iso");
    fs::write(&source, vec![0_u8; FIXTURE_SECTORS * SECTOR_BYTES]).unwrap();

    let result = run(["patch", path(&source), "-o", path(&output)]);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("NSR02 or NSR03"));
    assert!(!output.exists());
    assert_no_temporary(directory.path());
}

#[test]
fn non_file_inputs_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("output.iso");

    let result = run([
        "patch",
        path(directory.path()),
        "-o",
        path(&output),
        "--quiet",
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("not a regular file"));
    assert!(!output.exists());
    assert_no_temporary(directory.path());
}

#[test]
fn existing_outputs_are_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.iso");
    let output = directory.path().join("output.iso");
    fs::write(&source, fixture(UdfRevision::Nsr03)).unwrap();
    fs::write(&output, b"keep this").unwrap();

    let result = run(["patch", path(&source), "-o", path(&output)]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("already exists"));
    assert_eq!(fs::read(&output).unwrap(), b"keep this");
    assert_no_temporary(directory.path());
}

#[test]
fn direct_and_hard_link_aliases_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.iso");
    let hard_link = directory.path().join("alias.iso");
    let original = fixture(UdfRevision::Nsr03);
    fs::write(&source, &original).unwrap();

    for output in [&source, &hard_link] {
        if output == &hard_link {
            fs::hard_link(&source, output).unwrap();
        }
        let result = run(["patch", path(&source), "-o", path(output)]);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("aliases"));
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_no_temporary(directory.path());
    }
}

#[test]
fn symlink_alias_and_broken_symlink_are_rejected_when_supported() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.iso");
    fs::write(&source, fixture(UdfRevision::Nsr03)).unwrap();

    let alias = directory.path().join("alias.iso");
    if create_file_symlink(&source, &alias).is_ok() {
        let result = run(["patch", path(&source), "-o", path(&alias)]);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("aliases"));
    }

    let broken = directory.path().join("broken.iso");
    if create_file_symlink(&directory.path().join("missing.iso"), &broken).is_ok() {
        let result = run(["patch", path(&source), "-o", path(&broken)]);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("already exists"));
        assert!(fs::symlink_metadata(&broken).is_ok());
    }
    assert_no_temporary(directory.path());
}

#[test]
fn help_version_and_usage_use_clap_exit_behavior() {
    let help = run(["--help"]);
    assert_success(&help);
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
    assert!(help.stderr.is_empty());

    let version = run(["--version"]);
    assert_success(&version);
    let expected_version = format!("esr-disc-patcher {}", env!("CARGO_PKG_VERSION"));
    assert!(String::from_utf8_lossy(&version.stdout).starts_with(&expected_version));
    assert!(version.stderr.is_empty());

    let usage = run(std::iter::empty::<&str>());
    assert_eq!(usage.status.code(), Some(2));
    assert!(usage.stdout.is_empty());
    assert!(String::from_utf8_lossy(&usage.stderr).contains("Usage:"));
}

#[test]
fn missing_output_directory_fails_without_leaving_files() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.iso");
    let output = directory.path().join("missing").join("output.iso");
    fs::write(&source, fixture(UdfRevision::Nsr03)).unwrap();

    let result = run(["patch", path(&source), "-o", path(&output)]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("temporary output"));
    assert!(!output.exists());
    assert_no_temporary(directory.path());
}

#[cfg(unix)]
#[test]
fn output_preserves_unix_permission_bits() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.iso");
    let output = directory.path().join("output.iso");
    fs::write(&source, fixture(UdfRevision::Nsr03)).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();

    let result = run(["patch", path(&source), "-o", path(&output), "--quiet"]);
    assert_success(&result);
    assert_eq!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

fn run<I, S>(arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_esr-disc-patcher"))
        .args(arguments)
        .output()
        .unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().expect("temporary test paths are UTF-8")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_no_temporary(directory: &Path) {
    let leftovers: Vec<PathBuf> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|entry| {
            entry
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(TEMPORARY_PREFIX)
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files remain: {leftovers:?}"
    );
}

fn inspect_file(path: &Path) -> PatchState {
    let mut file = fs::File::open(path).unwrap();
    inspect(&mut file).unwrap().state
}

fn patched_fixture() -> Vec<u8> {
    let mut image = Cursor::new(fixture(UdfRevision::Nsr03));
    patch(&mut image).unwrap();
    image.into_inner()
}

fn fixture(revision: UdfRevision) -> Vec<u8> {
    let mut image = vec![0_u8; FIXTURE_SECTORS * SECTOR_BYTES];
    write_vrs(&mut image, 16, b"BEA01");
    write_vrs(
        &mut image,
        17,
        match revision {
            UdfRevision::Nsr02 => b"NSR02",
            UdfRevision::Nsr03 => b"NSR03",
        },
    );
    write_vrs(&mut image, 18, b"TEA01");

    for sector_number in [34_u64, 50] {
        let descriptor = partition_descriptor(revision, sector_number);
        sector_mut(&mut image, sector_number).copy_from_slice(&descriptor);
    }
    for (index, byte) in sector_mut(&mut image, 200).iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    image
}

fn write_vrs(image: &mut [u8], sector_number: u64, identifier: &[u8; 5]) {
    let descriptor = sector_mut(image, sector_number);
    descriptor[1..6].copy_from_slice(identifier);
    descriptor[6] = 1;
}

fn partition_descriptor(revision: UdfRevision, location: u64) -> [u8; SECTOR_BYTES] {
    let mut descriptor = [0_u8; SECTOR_BYTES];
    descriptor[0..2].copy_from_slice(&5_u16.to_le_bytes());
    descriptor[2..4].copy_from_slice(
        &match revision {
            UdfRevision::Nsr02 => 2_u16,
            UdfRevision::Nsr03 => 3_u16,
        }
        .to_le_bytes(),
    );
    descriptor[6..8].copy_from_slice(&1_u16.to_le_bytes());
    descriptor[10..12].copy_from_slice(&496_u16.to_le_bytes());
    descriptor[12..16].copy_from_slice(&(location as u32).to_le_bytes());
    descriptor[16..20].copy_from_slice(&1_u32.to_le_bytes());
    descriptor[20..22].copy_from_slice(&1_u16.to_le_bytes());
    descriptor[25..31].copy_from_slice(match revision {
        UdfRevision::Nsr02 => b"+NSR02",
        UdfRevision::Nsr03 => b"+NSR03",
    });
    descriptor[184..188].copy_from_slice(&1_u32.to_le_bytes());
    descriptor[188..192].copy_from_slice(&160_u32.to_le_bytes());
    descriptor[192..196].copy_from_slice(&96_u32.to_le_bytes());
    refresh_tag(&mut descriptor);
    descriptor
}

fn refresh_tag(descriptor: &mut [u8; SECTOR_BYTES]) {
    let crc = descriptor_crc(&descriptor[16..512]);
    descriptor[8..10].copy_from_slice(&crc.to_le_bytes());
    descriptor[4] = descriptor[..16]
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 4)
        .fold(0_u8, |sum, (_, byte)| sum.wrapping_add(*byte));
}

fn descriptor_crc(bytes: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn sector_mut(image: &mut [u8], sector_number: u64) -> &mut [u8; SECTOR_BYTES] {
    let start = sector_number as usize * SECTOR_BYTES;
    (&mut image[start..start + SECTOR_BYTES])
        .try_into()
        .unwrap()
}

#[cfg(unix)]
fn create_file_symlink(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn create_file_symlink(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}
