use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bhead::BlockCode;
use crate::error::{BlendError, Result};
use crate::header::CompressionKind;
use crate::input::load_path;
use crate::summary::summarize;
use crate::view::{BlendFile, FieldValue, StructView};

const DEFAULT_ZSTD_LEVEL: i32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum SceneSelector {
    First,
    Name(String),
    OldPtr(u64),
}

impl Default for SceneSelector {
    fn default() -> Self {
        Self::First
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneRenderEdit {
    pub resolution_x: Option<i32>,
    pub resolution_y: Option<i32>,
    pub frame_start: Option<i32>,
    pub frame_end: Option<i32>,
    pub frame_current: Option<i32>,
    pub fps: Option<f32>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOptions {
    #[serde(default = "default_backup")]
    pub backup: bool,
    #[serde(default)]
    pub thread_count: Option<usize>,
    #[serde(default)]
    pub zstd_level: Option<i32>,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            backup: true,
            thread_count: None,
            zstd_level: None,
        }
    }
}

fn default_backup() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub compression: CompressionKind,
    pub patch_count: usize,
    pub bytes_changed: usize,
    pub thread_count: usize,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlendPatch {
    pub offset: usize,
    pub old_bytes: Vec<u8>,
    pub new_bytes: Vec<u8>,
}

pub struct BlendEditSession {
    path: PathBuf,
    file: BlendFile,
    patches: Vec<BlendPatch>,
}

impl BlendEditSession {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = BlendFile::open(&path)?;
        Ok(Self {
            path,
            file,
            patches: Vec::new(),
        })
    }

    pub fn file(&self) -> &BlendFile {
        &self.file
    }

    pub fn patches(&self) -> &[BlendPatch] {
        &self.patches
    }

    pub fn edit_scene_render(
        &mut self,
        selector: SceneSelector,
        edit: SceneRenderEdit,
    ) -> Result<()> {
        let mut patches = Vec::new();
        {
            let scene = select_scene(&self.file, &selector)?;
            let render = scene
                .field("r")
                .and_then(|field| field.as_struct_view())
                .ok_or_else(|| missing_field(&scene, "r"))?;

            if let Some(value) = edit.resolution_x {
                patch_i32_or_i16_field(&self.file, &render, "xsch", value, &mut patches)?;
            }
            if let Some(value) = edit.resolution_y {
                patch_i32_or_i16_field(&self.file, &render, "ysch", value, &mut patches)?;
            }
            if let Some(value) = edit.frame_start {
                patch_i32_or_i16_field(&self.file, &render, "sfra", value, &mut patches)?;
            }
            if let Some(value) = edit.frame_end {
                patch_i32_or_i16_field(&self.file, &render, "efra", value, &mut patches)?;
            }
            if let Some(value) = edit.frame_current {
                patch_i32_or_i16_field(&self.file, &render, "cfra", value, &mut patches)?;
            }
            if let Some(value) = edit.fps {
                patch_fps(&self.file, &render, value, &mut patches)?;
            }
            if let Some(value) = edit.output_path {
                patch_render_output_path(&self.file, &render, &value, &mut patches)?;
            }
        }

        self.patches.extend(patches);
        Ok(())
    }

    pub fn commit(self, options: WriteOptions) -> Result<WriteReport> {
        let options = WriteOptions {
            backup: options.backup,
            thread_count: options.thread_count,
            zstd_level: options.zstd_level,
        };
        let path = self.path.clone();
        let compression = self.file.header().compression;
        let patch_count = self.patches.len();
        let bytes_changed = self.patches.iter().map(|patch| patch.new_bytes.len()).sum();
        let thread_count = normalized_thread_count(options.thread_count);

        let mut patches = self.patches;
        validate_patches(self.file.data(), &mut patches)?;

        if patches.is_empty() {
            let reopened = BlendFile::open(&path)?;
            summarize(&reopened).map_err(|error| BlendError::Verification(error.to_string()))?;
            return Ok(WriteReport {
                path,
                backup_path: None,
                compression,
                patch_count,
                bytes_changed,
                thread_count,
                verified: true,
            });
        }

        drop(self.file);

        let backup_path = if options.backup {
            Some(create_backup(&path)?)
        } else {
            None
        };

        let temp_path = temp_output_path(&path);
        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }

