// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::{Read, Seek, Write};

use crate::payload;
use crate::sector::{self, SECTOR_BYTES};
use crate::udf::{self, PartitionDescriptor};
use crate::{Error, ImageInfo, PatchState, UdfRevision};

pub(crate) const FIRST_BACKUP_SECTOR: u64 = 14;
pub(crate) const SECOND_BACKUP_SECTOR: u64 = 15;
pub(crate) const FIRST_DESCRIPTOR_SECTOR: u64 = 34;
pub(crate) const SECOND_DESCRIPTOR_SECTOR: u64 = 50;
pub(crate) const PAYLOAD_FIRST_SECTOR: u64 = 128;
pub(crate) const PAYLOAD_SECTOR_COUNT: u64 = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DetailedState {
    Unpatched,
    Patched,
    OccupiedReservedSectors,
    Inconsistent,
}

pub(crate) struct Analysis {
    pub(crate) sector_count: u64,
    pub(crate) revision: UdfRevision,
    pub(crate) state: DetailedState,
    live_descriptors: [[u8; SECTOR_BYTES]; 2],
    backup_descriptors: [[u8; SECTOR_BYTES]; 2],
}

impl Analysis {
    pub(crate) fn info(&self) -> ImageInfo {
        let state = match self.state {
            DetailedState::Unpatched => PatchState::Unpatched,
            DetailedState::Patched => PatchState::Patched,
            DetailedState::OccupiedReservedSectors | DetailedState::Inconsistent => {
                PatchState::Inconsistent
            }
        };
        ImageInfo {
            state,
            udf_revision: self.revision,
            sector_count: self.sector_count,
        }
    }
}

pub(crate) fn analyze<R: Read + Seek + ?Sized>(image: &mut R) -> Result<Analysis, Error> {
    let sector_count = sector::image_sector_count(image)?;
    let revision = udf::recognize(image)?;

    let live_descriptors = [
        sector::read(image, FIRST_DESCRIPTOR_SECTOR)?,
        sector::read(image, SECOND_DESCRIPTOR_SECTOR)?,
    ];
    let parsed_live = [
        udf::parse_partition_descriptor(&live_descriptors[0], FIRST_DESCRIPTOR_SECTOR, revision)?,
        udf::parse_partition_descriptor(&live_descriptors[1], SECOND_DESCRIPTOR_SECTOR, revision)?,
    ];

    let live_descriptors_match =
        udf::equivalent_except_partition_start(&live_descriptors[0], &live_descriptors[1]);
    validate_partition_bounds(parsed_live[0], sector_count)?;
    validate_partition_bounds(parsed_live[1], sector_count)?;

    let backup_descriptors = [
        sector::read(image, FIRST_BACKUP_SECTOR)?,
        sector::read(image, SECOND_BACKUP_SECTOR)?,
    ];
    let payload_bytes = read_payload(image)?;
    let backups_empty = backup_descriptors.iter().all(|bytes| is_zero(bytes));
    let payload_empty = is_zero(&payload_bytes);

    let state = if !live_descriptors_match {
        DetailedState::Inconsistent
    } else if parsed_live[0].start == parsed_live[1].start
        && parsed_live[0].start != PAYLOAD_FIRST_SECTOR as u32
    {
        if parsed_live[0].start < PAYLOAD_FIRST_SECTOR as u32 + PAYLOAD_SECTOR_COUNT as u32 {
            return Err(Error::UnsupportedLayout {
                reason: "the partition overlaps sectors reserved by the ESR patch format",
            });
        }
        if backups_empty && payload_empty {
            DetailedState::Unpatched
        } else {
            DetailedState::OccupiedReservedSectors
        }
    } else if is_complete_patch(
        &live_descriptors,
        &parsed_live,
        &backup_descriptors,
        &payload_bytes,
        revision,
        sector_count,
    ) {
        DetailedState::Patched
    } else {
        DetailedState::Inconsistent
    };

    Ok(Analysis {
        sector_count,
        revision,
        state,
        live_descriptors,
        backup_descriptors,
    })
}

