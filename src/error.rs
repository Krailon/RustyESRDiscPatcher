// SPDX-License-Identifier: GPL-3.0-or-later

use std::io;

/// An image validation, patch-state, or I/O failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The underlying image stream could not be read, written, sought, or flushed.
    #[error("image I/O failed: {0}")]
    Io(#[from] io::Error),

    /// The stream length is not a whole number of logical sectors.
    #[error("image length {bytes} is not a multiple of 2048 bytes")]
    InvalidImageLength { bytes: u64 },

    /// The image does not contain every sector needed by the patch format.
    #[error("image has {sector_count} sectors; at least {required_sectors} sectors are required")]
    ImageTooSmall {
        sector_count: u64,
        required_sectors: u64,
    },

    /// A logical sector could not be represented as a byte offset.
    #[error("byte offset for logical sector {sector} overflows u64")]
    ArithmeticOverflow { sector: u64 },

    /// No supported, complete UDF Volume Recognition Sequence was found.
    #[error("a complete NSR02 or NSR03 UDF Volume Recognition Sequence was not found")]
    MissingUdfRecognition,

    /// A UDF recognition descriptor was present but malformed or contradictory.
    #[error("invalid UDF Volume Recognition Sequence at sector {sector}: {reason}")]
    InvalidVolumeRecognition { sector: u64, reason: &'static str },

    /// A required UDF descriptor was malformed.
    #[error("invalid UDF Partition Descriptor at sector {sector}: {reason}")]
    InvalidDescriptor { sector: u64, reason: &'static str },

    /// The image is valid UDF but does not use the fixed layout supported here.
    #[error("unsupported UDF layout: {reason}")]
    UnsupportedLayout { reason: &'static str },

    /// A declared partition extends beyond the image.
    #[error(
        "partition at sector {start} with length {length} exceeds the {sector_count}-sector image"
    )]
    PartitionOutOfBounds {
        start: u32,
        length: u32,
        sector_count: u64,
    },

    /// An unpatched image already contains data in sectors reserved for ESR.
    #[error("sectors reserved for the ESR patch already contain data")]
    ReservedSectorsInUse,

    /// Patching was requested for an already patched image.
    #[error("image is already patched")]
    AlreadyPatched,

    /// Unpatching was requested for an unpatched image.
    #[error("image is not patched")]
    NotPatched,

    /// Patch-related sectors contain an incomplete or contradictory state.
    #[error("image contains an incomplete or inconsistent ESR patch")]
    InconsistentState,

    /// The payload compiled into this executable failed its integrity check.
    #[error("embedded ESR DVD-Video payload failed its integrity check")]
    InvalidEmbeddedPayload,

    /// The post-write inspection did not establish the requested state.
    #[error("post-write verification did not establish the requested patch state")]
    VerificationFailed,
}
