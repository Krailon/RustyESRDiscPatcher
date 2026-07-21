# Third-Party Notices

This document records the source and licensing basis for compatibility material
and Rust dependencies used by ESR Disc Patcher. The DVD-Video metadata described
below is stored unmodified at `assets/dvd_video_data.bin` and embedded into the
program at compile time.

## Format Standards And Reference Policy

UDF parsing and validation are implemented from ECMA-167, third edition, and
ECMA TR/71, the DVD read-only disk UDF Bridge specification:

- ECMA-167: <https://ecma-international.org/publications-and-standards/standards/ecma-167/>
- ECMA TR/71: <https://ecma-international.org/publications-and-standards/technical-reports/ecma-tr-71/>

These standards are the correctness authority. The historical tools below are
used only for payload provenance, ESR format compatibility, and known-good
output comparison. They are not executed by the application or test suite.

## ESR Patch Algorithm And DVD-Video Metadata

The ESR disc patch format and its 24 KiB DVD-Video metadata payload originate
from earlier ESR patcher work.

### Original Attribution

The legacy source identifies the implementation as:

```text
ESRPATCH v0.24 - PS2 ISO patcher for ffgriever's ESR project
By: bootsector - http://www.brunofreitas.com/
```

That attribution identifies bootsector, also named Bruno Freitas, as the author
of ESRPATCH v0.24. The payload is present as the `dvdvdata` constant within the
GPL-3.0-or-later-licensed `Patcher.cpp` source:

- Project: `ali-raheem/esrtool-legacy`
- Source: <https://github.com/ali-raheem/esrtool-legacy/blob/b5062e732028a43d400db38899ff52e3c3a7bb34/Patcher.cpp>
- License: GNU General Public License, version 3 or later

The archived legacy implementation is not treated as safe for untrusted input.
Its history includes an out-of-bounds descriptor CRC read fixed in revision
`dde7afb09a5682b4980b890d6a2f2ac7ff02342c` and an unpatch stack over-read fixed
in revision `10f165095140b133006e0f79517adfe9340f829d`. The Rust implementation uses
checked reads, explicit little-endian decoding, bounded CRC lengths, complete
state validation, and disposable output copies instead of porting that code.

### OpenESRDiscPatcher Reference

This project uses James Pelster's OpenESRDiscPatcher as its primary behavioral
reference:

- Project: <https://github.com/CaptainSwag101/OpenESRDiscPatcher>
- Copyright: 2020-2022 James Pelster
- License: GNU General Public License, version 3 or later
- Reviewed revision: `1e490c63b6b0c029b2221e9289a22982af8814a8`
- Payload introduction: `b3fbfc7ca50a29ec1f3bae80dcb9594e2d3d47ea`
- Payload blob: `055b2ca59017cc3897a23c95ea1738a8a1ccb881`

OpenESRDiscPatcher credits its implementation as based on code by jolek and
bootsector from PSX-Place.com.

### Independent MIT Corroboration

The MIT-licensed Rust project `ali-raheem/esrtool` also contains the same 24 KiB
payload:

- Project: <https://github.com/ali-raheem/esrtool>
- License: MIT
- Payload identity: byte-for-byte equal to the GPL sources and SHA-256 below

No source code or other material from this MIT project is incorporated here.
It was reviewed only as independent corroboration and is not relied on to
relicense the payload or the earlier GPL compatibility work.

### Payload Identity And Use

The payload represented by the legacy `dvdvdata` constant is byte-for-byte
identical to OpenESRDiscPatcher's `dvd_video_data.bin`:

- Size: 24,576 bytes, or 12 logical sectors of 2,048 bytes
- SHA-256:
  `d61083e8bc90a959c21958e46216a8531c2095c2f6f780b779d9489f3fd5a845`
- OpenESRDiscPatcher source:
  <https://github.com/CaptainSwag101/OpenESRDiscPatcher/blob/1e490c63b6b0c029b2221e9289a22982af8814a8/dvd_video_data.bin>

ESR Disc Patcher redistributes the bytes unmodified under GNU GPL version 3 or
later, preserves these notices, and embeds them directly into the executable so
they are not a runtime dependency. Automated checks enforce both exact length
and SHA-256 before patching.

The differential compatibility test uses a deterministic synthetic NSR03 image.
Its patched output SHA-256 is
`fb8078516e02e09e9f5f2ae38b6530c54409baa746834fc6d8e2e9275e71a2f4`, generated
with `esrtool-legacy` v0.25.3 revision
`10f165095140b133006e0f79517adfe9340f829d`. The legacy executable is not stored,
built, or run by this repository.

## Rust Dependencies

The following direct dependencies are compiled into the program:

- `clap` 4.6.3, MIT OR Apache-2.0
- `indicatif` 0.18.6, MIT
- `same-file` 1.0.6, MIT or the Unlicense
- `sha2` 0.11.0, MIT OR Apache-2.0
- `tempfile` 3.27.0, MIT OR Apache-2.0
- `thiserror` 2.0.19, MIT OR Apache-2.0

Their complete native and target-specific dependency graph is pinned in
`Cargo.lock`. The graph uses these additional license groups:

- MIT OR Apache-2.0: `anstream`, `anstyle`, `anstyle-parse`, `anstyle-query`,
  `anstyle-wincon`, `bitflags`, `block-buffer`, `bumpalo`, `cfg-if`,
  `clap_builder`, `clap_derive`, `clap_lex`, `colorchoice`, `const-oid`,
  `cpufeatures`, `crypto-common`, `digest`, `encode_unicode`, `errno`,
  `fastrand`, `futures-core`, `futures-task`, `futures-util`, `getrandom`,
  `heck`, `hybrid-array`, `is_terminal_polyfill`, `js-sys`, `libc`, `once_cell`,
  `once_cell_polyfill`, `pin-project-lite`, `portable-atomic`, `proc-macro2`,
  `quote`, `rustversion`, `syn`, `thiserror-impl`, `typenum`, `unicode-width`,
  `utf8parse`, `wasm-bindgen`, `wasm-bindgen-macro`,
  `wasm-bindgen-macro-support`, `wasm-bindgen-shared`, `web-time`,
  `windows-link`, and `windows-sys`
- MIT: `console`, `slab`, `strsim`, and `unit-prefix`
- MIT or the Unlicense: `winapi-util`
- Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT: `linux-raw-sys` and
  `rustix`
- MIT OR Apache-2.0 OR LGPL-2.1-or-later: `r-efi`
- `(MIT OR Apache-2.0) AND Unicode-3.0`: `unicode-ident`

Source and complete license texts are available through each package's entry at
<https://crates.io/>.

## No Sony Or Game Content

The payload is UDF/DVD-Video filesystem metadata used to identify an ESR disc as
DVD-Video media. This project does not include Sony firmware, BIOS data,
encryption keys, game executables, or disc images.
