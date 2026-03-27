use std::collections::BTreeMap;

use serde::Serialize;

use crate::array_view::{iter_listbase, read_struct_array};
use crate::error::Result;
use crate::view::{BlendFile, BlockRef, StructView};

#[derive(Debug, Clone, Serialize)]
pub struct SchemaCounts {
    pub names: usize,
    pub types: usize,
    pub structs: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockCodeSummary {
    pub code: String,
    pub count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdCodeSummary {
    pub code: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdReference {
    pub code: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneSummary {
    pub name: String,
    pub camera: Option<String>,
    pub world: Option<String>,
    pub master_collection: Option<String>,
    pub frame_current: i32,
    pub frame_start: i32,
    pub frame_end: i32,
    pub fps: f32,
    pub resolution_x: i32,
    pub resolution_y: i32,
    pub render_engine: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModifierSummary {
    pub name: String,
    pub modifier_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectSummary {
    pub name: String,
    pub object_type: String,
    pub data_target: Option<IdReference>,
    pub parent: Option<String>,
    pub library_path: Option<String>,
    pub location: [f32; 3],
    pub rotation_euler: [f32; 3],
    pub scale: [f32; 3],
    pub rotation_mode: String,
    pub material_slot_count: i32,
    pub has_animation: bool,
    pub action: Option<String>,
    pub modifiers: Vec<ModifierSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshSummary {
    pub name: String,
    pub totvert: i32,
    pub totedge: i32,
    pub totloop: i32,
    pub totpoly: i32,
    pub totface: i32,
    pub material_slot_count: i32,
    pub has_legacy_mvert: bool,
    pub has_legacy_mloop: bool,
    pub has_legacy_mpoly: bool,
    pub has_poly_offset_indices: bool,
    pub active_uv_map_attribute: Option<String>,
    pub default_uv_map_attribute: Option<String>,
    pub vertex_attributes: Vec<String>,
    pub edge_attributes: Vec<String>,
    pub poly_attributes: Vec<String>,
    pub loop_attributes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamedIdSummary {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibrarySummary {
    pub name: String,
    pub filepath: Option<String>,
    pub packed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageSummary {
    pub name: String,
    pub filepath: Option<String>,
    pub packed: bool,
    pub source_code: i32,
    pub image_type_code: i32,
    pub generated_width: i32,
    pub generated_height: i32,
    pub colorspace: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionSummary {
    pub name: String,
    pub frame_start: f32,
    pub frame_end: f32,
    pub id_root: i32,
    pub layer_count: i32,
    pub slot_count: i32,
    pub curve_count: usize,
    pub marker_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextSummary {
    pub name: String,
    pub filepath: Option<String>,
    pub line_count: usize,
    pub is_external: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CameraSummary {
    pub name: String,
    pub camera_type: String,
    pub lens: f32,
    pub ortho_scale: f32,
    pub clip_start: f32,
    pub clip_end: f32,
    pub sensor_x: f32,
    pub sensor_y: f32,
    pub shift_x: f32,
    pub shift_y: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LightSummary {
    pub name: String,
    pub light_type: String,
    pub color: [f32; 3],
    pub energy: f32,
    pub range: f32,
    pub radius: f32,
    pub spot_size: f32,
    pub spot_blend: f32,
    pub sun_angle: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialSummary {
    pub name: String,
    pub use_nodes: bool,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub alpha: f32,
    pub blend_method: String,
    pub alpha_threshold: f32,
    pub slot_count: i32,
    pub node_tree: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorldSummary {
    pub name: String,
    pub use_nodes: bool,
    pub horizon_color: [f32; 3],
    pub exposure: f32,
    pub ao_distance: f32,
    pub mist_type: i32,
    pub node_tree: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdEntry {
    pub code: String,
    pub name: Option<String>,
    pub raw_name: Option<String>,
    pub file_offset: usize,
    pub old_ptr: u64,
    pub library_path: Option<String>,
    pub object_type: Option<String>,
    pub data_target: Option<IdReference>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileSummary {
    pub header: crate::header::BlendHeader,
    pub block_count: usize,
    pub id_count: usize,
    pub schema: SchemaCounts,
    pub block_codes: Vec<BlockCodeSummary>,
    pub id_codes: Vec<IdCodeSummary>,
    pub scenes: Vec<SceneSummary>,
    pub objects: Vec<ObjectSummary>,
    pub collections: Vec<NamedIdSummary>,
    pub libraries: Vec<LibrarySummary>,
    pub images: Vec<ImageSummary>,
    pub actions: Vec<ActionSummary>,
    pub texts: Vec<TextSummary>,
    pub meshes: Vec<MeshSummary>,
    pub cameras: Vec<CameraSummary>,
    pub lights: Vec<LightSummary>,
    pub materials: Vec<MaterialSummary>,
    pub worlds: Vec<WorldSummary>,
}

pub fn summarize(file: &BlendFile) -> Result<FileSummary> {
    let ids = collect_id_entries(file)?;
    let mut scenes = Vec::new();
    let mut objects = Vec::new();
    let mut collections = Vec::new();
    let mut libraries = Vec::new();
    let mut images = Vec::new();
    let mut actions = Vec::new();
    let mut texts = Vec::new();
    let mut meshes = Vec::new();
    let mut cameras = Vec::new();
    let mut lights = Vec::new();
    let mut materials = Vec::new();
    let mut worlds = Vec::new();

    let block_codes = summarize_block_codes(file);
    let id_codes = summarize_id_codes(file);

    for block in file.ids() {
        let code = block.header().code.as_string();
        let Ok(view) = block.struct_view() else {
            continue;
        };

        match code.as_str() {
            "SC" => scenes.push(scene_summary(file, &view)),
            "OB" => objects.push(object_summary(file, &view)),
            "GR" => {
                if let Some(name) = raw_id_name(&view).map(|value| strip_id_prefix(&value)) {
                    collections.push(NamedIdSummary { name });
                }
            }
            "LI" => libraries.push(library_summary(&view)),
            "IM" => images.push(image_summary(&view)),
            "AC" => actions.push(action_summary(file, &view)),
            "TX" => texts.push(text_summary(file, &view)),
            "ME" => meshes.push(mesh_summary(file, &view)),
            "CA" => cameras.push(camera_summary(&view)),
            "LA" => lights.push(light_summary(&view)),
            "MA" => materials.push(material_summary(file, &view)),
            "WO" => worlds.push(world_summary(file, &view)),
            _ => {}
        }
    }

    Ok(FileSummary {
        header: file.header().clone(),
        block_count: file.blocks().len(),
        id_count: ids.len(),
        schema: SchemaCounts {
            names: file.schema().names_count,
            types: file.schema().types_count,
            structs: file.schema().structs.len(),
        },
        block_codes,
        id_codes,
        scenes,
        objects,
        collections,
        libraries,
        images,
        actions,
        texts,
        meshes,
        cameras,
        lights,
        materials,
        worlds,
    })
}

pub fn collect_id_entries(file: &BlendFile) -> Result<Vec<IdEntry>> {
    let mut entries = Vec::new();

    for block in file.ids() {
        let code = block.header().code.as_string();
        let view = match block.struct_view() {
            Ok(view) => view,
            Err(_) => continue,
        };
        let raw_name = raw_id_name(&view);
        let name = raw_name.as_ref().map(|value| strip_id_prefix(value));
        let library_path = library_path(file, &view);

        let (object_type, data_target, parent) = if code == "OB" {
            (
                field_i16(&view, "type").map(object_type_name),
                field_pointer(&view, "data")
                    .and_then(|ptr| resolve_id_reference(file, ptr).ok().flatten()),
                field_pointer(&view, "parent")
                    .and_then(|ptr| resolve_id_reference(file, ptr).ok().flatten())
                    .and_then(|reference| reference.name),
            )
        } else {
            (None, None, None)
        };

        entries.push(IdEntry {
            code,
            name,
            raw_name,
            file_offset: block.header().file_offset,
            old_ptr: block.header().old_ptr,
            library_path,
            object_type,
            data_target,
            parent,
        });
    }

    Ok(entries)
}

fn summarize_block_codes(file: &BlendFile) -> Vec<BlockCodeSummary> {
    let mut counts = BTreeMap::<String, (usize, u64)>::new();
    for block in file.blocks() {
        let entry = counts.entry(block.code.as_string()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += block.len;
    }
    counts
        .into_iter()
        .map(|(code, (count, bytes))| BlockCodeSummary { code, count, bytes })
        .collect()
}

fn summarize_id_codes(file: &BlendFile) -> Vec<IdCodeSummary> {
    let mut counts = BTreeMap::<String, usize>::new();
    for block in file.ids() {
        *counts.entry(block.header().code.as_string()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(code, count)| IdCodeSummary { code, count })
        .collect()
}

fn scene_summary(file: &BlendFile, view: &StructView<'_>) -> SceneSummary {
    let render = view.field("r").and_then(|field| field.as_struct_view());
    SceneSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Scene".to_owned()),
        camera: field_pointer(view, "camera")
            .and_then(|ptr| resolve_id_reference(file, ptr).ok().flatten())
            .and_then(|reference| reference.name),
        world: field_pointer(view, "world")
            .and_then(|ptr| resolve_id_reference(file, ptr).ok().flatten())
            .and_then(|reference| reference.name),
        master_collection: field_pointer(view, "master_collection")
            .and_then(|ptr| resolve_id_reference(file, ptr).ok().flatten())
            .and_then(|reference| reference.name),
        frame_current: render
            .as_ref()
            .and_then(|render| field_i32_or_i16(render, "cfra"))
            .unwrap_or_default(),
        frame_start: render
            .as_ref()
            .and_then(|render| field_i32_or_i16(render, "sfra"))
            .unwrap_or_default(),
        frame_end: render
            .as_ref()
            .and_then(|render| field_i32_or_i16(render, "efra"))
            .unwrap_or_default(),
        fps: render
            .as_ref()
            .map(render_fps)
            .unwrap_or(0.0),
        resolution_x: render
            .as_ref()
            .and_then(|render| field_i32_or_i16(render, "xsch"))
            .unwrap_or_default(),
        resolution_y: render
            .as_ref()
            .and_then(|render| field_i32_or_i16(render, "ysch"))
            .unwrap_or_default(),
        render_engine: render
            .as_ref()
            .and_then(|render| render.field("engine"))
            .and_then(|field| field.as_c_string())
            .filter(|value| !value.is_empty()),
        output_path: render.as_ref().and_then(render_output_path),
    }
}

fn object_summary(file: &BlendFile, view: &StructView<'_>) -> ObjectSummary {
    ObjectSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Object".to_owned()),
        object_type: field_i16(view, "type")
            .map(object_type_name)
            .unwrap_or_else(|| "Unknown".to_owned()),
        data_target: field_pointer(view, "data")
            .and_then(|ptr| resolve_id_reference(file, ptr).ok().flatten()),
        parent: field_pointer(view, "parent")
            .and_then(|ptr| resolve_id_reference(file, ptr).ok().flatten())
            .and_then(|reference| reference.name),
        library_path: library_path(file, view),
        location: field_f32_array::<3>(view, "loc").unwrap_or([0.0, 0.0, 0.0]),
        rotation_euler: field_f32_array::<3>(view, "rot").unwrap_or([0.0, 0.0, 0.0]),
        scale: field_f32_array::<3>(view, "size").unwrap_or([1.0, 1.0, 1.0]),
        rotation_mode: field_i16(view, "rotmode")
            .map(rotation_mode_name)
            .unwrap_or_else(|| "Unknown".to_owned()),
        material_slot_count: field_i32(view, "totcol").unwrap_or_default(),
        has_animation: object_has_animation(file, view),
        action: object_action_name(file, view),
        modifiers: object_modifiers(file, view),
    }
}

fn library_summary(view: &StructView<'_>) -> LibrarySummary {
    LibrarySummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Library".to_owned()),
        filepath: field_c_string(view, "name"),
        packed: field_pointer(view, "packedfile").is_some(),
    }
}

fn image_summary(view: &StructView<'_>) -> ImageSummary {
    ImageSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Image".to_owned()),
        filepath: field_c_string(view, "name"),
        packed: field_pointer(view, "packedfile").is_some(),
        source_code: field_i16(view, "source").unwrap_or_default() as i32,
        image_type_code: field_i16(view, "type").unwrap_or_default() as i32,
        generated_width: field_i32(view, "gen_x").unwrap_or_default(),
        generated_height: field_i32(view, "gen_y").unwrap_or_default(),
        colorspace: view
            .field("colorspace_settings")
            .and_then(|field| field.as_struct_view())
            .and_then(|settings| field_c_string(&settings, "name")),
    }
}

fn action_summary(file: &BlendFile, view: &StructView<'_>) -> ActionSummary {
    ActionSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Action".to_owned()),
        frame_start: field_f32(view, "frame_start").unwrap_or_default(),
        frame_end: field_f32(view, "frame_end").unwrap_or_default(),
        id_root: field_i32(view, "idroot").unwrap_or_default(),
        layer_count: field_i32(view, "layer_array_num").unwrap_or_default(),
        slot_count: field_i32(view, "slot_array_num").unwrap_or_default(),
        curve_count: count_listbase(file, view, "curves"),
        marker_count: count_listbase(file, view, "markers"),
    }
}

fn text_summary(file: &BlendFile, view: &StructView<'_>) -> TextSummary {
    let filepath = field_pointer_string(file, view, "name");
    TextSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Text".to_owned()),
        filepath: filepath.clone(),
        line_count: count_listbase(file, view, "lines"),
        is_external: filepath.is_some(),
    }
}

fn mesh_summary(file: &BlendFile, view: &StructView<'_>) -> MeshSummary {
    MeshSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Mesh".to_owned()),
        totvert: field_i32(view, "totvert").unwrap_or_default(),
        totedge: field_i32(view, "totedge").unwrap_or_default(),
        totloop: field_i32(view, "totloop").unwrap_or_default(),
        totpoly: field_i32(view, "totpoly").unwrap_or_default(),
        totface: field_i32(view, "totface").unwrap_or_default(),
        material_slot_count: field_i16(view, "totcol").unwrap_or_default() as i32,
        has_legacy_mvert: field_pointer(view, "mvert").is_some(),
        has_legacy_mloop: field_pointer(view, "mloop").is_some(),
        has_legacy_mpoly: field_pointer(view, "mpoly").is_some(),
        has_poly_offset_indices: field_pointer(view, "poly_offset_indices").is_some(),
        active_uv_map_attribute: field_pointer_string(file, view, "active_uv_map_attribute"),
        default_uv_map_attribute: field_pointer_string(file, view, "default_uv_map_attribute"),
        vertex_attributes: custom_data_layer_names(file, view, "vdata"),
        edge_attributes: custom_data_layer_names(file, view, "edata"),
        poly_attributes: custom_data_layer_names(file, view, "pdata"),
        loop_attributes: custom_data_layer_names(file, view, "ldata"),
    }
}

fn camera_summary(view: &StructView<'_>) -> CameraSummary {
    CameraSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Camera".to_owned()),
        camera_type: camera_type_name(field_u8(view, "type").unwrap_or_default()),
        lens: field_f32(view, "lens").unwrap_or_default(),
        ortho_scale: field_f32(view, "ortho_scale").unwrap_or_default(),
        clip_start: field_f32(view, "clipsta").unwrap_or_default(),
        clip_end: field_f32(view, "clipend").unwrap_or_default(),
        sensor_x: field_f32(view, "sensor_x").unwrap_or_default(),
        sensor_y: field_f32(view, "sensor_y").unwrap_or_default(),
        shift_x: field_f32(view, "shiftx").unwrap_or_default(),
        shift_y: field_f32(view, "shifty").unwrap_or_default(),
    }
}

fn light_summary(view: &StructView<'_>) -> LightSummary {
    LightSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Light".to_owned()),
        light_type: light_type_name(field_i16(view, "type").unwrap_or_default()),
        color: [
            field_f32(view, "r").unwrap_or(1.0),
            field_f32(view, "g").unwrap_or(1.0),
            field_f32(view, "b").unwrap_or(1.0),
        ],
        energy: field_f32(view, "energy_new")
            .filter(|value| *value > 0.0)
            .or_else(|| field_f32(view, "energy"))
            .unwrap_or_default(),
        range: field_f32(view, "att_dist").unwrap_or_default(),
        radius: field_f32(view, "radius").unwrap_or_default(),
        spot_size: field_f32(view, "spotsize").unwrap_or_default(),
        spot_blend: field_f32(view, "spotblend").unwrap_or_default(),
        sun_angle: field_f32(view, "sun_angle").unwrap_or_default(),
    }
}

fn material_summary(file: &BlendFile, view: &StructView<'_>) -> MaterialSummary {
    MaterialSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Material".to_owned()),
        use_nodes: field_bool(view, "use_nodes"),
        base_color: [
            field_f32(view, "r").unwrap_or(0.8),
            field_f32(view, "g").unwrap_or(0.8),
            field_f32(view, "b").unwrap_or(0.8),
            field_f32(view, "a")
                .or_else(|| field_f32(view, "alpha"))
                .unwrap_or(1.0),
        ],
        metallic: field_f32(view, "metallic").unwrap_or_default(),
        roughness: field_f32(view, "roughness").unwrap_or_default(),
        alpha: field_f32(view, "alpha")
            .or_else(|| field_f32(view, "a"))
            .unwrap_or(1.0),
        blend_method: blend_method_name(field_u8(view, "blend_method").unwrap_or_default()),
        alpha_threshold: field_f32(view, "alpha_threshold").unwrap_or_default(),
        slot_count: field_i16(view, "tot_slots").unwrap_or_default() as i32,
        node_tree: field_pointer(view, "nodetree").and_then(|ptr| node_tree_name(file, ptr)),
    }
}

fn world_summary(file: &BlendFile, view: &StructView<'_>) -> WorldSummary {
    WorldSummary {
        name: raw_id_name(view)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "World".to_owned()),
        use_nodes: field_bool(view, "use_nodes"),
        horizon_color: [
            field_f32(view, "horr").unwrap_or_default(),
            field_f32(view, "horg").unwrap_or_default(),
            field_f32(view, "horb").unwrap_or_default(),
        ],
        exposure: field_f32(view, "exposure").unwrap_or_default(),
        ao_distance: field_f32(view, "aodist").unwrap_or_default(),
        mist_type: field_i16(view, "mistype").unwrap_or_default() as i32,
        node_tree: field_pointer(view, "nodetree").and_then(|ptr| node_tree_name(file, ptr)),
    }
}

fn block_id_name(block: &BlockRef<'_>) -> Result<Option<String>> {
    Ok(block
        .struct_view()
        .ok()
        .and_then(|view| raw_id_name(&view))
        .map(|value| strip_id_prefix(&value)))
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

fn field_i16(view: &StructView<'_>, name: &str) -> Option<i16> {
    view.field(name)?.as_i16()
}

fn field_i32(view: &StructView<'_>, name: &str) -> Option<i32> {
    view.field(name)?.as_i32()
}

fn field_u8(view: &StructView<'_>, name: &str) -> Option<u8> {
    view.field(name)?.as_u8()
}

fn field_f32(view: &StructView<'_>, name: &str) -> Option<f32> {
    view.field(name)?.as_f32()
}

fn field_f32_array<const N: usize>(view: &StructView<'_>, name: &str) -> Option<[f32; N]> {
    view.field(name)?.as_f32_array::<N>()
}

fn field_c_string(view: &StructView<'_>, name: &str) -> Option<String> {
    view.field(name)?
        .as_c_string()
        .filter(|value| !value.is_empty())
}

fn field_pointer(view: &StructView<'_>, name: &str) -> Option<u64> {
    view.field(name)?.as_pointer().filter(|ptr| *ptr != 0)
}

fn field_bool(view: &StructView<'_>, name: &str) -> bool {
    field_u8(view, name)
        .map(|value| value != 0)
        .or_else(|| field_i16(view, name).map(|value| value != 0))
        .or_else(|| field_i32(view, name).map(|value| value != 0))
        .unwrap_or(false)
}

fn field_i32_or_i16(view: &StructView<'_>, name: &str) -> Option<i32> {
    field_i32(view, name).or_else(|| field_i16(view, name).map(i32::from))
}

fn field_pointer_string(file: &BlendFile, view: &StructView<'_>, name: &str) -> Option<String> {
    let ptr = field_pointer(view, name)?;
    file.read_c_string_at_ptr(ptr)
        .ok()
        .flatten()
        .filter(|value| !value.is_empty())
}

fn resolve_id_reference(file: &BlendFile, ptr: u64) -> Result<Option<IdReference>> {
    let Some(block) = file.resolve_old_ptr(ptr) else {
        return Ok(None);
    };
    if !block.header().is_id() {
        return Ok(None);
    }
    Ok(Some(IdReference {
        code: block.header().code.as_string(),
        name: block_id_name(&block)?,
    }))
}

fn library_path(file: &BlendFile, view: &StructView<'_>) -> Option<String> {
    let id_view = view.field("id")?.as_struct_view()?;
    let lib_ptr = id_view.field("lib")?.as_pointer()?;
    let library_block = file.resolve_old_ptr(lib_ptr)?;
    let library_view = library_block.struct_view().ok()?;
    library_view
        .field("filepath_abs")
        .and_then(|field| field.as_c_string())
        .or_else(|| {
            library_view
                .field("filepath")
                .and_then(|field| field.as_c_string())
        })
}

fn render_fps(render: &StructView<'_>) -> f32 {
    let fps = field_i32_or_i16(render, "frs_sec").unwrap_or_default() as f32;
    let base = field_f32(render, "frs_sec_base").unwrap_or(1.0).max(0.0001);
    fps / base
}

fn render_output_path(render: &StructView<'_>) -> Option<String> {
    render
        .field("filepath")
        .and_then(|field| field.as_c_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            render
                .field("pic")
                .and_then(|field| field.as_c_string())
                .filter(|value| !value.is_empty())
        })
}

fn object_has_animation(file: &BlendFile, view: &StructView<'_>) -> bool {
    let Some(adt_ptr) = field_pointer(view, "adt") else {
        return false;
    };
    let Some(adt) = file.view_old_ptr_as_struct(adt_ptr, "AnimData").ok().flatten() else {
        return true;
    };
    field_pointer(&adt, "action").is_some()
        || adt
            .field("drivers")
            .and_then(|field| field.as_struct_view())
            .and_then(|drivers| field_pointer(&drivers, "first"))
            .is_some()
}

fn object_action_name(file: &BlendFile, view: &StructView<'_>) -> Option<String> {
    let adt_ptr = field_pointer(view, "adt")?;
    let adt = file.view_old_ptr_as_struct(adt_ptr, "AnimData").ok().flatten()?;
    let action_ptr = field_pointer(&adt, "action")?;
    resolve_id_reference(file, action_ptr)
        .ok()
        .flatten()
        .and_then(|reference| reference.name)
}

fn object_modifiers(file: &BlendFile, view: &StructView<'_>) -> Vec<ModifierSummary> {
    let Some(list) = view.field("modifiers").and_then(|field| field.as_struct_view()) else {
        return Vec::new();
    };
    let Ok(blocks) = iter_listbase(file, &list) else {
        return Vec::new();
    };
    blocks
        .into_iter()
        .filter_map(|block| {
            let view = block.view_as("ModifierData").ok()?;
            Some(ModifierSummary {
                name: view
                    .field("name")
                    .and_then(|field| field.as_c_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Modifier".to_owned()),
                modifier_type: modifier_type_name(field_i32(&view, "type").unwrap_or_default()),
            })
        })
        .collect()
}

fn count_listbase(file: &BlendFile, parent: &StructView<'_>, field_name: &str) -> usize {
    let Some(list) = parent
        .field(field_name)
        .and_then(|field| field.as_struct_view())
    else {
        return 0;
    };
    iter_listbase(file, &list).map(|blocks| blocks.len()).unwrap_or(0)
}

fn custom_data_layer_names(
    file: &BlendFile,
    parent: &StructView<'_>,
    field_name: &str,
) -> Vec<String> {
    let Some(custom_data) = parent.field(field_name).and_then(|field| field.as_struct_view()) else {
        return Vec::new();
    };
    let layer_count = field_i32(&custom_data, "totlayer").unwrap_or_default().max(0) as usize;
    let Some(layer_ptr) = field_pointer(&custom_data, "layers") else {
        return Vec::new();
    };
    let Ok(Some(layers)) = read_struct_array(file, layer_ptr, "CustomDataLayer", layer_count) else {
        return Vec::new();
    };
    layers
        .iter()
        .filter_map(|layer| layer.field("name").and_then(|field| field.as_c_string()))
        .filter(|name| !name.is_empty())
        .collect()
}

fn node_tree_name(file: &BlendFile, ptr: u64) -> Option<String> {
    let view = file.view_old_ptr_as_struct(ptr, "bNodeTree").ok().flatten()?;
    raw_id_name(&view).map(|value| strip_id_prefix(&value))
}

fn camera_type_name(value: u8) -> String {
    match value {
        0 => "Perspective",
        1 => "Orthographic",
        2 => "Panorama",
        other => return format!("Unknown({other})"),
    }
    .to_owned()
}

fn light_type_name(value: i16) -> String {
    match value {
        0 => "Point",
        1 => "Sun",
        2 => "Spot",
        4 => "Area",
        other => return format!("Unknown({other})"),
    }
    .to_owned()
}

fn blend_method_name(value: u8) -> String {
    match value {
        0 => "Opaque",
        3 => "Clip",
        4 => "Hashed",
        5 => "Blend",
        other => return format!("Unknown({other})"),
    }
    .to_owned()
}

fn rotation_mode_name(value: i16) -> String {
    match value {
        -1 => "AxisAngle",
        0 => "Quaternion",
        1 => "XYZ Euler",
        2 => "XZY Euler",
        3 => "YXZ Euler",
        4 => "YZX Euler",
        5 => "ZXY Euler",
        6 => "ZYX Euler",
        other => return format!("Unknown({other})"),
    }
    .to_owned()
}

fn modifier_type_name(value: i32) -> String {
    match value {
        1 => "SubdivisionSurface",
        5 => "Mirror",
        7 => "Build",
        8 => "Decimate",
        11 => "Wave",
        12 => "Array",
        14 => "Boolean",
        15 => "EdgeSplit",
        16 => "Displace",
        19 => "Smooth",
        23 => "Solidify",
        25 => "Bevel",
        26 => "Shrinkwrap",
        32 => "Remesh",
        36 => "Weld",
        41 => "WeightedNormal",
        44 => "Triangulate",
        46 => "Node",
        other => return format!("Unknown({other})"),
    }
    .to_owned()
}

pub fn object_type_name(value: i16) -> String {
    match value {
        0 => "Empty",
        1 => "Mesh",
        2 => "CurvesLegacy",
        3 => "Surface",
        4 => "Font",
        5 => "MetaBall",
        10 => "Light",
        11 => "Camera",
        12 => "Speaker",
        13 => "LightProbe",
        22 => "Lattice",
        25 => "Armature",
        26 => "GreasePencilLegacy",
        27 => "Curves",
        28 => "PointCloud",
        29 => "Volume",
        30 => "GreasePencil",
        other => return format!("Unknown({other})"),
    }
    .to_owned()
}
