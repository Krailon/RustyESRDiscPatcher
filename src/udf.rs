// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::{Read, Seek};

use crate::sector::{self, SECTOR_BYTES};
use crate::{Error, UdfRevision};

pub(crate) const PARTITION_START_OFFSET: usize = 0xBC;
const PARTITION_LENGTH_OFFSET: usize = 0xC0;
const DESCRIPTOR_TAG_LENGTH: usize = 16;
const PARTITION_DESCRIPTOR_BODY_LENGTH: usize = 496;
const PARTITION_DESCRIPTOR_LENGTH: usize = DESCRIPTOR_TAG_LENGTH + PARTITION_DESCRIPTOR_BODY_LENGTH;
const PARTITION_DESCRIPTOR_TAG_ID: u16 = 5;
const VRS_FIRST_SECTOR: u64 = 16;
const VRS_END_SECTOR_EXCLUSIVE: u64 = 80;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PartitionDescriptor {
    pub(crate) start: u32,
    pub(crate) length: u32,
}

pub(crate) fn recognize<R: Read + Seek + ?Sized>(image: &mut R) -> Result<UdfRevision, Error> {
    let mut in_extended_area = false;
    let mut revision = None;

    for sector_number in VRS_FIRST_SECTOR..VRS_END_SECTOR_EXCLUSIVE {
        let descriptor = sector::read(image, sector_number)?;
        let identifier = &descriptor[1..6];

        match identifier {
            b"BEA01" => {
                validate_boundary_descriptor(&descriptor, sector_number)?;
                if in_extended_area || revision.is_some() {
                    return Err(Error::InvalidVolumeRecognition {
                        sector: sector_number,
                        reason: "nested or repeated BEA01 descriptor",
                    });
                }
                in_extended_area = true;
            }
            b"NSR02" | b"NSR03" if in_extended_area => {
                validate_nsr_descriptor(&descriptor, sector_number)?;
                if revision.is_some() {
                    return Err(Error::InvalidVolumeRecognition {
                        sector: sector_number,
                        reason: "multiple NSR descriptors in one extended area",
                    });
                }
                revision = Some(if identifier == b"NSR02" {
                    UdfRevision::Nsr02
                } else {
                    UdfRevision::Nsr03
                });
            }
            b"TEA01" if in_extended_area => {
                validate_boundary_descriptor(&descriptor, sector_number)?;
                return revision.ok_or(Error::InvalidVolumeRecognition {
                    sector: sector_number,
                    reason: "TEA01 appears before an NSR descriptor",
                });
            }
            _ => {}
        }
    }

    Err(Error::MissingUdfRecognition)
}

fn validate_boundary_descriptor(descriptor: &[u8; SECTOR_BYTES], sector: u64) -> Result<(), Error> {
    if descriptor[0] != 0 || descriptor[6] != 1 {
        return Err(Error::InvalidVolumeRecognition {
            sector,
            reason: "BEA01 and TEA01 require structure type 0 and version 1",
        });
    }
    if descriptor[7..].iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidVolumeRecognition {
            sector,
            reason: "BEA01 or TEA01 reserved bytes are not zero",
        });
    }
    Ok(())
}

fn validate_nsr_descriptor(descriptor: &[u8; SECTOR_BYTES], sector: u64) -> Result<(), Error> {
    if descriptor[0] != 0 || descriptor[6] != 1 || descriptor[7] != 0 {
        return Err(Error::InvalidVolumeRecognition {
            sector,
            reason: "NSR descriptor header is invalid",
        });
    }
    if descriptor[72..].iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidVolumeRecognition {
            sector,
            reason: "NSR reserved bytes are not zero",
        });
    }
    Ok(())
}