        match compression {
            CompressionKind::None => write_uncompressed_with_patches(&path, &temp_path, &patches),
            CompressionKind::Gzip | CompressionKind::Zstd => write_compressed_with_patches(
                &path,
                &temp_path,
                compression,
                &patches,
                options.zstd_level.unwrap_or(DEFAULT_ZSTD_LEVEL),
                thread_count,
            ),
        }?;

        verify_written_file(&temp_path)?;
        replace_file(&temp_path, &path)?;
        verify_written_file(&path)?;

        Ok(WriteReport {
            path,
            backup_path,
            compression,
            patch_count,
            bytes_changed,
            thread_count,
            verified: true,
        })
    }

}

fn select_scene<'a>(file: &'a BlendFile, selector: &SceneSelector) -> Result<StructView<'a>> {
    match selector {
        SceneSelector::First => first_scene(file),
        SceneSelector::Name(name) => {
            for block in file.ids() {
                if block.header().code != scene_code() {
                    continue;
                }
                let view = match block.struct_view() {
                    Ok(view) => view,
                    Err(_) => continue,
                };
                if scene_name(&view).as_deref() == Some(name.as_str()) {
                    return Ok(view);
                }
            }
            Err(BlendError::SceneNotFound {
                selector: format!("name={name}"),
            })
        }
        SceneSelector::OldPtr(old_ptr) => {
            let Some(block) = file.resolve_old_ptr(*old_ptr) else {
                return Err(BlendError::SceneNotFound {
                    selector: format!("old_ptr=0x{old_ptr:X}"),
                });
            };
            if block.header().code != scene_code() {
                return Err(BlendError::SceneNotFound {
                    selector: format!("old_ptr=0x{old_ptr:X}"),
                });
            }
            block.struct_view()
        }
    }
}

fn first_scene(file: &BlendFile) -> Result<StructView<'_>> {
    for block in file.ids() {
        if block.header().code == scene_code() {
            return block.struct_view();
        }
    }
    Err(BlendError::SceneNotFound {
        selector: "first".to_owned(),
    })
}

fn patch_i32_or_i16_field(
    file: &BlendFile,
    view: &StructView<'_>,
    field_name: &str,
    value: i32,
    patches: &mut Vec<BlendPatch>,
) -> Result<()> {
    let field = view
        .field(field_name)
        .ok_or_else(|| missing_field(view, field_name))?;
    let new_bytes = match field.field_def().size {
        2 => {
            let value = i16::try_from(value).map_err(|_| field_type_mismatch(view, field_name))?;
            value.to_le_bytes().to_vec()
        }
        4 => value.to_le_bytes().to_vec(),
        _ => return Err(field_type_mismatch(view, field_name)),
    };
    push_patch(patches, field_offset(file, &field)?, field.bytes(), new_bytes)
}

fn patch_fps(
    file: &BlendFile,
    render: &StructView<'_>,
    fps: f32,
    patches: &mut Vec<BlendPatch>,
) -> Result<()> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(field_type_mismatch(render, "frs_sec"));
    }

    let fps_integer = fps.round().max(1.0) as i32;
    patch_i32_or_i16_field(file, render, "frs_sec", fps_integer, patches)?;
    if let Some(base) = render.field("frs_sec_base") {
        if base.field_def().size == 4 {
            let base_value = (fps_integer as f32 / fps).max(0.0001);
            push_patch(
                patches,
                field_offset(file, &base)?,
                base.bytes(),
                base_value.to_le_bytes().to_vec(),
            )?;
        }
    }
    Ok(())
}

fn patch_render_output_path(
    file: &BlendFile,
    render: &StructView<'_>,
    value: &str,
    patches: &mut Vec<BlendPatch>,
) -> Result<()> {
    if render.field("filepath").is_some() {
        return patch_c_string_field(file, render, "filepath", value, patches);
    }
    patch_c_string_field(file, render, "pic", value, patches)
}

