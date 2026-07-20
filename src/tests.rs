// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use sha2::{Digest, Sha256};

use crate::image::{
    FIRST_BACKUP_SECTOR, FIRST_DESCRIPTOR_SECTOR, PAYLOAD_FIRST_SECTOR, PAYLOAD_SECTOR_COUNT,
    SECOND_BACKUP_SECTOR, SECOND_DESCRIPTOR_SECTOR,
};
use crate::payload;
use crate::sector::{self, SECTOR_BYTES};
use crate::udf;
use crate::{Error, PatchState, UdfRevision, inspect, patch, unpatch};

const FIXTURE_SECTORS: usize = 256;
const ORIGINAL_PARTITION_START: u32 = 160;
const ORIGINAL_PARTITION_LENGTH: u32 = 96;

#[test]
fn crc_matches_ecma_and_standard_check_vectors() {
    assert_eq!(udf::descriptor_crc(&[0x70, 0x6a, 0x77]), 0x3299);
    assert_eq!(udf::descriptor_crc(b"123456789"), 0x31c3);
}

#[test]
fn tag_checksum_excludes_only_checksum_byte() {
    let mut tag = [0_u8; 16];
    for (index, byte) in tag.iter_mut().enumerate() {
        *byte = index as u8;
    }
    assert_eq!(udf::tag_checksum(&tag), 116);
    tag[4] = 0xff;
    assert_eq!(udf::tag_checksum(&tag), 116);
}

#[test]
fn checked_sector_offsets_and_little_endian_fields() {
    assert_eq!(sector::byte_offset(34).unwrap(), 69_632);
    assert!(matches!(
        sector::byte_offset(u64::MAX),
        Err(Error::ArithmeticOverflow { .. })
    ));

    let bytes = [0x78, 0x56, 0x34, 0x12];
    assert_eq!(udf::read_u16(&bytes, 0), 0x5678);
    assert_eq!(udf::read_u32(&bytes, 0), 0x1234_5678);
}

#[test]
fn embedded_payload_has_pinned_identity() {
    assert_eq!(payload::BYTES.len(), 24_576);
    assert_eq!(Sha256::digest(payload::BYTES)[..], payload::EXPECTED_SHA256);
    payload::validate().unwrap();
}

#[test]
fn recognizes_nsr02_and_nsr03_images() {
    for revision in [UdfRevision::Nsr02, UdfRevision::Nsr03] {
        let mut image = Cursor::new(fixture(revision, 496));
        let info = inspect(&mut image).unwrap();
        assert_eq!(info.state, PatchState::Unpatched);
        assert_eq!(info.udf_revision, revision);
        assert_eq!(info.sector_count, FIXTURE_SECTORS as u64);
    }
}

#[test]
fn accepts_zero_length_descriptor_crc_when_stored_crc_is_zero() {
    let mut image = Cursor::new(fixture(UdfRevision::Nsr03, 0));
    assert_eq!(inspect(&mut image).unwrap().state, PatchState::Unpatched);
    patch(&mut image).unwrap();
    assert_eq!(inspect(&mut image).unwrap().state, PatchState::Patched);
}

#[test]
fn rejects_invalid_image_sizes_without_writing() {
    let mut unaligned = Cursor::new(vec![0_u8; FIXTURE_SECTORS * SECTOR_BYTES - 1]);
    assert!(matches!(
        patch(&mut unaligned),
        Err(Error::InvalidImageLength { .. })
    ));

    let mut too_small = Cursor::new(vec![0_u8; 139 * SECTOR_BYTES]);
    assert!(matches!(
        patch(&mut too_small),
        Err(Error::ImageTooSmall { .. })
    ));
}

#[test]
fn rejects_missing_and_malformed_volume_recognition() {
    let mut missing_bytes = fixture(UdfRevision::Nsr03, 496);
    sector_mut(&mut missing_bytes, 17).fill(0);
    let mut missing = Cursor::new(missing_bytes);
    assert!(matches!(
        inspect(&mut missing),
        Err(Error::InvalidVolumeRecognition { .. })
    ));

    let mut duplicate_bytes = fixture(UdfRevision::Nsr03, 496);
    sector_mut(&mut duplicate_bytes, 18).fill(0);
    write_vrs(&mut duplicate_bytes, 18, b"NSR03");
    write_vrs(&mut duplicate_bytes, 19, b"TEA01");
    let mut duplicate = Cursor::new(duplicate_bytes);
    assert!(matches!(
        inspect(&mut duplicate),
        Err(Error::InvalidVolumeRecognition { .. })
    ));

    let mut no_sequence = Cursor::new(vec![0_u8; FIXTURE_SECTORS * SECTOR_BYTES]);
    assert!(matches!(
        inspect(&mut no_sequence),
        Err(Error::MissingUdfRecognition)
    ));
}

