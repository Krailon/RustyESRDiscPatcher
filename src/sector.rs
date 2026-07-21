// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::{Read, Seek, SeekFrom, Write};

use crate::{Error, LOGICAL_SECTOR_SIZE};

pub(crate) const SECTOR_BYTES: usize = LOGICAL_SECTOR_SIZE as usize;
pub(crate) const REQUIRED_SECTORS: u64 = 140;

pub(crate) fn image_sector_count<R: Seek + ?Sized>(image: &mut R) -> Result<u64, Error> {
    let bytes = image.seek(SeekFrom::End(0))?;
    if bytes % LOGICAL_SECTOR_SIZE != 0 {
        return Err(Error::InvalidImageLength { bytes });
    }

    let sector_count = bytes / LOGICAL_SECTOR_SIZE;
    if sector_count < REQUIRED_SECTORS {
        return Err(Error::ImageTooSmall {
            sector_count,
            required_sectors: REQUIRED_SECTORS,
        });
    }

    Ok(sector_count)
}

pub(crate) fn byte_offset(sector: u64) -> Result<u64, Error> {
    sector
        .checked_mul(LOGICAL_SECTOR_SIZE)
        .ok_or(Error::ArithmeticOverflow { sector })
}

pub(crate) fn read<R: Read + Seek + ?Sized>(
    image: &mut R,
    sector: u64,
) -> Result<[u8; SECTOR_BYTES], Error> {
    let mut data = [0; SECTOR_BYTES];
    image.seek(SeekFrom::Start(byte_offset(sector)?))?;
    image.read_exact(&mut data)?;
    Ok(data)
}

pub(crate) fn write<W: Write + Seek + ?Sized>(
    image: &mut W,
    sector: u64,
    data: &[u8; SECTOR_BYTES],
) -> Result<(), Error> {
    image.seek(SeekFrom::Start(byte_offset(sector)?))?;
    image.write_all(data)?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn write_to_slice(image: &mut [u8], sector: u64, data: &[u8; SECTOR_BYTES]) {
    let start = usize::try_from(byte_offset(sector).expect("test sector offset fits usize"))
        .expect("test sector offset fits usize");
    image[start..start + SECTOR_BYTES].copy_from_slice(data);
}
