use serde::Serialize;

use crate::error::{BlendError, Result};

const LEGACY_HEADER_SIZE: usize = 12;
const LARGE_HEADER_SIZE: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionKind {
    None,
    Gzip,
    Zstd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BHeadType {
    BHead4,
    SmallBHead8,
    LargeBHead8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlendHeader {
    pub compression: CompressionKind,
    pub pointer_size: u8,
    pub endian: Endian,
    pub file_version: u16,
    pub file_format_version: u8,
    pub header_size: usize,
    pub bhead_type: BHeadType,
}

pub fn parse_blend_header(bytes: &[u8], compression: CompressionKind) -> Result<BlendHeader> {
    if bytes.len() < LEGACY_HEADER_SIZE {
        return Err(BlendError::TruncatedHeader);
    }

    if &bytes[0..7] != b"BLENDER" {
        return Err(BlendError::InvalidMagic);
    }

    if matches!(bytes[7], b'_' | b'-') {
        parse_legacy_header(bytes, compression)
    } else {
        parse_large_header(bytes, compression)
    }
}

fn parse_legacy_header(bytes: &[u8], compression: CompressionKind) -> Result<BlendHeader> {
    let pointer_size = match bytes[7] {
        b'_' => 4,
        b'-' => 8,
        other => {
            return Err(BlendError::InvalidHeader(format!(
                "unexpected pointer-size marker byte {other:?}"
            )));
        }
    };

    let endian = match bytes[8] {
        b'v' => Endian::Little,
        b'V' => return Err(BlendError::UnsupportedEndian),
        other => {
            return Err(BlendError::InvalidHeader(format!(
                "unexpected endian marker byte {other:?}"
            )));
        }
    };

    let version = parse_ascii_u16(&bytes[9..12], "legacy file version")?;
    let bhead_type = if pointer_size == 4 {
        BHeadType::BHead4
    } else {
        BHeadType::SmallBHead8
    };

    Ok(BlendHeader {
        compression,
        pointer_size,
        endian,
        file_version: version,
        file_format_version: 0,
        header_size: LEGACY_HEADER_SIZE,
        bhead_type,
    })
}

fn parse_large_header(bytes: &[u8], compression: CompressionKind) -> Result<BlendHeader> {
    if bytes.len() < LARGE_HEADER_SIZE {
        return Err(BlendError::TruncatedHeader);
    }

    let header_size = parse_ascii_u16(&bytes[7..9], "header size")? as usize;
    if header_size != LARGE_HEADER_SIZE {
        return Err(BlendError::InvalidHeader(format!(
            "unsupported large-header size {header_size}"
        )));
    }

    if bytes[9] != b'-' {
        return Err(BlendError::InvalidHeader(format!(
            "unexpected large-header pointer marker byte {:?}",
            bytes[9]
        )));
    }

    let file_format_version = parse_ascii_u16(&bytes[10..12], "file format version")?;
    if file_format_version != 1 {
        return Err(BlendError::InvalidHeader(format!(
            "unsupported file format version {file_format_version}"
        )));
    }

    let endian = match bytes[12] {
        b'v' => Endian::Little,
        b'V' => return Err(BlendError::UnsupportedEndian),
        other => {
            return Err(BlendError::InvalidHeader(format!(
                "unexpected large-header endian marker byte {other:?}"
            )));
        }
    };

    let file_version = parse_ascii_u16(&bytes[13..17], "file version")?;

    Ok(BlendHeader {
        compression,
        pointer_size: 8,
        endian,
        file_version,
        file_format_version: 1,
        header_size,
        bhead_type: BHeadType::LargeBHead8,
    })
}

fn parse_ascii_u16(bytes: &[u8], context: &str) -> Result<u16> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| BlendError::InvalidHeader(format!("invalid UTF-8 in {context}")))?;

    if !value.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(BlendError::InvalidHeader(format!(
            "{context} contains non-digit bytes"
        )));
    }

    value
        .parse::<u16>()
        .map_err(|_| BlendError::InvalidHeader(format!("failed to parse {context}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_header() {
        let header = parse_blend_header(b"BLENDER-v405", CompressionKind::None).unwrap();
        assert_eq!(header.pointer_size, 8);
        assert_eq!(header.endian, Endian::Little);
        assert_eq!(header.file_version, 405);
        assert_eq!(header.file_format_version, 0);
        assert_eq!(header.header_size, 12);
        assert_eq!(header.bhead_type, BHeadType::SmallBHead8);
    }

    #[test]
    fn parses_large_header() {
        let header = parse_blend_header(b"BLENDER17-01v0405", CompressionKind::Zstd).unwrap();
        assert_eq!(header.pointer_size, 8);
        assert_eq!(header.file_format_version, 1);
        assert_eq!(header.file_version, 405);
        assert_eq!(header.header_size, 17);
        assert_eq!(header.bhead_type, BHeadType::LargeBHead8);
        assert_eq!(header.compression, CompressionKind::Zstd);
    }

    #[test]
    fn rejects_big_endian() {
        let err = parse_blend_header(b"BLENDER-V405", CompressionKind::None).unwrap_err();
        assert!(matches!(err, BlendError::UnsupportedEndian));
    }

    #[test]
    fn rejects_invalid_magic() {
        let err = parse_blend_header(b"NOTBLEND-v405", CompressionKind::None).unwrap_err();
        assert!(matches!(err, BlendError::InvalidMagic));
    }
}