#[test]
fn rejects_bad_descriptor_checksum_crc_and_crc_length() {
    let original = fixture(UdfRevision::Nsr03, 496);

    let mut bad_checksum = original.clone();
    sector_mut(&mut bad_checksum, FIRST_DESCRIPTOR_SECTOR)[4] ^= 1;
    assert_invalid_descriptor(bad_checksum);

    let mut bad_crc = original.clone();
    sector_mut(&mut bad_crc, FIRST_DESCRIPTOR_SECTOR)[100] ^= 1;
    assert_invalid_descriptor(bad_crc);

    let mut bad_length = original;
    let descriptor = sector_mut(&mut bad_length, FIRST_DESCRIPTOR_SECTOR);
    descriptor[10..12].copy_from_slice(&497_u16.to_le_bytes());
    descriptor[4] = udf::tag_checksum(descriptor);
    assert_invalid_descriptor(bad_length);
}

#[test]
fn rejects_wrong_tag_location_padding_and_partition_bounds() {
    let mut wrong_location = fixture(UdfRevision::Nsr03, 496);
    let descriptor = sector_mut(&mut wrong_location, FIRST_DESCRIPTOR_SECTOR);
    descriptor[12..16].copy_from_slice(&35_u32.to_le_bytes());
    udf::refresh_descriptor_tag(descriptor);
    assert_invalid_descriptor(wrong_location);

    let mut bad_padding = fixture(UdfRevision::Nsr03, 496);
    let descriptor = sector_mut(&mut bad_padding, FIRST_DESCRIPTOR_SECTOR);
    descriptor[700] = 1;
    assert_invalid_descriptor(bad_padding);

    let mut out_of_bounds = fixture(UdfRevision::Nsr03, 496);
    for sector_number in [FIRST_DESCRIPTOR_SECTOR, SECOND_DESCRIPTOR_SECTOR] {
        let descriptor = sector_mut(&mut out_of_bounds, sector_number);
        descriptor[192..196].copy_from_slice(&97_u32.to_le_bytes());
        udf::refresh_descriptor_tag(descriptor);
    }
    let mut image = Cursor::new(out_of_bounds);
    assert!(matches!(
        inspect(&mut image),
        Err(Error::PartitionOutOfBounds { .. })
    ));
}

#[test]
fn malformed_preflight_never_writes() {
    let mut bytes = fixture(UdfRevision::Nsr03, 496);
    sector_mut(&mut bytes, FIRST_DESCRIPTOR_SECTOR)[4] ^= 1;
    let original = bytes.clone();
    let mut image = Cursor::new(bytes);

    assert!(matches!(
        patch(&mut image),
        Err(Error::InvalidDescriptor { .. })
    ));
    assert_eq!(image.into_inner(), original);
}

#[test]
fn occupied_reserved_sectors_are_inconsistent_and_not_patchable() {
    for sector_number in [
        FIRST_BACKUP_SECTOR,
        SECOND_BACKUP_SECTOR,
        PAYLOAD_FIRST_SECTOR,
        PAYLOAD_FIRST_SECTOR + PAYLOAD_SECTOR_COUNT - 1,
    ] {
        let mut bytes = fixture(UdfRevision::Nsr03, 496);
        sector_mut(&mut bytes, sector_number)[0] = 1;
        let mut image = Cursor::new(bytes);
        assert_eq!(inspect(&mut image).unwrap().state, PatchState::Inconsistent);
        assert!(matches!(
            patch(&mut image),
            Err(Error::ReservedSectorsInUse)
        ));
    }
}

#[test]
fn partial_and_contradictory_patch_states_are_inconsistent() {
    let mut mismatched_descriptors = fixture(UdfRevision::Nsr03, 496);
    let reserve = sector_mut(&mut mismatched_descriptors, SECOND_DESCRIPTOR_SECTOR);
    reserve[184..188].copy_from_slice(&2_u32.to_le_bytes());
    udf::refresh_descriptor_tag(reserve);
    assert_inconsistent(mismatched_descriptors);

    let mut mixed_live = fixture(UdfRevision::Nsr03, 496);
    udf::set_partition_start(
        sector_mut(&mut mixed_live, FIRST_DESCRIPTOR_SECTOR),
        PAYLOAD_FIRST_SECTOR as u32,
    );
    assert_inconsistent(mixed_live);

    let mut missing_backup = patched_fixture();
    sector_mut(&mut missing_backup, SECOND_BACKUP_SECTOR).fill(0);
    assert_inconsistent(missing_backup);

    let mut damaged_payload = patched_fixture();
    sector_mut(&mut damaged_payload, PAYLOAD_FIRST_SECTOR + 4)[31] ^= 1;
    assert_inconsistent(damaged_payload);

    let mut changed_backup = patched_fixture();
    sector_mut(&mut changed_backup, FIRST_BACKUP_SECTOR)[100] ^= 1;
    assert_inconsistent(changed_backup);
}

