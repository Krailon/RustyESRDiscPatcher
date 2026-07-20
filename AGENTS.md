# ESR Disc Patcher Project Guide

## Mission

Build a free, open-source ESR disc patcher for the PlayStation 2 homebrew
community. The program must patch, inspect, and unpatch compatible DVD ISO
images without requiring users to install Rust, a framework, a package manager,
or a separate payload file.

The first release is a command-line application. A desktop GUI is a later
milestone and must reuse the same patch engine rather than reimplementing disc
logic.

The intended uses are homebrew software and backups of legally owned media where
local law permits them. Do not present the project as a source of disc images or
copyrighted game data.

## Current State

Milestones 1 and 2 are complete. The repository contains a Rust 2024
application with:

- Package: `esr-disc-patcher-rs`
- User-facing binary name: `esr-disc-patcher`
- Separate library and binary targets
- GPL-3.0-or-later licensing and third-party attribution
- Project documentation and a legal-use disclaimer
- Cross-platform formatting, lint, and test CI
- A checked, reusable UDF inspection and ESR patch engine
- An embedded, integrity-checked 24 KiB DVD-Video metadata payload
- Direct dependencies: `sha2` 0.11 and `thiserror` 2.0

Keep the project as one Cargo package with both a library and a binary until a
real need for a workspace appears. The binary currently exits with a clear
development-status error. Milestone 3, the CLI and safe filesystem workflow,
is next.

## Reference And Licensing

