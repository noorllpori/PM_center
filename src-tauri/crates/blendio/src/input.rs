use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use memmap2::Mmap;

use crate::error::{BlendError, Result};
use crate::header::CompressionKind;

pub enum BlendBytes {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl BlendBytes {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Mapped(mmap) => mmap.as_ref(),
            Self::Owned(bytes) => bytes.as_slice(),
        }
    }
}

pub fn load_path(path: &Path) -> Result<(BlendBytes, CompressionKind)> {
    let mut file = File::open(path)?;
    let mut prefix = [0_u8; 7];
    let read = file.read(&mut prefix)?;

    if read < 4 {
        return Err(BlendError::TruncatedHeader);
    }

    file.seek(SeekFrom::Start(0))?;

    if read >= 7 && &prefix == b"BLENDER" {
        let mmap = unsafe { Mmap::map(&file)? };
        return Ok((BlendBytes::Mapped(mmap), CompressionKind::None));
    }

    if prefix[0..2] == [0x1F, 0x8B] {
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut bytes = Vec::new();
        decoder.read_to_end(&mut bytes)?;
        return Ok((BlendBytes::Owned(bytes), CompressionKind::Gzip));
    }

    if prefix[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
        let bytes = zstd::stream::decode_all(file)?;
        return Ok((BlendBytes::Owned(bytes), CompressionKind::Zstd));
    }

    Err(BlendError::InvalidMagic)
}
