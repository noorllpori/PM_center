use std::fmt;

use serde::{Serialize, Serializer};

use crate::error::{BlendError, Result};
use crate::header::{BHeadType, BlendHeader};

pub const fn blend_make_id(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (d as u32) << 24 | (c as u32) << 16 | (b as u32) << 8 | (a as u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockCode(u32);

impl BlockCode {
    pub const DATA: Self = Self(blend_make_id(b'D', b'A', b'T', b'A'));
    pub const GLOB: Self = Self(blend_make_id(b'G', b'L', b'O', b'B'));
    pub const DNA1: Self = Self(blend_make_id(b'D', b'N', b'A', b'1'));
    pub const TEST: Self = Self(blend_make_id(b'T', b'E', b'S', b'T'));
    pub const REND: Self = Self(blend_make_id(b'R', b'E', b'N', b'D'));
    pub const USER: Self = Self(blend_make_id(b'U', b'S', b'E', b'R'));
    pub const ENDB: Self = Self(blend_make_id(b'E', b'N', b'D', b'B'));

    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn is_id(self) -> bool {
        self.0 <= 0xFFFF
    }

    pub fn as_string(self) -> String {
        let bytes = self.0.to_le_bytes();
        let raw = if self.is_id() {
            &bytes[..2]
        } else {
            &bytes[..4]
        };
        let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    }
}

impl fmt::Display for BlockCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

impl Serialize for BlockCode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockHeader {
    pub code: BlockCode,
    pub len: u64,
    pub old_ptr: u64,
    pub sdna_index: u32,
    pub count: u64,
    pub file_offset: usize,
    #[serde(skip)]
    pub(crate) payload_offset: usize,
}

impl BlockHeader {
    pub fn is_id(&self) -> bool {
        self.code.is_id()
    }
}

pub fn parse_blocks(bytes: &[u8], header: &BlendHeader) -> Result<Vec<BlockHeader>> {
    let mut offset = header.header_size;
    let mut blocks = Vec::new();
    let mut saw_end = false;

    while offset < bytes.len() {
        let file_offset = offset;
        let (code, len, old_ptr, sdna_index, count, header_size) =
            parse_block_header(&bytes[offset..], header.bhead_type, file_offset)?;
        let payload_offset = file_offset + header_size;
        let payload_end =
            payload_offset
                .checked_add(len as usize)
                .ok_or(BlendError::TruncatedBlock {
                    offset: file_offset,
                })?;

        if payload_end > bytes.len() {
            return Err(BlendError::TruncatedBlock {
                offset: file_offset,
            });
        }

        blocks.push(BlockHeader {
            code,
            len,
            old_ptr,
            sdna_index,
            count,
            file_offset,
            payload_offset,
        });

        offset = payload_end;
        if code == BlockCode::ENDB {
            saw_end = true;
            break;
        }
    }

    if !saw_end {
        return Err(BlendError::MissingEndBlock);
    }

    Ok(blocks)
}

fn parse_block_header(
    bytes: &[u8],
    kind: BHeadType,
    offset: usize,
) -> Result<(BlockCode, u64, u64, u32, u64, usize)> {
    match kind {
        BHeadType::BHead4 => {
            const SIZE: usize = 20;
            if bytes.len() < SIZE {
                return Err(BlendError::TruncatedBlockHeader { offset });
            }

            let code = read_u32(bytes, 0);
            let len = read_u32(bytes, 4) as u64;
            let old_ptr = read_u32(bytes, 8) as u64;
            let sdna_index = read_u32(bytes, 12);
            let count = read_u32(bytes, 16) as u64;
            Ok((
                BlockCode::from_raw(code),
                len,
                old_ptr,
                sdna_index,
                count,
                SIZE,
            ))
        }
        BHeadType::SmallBHead8 => {
            const SIZE: usize = 24;
            if bytes.len() < SIZE {
                return Err(BlendError::TruncatedBlockHeader { offset });
            }

            let code = read_u32(bytes, 0);
            let len = read_u32(bytes, 4) as u64;
            let old_ptr = read_u64(bytes, 8);
            let sdna_index = read_u32(bytes, 16);
            let count = read_u32(bytes, 20) as u64;
            Ok((
                BlockCode::from_raw(code),
                len,
                old_ptr,
                sdna_index,
                count,
                SIZE,
            ))
        }
        BHeadType::LargeBHead8 => {
            const SIZE: usize = 32;
            if bytes.len() < SIZE {
                return Err(BlendError::TruncatedBlockHeader { offset });
            }

            let code = read_u32(bytes, 0);
            let sdna_index = read_u32(bytes, 4);
            let old_ptr = read_u64(bytes, 8);
            let len = read_u64(bytes, 16);
            let count = read_u64(bytes, 24);
            Ok((
                BlockCode::from_raw(code),
                len,
                old_ptr,
                sdna_index,
                count,
                SIZE,
            ))
        }
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{CompressionKind, Endian};

    fn make_header(bhead_type: BHeadType, header_size: usize) -> BlendHeader {
        BlendHeader {
            compression: CompressionKind::None,
            pointer_size: 8,
            endian: Endian::Little,
            file_version: 405,
            file_format_version: 0,
            header_size,
            bhead_type,
        }
    }

    #[test]
    fn parses_bhead4_blocks() {
        let mut bytes = b"BLENDER_v405".to_vec();
        bytes.extend_from_slice(&BlockCode::DATA.raw().to_le_bytes());
        bytes.extend_from_slice(&(4_u32).to_le_bytes());
        bytes.extend_from_slice(&(0x1234_u32).to_le_bytes());
        bytes.extend_from_slice(&(2_u32).to_le_bytes());
        bytes.extend_from_slice(&(1_u32).to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes.extend_from_slice(&BlockCode::ENDB.raw().to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());

        let header = make_header(BHeadType::BHead4, 12);
        let blocks = parse_blocks(&bytes, &header).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].code, BlockCode::DATA);
        assert_eq!(blocks[0].len, 4);
        assert_eq!(blocks[0].old_ptr, 0x1234);
    }

    #[test]
    fn parses_small_bhead8_blocks() {
        let mut bytes = b"BLENDER-v405".to_vec();
        bytes.extend_from_slice(&BlockCode::DATA.raw().to_le_bytes());
        bytes.extend_from_slice(&(4_u32).to_le_bytes());
        bytes.extend_from_slice(&(0x1234_u64).to_le_bytes());
        bytes.extend_from_slice(&(3_u32).to_le_bytes());
        bytes.extend_from_slice(&(1_u32).to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes.extend_from_slice(&BlockCode::ENDB.raw().to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u64).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());

        let header = make_header(BHeadType::SmallBHead8, 12);
        let blocks = parse_blocks(&bytes, &header).unwrap();
        assert_eq!(blocks[0].sdna_index, 3);
        assert_eq!(blocks[0].payload_offset, 36);
    }

    #[test]
    fn parses_large_bhead8_blocks() {
        let mut bytes = b"BLENDER17-01v0405".to_vec();
        bytes.extend_from_slice(&BlockCode::DATA.raw().to_le_bytes());
        bytes.extend_from_slice(&(5_u32).to_le_bytes());
        bytes.extend_from_slice(&(0x9988_u64).to_le_bytes());
        bytes.extend_from_slice(&(4_u64).to_le_bytes());
        bytes.extend_from_slice(&(1_u64).to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes.extend_from_slice(&BlockCode::ENDB.raw().to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u64).to_le_bytes());
        bytes.extend_from_slice(&(0_u64).to_le_bytes());
        bytes.extend_from_slice(&(0_u64).to_le_bytes());

        let header = BlendHeader {
            compression: CompressionKind::None,
            pointer_size: 8,
            endian: Endian::Little,
            file_version: 405,
            file_format_version: 1,
            header_size: 17,
            bhead_type: BHeadType::LargeBHead8,
        };
        let blocks = parse_blocks(&bytes, &header).unwrap();
        assert_eq!(blocks[0].code, BlockCode::DATA);
        assert_eq!(blocks[0].sdna_index, 5);
    }

    #[test]
    fn errors_when_endb_is_missing() {
        let bytes = b"BLENDER-v405".to_vec();
        let header = make_header(BHeadType::SmallBHead8, 12);
        let err = parse_blocks(&bytes, &header).unwrap_err();
        assert!(matches!(err, BlendError::MissingEndBlock));
    }
}