fn patch_c_string_field(
    file: &BlendFile,
    view: &StructView<'_>,
    field_name: &str,
    value: &str,
    patches: &mut Vec<BlendPatch>,
) -> Result<()> {
    let field = view
        .field(field_name)
        .ok_or_else(|| missing_field(view, field_name))?;
    if field.field_def().is_pointer || field.field_def().type_name != "char" {
        return Err(field_type_mismatch(view, field_name));
    }

    let value_bytes = value.as_bytes();
    let capacity = field.bytes().len();
    if value_bytes.len() >= capacity {
        return Err(BlendError::StringTooLong {
            field_name: field_name.to_owned(),
            actual: value_bytes.len(),
            capacity,
        });
    }

    let mut new_bytes = vec![0_u8; capacity];
    new_bytes[..value_bytes.len()].copy_from_slice(value_bytes);
    push_patch(patches, field_offset(file, &field)?, field.bytes(), new_bytes)
}

fn push_patch(
    patches: &mut Vec<BlendPatch>,
    offset: usize,
    old_bytes: &[u8],
    new_bytes: Vec<u8>,
) -> Result<()> {
    patches.push(BlendPatch {
        offset,
        old_bytes: old_bytes.to_vec(),
        new_bytes,
    });
    Ok(())
}

fn scene_code() -> BlockCode {
    BlockCode::from_raw(crate::bhead::blend_make_id(b'S', b'C', 0, 0))
}

fn scene_name(view: &StructView<'_>) -> Option<String> {
    view.field("id")?
        .as_struct_view()?
        .field("name")?
        .as_c_string()
        .map(|value| value.chars().skip(2).collect())
}

fn field_offset(file: &BlendFile, field: &FieldValue<'_>) -> Result<usize> {
    let base = file.data().as_ptr() as usize;
    let start = field.bytes().as_ptr() as usize;
    start
        .checked_sub(base)
        .ok_or(BlendError::InvalidPatchRange { start, end: start })
}

fn missing_field(view: &StructView<'_>, field_name: &str) -> BlendError {
    BlendError::MissingField {
        struct_name: view.struct_def().type_name.clone(),
        field_name: field_name.to_owned(),
    }
}

fn field_type_mismatch(view: &StructView<'_>, field_name: &str) -> BlendError {
    BlendError::FieldTypeMismatch {
        struct_name: view.struct_def().type_name.clone(),
        field_name: field_name.to_owned(),
    }
}

fn validate_patches(source: &[u8], patches: &mut [BlendPatch]) -> Result<()> {
    patches.sort_by_key(|patch| patch.offset);

    let mut previous_end = 0usize;
    for patch in patches {
        let end = patch
            .offset
            .checked_add(patch.old_bytes.len())
            .ok_or(BlendError::InvalidPatchRange {
                start: patch.offset,
                end: usize::MAX,
            })?;
        if end > source.len() || patch.old_bytes.len() != patch.new_bytes.len() {
            return Err(BlendError::InvalidPatchRange {
                start: patch.offset,
                end,
            });
        }
        if patch.offset < previous_end {
            return Err(BlendError::PatchConflict {
                offset: patch.offset,
            });
        }
        if source.get(patch.offset..end) != Some(patch.old_bytes.as_slice()) {
            return Err(BlendError::PatchOldBytesMismatch {
                offset: patch.offset,
            });
        }
        previous_end = end;
    }

    Ok(())
}

fn write_uncompressed_with_patches(
    source_path: &Path,
    temp_path: &Path,
    patches: &[BlendPatch],
) -> Result<()> {
    fs::copy(source_path, temp_path)?;
    let mut file = OpenOptions::new().read(true).write(true).open(temp_path)?;
    for patch in patches {
        file.seek(SeekFrom::Start(patch.offset as u64))?;
        file.write_all(&patch.new_bytes)?;
    }
    file.sync_all()?;
    Ok(())
}

