use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::error::Result;
use crate::summary::{
    ActionSummary, FileSummary, IdEntry, IdReference, ImageSummary, LibrarySummary,
    MaterialSummary, MeshSummary, ModifierSummary, ObjectSummary, SceneSummary, TextSummary,
    WorldSummary, collect_id_entries, summarize,
};
use crate::{BHeadType, BlendError, BlendFile, CompressionKind, Endian, FieldDef, StructDef};

#[derive(Debug, Parser)]
#[command(name = "blendio", version, about = "Read Blender 4.5 .blend files")]
pub struct Cli {
    #[arg(long, global = true, help = "Print JSON to stdout instead of text")]
    json: bool,
    #[arg(long = "json-out", global = true, help = "Write pretty JSON to a file")]
    json_out: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Info {
        file: PathBuf,
    },
    Blocks {
        file: PathBuf,
    },
    Sdna {
        file: PathBuf,
        #[arg(long = "type")]
        struct_name: Option<String>,
    },
    Ids {
        file: PathBuf,
    },
}

#[derive(Debug, Serialize)]
struct SchemaOverview {
    pointer_size: u8,
    names_count: usize,
    types_count: usize,
    structs_count: usize,
    structs: Vec<StructOverview>,
}

#[derive(Debug, Serialize)]
struct StructOverview {
    index: usize,
    type_name: String,
    size: usize,
    field_count: usize,
}

#[derive(Debug, Serialize)]
struct StructDetail<'a> {
    index: usize,
    type_name: &'a str,
    size: usize,
    fields: &'a [FieldDef],
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Info { file } => {
            let blend = BlendFile::open(&file)?;
            let summary = summarize(&blend)?;
            emit_output(
                &summary,
                &render_info_text(&file, &summary),
                cli.json,
                cli.json_out.as_deref(),
            )
        }
        Commands::Blocks { file } => {
            let blend = BlendFile::open(&file)?;
            emit_output(
                blend.blocks(),
                &render_blocks_text(&file, &blend),
                cli.json,
                cli.json_out.as_deref(),
            )
        }
        Commands::Sdna { file, struct_name } => {
            let blend = BlendFile::open(&file)?;
            if let Some(struct_name) = struct_name {
                let struct_def = blend.schema().struct_by_name(&struct_name).ok_or_else(|| {
                    BlendError::InvalidSdnaOwned(format!("unknown struct type {struct_name}"))
                })?;
                let detail = StructDetail::from(struct_def);
                emit_output(
                    &detail,
                    &render_sdna_detail_text(&file, &detail),
                    cli.json,
                    cli.json_out.as_deref(),
                )
            } else {
                let overview = SchemaOverview {
                    pointer_size: blend.schema().pointer_size,
                    names_count: blend.schema().names_count,
                    types_count: blend.schema().types_count,
                    structs_count: blend.schema().structs.len(),
                    structs: blend
                        .schema()
                        .structs
                        .iter()
                        .map(StructOverview::from)
                        .collect(),
                };
                emit_output(
                    &overview,
                    &render_sdna_overview_text(&file, &overview),
                    cli.json,
                    cli.json_out.as_deref(),
                )
            }
        }
        Commands::Ids { file } => {
            let blend = BlendFile::open(&file)?;
            let ids = collect_id_entries(&blend)?;
            emit_output(
                &ids,
                &render_ids_text(&file, &ids),
                cli.json,
                cli.json_out.as_deref(),
            )
        }
    }
}