Use
[CaptainSwag101/OpenESRDiscPatcher](https://github.com/CaptainSwag101/OpenESRDiscPatcher)
as a behavioral compatibility reference. ECMA-167 and ECMA TR/71 are the
authoritative sources for UDF validation. Historical patchers are evidence for
the ESR-specific transformation and expected output, not correctness or safety
oracles. OpenESRDiscPatcher is GPL-3.0-or-later software. The Rust project must
also use `GPL-3.0-or-later`, retain attribution for adapted work, and include
the full license and a third-party notices file.

The reference repository includes `dvd_video_data.bin` without a separate
per-file provenance statement. The same bytes were traced to the `dvdvdata`
constant in GPL-3.0-or-later-licensed ESRPATCH v0.24-derived source, which
attributes the original patcher to bootsector/Bruno Freitas. The review and
source lineage are recorded in `THIRD_PARTY_NOTICES.md`, including:

- Source repository and source commit
  `1e490c63b6b0c029b2221e9289a22982af8814a8`
- File name and purpose
- SHA-256
  `d61083e8bc90a959c21958e46216a8531c2095c2f6f780b779d9489f3fd5a845`
- Applicable license, source lineage, and attribution

The payload is vendored at `assets/dvd_video_data.bin`, embedded with
`include_bytes!`, and checked against the recorded length and SHA-256 by its
compile-time type, before mutation, and in tests. Do not silently replace it
with data from an unknown source.

The MIT-licensed `ali-raheem/esrtool` contains a byte-identical payload and was
used only as independent corroboration. No code or licensing material was
copied from it. The archived GPL legacy patcher has known historical memory-
safety and malformed-input bugs, so never execute it on untrusted images or use
it as a validation specification. See `THIRD_PARTY_NOTICES.md` for exact source
revisions and limitations.

## Version 1 Scope

Version 1 supports:

- Windows, Linux, and macOS
- DVD ISO images composed of 2048-byte logical sectors
- The UDF layout used by ESR-compatible PlayStation 2 DVD images
- Read-only inspection
- Non-destructive patching to a new image
- Non-destructive unpatching to a new image

Version 1 does not support:

- In-place modification
- PlayStation 2 CD images
- Raw 2352-byte BIN images or cue sheets
- CHD or other compressed/container formats
- Images with unsupported or ambiguous UDF layouts
- Downloading, burning, ripping, or supplying game images
- A desktop GUI

Reject unsupported inputs with a precise diagnostic. Never guess at a sector
layout and never partially patch an image that failed validation.

## Command-Line Contract

Expose these commands:

```text
esr-disc-patcher inspect INPUT
esr-disc-patcher patch INPUT [-o OUTPUT] [--quiet]
esr-disc-patcher unpatch INPUT [-o OUTPUT] [--quiet]
```

`patch` defaults to `<stem>_patched.iso`. `unpatch` defaults to
`<stem>_unpatched.iso`. Preserve the input's parent directory and extension.

The CLI contract is:

- `inspect` prints exactly `patched`, `unpatched`, or `inconsistent` to stdout.
- `patched` and `unpatched` inspection results exit successfully.
- `inconsistent`, unsupported, or malformed images produce a nonzero exit.
- Usage errors use Clap's exit behavior. Operational errors use exit code 1.
- Diagnostics and progress belong on stderr; machine-readable state belongs on
  stdout.
- `--quiet` suppresses progress and success messages, but never errors.
- `--help` and `--version` must work without opening an image.
- Existing output paths are rejected. Version 1 has no overwrite flag.
- An output that aliases the input, including a hard link or resolved symlink,
  is rejected.

Use `clap` for argument parsing. Keep CLI presentation and filesystem workflow
out of the patch engine.

## Core Architecture

`src/lib.rs` owns the reusable patch engine. `src/main.rs` owns argument
parsing, user messages, streamed copying, temporary output management, and exit
status. Keep the implementation separated into small modules for image layout,
UDF descriptor handling, payload data, and errors; do not expose those modules
unless callers need them.

The library's stable surface should be equivalent to:

```rust
pub enum PatchState {
    Unpatched,
    Patched,
    Inconsistent,
}

pub enum UdfRevision {
    Nsr02,
    Nsr03,
}

pub struct ImageInfo {
    pub state: PatchState,
    pub udf_revision: UdfRevision,
    pub sector_count: u64,
}

pub fn inspect<R: Read + Seek>(image: &mut R) -> Result<ImageInfo, Error>;
pub fn patch<R: Read + Write + Seek>(image: &mut R) -> Result<(), Error>;
pub fn unpatch<R: Read + Write + Seek>(image: &mut R) -> Result<(), Error>;
```

The mutation functions operate on an already-created output copy. They must run
the same preflight checks as `inspect`, mutate only documented sectors, flush
their writes, and inspect the result before returning success.

Use structured errors covering at least I/O failure, invalid image length,
missing UDF recognition, unsupported descriptor layout, wrong current state,
inconsistent/partial patch state, occupied reserved sectors, arithmetic bounds,
and invalid embedded payload. Use `thiserror` unless the standard library alone
remains clearer.

No operation may load a multi-gigabyte ISO into memory. Use fixed-size sector
buffers and checked `u64` offset arithmetic.

## Image Recognition And Patch Format

Use 2048 bytes as the only supported logical sector size. Before seeking or
writing, require the file length to be a multiple of 2048 and large enough to
contain every accessed sector, including sectors 128 through 139.

Preflight validation must:

1. Recognize a valid UDF Volume Recognition Sequence containing `NSR02` or
   `NSR03` in the bounded descriptor area beginning at sector 16.
2. Parse and validate the UDF descriptor tags at sectors 34 and 50, including
   tag checksum, CRC length bounds, and descriptor CRC.
3. Require both descriptors to be compatible Partition Descriptors and to agree
   on the relevant partition location.
4. Classify the complete patch state using both backup sectors, both live
   descriptors, and the embedded payload range. A single marker is insufficient.
5. For an unpatched image, require backup sectors 14 and 15 and payload sectors
   128 through 139 to be empty before declaring the image patchable. This makes
   reversal lossless.

The patch operation must perform these exact logical changes:

1. Copy sector 34 to backup sector 14.
2. Copy sector 50 to backup sector 15.
3. Set the little-endian 32-bit partition starting location at descriptor byte
   offset `0xBC` in sectors 34 and 50 to sector 128.
4. Recalculate each descriptor's CRC-ITU-T (CRC-16/CCITT polynomial `0x1021`,
   initial value zero, no reflection) across the descriptor-defined CRC range.
5. Recalculate each UDF descriptor tag checksum across bytes 0 through 15 while
   excluding checksum byte 4.
6. Write the 24 KiB DVD-Video payload into sectors 128 through 139.
7. Flush and re-inspect the image; report success only when it classifies as
   `Patched`.

The unpatch operation must accept only a complete, internally consistent patched
state. It must restore sectors 34 and 50 from sectors 14 and 15, zero sectors 14,
15, and 128 through 139, flush, and verify the resulting `Unpatched` state.