fn is_complete_patch(
    live: &[[u8; SECTOR_BYTES]; 2],
    parsed_live: &[PartitionDescriptor; 2],
    backups: &[[u8; SECTOR_BYTES]; 2],
    payload_bytes: &[u8; payload::LENGTH],
    revision: UdfRevision,
    sector_count: u64,
) -> bool {
    if parsed_live
        .iter()
        .any(|descriptor| descriptor.start != PAYLOAD_FIRST_SECTOR as u32)
        || payload_bytes != payload::BYTES
        || backups.iter().any(|bytes| is_zero(bytes))
    {
        return false;
    }

    let parsed_backups = match (
        udf::parse_partition_descriptor(&backups[0], FIRST_DESCRIPTOR_SECTOR, revision),
        udf::parse_partition_descriptor(&backups[1], SECOND_DESCRIPTOR_SECTOR, revision),
    ) {
        (Ok(first), Ok(second)) => [first, second],
        _ => return false,
    };

    if parsed_backups[0].start != parsed_backups[1].start
        || parsed_backups[0].start < PAYLOAD_FIRST_SECTOR as u32 + PAYLOAD_SECTOR_COUNT as u32
        || !udf::equivalent_except_partition_start(&backups[0], &backups[1])
        || parsed_backups
            .iter()
            .any(|descriptor| validate_partition_bounds(*descriptor, sector_count).is_err())
    {
        return false;
    }

    backups.iter().zip(live).all(|(backup, live)| {
        let mut normalized = *backup;
        udf::set_partition_start(&mut normalized, PAYLOAD_FIRST_SECTOR as u32);
        normalized == *live
    })
}

fn validate_partition_bounds(
    descriptor: PartitionDescriptor,
    sector_count: u64,
) -> Result<(), Error> {
    let end = u64::from(descriptor.start) + u64::from(descriptor.length);
    if end > sector_count {
        return Err(Error::PartitionOutOfBounds {
            start: descriptor.start,
            length: descriptor.length,
            sector_count,
        });
    }
    Ok(())
}

fn read_payload<R: Read + Seek + ?Sized>(image: &mut R) -> Result<[u8; payload::LENGTH], Error> {
    let mut bytes = [0; payload::LENGTH];
    for index in 0..PAYLOAD_SECTOR_COUNT {
        let sector_bytes = sector::read(image, PAYLOAD_FIRST_SECTOR + index)?;
        let start = usize::try_from(index).expect("payload sector index fits usize") * SECTOR_BYTES;
        bytes[start..start + SECTOR_BYTES].copy_from_slice(&sector_bytes);
    }
    Ok(bytes)
}

fn is_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

pub(crate) fn apply_patch<R: Write + Seek + ?Sized>(
    image: &mut R,
    analysis: &Analysis,
) -> Result<(), Error> {
    let backups = analysis.live_descriptors;
    let mut patched_descriptors = analysis.live_descriptors;
    for descriptor in &mut patched_descriptors {
        udf::set_partition_start(descriptor, PAYLOAD_FIRST_SECTOR as u32);
    }

    sector::write(image, FIRST_BACKUP_SECTOR, &backups[0])?;
    sector::write(image, SECOND_BACKUP_SECTOR, &backups[1])?;
    sector::write(image, FIRST_DESCRIPTOR_SECTOR, &patched_descriptors[0])?;
    sector::write(image, SECOND_DESCRIPTOR_SECTOR, &patched_descriptors[1])?;
    write_payload(image, payload::BYTES)?;
    image.flush()?;
    Ok(())
}

pub(crate) fn remove_patch<R: Write + Seek + ?Sized>(
    image: &mut R,
    analysis: &Analysis,
) -> Result<(), Error> {
    let zero_sector = [0; SECTOR_BYTES];

    sector::write(
        image,
        FIRST_DESCRIPTOR_SECTOR,
        &analysis.backup_descriptors[0],
    )?;
    sector::write(
        image,
        SECOND_DESCRIPTOR_SECTOR,
        &analysis.backup_descriptors[1],
    )?;
    sector::write(image, FIRST_BACKUP_SECTOR, &zero_sector)?;
    sector::write(image, SECOND_BACKUP_SECTOR, &zero_sector)?;
    for index in 0..PAYLOAD_SECTOR_COUNT {
        sector::write(image, PAYLOAD_FIRST_SECTOR + index, &zero_sector)?;
    }
    image.flush()?;
    Ok(())
}

fn write_payload<W: Write + Seek + ?Sized>(
    image: &mut W,
    bytes: &[u8; payload::LENGTH],
) -> Result<(), Error> {
    for index in 0..PAYLOAD_SECTOR_COUNT {
        let start = usize::try_from(index).expect("payload sector index fits usize") * SECTOR_BYTES;
        let sector_bytes: &[u8; SECTOR_BYTES] = bytes[start..start + SECTOR_BYTES]
            .try_into()
            .expect("payload length is an exact number of sectors");
        sector::write(image, PAYLOAD_FIRST_SECTOR + index, sector_bytes)?;
    }
    Ok(())
}