#[test]
fn patch_rejects_wrong_current_states() {
    let mut patched = Cursor::new(patched_fixture());
    assert!(matches!(patch(&mut patched), Err(Error::AlreadyPatched)));

    let mut unpatched = Cursor::new(fixture(UdfRevision::Nsr03, 496));
    assert!(matches!(unpatch(&mut unpatched), Err(Error::NotPatched)));
}

#[test]
fn patch_changes_only_documented_sectors() {
    let original = fixture(UdfRevision::Nsr03, 496);
    let mut image = Cursor::new(original.clone());
    patch(&mut image).unwrap();
    let patched = image.into_inner();

    let changed: BTreeSet<u64> = (0..FIXTURE_SECTORS as u64)
        .filter(|sector_number| {
            sector_slice(&original, *sector_number) != sector_slice(&patched, *sector_number)
        })
        .collect();
    let expected: BTreeSet<u64> = [
        FIRST_BACKUP_SECTOR,
        SECOND_BACKUP_SECTOR,
        FIRST_DESCRIPTOR_SECTOR,
        SECOND_DESCRIPTOR_SECTOR,
    ]
    .into_iter()
    .chain(PAYLOAD_FIRST_SECTOR..PAYLOAD_FIRST_SECTOR + PAYLOAD_SECTOR_COUNT)
    .collect();
    assert_eq!(changed, expected);
}

#[test]
fn patch_and_unpatch_round_trip_is_byte_exact() {
    let original = fixture(UdfRevision::Nsr02, 496);
    let original_digest = Sha256::digest(&original);
    let mut image = Cursor::new(original);

    patch(&mut image).unwrap();
    assert_eq!(inspect(&mut image).unwrap().state, PatchState::Patched);
    unpatch(&mut image).unwrap();
    assert_eq!(inspect(&mut image).unwrap().state, PatchState::Unpatched);
    assert_eq!(Sha256::digest(image.get_ref()), original_digest);
}

#[test]
fn patched_synthetic_fixture_matches_legacy_golden_digest() {
    let mut image = Cursor::new(fixture(UdfRevision::Nsr03, 496));
    patch(&mut image).unwrap();

    // Generated once with esrtool-legacy v0.25.3 from the same synthetic
    // fixture. The legacy executable is never built or run by the test suite.
    let expected = [
        0xfb, 0x80, 0x78, 0x51, 0x6e, 0x02, 0xe0, 0x9e, 0x9f, 0x5f, 0x2a, 0xe3, 0x8b, 0x65, 0x30,
        0xc5, 0x44, 0x09, 0xba, 0xa7, 0x46, 0x83, 0x4f, 0xc6, 0xd8, 0xe2, 0xe9, 0x27, 0x5e, 0x71,
        0xa2, 0xf4,
    ];
    assert_eq!(Sha256::digest(image.get_ref())[..], expected);
}

#[test]
fn propagates_injected_read_seek_write_and_flush_failures() {
    for failure in [
        Failure::Read(0),
        Failure::Seek(0),
        Failure::Write(2),
        Failure::Flush,
    ] {
        let mut image = FaultStream::new(fixture(UdfRevision::Nsr03, 496), failure);
        assert!(matches!(patch(&mut image), Err(Error::Io(_))));
    }
}

#[test]
fn failed_post_write_inspection_is_reported_as_verification_failure() {
    let mut image = CorruptOnFlush::new(fixture(UdfRevision::Nsr03, 496));
    assert!(matches!(patch(&mut image), Err(Error::VerificationFailed)));
}

fn fixture(revision: UdfRevision, crc_length: u16) -> Vec<u8> {
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

    for sector_number in [FIRST_DESCRIPTOR_SECTOR, SECOND_DESCRIPTOR_SECTOR] {
        let descriptor = make_partition_descriptor(revision, sector_number, crc_length);
        sector::write_to_slice(&mut image, sector_number, &descriptor);
    }

    for (index, byte) in sector_mut(&mut image, 200).iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    image
}

