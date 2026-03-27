use std::collections::HashMap;
use std::path::Path;

use crate::bhead::{BlockCode, BlockHeader, parse_blocks};
use crate::error::{BlendError, Result};
use crate::header::{CompressionKind, parse_blend_header};
use crate::input::{BlendBytes, load_path};
use crate::sdna::{FieldDef, Schema, StructDef};

pub struct BlendFile {
    storage: BlendBytes,
    header: crate::header::BlendHeader,
    blocks: Vec<BlockHeader>,
    schema: Schema,
    old_ptr_index: HashMap<u64, usize>,
}

#[derive(Clone, Copy)]
pub struct BlockRef<'a> {
    file: &'a BlendFile,
    index: usize,
}

#[derive(Clone, Copy)]
pub struct StructView<'a> {
    file: &'a BlendFile,
    struct_def: &'a StructDef,
    bytes: &'a [u8],
}

#[derive(Clone, Copy)]
pub struct FieldValue<'a> {
    file: &'a BlendFile,
    field: &'a FieldDef,
    bytes: &'a [u8],
}

impl BlendFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let (storage, compression) = load_path(path.as_ref())?;
        Self::from_storage(storage, compression)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Self::from_storage(BlendBytes::Owned(bytes), CompressionKind::None)
    }

    pub fn header(&self) -> &crate::header::BlendHeader {
        &self.header
    }

    pub fn blocks(&self) -> &[BlockHeader] {
        &self.blocks
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn ids(&self) -> Vec<BlockRef<'_>> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| block.is_id().then_some(BlockRef { file: self, index }))
            .collect()
    }

    pub fn resolve_old_ptr(&self, old_ptr: u64) -> Option<BlockRef<'_>> {
        let index = self.old_ptr_index.get(&old_ptr)?;
        Some(BlockRef {
            file: self,
            index: *index,
        })
    }

    pub fn block(&self, index: usize) -> Option<BlockRef<'_>> {
        self.blocks.get(index)?;
        Some(BlockRef { file: self, index })
    }

    pub fn view_old_ptr_as_struct(
        &self,
        old_ptr: u64,
        struct_name: &str,
    ) -> Result<Option<StructView<'_>>> {
        if old_ptr == 0 {
            return Ok(None);
        }
        let Some(block) = self.resolve_old_ptr(old_ptr) else {
            return Ok(None);
        };
        block.view_as(struct_name).map(Some)
    }

    pub fn read_c_string_at_ptr(&self, old_ptr: u64) -> Result<Option<String>> {
        if old_ptr == 0 {
            return Ok(None);
        }
        let Some(block) = self.resolve_old_ptr(old_ptr) else {
            return Ok(None);
        };
        let bytes = block.bytes();
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
        Ok(Some(String::from_utf8_lossy(&bytes[..end]).into_owned()))
    }

    fn from_storage(storage: BlendBytes, compression: CompressionKind) -> Result<Self> {
        let bytes = storage.as_slice();
        let header = parse_blend_header(bytes, compression)?;
        let blocks = parse_blocks(bytes, &header)?;
        let dna_block = blocks
            .iter()
            .find(|block| block.code == BlockCode::DNA1)
            .ok_or(BlendError::MissingDnaBlock)?;
        let dna_end = dna_block
            .payload_offset
            .checked_add(dna_block.len as usize)
            .ok_or(BlendError::TruncatedBlock {
                offset: dna_block.file_offset,
            })?;
        let schema = Schema::parse(
            &bytes[dna_block.payload_offset..dna_end],
            header.pointer_size,
        )?;
        let old_ptr_index = blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| (block.old_ptr != 0).then_some((block.old_ptr, index)))
            .collect();

        Ok(Self {
            storage,
            header,
            blocks,
            schema,
            old_ptr_index,
        })
    }

    fn data(&self) -> &[u8] {
        self.storage.as_slice()
    }

    fn block_bytes(&self, index: usize) -> &[u8] {
        let block = &self.blocks[index];
        let end = block.payload_offset + block.len as usize;
        &self.data()[block.payload_offset..end]
    }
}

