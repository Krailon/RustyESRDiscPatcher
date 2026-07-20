// SPDX-License-Identifier: GPL-3.0-or-later

#![forbid(unsafe_code)]
//! Reusable ESR disc inspection and patching library.
//!
//! The mutation functions operate on a seekable, writable copy of an image.
//! They validate the complete supported UDF layout before writing, modify only
//! the sectors used by the ESR patch format, flush, and verify the final state.
//! A lower-level I/O failure can still leave that supplied stream partially
//! modified, so callers must not pass their only copy of an image.

mod error;
mod image;
mod payload;
mod sector;
mod udf;

pub use error::Error;

use std::io::{Read, Seek, Write};

use image::DetailedState;

/// The only logical sector size supported by the ESR patch format.
pub const LOGICAL_SECTOR_SIZE: u64 = 2_048;

/// The complete ESR patch state of a supported image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchState {
    /// The image is structurally valid and its ESR-reserved sectors are empty.
    Unpatched,
    /// The image contains a complete, internally consistent ESR patch.
    Patched,
    /// The image contains occupied, partial, or contradictory ESR artifacts.
    Inconsistent,
}

/// The UDF revision declared by the image's Volume Recognition Sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UdfRevision {
    /// ECMA-167 second-edition structures (`NSR02`).
    Nsr02,
    /// ECMA-167 third-edition structures (`NSR03`).
    Nsr03,
}

/// Information established by fully validating a supported image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ImageInfo {
    /// The complete ESR patch state.
    pub state: PatchState,
    /// The UDF revision recognized in the Volume Recognition Sequence.
    pub udf_revision: UdfRevision,
    /// The image length in 2048-byte logical sectors.
    pub sector_count: u64,
}

/// Inspect and classify an ESR-compatible DVD image without modifying it.
pub fn inspect<R: Read + Seek + ?Sized>(image: &mut R) -> Result<ImageInfo, Error> {
    Ok(image::analyze(image)?.info())
}

/// Apply the ESR patch to a validated, unpatched image copy.
///
/// Callers are responsible for creating the disposable copy. An I/O failure
/// after writing begins can leave the supplied stream partially modified.
pub fn patch<R: Read + Write + Seek + ?Sized>(image: &mut R) -> Result<(), Error> {
    payload::validate()?;
    let analysis = image::analyze(image)?;

    match analysis.state {
        DetailedState::Unpatched => image::apply_patch(image, &analysis)?,
        DetailedState::Patched => return Err(Error::AlreadyPatched),
        DetailedState::OccupiedReservedSectors => {
            return Err(Error::ReservedSectorsInUse);
        }
        DetailedState::Inconsistent => return Err(Error::InconsistentState),
    }

    verify_state(image, PatchState::Patched)
}

/// Remove a complete ESR patch from a validated image copy.
///
/// Callers are responsible for creating the disposable copy. An I/O failure
/// after writing begins can leave the supplied stream partially modified.
pub fn unpatch<R: Read + Write + Seek + ?Sized>(image: &mut R) -> Result<(), Error> {
    payload::validate()?;
    let analysis = image::analyze(image)?;

    match analysis.state {
        DetailedState::Patched => image::remove_patch(image, &analysis)?,
        DetailedState::Unpatched => return Err(Error::NotPatched),
        DetailedState::OccupiedReservedSectors | DetailedState::Inconsistent => {
            return Err(Error::InconsistentState);
        }
    }

    verify_state(image, PatchState::Unpatched)
}

fn verify_state<R: Read + Seek + ?Sized>(image: &mut R, expected: PatchState) -> Result<(), Error> {
    match image::analyze(image) {
        Ok(analysis) if analysis.info().state == expected => Ok(()),
        Err(Error::Io(error)) => Err(Error::Io(error)),
        Ok(_) | Err(_) => Err(Error::VerificationFailed),
    }
}

#[cfg(test)]
mod tests;
