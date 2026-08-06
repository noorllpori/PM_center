use std::path::Path;

use serde::Serialize;

use crate::array_view::iter_listbase;
use crate::error::Result;
use crate::view::{BlendFile, StructView};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDataSummary {
    pub images: Vec<ExternalImage>,
    pub libraries: Vec<ExternalLibrary>,
    pub texts: Vec<ExternalText>,
    pub linked_ids: Vec<LinkedId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalImage {
    pub name: String,
    pub filepath: Option<String>,
    pub resolved_path: Option<String>,
    pub packed: bool,
    pub source_code: i32,
    pub source: String,
    pub image_type_code: i32,
    pub image_type: String,
    pub generated_width: i32,
    pub generated_height: i32,
    pub colorspace: Option<String>,
    pub library_path: Option<String>,
    pub is_external: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLibrary {
    pub name: String,
    pub filepath: Option<String>,
    pub resolved_path: Option<String>,
    pub packed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalText {
    pub name: String,
    pub filepath: Option<String>,
    pub resolved_path: Option<String>,
    pub line_count: usize,
    pub is_external: bool,
    pub library_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedId {
    pub code: String,
    pub kind: String,
    pub name: Option<String>,
    pub library_path: Option<String>,
}

pub fn collect_external_data(file: &BlendFile) -> Result<ExternalDataSummary> {
    collect_external_data_with_base(file, None)
}

pub fn collect_external_data_with_base(
    file: &BlendFile,
    blend_path: Option<&Path>,
) -> Result<ExternalDataSummary> {
    let mut images = Vec::new();
    let mut libraries = Vec::new();
    let mut texts = Vec::new();
    let mut linked_ids = Vec::new();

    for block in file.ids() {
        let code = block.header().code.as_string();
        let view = match block.struct_view() {
            Ok(view) => view,
            Err(_) => continue,
        };

        match code.as_str() {
            "IM" => images.push(external_image(file, &view, blend_path)),
            "LI" => libraries.push(external_library(&view, blend_path)),
            "TX" => texts.push(external_text(file, &view, blend_path)),
            _ => {
                if let Some(library_path) = library_path(file, &view) {
                    linked_ids.push(LinkedId {
                        code: code.clone(),
                        kind: id_code_label(&code).to_owned(),
                        name: raw_id_name(&view).map(|value| strip_id_prefix(&value)),
                        library_path: Some(library_path),
                    });
                }
            }
        }
    }

    Ok(ExternalDataSummary {
        images,
        libraries,
        texts,
        linked_ids,
    })
}

fn external_image(
    file: &BlendFile,
    view: &StructView<'_>,
    blend_path: Option<&Path>,
) -> ExternalImage {
    let filepath = first_c_string(view, &["filepath_abs", "filepath", "name"]);
    let packed = has_packed_file(file, view);
    let source_code = field_numeric_code(view, "source").unwrap_or_default();
    let image_type_code = field_numeric_code(view, "type").unwrap_or_default();
    let is_external = filepath
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
        && !packed
        && matches!(source_code, 1 | 2 | 3 | 6);

    ExternalImage {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Image".to_owned()),
        resolved_path: filepath
            .as_deref()
            .and_then(|value| resolve_blender_path(value, blend_path)),
        filepath,
        packed,
        source_code,
        source: image_source_name(source_code).to_owned(),
        image_type_code,
        image_type: image_type_name(image_type_code).to_owned(),
        generated_width: field_i32(view, "gen_x").unwrap_or_default(),
        generated_height: field_i32(view, "gen_y").unwrap_or_default(),
        colorspace: view
            .field("colorspace_settings")
            .and_then(|field| field.as_struct_view())
            .and_then(|settings| field_c_string(&settings, "name")),
        library_path: library_path(file, view),
        is_external,
    }
}

fn external_library(view: &StructView<'_>, blend_path: Option<&Path>) -> ExternalLibrary {
    let filepath = first_c_string(view, &["filepath_abs", "filepath", "name"]);

    ExternalLibrary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Library".to_owned()),
        resolved_path: filepath
            .as_deref()
            .and_then(|value| resolve_blender_path(value, blend_path)),
        filepath,
        packed: field_pointer(view, "packedfile").is_some(),
    }
}

fn external_text(
    file: &BlendFile,
    view: &StructView<'_>,
    blend_path: Option<&Path>,
) -> ExternalText {
    let filepath = first_pointer_string(file, view, &["filepath", "name"]);

    ExternalText {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Text".to_owned()),
        resolved_path: filepath
            .as_deref()
            .and_then(|value| resolve_blender_path(value, blend_path)),
        filepath: filepath.clone(),
        line_count: count_listbase(file, view, "lines"),
        is_external: filepath.is_some(),
        library_path: library_path(file, view),
    }
}

fn has_packed_file(file: &BlendFile, view: &StructView<'_>) -> bool {
    if field_pointer(view, "packedfile").is_some() {
        return true;
    }

    view.field("packedfiles")
        .and_then(|field| field.as_struct_view())
        .and_then(|list| {
            list.field("first")
                .and_then(|field| field.as_pointer())
                .filter(|ptr| *ptr != 0)
        })
        .and_then(|ptr| file.resolve_old_ptr(ptr))
        .is_some()
}

fn count_listbase(file: &BlendFile, parent: &StructView<'_>, field_name: &str) -> usize {
    let Some(list) = parent
        .field(field_name)
        .and_then(|field| field.as_struct_view())
    else {
        return 0;
    };
    iter_listbase(file, &list)
        .map(|blocks| blocks.len())
        .unwrap_or(0)
}

fn first_c_string(view: &StructView<'_>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| field_c_string(view, name))
}

fn first_pointer_string(file: &BlendFile, view: &StructView<'_>, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| field_pointer_string(file, view, name))
}

fn raw_id_name(view: &StructView<'_>) -> Option<String> {
    view.field("id")?
        .as_struct_view()?
        .field("name")?
        .as_c_string()
}

fn strip_id_prefix(value: &str) -> String {
    value.chars().skip(2).collect()
}

fn field_i32(view: &StructView<'_>, name: &str) -> Option<i32> {
    view.field(name)?.as_i32()
}

fn field_numeric_code(view: &StructView<'_>, name: &str) -> Option<i32> {
    let field = view.field(name)?;
    match field.field_def().size {
        1 => field.as_u8().map(i32::from),
        2 => field.as_i16().map(i32::from),
        4 => field.as_i32(),
        _ => None,
    }
}

fn field_c_string(view: &StructView<'_>, name: &str) -> Option<String> {
    view.field(name)?
        .as_c_string()
        .filter(|value| !value.trim().is_empty())
}

fn field_pointer(view: &StructView<'_>, name: &str) -> Option<u64> {
    view.field(name)?.as_pointer().filter(|ptr| *ptr != 0)
}

fn field_pointer_string(file: &BlendFile, view: &StructView<'_>, name: &str) -> Option<String> {
    let ptr = field_pointer(view, name)?;
    file.read_c_string_at_ptr(ptr)
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
}

fn library_path(file: &BlendFile, view: &StructView<'_>) -> Option<String> {
    let id_view = view.field("id")?.as_struct_view()?;
    let lib_ptr = id_view.field("lib")?.as_pointer()?;
    let library_block = file.resolve_old_ptr(lib_ptr)?;
    let library_view = library_block.struct_view().ok()?;
    first_c_string(&library_view, &["filepath_abs", "filepath", "name"])
}

fn resolve_blender_path(value: &str, blend_path: Option<&Path>) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(relative) = trimmed.strip_prefix("//") {
        let parent = blend_path?.parent()?;
        return Some(parent.join(relative).to_string_lossy().to_string());
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Some(trimmed.to_owned());
    }

    blend_path
        .and_then(Path::parent)
        .map(|parent| parent.join(path).to_string_lossy().to_string())
}

fn image_source_name(value: i32) -> &'static str {
    match value {
        1 => "File",
        2 => "Sequence",
        3 => "Movie",
        4 => "Generated",
        5 => "Viewer",
        6 => "Tiled",
        _ => "Unknown",
    }
}

fn image_type_name(value: i32) -> &'static str {
    match value {
        0 => "Image",
        1 => "Multilayer",
        2 => "UvTest",
        4 => "RenderResult",
        5 => "Compositing",
        _ => "Unknown",
    }
}

fn id_code_label(code: &str) -> &'static str {
    match code {
        "SC" => "Scene",
        "OB" => "Object",
        "ME" => "Mesh",
        "CU" => "Curve",
        "CA" => "Camera",
        "LA" => "Light",
        "MA" => "Material",
        "TE" => "Texture",
        "IM" => "Image",
        "LI" => "Library",
        "WO" => "World",
        "GR" => "Collection",
        "AC" => "Action",
        "TX" => "Text",
        "NT" => "NodeTree",
        "AR" => "Armature",
        "IP" => "Ipo",
        _ => "ID",
    }
}
