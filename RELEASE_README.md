# ESR Disc Patcher

ESR Disc Patcher inspects, patches, and unpatches compatible PlayStation 2 DVD
ISO images for use with an ESR loader. The executable is standalone: the ESR
payload is embedded and no application runtime or separate data download is
required.

## Requirements

- A compatible DVD ISO containing 2048-byte logical sectors
- Windows 10 or later on x86_64, Linux on x86_64, or a matching Intel/Apple
  Silicon macOS release
- Enough free space for a complete output copy of the source image

CD images, raw 2352-byte BIN images, cue sheets, CHD files, and other compressed
containers are not supported.

## Verify The Download

Verify the archive against the accompanying `SHA256SUMS` file before extracting
or running it.

On Linux:

```sh
sha256sum --ignore-missing --check SHA256SUMS
```

On macOS, compare the result for the downloaded archive with `SHA256SUMS`:

```sh
shasum -a 256 rusty-esr-disc-patcher-*.tar.gz
```

On Windows PowerShell:

```powershell
Get-FileHash .\rusty-esr-disc-patcher-*.zip -Algorithm SHA256
```

The Windows and macOS executables are currently unsigned, and the macOS
executables are not notarized. The operating system may therefore show its
standard warning for software downloaded from the Internet.

## Usage

Run these commands from a terminal in the directory containing the executable:

```text
esr-disc-patcher inspect INPUT
esr-disc-patcher patch INPUT [-o OUTPUT] [--quiet]
esr-disc-patcher unpatch INPUT [-o OUTPUT] [--quiet]
```

On Windows, use `esr-disc-patcher.exe`. On Linux or macOS, use
`./esr-disc-patcher` if the current directory is not searched for commands.

`inspect` prints `patched`, `unpatched`, or `inconsistent`. Patch and unpatch
always create a new image and never modify the source. Without `-o`, the output
is named `<stem>_patched.<ext>` or `<stem>_unpatched.<ext>` beside the input.
Existing output paths are never overwritten. Use `--quiet` to suppress progress
and success messages; errors are always displayed.

Keep an independently verified archival image. No software can protect against
every storage, hardware, or power failure.

## License And Attribution

ESR Disc Patcher is licensed under the GNU General Public License, version 3 or
later. See `LICENSE` and `THIRD_PARTY_NOTICES.md` in this archive.

This project is not affiliated with or endorsed by Sony Interactive
Entertainment. PlayStation is a trademark of Sony Interactive Entertainment.
