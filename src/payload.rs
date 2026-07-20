// SPDX-License-Identifier: GPL-3.0-or-later

use sha2::{Digest, Sha256};

use crate::Error;

pub(crate) const LENGTH: usize = 24_576;
pub(crate) const BYTES: &[u8; LENGTH] = include_bytes!("../assets/dvd_video_data.bin");
pub(crate) const EXPECTED_SHA256: [u8; 32] = [
    0xd6, 0x10, 0x83, 0xe8, 0xbc, 0x90, 0xa9, 0x59, 0xc2, 0x19, 0x58, 0xe4, 0x62, 0x16, 0xa8, 0x53,
    0x1c, 0x20, 0x95, 0xc2, 0xf6, 0xf7, 0x80, 0xb7, 0x79, 0xd9, 0x48, 0x9f, 0x3f, 0xd5, 0xa8, 0x45,
];

pub(crate) fn validate() -> Result<(), Error> {
    let digest = Sha256::digest(BYTES);
    if digest[..] == EXPECTED_SHA256 {
        Ok(())
    } else {
        Err(Error::InvalidEmbeddedPayload)
    }
}