pub(crate) fn parse_partition_descriptor(
    descriptor: &[u8; SECTOR_BYTES],
    declared_location: u64,
    revision: UdfRevision,
) -> Result<PartitionDescriptor, Error> {
    let invalid = |reason| Error::InvalidDescriptor {
        sector: declared_location,
        reason,
    };

    if read_u16(descriptor, 0) != PARTITION_DESCRIPTOR_TAG_ID {
        return Err(invalid("descriptor tag identifier is not 5"));
    }
    if !matches!(read_u16(descriptor, 2), 2 | 3) {
        return Err(invalid("descriptor version is not 2 or 3"));
    }
    if descriptor[5] != 0 {
        return Err(invalid("descriptor tag reserved byte is not zero"));
    }
    if read_u32(descriptor, 12) != u32::try_from(declared_location).unwrap_or(u32::MAX) {
        return Err(invalid("descriptor tag location is incorrect"));
    }
    if descriptor[4] != tag_checksum(descriptor) {
        return Err(invalid("descriptor tag checksum is incorrect"));
    }

    let crc_length = usize::from(read_u16(descriptor, 10));
    if crc_length > PARTITION_DESCRIPTOR_BODY_LENGTH {
        return Err(invalid("descriptor CRC length exceeds 496 bytes"));
    }
    let stored_crc = read_u16(descriptor, 8);
    let computed_crc = descriptor_crc(&descriptor[DESCRIPTOR_TAG_LENGTH..][..crc_length]);
    if stored_crc != computed_crc {
        return Err(invalid("descriptor CRC is incorrect"));
    }

    if read_u16(descriptor, 20) != 1 {
        return Err(invalid("partition flags are not the allocated value"));
    }
    if descriptor[24] != 0 {
        return Err(invalid("partition contents identifier flags are not zero"));
    }
    let expected_identifier = match revision {
        UdfRevision::Nsr02 => b"+NSR02",
        UdfRevision::Nsr03 => b"+NSR03",
    };
    if &descriptor[25..31] != expected_identifier
        || descriptor[31..56].iter().any(|byte| *byte != 0)
    {
        return Err(invalid(
            "partition contents identifier does not match the UDF revision",
        ));
    }
    if read_u32(descriptor, 184) > 4 {
        return Err(invalid(
            "partition access type is outside the defined range",
        ));
    }
    if descriptor[356..PARTITION_DESCRIPTOR_LENGTH]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(invalid("partition descriptor reserved bytes are not zero"));
    }
    if descriptor[PARTITION_DESCRIPTOR_LENGTH..]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(invalid(
            "logical-sector padding after the descriptor is not zero",
        ));
    }

    let length = read_u32(descriptor, PARTITION_LENGTH_OFFSET);
    if length == 0 {
        return Err(invalid("partition length is zero"));
    }

    Ok(PartitionDescriptor {
        start: read_u32(descriptor, PARTITION_START_OFFSET),
        length,
    })
}

pub(crate) fn equivalent_except_partition_start(
    first: &[u8; SECTOR_BYTES],
    second: &[u8; SECTOR_BYTES],
) -> bool {
    first[DESCRIPTOR_TAG_LENGTH..PARTITION_START_OFFSET]
        == second[DESCRIPTOR_TAG_LENGTH..PARTITION_START_OFFSET]
        && first[PARTITION_START_OFFSET + 4..PARTITION_DESCRIPTOR_LENGTH]
            == second[PARTITION_START_OFFSET + 4..PARTITION_DESCRIPTOR_LENGTH]
}

pub(crate) fn set_partition_start(descriptor: &mut [u8; SECTOR_BYTES], start: u32) {
    descriptor[PARTITION_START_OFFSET..PARTITION_START_OFFSET + 4]
        .copy_from_slice(&start.to_le_bytes());
    refresh_descriptor_tag(descriptor);
}

pub(crate) fn refresh_descriptor_tag(descriptor: &mut [u8; SECTOR_BYTES]) {
    let crc_length = usize::from(read_u16(descriptor, 10));
    let crc = descriptor_crc(&descriptor[DESCRIPTOR_TAG_LENGTH..][..crc_length]);
    descriptor[8..10].copy_from_slice(&crc.to_le_bytes());
    descriptor[4] = tag_checksum(descriptor);
}

pub(crate) fn tag_checksum(descriptor: &[u8]) -> u8 {
    descriptor[..DESCRIPTOR_TAG_LENGTH]
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 4)
        .fold(0_u8, |sum, (_, byte)| sum.wrapping_add(*byte))
}

pub(crate) fn descriptor_crc(bytes: &[u8]) -> u16 {
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

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