Use named constants for all sector numbers, field offsets, lengths, and UDF tag
values. Comments should explain the disc-format reason for an offset, not repeat
what the code already says.

## Output Safety

The source ISO is read-only for the entire command. The CLI must:

1. Validate the source before copying.
2. Create a uniquely named temporary file in the output directory.
3. Stream the source into it using a bounded buffer.
4. Apply the requested operation to the temporary copy.
5. Flush file contents and re-open or rewind the file for post-write inspection.
6. Publish it with a no-clobber rename only after verification succeeds.
7. Remove the temporary file after every handled failure.

Use a cross-platform temporary-file abstraction such as `tempfile` and its
no-clobber persistence API. A crash may leave an identifiable temporary file,
but it must never leave a partial file at the requested output path.

Do not add `unsafe` code without a documented requirement and maintainer review.
Prefer the standard library over a full ISO/UDF library because version 1 needs
only bounded validation of a fixed compatibility layout.

## Testing Strategy

Do not commit commercial or otherwise non-redistributable disc images. Tests
must construct small synthetic images or use fixtures whose redistribution terms
are recorded.

Unit tests must cover:

- CRC-ITU-T known vectors and UDF tag checksums
- Little-endian field parsing and checked sector-to-byte conversion
- `NSR02` and `NSR03` recognition
- Valid and invalid UDF descriptor tags and CRC lengths
- Truncated, non-sector-aligned, and too-small inputs
- Patched, unpatched, and every partial/inconsistent state
- Payload length and SHA-256

Integration tests must cover:

- Patch, inspect, and unpatch command behavior
- Default and explicit output naming
- Existing output rejection and input/output alias rejection
- Cleanup after validation, copy, patch, and verification failures
- `--quiet`, stdout/stderr separation, help, version, and exit codes
- A patch/unpatch round trip whose final SHA-256 equals the original
- A sector-level diff proving that patching changes only sectors 14, 15, 34, 50,
  and 128 through 139
- Byte-for-byte comparison with reference-patcher output on a legal,
  redistributable compatibility fixture

Add fuzz or property tests for descriptor parsing and truncation once the core
engine is stable. Fuzz input must never cause panics, unbounded allocation, or
out-of-range seeks.

Run these checks before accepting changes:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --all-features --no-deps
```

## Delivery Plan

### Milestone 1: Foundation (Complete)

- Add GPL-3.0-or-later licensing, attribution, project README, and legal-use
  disclaimer.
- Resolve and record payload provenance before embedding or releasing it.
- Establish the library/binary split and CI formatting, lint, and test jobs.
- Keep dependencies minimal and commit `Cargo.lock` for the application.

### Milestone 2: Patch Engine (Complete)

- Implement checked sector I/O, UDF recognition, descriptor parsing, checksum
  routines, and complete patch-state classification.
- Implement patch and unpatch operations against seekable in-memory fixtures.
- Establish differential compatibility fixtures against the reference tool.

### Milestone 3: CLI And Safe Filesystem Workflow

- Implement the three-command CLI and documented output naming.
- Add streaming copy, temporary output, no-clobber persistence, progress, and
  cleanup behavior.
- Complete end-to-end and failure-path tests on all target operating systems.

### Milestone 4: Standalone Releases

- Build an x86_64 Windows executable, a statically linked x86_64 Linux musl
  executable, and x86_64 plus ARM64 macOS executables.
- Package each executable with the license, notices, and concise usage README.
- Publish tagged GitHub releases with SHA-256 checksum files and source archives.
- Signing and notarization are optional until credentials exist; no release may
  require an application runtime or separate payload download.

### Milestone 5: Desktop GUI

- Select a cross-platform Rust GUI toolkit only after the library API and CLI
  behavior are stable.
- Reuse the library and safe output workflow exactly.
- Preserve standalone distribution and all validation/error detail.

## Contribution Rules

- Keep changes scoped to the current milestone and established module boundary.
- Do not duplicate patch logic in the CLI, tests, or future GUI.
- Add tests for every behavior change and malformed-input fix.
- Avoid unrelated refactors and dependency additions without a concrete benefit.
- Keep user-facing errors actionable and avoid leaking temporary implementation
  details.
- Update this document when product scope, compatibility rules, CLI contracts,
  release targets, or safety invariants change.
- Never commit proprietary images, keys, firmware, BIOS data, or unverified
  binary payloads.