fn write_compressed_with_patches(
    source_path: &Path,
    temp_path: &Path,
    compression: CompressionKind,
    patches: &[BlendPatch],
    zstd_level: i32,
    thread_count: usize,
) -> Result<()> {
    let (storage, detected) = load_path(source_path)?;
    if detected != compression {
        return Err(BlendError::CompressionWrite(format!(
            "compression changed while writing: expected {compression:?}, got {detected:?}"
        )));
    }
    let mut bytes = storage.as_slice().to_vec();
    for patch in patches {
        let end = patch.offset + patch.new_bytes.len();
        bytes[patch.offset..end].copy_from_slice(&patch.new_bytes);
    }

    let output = File::create(temp_path)?;
    let writer = BufWriter::new(output);
    match compression {
        CompressionKind::Gzip => {
            let mut encoder =
                flate2::write::GzEncoder::new(writer, flate2::Compression::default());
            encoder
                .write_all(&bytes)
                .map_err(|error| BlendError::CompressionWrite(error.to_string()))?;
            encoder
                .finish()
                .map_err(|error| BlendError::CompressionWrite(error.to_string()))?;
        }
        CompressionKind::Zstd => {
            let mut encoder = zstd::stream::Encoder::new(writer, zstd_level)
                .map_err(|error| BlendError::CompressionWrite(error.to_string()))?;
            encoder
                .multithread(thread_count as u32)
                .map_err(|error| BlendError::CompressionWrite(error.to_string()))?;
            encoder
                .write_all(&bytes)
                .map_err(|error| BlendError::CompressionWrite(error.to_string()))?;
            encoder
                .finish()
                .map_err(|error| BlendError::CompressionWrite(error.to_string()))?;
        }
        CompressionKind::None => unreachable!("uncompressed path is handled separately"),
    }

    Ok(())
}

fn verify_written_file(path: &Path) -> Result<()> {
    let file = BlendFile::open(path)?;
    summarize(&file).map_err(|error| BlendError::Verification(error.to_string()))?;
    Ok(())
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    let mut backup_path = backup_path(path);
    let mut index = 1usize;
    while backup_path.exists() {
        backup_path = backup_path.with_extension(format!(
            "{}.{}",
            backup_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("bak"),
            index
        ));
        index += 1;
    }
    fs::copy(path, &backup_path)?;
    Ok(backup_path)
}

fn backup_path(path: &Path) -> PathBuf {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file.blend");
    path.with_file_name(format!("{file_name}.pmc-bak-{timestamp}"))
}

fn temp_output_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file.blend");
    path.with_file_name(format!(
        ".{file_name}.pmc-write-{}.tmp",
        std::process::id()
    ))
}

fn replace_file(temp_path: &Path, target_path: &Path) -> Result<()> {
    match fs::rename(temp_path, target_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(target_path)?;
            fs::rename(temp_path, target_path)?;
            Ok(())
        }
        Err(error) => Err(BlendError::Io(error)),
    }
}

fn normalized_thread_count(value: Option<usize>) -> usize {
    value
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
        })
        .clamp(1, 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_overlapping_patches() {
        let mut patches = vec![
            BlendPatch {
                offset: 2,
                old_bytes: vec![2, 3],
                new_bytes: vec![9, 9],
            },
            BlendPatch {
                offset: 3,
                old_bytes: vec![3],
                new_bytes: vec![8],
            },
        ];
        let err = validate_patches(&[0, 1, 2, 3, 4], &mut patches).unwrap_err();
        assert!(matches!(err, BlendError::PatchConflict { .. }));
    }

    #[test]
    fn rejects_mismatched_old_bytes() {
        let mut patches = vec![BlendPatch {
            offset: 1,
            old_bytes: vec![8],
            new_bytes: vec![9],
        }];
        let err = validate_patches(&[0, 1, 2], &mut patches).unwrap_err();
        assert!(matches!(err, BlendError::PatchOldBytesMismatch { .. }));
    }

    #[test]
    fn normalizes_thread_count() {
        assert_eq!(normalized_thread_count(Some(0)), 1);
        assert_eq!(normalized_thread_count(Some(99)), 8);
        assert_eq!(normalized_thread_count(Some(4)), 4);
    }
}
