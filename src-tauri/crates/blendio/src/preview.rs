use std::path::Path;

use serde::Serialize;

use crate::bhead::{BlockCode, parse_block_header};
use crate::error::{BlendError, Result};
use crate::header::{CompressionKind, parse_blend_header};
use crate::input::load_path;
use crate::view::BlendFile;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlendPreview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn extract_preview(file: &BlendFile) -> Result<Option<BlendPreview>> {
    if file.header().file_version < 250 {
        return Ok(None);
    }

    for (index, block) in file.blocks().iter().enumerate() {
        match block.code {
            BlockCode::TEST => {
                let Some(block_ref) = file.block(index) else {
                    return Ok(None);
                };
                return Ok(parse_preview_payload(block_ref.bytes()));
            }
            BlockCode::REND => continue,
            _ => return Ok(None),
        }
    }

    Ok(None)
}

pub fn extract_preview_from_path(path: impl AsRef<Path>) -> Result<Option<BlendPreview>> {
    let (storage, compression) = load_path(path.as_ref())?;
    extract_preview_from_bytes(storage.as_slice(), compression)
}

pub fn extract_preview_from_bytes(
    bytes: &[u8],
    compression: CompressionKind,
) -> Result<Option<BlendPreview>> {
    let header = parse_blend_header(bytes, compression)?;
    if header.file_version < 250 {
        return Ok(None);
    }

    let mut offset = header.header_size;
    while offset < bytes.len() {
        let (code, len, _old_ptr, _sdna_index, _count, header_size) =
            parse_block_header(&bytes[offset..], header.bhead_type, offset)?;
        let payload_offset = offset
            .checked_add(header_size)
            .ok_or(BlendError::TruncatedBlock { offset })?;
        let payload_end =
            payload_offset
                .checked_add(len as usize)
                .ok_or(BlendError::TruncatedBlock {
                    offset: payload_offset,
                })?;

        if payload_end > bytes.len() {
            return Err(BlendError::TruncatedBlock { offset });
        }

        match code {
            BlockCode::TEST => {
                return Ok(parse_preview_payload(&bytes[payload_offset..payload_end]));
            }
            BlockCode::REND => {
                offset = payload_end;
            }
            _ => return Ok(None),
        }
    }

    Ok(None)
}

fn parse_preview_payload(payload: &[u8]) -> Option<BlendPreview> {
    if payload.len() < 8 {
        return None;
    }

    let width = i32::from_le_bytes(payload.get(0..4)?.try_into().ok()?);
    let height = i32::from_le_bytes(payload.get(4..8)?.try_into().ok()?);
    if width <= 0 || height <= 0 {
        return None;
    }

    let width = width as u32;
    let height = height as u32;
    let expected_size = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    let rgba = payload.get(8..)?;
    if rgba.len() != expected_size {
        return None;
    }

    Some(BlendPreview {
        width,
        height,
        rgba: vertically_flipped_rgba(rgba, width as usize, height as usize),
    })
}

fn vertically_flipped_rgba(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let row_size = width * 4;
    let mut flipped = Vec::with_capacity(rgba.len());

    for row in (0..height).rev() {
        let start = row * row_size;
        let end = start + row_size;
        flipped.extend_from_slice(&rgba[start..end]);
    }

    flipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bhead::BlockCode;

    #[test]
    fn extracts_and_flips_preview_block() {
        let mut bytes = b"BLENDER-v250".to_vec();
        let pixels = [1_u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

        bytes.extend_from_slice(&BlockCode::TEST.raw().to_le_bytes());
        bytes.extend_from_slice(&(8_u32 + pixels.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u64).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(2_i32).to_le_bytes());
        bytes.extend_from_slice(&(2_i32).to_le_bytes());
        bytes.extend_from_slice(&pixels);

        let preview = extract_preview_from_bytes(&bytes, CompressionKind::None)
            .unwrap()
            .unwrap();

        assert_eq!(preview.width, 2);
        assert_eq!(preview.height, 2);
        assert_eq!(
            preview.rgba,
            vec![9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn stops_when_first_block_is_not_thumbnail_or_render_info() {
        let mut bytes = b"BLENDER-v250".to_vec();
        bytes.extend_from_slice(&BlockCode::DATA.raw().to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u64).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());

        let preview = extract_preview_from_bytes(&bytes, CompressionKind::None).unwrap();
        assert!(preview.is_none());
    }
}
