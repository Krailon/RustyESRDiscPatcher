# ESR Disc Patcher

ESR Disc Patcher is a cross-platform tool for inspecting, patching, and
unpatching compatible PlayStation 2 DVD ISO images for use with an ESR loader.
It is written in Rust and is intended to ship as standalone executables with no
application runtime, package manager, or separate payload download.

## Project Status

This project is in early development. The reusable Rust patch engine is
implemented and tested, including strict UDF validation, complete patch-state
inspection, patching, and byte-exact unpatching. The command-line and safe file
copy workflow are the next milestone, so the binary still exits with a clear
development-status error.

The first usable command-line release will provide these commands:

```text
esr-disc-patcher inspect INPUT
esr-disc-patcher patch INPUT [-o OUTPUT] [--quiet]
esr-disc-patcher unpatch INPUT [-o OUTPUT] [--quiet]
```

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

The planned patching workflow treats the source image as read-only. It creates a
temporary copy in the destination directory, modifies and verifies that copy,
then publishes it without overwriting an existing path. Unsupported, truncated,
or inconsistent images will be rejected before patching.

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
```

The project guide and implementation roadmap are in [AGENTS.md](AGENTS.md).
The library API currently exposes `inspect`, `patch`, and `unpatch` for seekable
streams. Mutation functions must receive a disposable writable copy; they do
not provide the filesystem safety workflow planned for the CLI.

## Roadmap

1. Project foundation, licensing, provenance, crate structure, and CI.
2. Checked UDF inspection and the reusable patch engine. (Complete)
3. Command-line interface and failure-safe output workflow.
4. Standalone release archives for supported platforms.
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