impl<'a> BlockRef<'a> {
    pub fn header(&self) -> &'a BlockHeader {
        &self.file.blocks[self.index]
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.file.block_bytes(self.index)
    }

    pub fn struct_def(&self) -> Option<&'a StructDef> {
        self.file
            .schema
            .struct_by_index(self.header().sdna_index as usize)
    }

    pub fn struct_view(&self) -> Result<StructView<'a>> {
        let struct_def = self
            .struct_def()
            .ok_or_else(|| BlendError::NonStructBlock {
                code: self.header().code.as_string(),
                offset: self.header().file_offset,
            })?;
        let expected_len = struct_def.size.saturating_mul(self.header().count as usize);
        if self.header().count == 0 || expected_len != self.header().len as usize {
            return Err(BlendError::NonStructBlock {
                code: self.header().code.as_string(),
                offset: self.header().file_offset,
            });
        }
        Ok(StructView {
            file: self.file,
            struct_def,
            bytes: &self.bytes()[0..struct_def.size],
        })
    }

    pub fn view_as(&self, struct_name: &str) -> Result<StructView<'a>> {
        let struct_def =
            self.file
                .schema
                .struct_by_name(struct_name)
                .ok_or_else(|| BlendError::InvalidSdnaOwned(format!(
                    "unknown struct type {struct_name}"
                )))?;
        if self.bytes().len() < struct_def.size {
            return Err(BlendError::NonStructBlock {
                code: self.header().code.as_string(),
                offset: self.header().file_offset,
            });
        }
        Ok(StructView {
            file: self.file,
            struct_def,
            bytes: &self.bytes()[0..struct_def.size],
        })
    }
}

impl<'a> StructView<'a> {
    pub(crate) fn from_parts(
        file: &'a BlendFile,
        struct_def: &'a StructDef,
        bytes: &'a [u8],
    ) -> Self {
        Self {
            file,
            struct_def,
            bytes,
        }
    }

    pub fn struct_def(&self) -> &'a StructDef {
        self.struct_def
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn field(&self, name: &str) -> Option<FieldValue<'a>> {
        let field = self.struct_def.field(name)?;
        let end = field.offset.checked_add(field.size)?;
        let bytes = self.bytes.get(field.offset..end)?;
        Some(FieldValue {
            file: self.file,
            field,
            bytes,
        })
    }

    pub fn file(&self) -> &'a BlendFile {
        self.file
    }
}

impl<'a> FieldValue<'a> {
    pub fn field_def(&self) -> &'a FieldDef {
        self.field
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn as_struct_view(&self) -> Option<StructView<'a>> {
        let struct_index = self.field.struct_index?;
        let struct_def = self.file.schema.struct_by_index(struct_index)?;
        Some(StructView {
            file: self.file,
            struct_def,
            bytes: self.bytes,
        })
    }

    pub fn as_c_string(&self) -> Option<String> {
        if self.field.is_pointer || self.field.type_name != "char" {
            return None;
        }

        let end = self
            .bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.bytes.len());
        Some(String::from_utf8_lossy(&self.bytes[..end]).into_owned())
    }

    pub fn as_pointer(&self) -> Option<u64> {
        if !self.field.is_pointer {
            return None;
        }

        match self.file.header.pointer_size {
            4 => Some(u32::from_le_bytes(self.bytes.get(0..4)?.try_into().ok()?) as u64),
            8 => Some(u64::from_le_bytes(self.bytes.get(0..8)?.try_into().ok()?)),
            _ => None,
        }
    }

    pub fn as_i16(&self) -> Option<i16> {
        Some(i16::from_le_bytes(self.bytes.get(0..2)?.try_into().ok()?))
    }

    pub fn as_i32(&self) -> Option<i32> {
        Some(i32::from_le_bytes(self.bytes.get(0..4)?.try_into().ok()?))
    }

    pub fn as_u16(&self) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes.get(0..2)?.try_into().ok()?))
    }

    pub fn as_u8(&self) -> Option<u8> {
        self.bytes.first().copied()
    }

    pub fn as_u32(&self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes.get(0..4)?.try_into().ok()?))
    }

    pub fn as_f32(&self) -> Option<f32> {
        Some(f32::from_le_bytes(self.bytes.get(0..4)?.try_into().ok()?))
    }

    pub fn as_f32_array<const N: usize>(&self) -> Option<[f32; N]> {
        let total = N.checked_mul(4)?;
        let bytes = self.bytes.get(0..total)?;
        let mut values = [0.0_f32; N];
        for (index, value) in values.iter_mut().enumerate() {
            let start = index * 4;
            *value = f32::from_le_bytes(bytes[start..start + 4].try_into().ok()?);
        }
        Some(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bhead::blend_make_id;

    #[test]
    fn blend_file_requires_dna1() {
        let mut bytes = b"BLENDER-v405".to_vec();
        bytes.extend_from_slice(&blend_make_id(b'E', b'N', b'D', b'B').to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u64).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());

        let err = BlendFile::from_bytes(bytes).err().unwrap();
        assert!(matches!(err, BlendError::MissingDnaBlock));
    }
}