fn emit_output<T: Serialize + ?Sized>(
    value: &T,
    text: &str,
    json: bool,
    json_out: Option<&Path>,
) -> Result<()> {
    if let Some(path) = json_out {
        std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn render_info_text(path: &Path, summary: &FileSummary) -> String {
    let mut out = String::new();
    writeln!(out, "Blend File Summary").unwrap();
    writeln!(out, "File: {}", path.display()).unwrap();
    writeln!(
        out,
        "Compression: {}",
        compression_label(summary.header.compression)
    )
    .unwrap();
    writeln!(out, "Pointer size: {} bytes", summary.header.pointer_size).unwrap();
    writeln!(out, "Endian: {}", endian_label(summary.header.endian)).unwrap();
    writeln!(out, "Blender version: {}", summary.header.file_version).unwrap();
    writeln!(
        out,
        "File format version: {}",
        summary.header.file_format_version
    )
    .unwrap();
    writeln!(out, "Header size: {} bytes", summary.header.header_size).unwrap();
    writeln!(out, "BHead layout: {}", bhead_label(summary.header.bhead_type)).unwrap();
    writeln!(out, "Block count: {}", summary.block_count).unwrap();
    writeln!(out, "ID count: {}", summary.id_count).unwrap();
    writeln!(
        out,
        "SDNA: names={} types={} structs={}",
        summary.schema.names, summary.schema.types, summary.schema.structs
    )
    .unwrap();
    writeln!(
        out,
        "Sections: scenes={} objects={} collections={} libraries={} images={} actions={} texts={} meshes={} cameras={} lights={} materials={} worlds={}",
        summary.scenes.len(),
        summary.objects.len(),
        summary.collections.len(),
        summary.libraries.len(),
        summary.images.len(),
        summary.actions.len(),
        summary.texts.len(),
        summary.meshes.len(),
        summary.cameras.len(),
        summary.lights.len(),
        summary.materials.len(),
        summary.worlds.len()
    )
    .unwrap();
    writeln!(out).unwrap();

    render_block_code_section(&mut out, &summary.block_codes);
    render_id_code_section(&mut out, &summary.id_codes);
    render_scene_section(&mut out, &summary.scenes);
    render_object_section(&mut out, &summary.objects);
    render_named_section(&mut out, "Collections", &summary.collections);
    render_library_section(&mut out, &summary.libraries);
    render_image_section(&mut out, &summary.images);
    render_action_section(&mut out, &summary.actions);
    render_text_section(&mut out, &summary.texts);
    render_mesh_section(&mut out, &summary.meshes);
    render_camera_section(&mut out, &summary.cameras);
    render_light_section(&mut out, &summary.lights);
    render_material_section(&mut out, &summary.materials);
    render_world_section(&mut out, &summary.worlds);
    out
}

fn render_blocks_text(path: &Path, file: &BlendFile) -> String {
    let mut out = String::new();
    writeln!(out, "Blend Blocks").unwrap();
    writeln!(out, "File: {}", path.display()).unwrap();
    writeln!(out, "Count: {}", file.blocks().len()).unwrap();
    writeln!(out).unwrap();

    for (index, block) in file.blocks().iter().enumerate() {
        writeln!(
            out,
            "[{index:04}] code={} len={} old_ptr={} sdna={} count={} file_offset={}",
            block.code.as_string(),
            block.len,
            format_pointer(block.old_ptr),
            block.sdna_index,
            block.count,
            block.file_offset
        )
        .unwrap();
    }

    out
}

fn render_ids_text(path: &Path, ids: &[IdEntry]) -> String {
    let mut out = String::new();
    writeln!(out, "Blend ID Blocks").unwrap();
    writeln!(out, "File: {}", path.display()).unwrap();
    writeln!(out, "Count: {}", ids.len()).unwrap();
    writeln!(out).unwrap();

    for entry in ids {
        writeln!(
            out,
            "{} {} old_ptr={} offset={} object_type={} data={} parent={} library={}",
            entry.code,
            entry.name.as_deref().unwrap_or("-"),
            format_pointer(entry.old_ptr),
            entry.file_offset,
            entry.object_type.as_deref().unwrap_or("-"),
            format_id_reference(entry.data_target.as_ref()),
            entry.parent.as_deref().unwrap_or("-"),
            entry.library_path.as_deref().unwrap_or("-")
        )
        .unwrap();
    }

    out
}

fn render_sdna_overview_text(path: &Path, overview: &SchemaOverview) -> String {
    let mut out = String::new();
    writeln!(out, "SDNA Overview").unwrap();
    writeln!(out, "File: {}", path.display()).unwrap();
    writeln!(out, "Pointer size: {} bytes", overview.pointer_size).unwrap();
    writeln!(
        out,
        "Counts: names={} types={} structs={}",
        overview.names_count, overview.types_count, overview.structs_count
    )
    .unwrap();
    writeln!(out).unwrap();

    for item in &overview.structs {
        writeln!(
            out,
            "[{index:04}] {name} size={size} fields={fields}",
            index = item.index,
            name = item.type_name,
            size = item.size,
            fields = item.field_count
        )
        .unwrap();
    }

    out
}

fn render_sdna_detail_text(path: &Path, detail: &StructDetail<'_>) -> String {
    let mut out = String::new();
    writeln!(out, "SDNA Struct Detail").unwrap();
    writeln!(out, "File: {}", path.display()).unwrap();
    writeln!(out, "Type: {}", detail.type_name).unwrap();
    writeln!(out, "Index: {}", detail.index).unwrap();
    writeln!(out, "Size: {} bytes", detail.size).unwrap();
    writeln!(out, "Field count: {}", detail.fields.len()).unwrap();
    writeln!(out).unwrap();

    for field in detail.fields {
        writeln!(
            out,
            "@{offset:04} {type_name} {name} normalized={normalized} size={size} arr={array_len} ptr={ptr} fn_ptr={fn_ptr}",
            offset = field.offset,
            type_name = field.type_name,
            name = field.name,
            normalized = field.normalized_name,
            size = field.size,
            array_len = field.array_len,
            ptr = yes_no(field.is_pointer),
            fn_ptr = yes_no(field.is_function_pointer)
        )
        .unwrap();
    }

    out
}

fn render_block_code_section(out: &mut String, items: &[crate::summary::BlockCodeSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Block codes ({})", items.len()).unwrap();
    for item in items {
        writeln!(out, "  {} count={} bytes={}", item.code, item.count, item.bytes).unwrap();
    }
    writeln!(out).unwrap();
}

fn render_id_code_section(out: &mut String, items: &[crate::summary::IdCodeSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "ID codes ({})", items.len()).unwrap();
    for item in items {
        writeln!(out, "  {} count={}", item.code, item.count).unwrap();
    }
    writeln!(out).unwrap();
}

fn render_scene_section(out: &mut String, items: &[SceneSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Scenes ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} camera={} world={} collection={}",
            item.name,
            item.camera.as_deref().unwrap_or("-"),
            item.world.as_deref().unwrap_or("-"),
            item.master_collection.as_deref().unwrap_or("-")
        )
        .unwrap();
        writeln!(
            out,
            "    frame={} range={}..{} fps={:.3} resolution={}x{} engine={} output={}",
            item.frame_current,
            item.frame_start,
            item.frame_end,
            item.fps,
            item.resolution_x,
            item.resolution_y,
            item.render_engine.as_deref().unwrap_or("-"),
            item.output_path.as_deref().unwrap_or("-")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_object_section(out: &mut String, items: &[ObjectSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Objects ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} type={} data={} parent={} action={} materials={} animation={} library={}",
            item.name,
            item.object_type,
            format_id_reference(item.data_target.as_ref()),
            item.parent.as_deref().unwrap_or("-"),
            item.action.as_deref().unwrap_or("-"),
            item.material_slot_count,
            yes_no(item.has_animation),
            item.library_path.as_deref().unwrap_or("-")
        )
        .unwrap();
        writeln!(
            out,
            "    loc={} rot={} scale={} rotation_mode={} modifiers={}",
            format_vec3(item.location),
            format_vec3(item.rotation_euler),
            format_vec3(item.scale),
            item.rotation_mode,
            format_modifiers(&item.modifiers)
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_named_section(out: &mut String, title: &str, items: &[crate::summary::NamedIdSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "{title} ({})", items.len()).unwrap();
    for item in items {
        writeln!(out, "  {}", item.name).unwrap();
    }
    writeln!(out).unwrap();
}

fn render_library_section(out: &mut String, items: &[LibrarySummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Libraries ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} path={} packed={}",
            item.name,
            item.filepath.as_deref().unwrap_or("-"),
            yes_no(item.packed)
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_image_section(out: &mut String, items: &[ImageSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Images ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} path={} packed={} source_code={} type_code={} generated={}x{} colorspace={}",
            item.name,
            item.filepath.as_deref().unwrap_or("-"),
            yes_no(item.packed),
            item.source_code,
            item.image_type_code,
            item.generated_width,
            item.generated_height,
            item.colorspace.as_deref().unwrap_or("-")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_action_section(out: &mut String, items: &[ActionSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Actions ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} frames={:.3}..{:.3} id_root={} layers={} slots={} curves={} markers={}",
            item.name,
            item.frame_start,
            item.frame_end,
            item.id_root,
            item.layer_count,
            item.slot_count,
            item.curve_count,
            item.marker_count
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_text_section(out: &mut String, items: &[TextSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Texts ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} lines={} external={} path={}",
            item.name,
            item.line_count,
            yes_no(item.is_external),
            item.filepath.as_deref().unwrap_or("-")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_mesh_section(out: &mut String, items: &[MeshSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Meshes ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} verts={} edges={} loops={} polys={} faces={} materials={}",
            item.name,
            item.totvert,
            item.totedge,
            item.totloop,
            item.totpoly,
            item.totface,
            item.material_slot_count
        )
        .unwrap();
        writeln!(
            out,
            "    mvert={} mloop={} mpoly={} poly_offset_indices={} active_uv={} default_uv={}",
            yes_no(item.has_legacy_mvert),
            yes_no(item.has_legacy_mloop),
            yes_no(item.has_legacy_mpoly),
            yes_no(item.has_poly_offset_indices),
            item.active_uv_map_attribute.as_deref().unwrap_or("-"),
            item.default_uv_map_attribute.as_deref().unwrap_or("-")
        )
        .unwrap();
        writeln!(
            out,
            "    vertex_attrs={} edge_attrs={} poly_attrs={} loop_attrs={}",
            format_name_list(&item.vertex_attributes),
            format_name_list(&item.edge_attributes),
            format_name_list(&item.poly_attributes),
            format_name_list(&item.loop_attributes)
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_camera_section(out: &mut String, items: &[crate::summary::CameraSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Cameras ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} type={} lens={:.3} ortho_scale={:.3} clip={:.5}..{:.5} sensor={}x{} shift=({:.5}, {:.5})",
            item.name,
            item.camera_type,
            item.lens,
            item.ortho_scale,
            item.clip_start,
            item.clip_end,
            item.sensor_x,
            item.sensor_y,
            item.shift_x,
            item.shift_y
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_light_section(out: &mut String, items: &[crate::summary::LightSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Lights ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} type={} color={} energy={:.3} range={:.3} radius={:.3} spot_size={:.3} spot_blend={:.3} sun_angle={:.3}",
            item.name,
            item.light_type,
            format_vec3(item.color),
            item.energy,
            item.range,
            item.radius,
            item.spot_size,
            item.spot_blend,
            item.sun_angle
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_material_section(out: &mut String, items: &[MaterialSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Materials ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} use_nodes={} base_color={} metallic={:.3} roughness={:.3} alpha={:.3}",
            item.name,
            yes_no(item.use_nodes),
            format_vec4(item.base_color),
            item.metallic,
            item.roughness,
            item.alpha
        )
        .unwrap();
        writeln!(
            out,
            "    blend_method={} alpha_threshold={:.3} slots={} node_tree={}",
            item.blend_method,
            item.alpha_threshold,
            item.slot_count,
            item.node_tree.as_deref().unwrap_or("-")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn render_world_section(out: &mut String, items: &[WorldSummary]) {
    if items.is_empty() {
        return;
    }

    writeln!(out, "Worlds ({})", items.len()).unwrap();
    for item in items {
        writeln!(
            out,
            "  {} use_nodes={} horizon={} exposure={:.3} ao_distance={:.3} mist_type={} node_tree={}",
            item.name,
            yes_no(item.use_nodes),
            format_vec3(item.horizon_color),
            item.exposure,
            item.ao_distance,
            item.mist_type,
            item.node_tree.as_deref().unwrap_or("-")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn format_id_reference(value: Option<&IdReference>) -> String {
    match value {
        Some(value) => match value.name.as_deref() {
            Some(name) => format!("{}:{name}", value.code),
            None => value.code.clone(),
        },
        None => "-".to_owned(),
    }
}

fn format_modifiers(items: &[ModifierSummary]) -> String {
    if items.is_empty() {
        return "-".to_owned();
    }

    items.iter()
        .map(|item| format!("{}:{}", item.name, item.modifier_type))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_name_list(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_owned()
    } else {
        items.join(", ")
    }
}

fn format_pointer(value: u64) -> String {
    if value == 0 {
        "0x0".to_owned()
    } else {
        format!("0x{value:016X}")
    }
}

fn format_vec3(value: [f32; 3]) -> String {
    format!("[{:.6}, {:.6}, {:.6}]", value[0], value[1], value[2])
}

fn format_vec4(value: [f32; 4]) -> String {
    format!(
        "[{:.6}, {:.6}, {:.6}, {:.6}]",
        value[0], value[1], value[2], value[3]
    )
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn compression_label(value: CompressionKind) -> &'static str {
    match value {
        CompressionKind::None => "none",
        CompressionKind::Gzip => "gzip",
        CompressionKind::Zstd => "zstd",
    }
}

fn endian_label(value: Endian) -> &'static str {
    match value {
        Endian::Little => "little",
        Endian::Big => "big",
    }
}

fn bhead_label(value: BHeadType) -> &'static str {
    match value {
        BHeadType::BHead4 => "bhead4",
        BHeadType::SmallBHead8 => "small_bhead8",
        BHeadType::LargeBHead8 => "large_bhead8",
    }
}

impl From<&StructDef> for StructOverview {
    fn from(value: &StructDef) -> Self {
        Self {
            index: value.index,
            type_name: value.type_name.clone(),
            size: value.size,
            field_count: value.fields.len(),
        }
    }
}

impl<'a> From<&'a StructDef> for StructDetail<'a> {
    fn from(value: &'a StructDef) -> Self {
        Self {
            index: value.index,
            type_name: &value.type_name,
            size: value.size,
            fields: &value.fields,
        }
    }
}