fn write_vrs(image: &mut [u8], sector_number: u64, identifier: &[u8; 5]) {
    let descriptor = sector_mut(image, sector_number);
    descriptor[0] = 0;
    descriptor[1..6].copy_from_slice(identifier);
    descriptor[6] = 1;
}

fn make_partition_descriptor(
    revision: UdfRevision,
    location: u64,
    crc_length: u16,
) -> [u8; SECTOR_BYTES] {
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
    descriptor[10..12].copy_from_slice(&crc_length.to_le_bytes());
    descriptor[12..16].copy_from_slice(&(location as u32).to_le_bytes());
    descriptor[16..20].copy_from_slice(&1_u32.to_le_bytes());
    descriptor[20..22].copy_from_slice(&1_u16.to_le_bytes());
    descriptor[25..31].copy_from_slice(match revision {
        UdfRevision::Nsr02 => b"+NSR02",
        UdfRevision::Nsr03 => b"+NSR03",
    });
    descriptor[184..188].copy_from_slice(&1_u32.to_le_bytes());
    descriptor[188..192].copy_from_slice(&ORIGINAL_PARTITION_START.to_le_bytes());
    descriptor[192..196].copy_from_slice(&ORIGINAL_PARTITION_LENGTH.to_le_bytes());
    udf::refresh_descriptor_tag(&mut descriptor);
    descriptor
}

fn patched_fixture() -> Vec<u8> {
    let mut image = Cursor::new(fixture(UdfRevision::Nsr03, 496));
    patch(&mut image).unwrap();
    image.into_inner()
}

fn assert_invalid_descriptor(bytes: Vec<u8>) {
    let mut image = Cursor::new(bytes);
    assert!(matches!(
        inspect(&mut image),
        Err(Error::InvalidDescriptor { .. })
    ));
}

fn assert_inconsistent(bytes: Vec<u8>) {
    let mut image = Cursor::new(bytes);
    assert_eq!(inspect(&mut image).unwrap().state, PatchState::Inconsistent);
    assert!(matches!(patch(&mut image), Err(Error::InconsistentState)));
    assert!(matches!(unpatch(&mut image), Err(Error::InconsistentState)));
}

fn sector_slice(image: &[u8], sector_number: u64) -> &[u8] {
    let start = sector_number as usize * SECTOR_BYTES;
    &image[start..start + SECTOR_BYTES]
}

fn sector_mut(image: &mut [u8], sector_number: u64) -> &mut [u8; SECTOR_BYTES] {
    let start = sector_number as usize * SECTOR_BYTES;
    (&mut image[start..start + SECTOR_BYTES])
        .try_into()
        .expect("test fixture contains the requested sector")
}

#[derive(Clone, Copy)]
enum Failure {
    Read(usize),
    Seek(usize),
    Write(usize),
    Flush,
}

struct FaultStream {
    inner: Cursor<Vec<u8>>,
    failure: Failure,
    reads: usize,
    seeks: usize,
    writes: usize,
}

impl FaultStream {
    fn new(bytes: Vec<u8>, failure: Failure) -> Self {
        Self {
            inner: Cursor::new(bytes),
            failure,
            reads: 0,
            seeks: 0,
            writes: 0,
        }
    }
}

impl Read for FaultStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let call = self.reads;
        self.reads += 1;
        if matches!(self.failure, Failure::Read(target) if target == call) {
            Err(io::Error::other("injected read failure"))
        } else {
            self.inner.read(buffer)
        }
    }
}

impl Seek for FaultStream {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let call = self.seeks;
        self.seeks += 1;
        if matches!(self.failure, Failure::Seek(target) if target == call) {
            Err(io::Error::other("injected seek failure"))
        } else {
            self.inner.seek(position)
        }
    }
}

impl Write for FaultStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let call = self.writes;
        self.writes += 1;
        if matches!(self.failure, Failure::Write(target) if target == call) {
            Err(io::Error::other("injected write failure"))
        } else {
            self.inner.write(buffer)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if matches!(self.failure, Failure::Flush) {
            Err(io::Error::other("injected flush failure"))
        } else {
            self.inner.flush()
        }
    }
}

struct CorruptOnFlush {
    inner: Cursor<Vec<u8>>,
}

impl CorruptOnFlush {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            inner: Cursor::new(bytes),
        }
    }
}

impl Read for CorruptOnFlush {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl Seek for CorruptOnFlush {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

impl Write for CorruptOnFlush {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        let offset = FIRST_DESCRIPTOR_SECTOR as usize * SECTOR_BYTES + 4;
        self.inner.get_mut()[offset] ^= 1;
        Ok(())
    }
}
