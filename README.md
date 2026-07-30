# ESR Disc Patcher

ESR Disc Patcher is a cross-platform tool for inspecting, patching, and
unpatching compatible PlayStation 2 DVD ISO images for use with an ESR loader.
It is written in Rust and is intended to ship as standalone executables with no
application runtime, package manager, or separate payload download.

## Project Status

The command-line application, reusable Rust patch engine, and standalone
release automation are implemented and tested. The tool performs strict UDF
validation, complete patch-state inspection, failure-safe output copying,
patching, and byte-exact unpatching.

## Usage

The command-line interface provides these commands:

```text
esr-disc-patcher inspect INPUT
esr-disc-patcher patch INPUT [-o OUTPUT] [--quiet]
esr-disc-patcher unpatch INPUT [-o OUTPUT] [--quiet]
```

`inspect` prints exactly `patched`, `unpatched`, or `inconsistent`. Patch and
unpatch default to `<stem>_patched.<ext>` and `<stem>_unpatched.<ext>` in the
input directory. Inputs without an extension use `.iso`. Existing outputs are
never overwritten.

Patch and unpatch show an interactive byte progress bar on stderr. Use
`--quiet` to suppress progress and success messages; errors are always shown.

## Releases

Stable tags matching the version in `Cargo.toml` publish standalone archives
for x86_64 Windows, statically linked x86_64 Linux, Intel macOS, and Apple
Silicon macOS. Each archive contains the executable, license, third-party
notices, and a concise usage guide. Releases also include a source archive and
`SHA256SUMS` covering every attached project archive.

Windows releases support Windows 10 or later. Intel macOS releases target macOS
10.13 or later, and Apple Silicon releases target macOS 11.0 or later. Windows
and macOS binaries are currently unsigned, and macOS binaries are not notarized.

## Goals

- Support Windows, Linux, and macOS from one Rust codebase.
- Accept compatible DVD ISO images with 2048-byte logical sectors.
- Inspect patch state without modifying the source image.
- Patch and unpatch into a new output image, never in place.
- Verify image structure and the final output before reporting success.
- Distribute standalone release binaries without external runtime dependencies.

Raw-sector BIN images, PlayStation 2 CD images, CHD files, and other compressed
containers are outside the first release's scope.

## Data Safety

The patching workflow treats the source image as read-only. It creates a
temporary copy in the destination directory, modifies and verifies that copy,
synchronizes it, then publishes it without overwriting an existing path. Direct
paths, symbolic links, and hard links that alias the input are rejected.
Unsupported, truncated, or inconsistent images are rejected before copying.

Keep an independently verified archival image even when using tools designed to
be reversible. No software can protect against every storage, hardware, or
power failure.

## Development

Building from source requires the current stable Rust toolchain with `rustfmt`
and Clippy.

```sh
cargo build --locked
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --all-features --no-deps
```

The project guide and implementation roadmap are in [AGENTS.md](AGENTS.md).
The library API exposes `inspect`, `patch`, and `unpatch` for seekable streams.
Library mutation functions still require a disposable writable copy; the CLI
provides the failure-safe filesystem workflow.

## Roadmap

1. Project foundation, licensing, provenance, crate structure, and CI.
2. Checked UDF inspection and the reusable patch engine. (Complete)
3. Command-line interface and failure-safe output workflow. (Complete)
4. Standalone release archives for supported platforms. (Complete)
5. A desktop GUI using the same patch engine.

## Legal And Attribution

This software is intended for homebrew applications and backups of legally
owned media where local law permits them. It does not provide disc images,
console firmware, encryption keys, or copyrighted game content. Users are
responsible for complying with the laws that apply to them.

This project is not affiliated with or endorsed by Sony Interactive
Entertainment. PlayStation is a trademark of Sony Interactive Entertainment.

The compatibility design is based on the GPL-licensed
[OpenESRDiscPatcher](https://github.com/CaptainSwag101/OpenESRDiscPatcher) and
earlier ESR patcher work. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
for provenance and attribution details.

ESR Disc Patcher is licensed under the GNU General Public License, version 3 or
later. See [LICENSE](LICENSE).
