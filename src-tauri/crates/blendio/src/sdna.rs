use std::collections::HashMap;

use serde::Serialize;

use crate::error::{BlendError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    pub pointer_size: u8,
    pub names_count: usize,
    pub types_count: usize,
    pub structs: Vec<StructDef>,
    #[serde(skip)]
    names: Vec<String>,
    #[serde(skip)]
    types: Vec<String>,
    #[serde(skip)]
    type_lengths: Vec<u16>,
    #[serde(skip)]
    struct_lookup: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructDef {
    pub index: usize,
    pub type_name: String,
    pub size: usize,
    pub fields: Vec<FieldDef>,
    #[serde(skip)]
    field_lookup: HashMap<String, usize>,
    #[serde(skip)]
    normalized_lookup: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDef {
    pub name: String,
    pub normalized_name: String,
    pub type_name: String,
    pub type_index: usize,
    pub offset: usize,
    pub size: usize,
    pub array_len: usize,
    pub is_pointer: bool,
    pub is_function_pointer: bool,
    pub struct_index: Option<usize>,
}

impl Schema {
    pub fn parse(bytes: &[u8], pointer_size: u8) -> Result<Self> {
        if bytes.len() < 8 || &bytes[0..4] != b"SDNA" || &bytes[4..8] != b"NAME" {
            return Err(BlendError::InvalidSdna("missing SDNA/NAME header"));
        }

        let mut cursor = 8;
        let names_count = read_u32(bytes, &mut cursor)? as usize;
        let names = read_strings(bytes, &mut cursor, names_count)?;
        cursor = align4(cursor);

        expect_tag(bytes, &mut cursor, b"TYPE")?;
        let types_count = read_u32(bytes, &mut cursor)? as usize;
        let types = read_strings(bytes, &mut cursor, types_count)?;
        cursor = align4(cursor);

        expect_tag(bytes, &mut cursor, b"TLEN")?;
        let mut type_lengths = Vec::with_capacity(types_count);
        for _ in 0..types_count {
            type_lengths.push(read_u16(bytes, &mut cursor)?);
        }
        cursor = align4(cursor);

        expect_tag(bytes, &mut cursor, b"STRC")?;
        let struct_count = read_u32(bytes, &mut cursor)? as usize;

        let mut raw_structs = Vec::with_capacity(struct_count);
        for _ in 0..struct_count {
            let type_index = read_u16(bytes, &mut cursor)? as usize;
            let field_count = read_u16(bytes, &mut cursor)? as usize;
            let mut fields = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                let field_type = read_u16(bytes, &mut cursor)? as usize;
                let field_name = read_u16(bytes, &mut cursor)? as usize;
                fields.push((field_type, field_name));
            }
            raw_structs.push((type_index, fields));
        }

        let mut struct_lookup = HashMap::with_capacity(raw_structs.len());
        for (index, (type_index, _)) in raw_structs.iter().enumerate() {
            let type_name = types
                .get(*type_index)
                .ok_or(BlendError::InvalidSdna("struct type index out of bounds"))?;
            struct_lookup.insert(type_name.clone(), index);
        }

        let structs = raw_structs
            .into_iter()
            .enumerate()
            .map(|(index, (type_index, fields))| {
                let type_name = types
                    .get(type_index)
                    .ok_or(BlendError::InvalidSdna("struct type index out of bounds"))?
                    .clone();
                let size = *type_lengths
                    .get(type_index)
                    .ok_or(BlendError::InvalidSdna("type length index out of bounds"))?
                    as usize;
                let mut offset = 0usize;
                let mut field_defs = Vec::with_capacity(fields.len());
                let mut field_lookup = HashMap::with_capacity(fields.len());
                let mut normalized_lookup = HashMap::with_capacity(fields.len());

                for (field_type_index, field_name_index) in fields {
                    let name = names
                        .get(field_name_index)
                        .ok_or(BlendError::InvalidSdna("field name index out of bounds"))?
                        .clone();
                    let normalized_name = normalize_field_name(&name);
                    let type_name_for_field = types
                        .get(field_type_index)
                        .ok_or(BlendError::InvalidSdna("field type index out of bounds"))?
                        .clone();
                    let array_len = field_array_len(&name);
                    let is_function_pointer = name.starts_with("(*");
                    let is_pointer = is_function_pointer || name.starts_with('*');
                    let base_size = if is_pointer {
                        pointer_size as usize
                    } else {
                        *type_lengths
                            .get(field_type_index)
                            .ok_or(BlendError::InvalidSdna(
                                "field type length index out of bounds",
                            ))? as usize
                    };
                    let size = base_size.saturating_mul(array_len);
                    let struct_index = if is_pointer {
                        None
                    } else {
                        struct_lookup.get(&type_name_for_field).copied()
                    };

                    field_lookup.insert(name.clone(), field_defs.len());
                    normalized_lookup
                        .entry(normalized_name.clone())
                        .or_insert(field_defs.len());

                    field_defs.push(FieldDef {
                        name,
                        normalized_name,
                        type_name: type_name_for_field,
                        type_index: field_type_index,
                        offset,
                        size,
                        array_len,
                        is_pointer,
                        is_function_pointer,
                        struct_index,
                    });
                    offset = offset.saturating_add(size);
                }

                Ok(StructDef {
                    index,
                    type_name,
                    size,
                    fields: field_defs,
                    field_lookup,
                    normalized_lookup,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            pointer_size,
            names_count,
            types_count,
            structs,
            names,
            types,
            type_lengths,
            struct_lookup,
        })
    }

    pub fn struct_by_name(&self, name: &str) -> Option<&StructDef> {
        let index = self.struct_lookup.get(name)?;
        self.structs.get(*index)
    }

    pub fn struct_by_index(&self, index: usize) -> Option<&StructDef> {
        self.structs.get(index)
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn types(&self) -> &[String] {
        &self.types
    }

    pub fn field_type_length(&self, index: usize) -> Option<u16> {
        self.type_lengths.get(index).copied()
    }
}

impl StructDef {
    pub fn field(&self, name: &str) -> Option<&FieldDef> {
        if let Some(index) = self.field_lookup.get(name) {
            return self.fields.get(*index);
        }

        self.normalized_lookup
            .get(name)
            .and_then(|index| self.fields.get(*index))
    }
}

fn expect_tag(bytes: &[u8], cursor: &mut usize, tag: &[u8; 4]) -> Result<()> {
    let end = cursor
        .checked_add(4)
        .ok_or(BlendError::InvalidSdna("cursor overflow"))?;

    if end > bytes.len() {
        return Err(BlendError::InvalidSdna("unexpected end of SDNA"));
    }

    if &bytes[*cursor..end] != tag {
        return Err(BlendError::InvalidSdnaOwned(format!(
            "expected {:?}",
            std::str::from_utf8(tag).unwrap_or("tag")
        )));
    }

    *cursor = end;
    Ok(())
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    let end = cursor
        .checked_add(4)
        .ok_or(BlendError::InvalidSdna("cursor overflow"))?;
    if end > bytes.len() {
        return Err(BlendError::InvalidSdna("unexpected end of SDNA"));
    }
    let value = u32::from_le_bytes(bytes[*cursor..end].try_into().unwrap());
    *cursor = end;
    Ok(value)
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16> {
    let end = cursor
        .checked_add(2)
        .ok_or(BlendError::InvalidSdna("cursor overflow"))?;
    if end > bytes.len() {
        return Err(BlendError::InvalidSdna("unexpected end of SDNA"));
    }
    let value = u16::from_le_bytes(bytes[*cursor..end].try_into().unwrap());
    *cursor = end;
    Ok(value)
}

fn read_strings(bytes: &[u8], cursor: &mut usize, count: usize) -> Result<Vec<String>> {
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let start = *cursor;
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|index| start + index)
            .ok_or(BlendError::InvalidSdna("unterminated SDNA string"))?;
        let value = std::str::from_utf8(&bytes[start..end])
            .map_err(|_| BlendError::InvalidSdna("invalid UTF-8 in SDNA string"))?;
        values.push(value.to_owned());
        *cursor = end + 1;
    }
    Ok(values)
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

fn normalize_field_name(name: &str) -> String {
    let without_arrays = name.split('[').next().unwrap_or(name);
    if let Some(inner) = without_arrays
        .strip_prefix("(*")
        .and_then(|value| value.split(')').next())
    {
        return inner.to_owned();
    }

    without_arrays.trim_start_matches('*').to_owned()
}

fn field_array_len(name: &str) -> usize {
    let mut len = 1usize;
    let mut rest = name;
    while let Some(start) = rest.find('[') {
        let rest_after = &rest[start + 1..];
        if let Some(end) = rest_after.find(']') {
            if let Ok(value) = rest_after[..end].parse::<usize>() {
                len = len.saturating_mul(value.max(1));
            }
            rest = &rest_after[end + 1..];
        } else {
            break;
        }
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sdna(pointer_size: u8) -> Vec<u8> {
        let names = ["value", "*ptr", "name[4]"];
        let types = ["int", "void", "char", "MyStruct"];
        let lengths = [4_u16, 0_u16, 1_u16, 0_u16];

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SDNANAME");
        bytes.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for name in &names {
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(0);
        }
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }

        bytes.extend_from_slice(b"TYPE");
        bytes.extend_from_slice(&(types.len() as u32).to_le_bytes());
        for ty in &types {
            bytes.extend_from_slice(ty.as_bytes());
            bytes.push(0);
        }
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }

        bytes.extend_from_slice(b"TLEN");
        let struct_len = 4 + pointer_size as u16 + 4;
        for (index, len) in lengths.iter().enumerate() {
            let value = if index == 3 { struct_len } else { *len };
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }

        bytes.extend_from_slice(b"STRC");
        bytes.extend_from_slice(&(1_u32).to_le_bytes());
        bytes.extend_from_slice(&(3_u16).to_le_bytes());
        bytes.extend_from_slice(&(3_u16).to_le_bytes());
        bytes.extend_from_slice(&(0_u16).to_le_bytes());
        bytes.extend_from_slice(&(0_u16).to_le_bytes());
        bytes.extend_from_slice(&(1_u16).to_le_bytes());
        bytes.extend_from_slice(&(1_u16).to_le_bytes());
        bytes.extend_from_slice(&(2_u16).to_le_bytes());
        bytes.extend_from_slice(&(2_u16).to_le_bytes());
        bytes
    }

    #[test]
    fn parses_sdna_sections_and_offsets() {
        let schema = Schema::parse(&build_sdna(8), 8).unwrap();
        assert_eq!(schema.names_count, 3);
        assert_eq!(schema.types_count, 4);
        let my_struct = schema.struct_by_name("MyStruct").unwrap();
        assert_eq!(my_struct.size, 16);
        assert_eq!(my_struct.field("value").unwrap().offset, 0);
        assert_eq!(my_struct.field("ptr").unwrap().offset, 4);
        assert_eq!(my_struct.field("name").unwrap().offset, 12);
        assert!(my_struct.field("ptr").unwrap().is_pointer);
        assert_eq!(my_struct.field("name").unwrap().array_len, 4);
    }

    #[test]
    fn errors_on_invalid_sdna() {
        let err = Schema::parse(b"NOPE", 8).unwrap_err();
        assert!(matches!(err, BlendError::InvalidSdna(_)));
    }
}
