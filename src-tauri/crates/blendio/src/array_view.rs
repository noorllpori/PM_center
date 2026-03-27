use std::collections::HashSet;

use crate::error::{BlendError, Result};
use crate::sdna::StructDef;
use crate::view::{BlendFile, BlockRef, StructView};

#[derive(Clone, Copy)]
pub struct StructArrayView<'a> {
    file: &'a BlendFile,
    struct_def: &'a StructDef,
    bytes: &'a [u8],
    len: usize,
}

#[derive(Clone, Copy)]
pub struct PointerArrayView<'a> {
    file: &'a BlendFile,
    bytes: &'a [u8],
    len: usize,
}

pub fn read_struct_array<'a>(
    file: &'a BlendFile,
    old_ptr: u64,
    struct_name: &str,
    count: usize,
) -> Result<Option<StructArrayView<'a>>> {
    if old_ptr == 0 || count == 0 {
        return Ok(None);
    }

    let Some(block) = file.resolve_old_ptr(old_ptr) else {
        return Ok(None);
    };
    let struct_def =
        file.schema()
            .struct_by_name(struct_name)
            .ok_or_else(|| BlendError::InvalidSdnaOwned(format!("unknown struct type {struct_name}")))?;
    let expected_len = struct_def
        .size
        .checked_mul(count)
        .ok_or_else(|| BlendError::Export(format!("array size overflow for {struct_name}")))?;
    if block.bytes().len() < expected_len {
        return Err(BlendError::TruncatedBlock {
            offset: block.header().file_offset,
        });
    }

    Ok(Some(StructArrayView {
        file,
        struct_def,
        bytes: &block.bytes()[..expected_len],
        len: count,
    }))
}

pub fn read_pointer_array<'a>(
    file: &'a BlendFile,
    old_ptr: u64,
    count: usize,
) -> Result<Option<PointerArrayView<'a>>> {
    if old_ptr == 0 || count == 0 {
        return Ok(None);
    }

    let Some(block) = file.resolve_old_ptr(old_ptr) else {
        return Ok(None);
    };
    let stride = file.header().pointer_size as usize;
    let expected_len = stride
        .checked_mul(count)
        .ok_or_else(|| BlendError::Export("pointer array size overflow".to_owned()))?;
    if block.bytes().len() < expected_len {
        return Err(BlendError::TruncatedBlock {
            offset: block.header().file_offset,
        });
    }

    Ok(Some(PointerArrayView {
        file,
        bytes: &block.bytes()[..expected_len],
        len: count,
    }))
}

pub fn iter_listbase<'a>(file: &'a BlendFile, list: &StructView<'a>) -> Result<Vec<BlockRef<'a>>> {
    let Some(first) = list.field("first").and_then(|field| field.as_pointer()) else {
        return Ok(Vec::new());
    };
    if first == 0 {
        return Ok(Vec::new());
    }

    let mut current = first;
    let mut visited = HashSet::new();
    let mut items = Vec::new();

    while current != 0 {
        if !visited.insert(current) {
            return Err(BlendError::Export(format!(
                "list cycle detected at old pointer 0x{current:X}"
            )));
        }

        let block = file
            .resolve_old_ptr(current)
            .ok_or(BlendError::MissingOldPointer { ptr: current })?;
        let bytes = block.bytes();
        let pointer_size = file.header().pointer_size as usize;
        if bytes.len() < pointer_size {
            return Err(BlendError::TruncatedBlock {
                offset: block.header().file_offset,
            });
        }
        items.push(block);
        current = read_pointer(bytes, file.header().pointer_size);
    }

    Ok(items)
}

impl<'a> StructArrayView<'a> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, index: usize) -> Option<StructView<'a>> {
        if index >= self.len {
            return None;
        }
        let start = index.checked_mul(self.struct_def.size)?;
        let end = start.checked_add(self.struct_def.size)?;
        Some(StructView::from_parts(
            self.file,
            self.struct_def,
            self.bytes.get(start..end)?,
        ))
    }

    pub fn iter(&self) -> impl Iterator<Item = StructView<'a>> + '_ {
        (0..self.len).filter_map(move |index| self.get(index))
    }
}

impl<'a> PointerArrayView<'a> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get(&self, index: usize) -> Option<u64> {
        if index >= self.len {
            return None;
        }
        let stride = self.file.header().pointer_size as usize;
        let start = index.checked_mul(stride)?;
        Some(read_pointer(
            self.bytes.get(start..start + stride)?,
            self.file.header().pointer_size,
        ))
    }

    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        (0..self.len).filter_map(move |index| self.get(index))
    }
}

fn read_pointer(bytes: &[u8], pointer_size: u8) -> u64 {
    match pointer_size {
        4 => u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as u64,
        8 => u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        other => panic!("unsupported pointer size {other}"),
    }
}
