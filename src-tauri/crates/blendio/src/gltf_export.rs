use std::collections::HashMap;
use std::f32::consts::FRAC_1_SQRT_2;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::BlendFile;
use crate::animation::extract_object_animations;
use crate::error::Result;
use crate::material::MaterialResolver;
use crate::mesh::build_object_mesh;
use crate::report::{AxisMode, ExportOptions, ExportReport, ExportWarningKind};
use crate::summary::object_type_name;
use crate::view::StructView;

#[derive(Debug, Clone, Serialize)]
pub struct ExportScene {
    pub nodes: Vec<ExportNode>,
    pub scene_nodes: Vec<usize>,
    pub meshes: Vec<ExportMesh>,
    pub materials: Vec<ExportMaterial>,
    pub textures: Vec<ExportTexture>,
    pub images: Vec<ExportImage>,
    pub animations: Vec<ExportAnimation>,
    pub lights: Vec<ExportLight>,
    pub fps: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportNode {
    pub name: String,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
    pub camera: Option<ExportCamera>,
    pub light: Option<usize>,
    #[serde(skip)]
    pub(crate) old_ptr: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportMesh {
    pub name: String,
    pub primitives: Vec<ExportPrimitive>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportPrimitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub texcoords0: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportMaterial {
    pub name: String,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: [f32; 3],
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: Option<f32>,
    pub base_color_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    pub emissive_texture: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportTexture {
    pub name: String,
    pub image: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportImage {
    pub name: String,
    pub mime_type: String,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportCamera {
    pub kind: CameraKind,
    pub yfov: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub znear: f32,
    pub zfar: f32,
    pub xmag: Option<f32>,
    pub ymag: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraKind {
    Perspective,
    Orthographic,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportLight {
    pub name: String,
    pub kind: LightKind,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: Option<f32>,
    pub inner_cone_angle: Option<f32>,
    pub outer_cone_angle: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LightKind {
    Directional,
    Point,
    Spot,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportAnimation {
    pub name: String,
    pub channels: Vec<ExportAnimationChannel>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportAnimationChannel {
    pub node: usize,
    pub path: AnimationPath,
    pub keyframes: Vec<KeyframeValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationPath {
    Translation,
    Rotation,
    Scale,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyframeValue {
    pub time_seconds: f32,
    pub values: Vec<f32>,
}

pub fn export_glb(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    options: &ExportOptions,
) -> Result<ExportReport> {
    let file = BlendFile::open(input_path.as_ref())?;
    let (scene, report) = build_export_scene(&file, options)?;
    let bytes = write_glb(&scene, options)?;
    fs::write(output_path, bytes)?;
    Ok(report)
}

pub fn build_export_scene(
    file: &BlendFile,
    options: &ExportOptions,
) -> Result<(ExportScene, ExportReport)> {
    let mut report = ExportReport::default();
    let mut material_resolver = MaterialResolver::new(file, options);
    let mut scene = ExportScene {
        nodes: Vec::new(),
        scene_nodes: Vec::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        textures: Vec::new(),
        images: Vec::new(),
        animations: Vec::new(),
        lights: Vec::new(),
        fps: scene_fps(file).unwrap_or(24.0),
    };
    let mut node_by_object_ptr = HashMap::new();
    let mut parent_ptrs = Vec::new();

    for block in file.ids() {
        if block.header().code.as_string() != "OB" {
            continue;
        }
        let view = block.struct_view()?;
        let object_name = raw_id_name(&view)
            .map(|name| strip_id_prefix(&name))
            .unwrap_or_else(|| format!("Object_{:X}", block.header().old_ptr));
        let object_type = view.field("type").and_then(|field| field.as_i16()).unwrap_or_default();
        let parent_ptr = view
            .field("parent")
            .and_then(|field| field.as_pointer())
            .filter(|ptr| *ptr != 0);
        let node_index = scene.nodes.len();
        scene.nodes.push(ExportNode {
            name: object_name.clone(),
            translation: view
                .field("loc")
                .and_then(|field| field.as_f32_array::<3>())
                .unwrap_or([0.0, 0.0, 0.0]),
            rotation: object_rotation(&view),
            scale: view
                .field("size")
                .and_then(|field| field.as_f32_array::<3>())
                .unwrap_or([1.0, 1.0, 1.0]),
            parent: None,
            children: Vec::new(),
            mesh: None,
            camera: None,
            light: None,
            old_ptr: block.header().old_ptr,
        });
        node_by_object_ptr.insert(block.header().old_ptr, node_index);
        parent_ptrs.push(parent_ptr);

        match object_type {
            1 if options.include_meshes => {
                if let Some(mesh) =
                    build_object_mesh(
                        file,
                        &view,
                        &object_name,
                        options,
                        &mut material_resolver,
                        &mut report,
                    )?
                {
                    scene.nodes[node_index].mesh = Some(scene.meshes.len());
                    scene.meshes.push(mesh);
                } else {
                    report.skip_object(object_name.clone());
                }
            }
            10 if options.include_lights => {
                if let Some(light) = build_light(file, &view, &object_name, options, &mut report)? {
                    scene.nodes[node_index].light = Some(scene.lights.len());
                    scene.lights.push(light);
                }
            }
            11 if options.include_cameras => {
                if let Some(camera) = build_camera(file, &view)? {
                    scene.nodes[node_index].camera = Some(camera);
                }
            }
            0 => {}
            _ => {
                report.warn(
                    options,
                    ExportWarningKind::UnsupportedObjectType,
                    Some(&object_name),
                    format!("object type {} is not exported", object_type_name(object_type)),
                )?;
                report.add_unsupported_feature(format!(
                    "object:{object_name}:{}",
                    object_type_name(object_type)
                ));
            }
        }
    }

    for (node_index, parent_ptr) in parent_ptrs.into_iter().enumerate() {
        if let Some(parent_ptr) = parent_ptr {
            if let Some(parent_index) = node_by_object_ptr.get(&parent_ptr).copied() {
                scene.nodes[node_index].parent = Some(parent_index);
                scene.nodes[parent_index].children.push(node_index);
                continue;
            }
        }
        scene.scene_nodes.push(node_index);
    }

    if options.include_object_trs_animation {
        scene.animations = extract_object_animations(file, &scene, options, &mut report)?;
    }

    let (materials, textures, images) = material_resolver.into_parts();
    scene.materials = materials;
    scene.textures = textures;
    scene.images = images;
    report.exported_mesh_count = scene.meshes.len();
    report.exported_material_count = scene.materials.len();
    report.exported_animation_count = scene.animations.len();
    Ok((scene, report))
}

fn build_camera(file: &BlendFile, object: &StructView<'_>) -> Result<Option<ExportCamera>> {
    let Some(camera_ptr) = object.field("data").and_then(|field| field.as_pointer()) else {
        return Ok(None);
    };
    let Some(camera_view) = file.view_old_ptr_as_struct(camera_ptr, "Camera")? else {
        return Ok(None);
    };
    let clip_start = camera_view
        .field("clipsta")
        .and_then(|field| field.as_f32())
        .unwrap_or(0.1)
        .max(0.001);
    let clip_end = camera_view
        .field("clipend")
        .and_then(|field| field.as_f32())
        .unwrap_or(1000.0)
        .max(clip_start + 0.001);
    let camera_type = camera_view.field("type").and_then(|field| field.as_u8()).unwrap_or(0);
    let aspect_ratio = scene_aspect_ratio(file);
    if camera_type == 1 {
        let ortho_scale = camera_view
            .field("ortho_scale")
            .and_then(|field| field.as_f32())
            .unwrap_or(1.0);
        let ymag = ortho_scale * 0.5;
        let xmag = aspect_ratio.unwrap_or(1.0) * ymag;
        return Ok(Some(ExportCamera {
            kind: CameraKind::Orthographic,
            yfov: None,
            aspect_ratio,
            znear: clip_start,
            zfar: clip_end,
            xmag: Some(xmag),
            ymag: Some(ymag),
        }));
    }
    let lens = camera_view
        .field("lens")
        .and_then(|field| field.as_f32())
        .unwrap_or(50.0)
        .max(1.0);
    let sensor_x = camera_view
        .field("sensor_x")
        .and_then(|field| field.as_f32())
        .unwrap_or(36.0);
    let sensor_y = camera_view
        .field("sensor_y")
        .and_then(|field| field.as_f32())
        .unwrap_or(24.0);
    let sensor_fit = camera_view
        .field("sensor_fit")
        .and_then(|field| field.as_u8())
        .unwrap_or(0);
    let aspect = aspect_ratio.unwrap_or(1.0).max(0.0001);
    let sensor_height = match sensor_fit {
        2 => sensor_y,
        1 => sensor_x / aspect,
        _ => {
            if aspect >= 1.0 {
                sensor_x / aspect
            } else {
                sensor_y
            }
        }
    };
    let yfov = 2.0 * (sensor_height / (2.0 * lens)).atan();
    Ok(Some(ExportCamera {
        kind: CameraKind::Perspective,
        yfov: Some(yfov),
        aspect_ratio,
        znear: clip_start,
        zfar: clip_end,
        xmag: None,
        ymag: None,
    }))
}

fn build_light(
    file: &BlendFile,
    object: &StructView<'_>,
    object_name: &str,
    options: &ExportOptions,
    report: &mut ExportReport,
) -> Result<Option<ExportLight>> {
    let Some(light_ptr) = object.field("data").and_then(|field| field.as_pointer()) else {
        return Ok(None);
    };
    let Some(light_view) = file.view_old_ptr_as_struct(light_ptr, "Lamp")? else {
        return Ok(None);
    };
    let light_type = light_view
        .field("type")
        .and_then(|field| field.as_i16())
        .unwrap_or(0);
    let kind = match light_type {
        0 => LightKind::Point,
        1 => LightKind::Directional,
        2 => LightKind::Spot,
        4 => {
            report.warn(
                options,
                ExportWarningKind::UnsupportedObjectType,
                Some(object_name),
                "area lights are approximated as point lights in this exporter",
            )?;
            LightKind::Point
        }
        other => {
            report.warn(
                options,
                ExportWarningKind::UnsupportedObjectType,
                Some(object_name),
                format!("unsupported light type {other}"),
            )?;
            report.add_unsupported_feature(format!("light:{object_name}:{other}"));
            return Ok(None);
        }
    };
    Ok(Some(ExportLight {
        name: object_name.to_owned(),
        kind,
        color: [
            light_view.field("r").and_then(|field| field.as_f32()).unwrap_or(1.0),
            light_view.field("g").and_then(|field| field.as_f32()).unwrap_or(1.0),
            light_view.field("b").and_then(|field| field.as_f32()).unwrap_or(1.0),
        ],
        intensity: light_view
            .field("energy_new")
            .and_then(|field| field.as_f32())
            .filter(|value| *value > 0.0)
            .or_else(|| light_view.field("energy").and_then(|field| field.as_f32()))
            .unwrap_or(1.0)
            .max(0.0),
        range: light_view
            .field("att_dist")
            .and_then(|field| field.as_f32())
            .filter(|value| *value > 0.0),
        inner_cone_angle: if kind == LightKind::Spot {
            let outer = light_view
                .field("spotsize")
                .and_then(|field| field.as_f32())
                .unwrap_or(0.7853982)
                * 0.5;
            let blend = light_view
                .field("spotblend")
                .and_then(|field| field.as_f32())
                .unwrap_or(0.15)
                .clamp(0.0, 1.0);
            Some(outer * (1.0 - blend))
        } else {
            None
        },
        outer_cone_angle: if kind == LightKind::Spot {
            Some(
                light_view
                    .field("spotsize")
                    .and_then(|field| field.as_f32())
                    .unwrap_or(0.7853982)
                    * 0.5,
            )
        } else {
            None
        },
    }))
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

fn scene_fps(file: &BlendFile) -> Option<f32> {
    let scene = file
        .ids()
        .into_iter()
        .find(|block| block.header().code.as_string() == "SC")?
        .struct_view()
        .ok()?;
    let render = scene.field("r")?.as_struct_view()?;
    let frs_sec = render.field("frs_sec")?.as_i16()? as f32;
    let frs_sec_base = render.field("frs_sec_base")?.as_f32()?.max(0.0001);
    Some(frs_sec / frs_sec_base)
}

fn scene_aspect_ratio(file: &BlendFile) -> Option<f32> {
    let scene = file
        .ids()
        .into_iter()
        .find(|block| block.header().code.as_string() == "SC")?
        .struct_view()
        .ok()?;
    let render = scene.field("r")?.as_struct_view()?;
    let width = render.field("xsch")?.as_i16()? as f32;
    let height = render.field("ysch")?.as_i16()? as f32;
    Some((width / height).max(0.0001))
}

fn object_rotation(object: &StructView<'_>) -> [f32; 4] {
    let rotmode = object
        .field("rotmode")
        .and_then(|field| field.as_i16())
        .unwrap_or(1);
    match rotmode {
        0 => {
            let quat = object
                .field("quat")
                .and_then(|field| field.as_f32_array::<4>())
                .unwrap_or([1.0, 0.0, 0.0, 0.0]);
            normalize_quaternion([quat[1], quat[2], quat[3], quat[0]])
        }
        -1 => {
            let angle = object
                .field("rotAngle")
                .and_then(|field| field.as_f32())
                .unwrap_or(0.0);
            let axis = object
                .field("rotAxis")
                .and_then(|field| field.as_f32_array::<3>())
                .unwrap_or([0.0, 0.0, 1.0]);
            quaternion_from_axis_angle(axis, angle)
        }
        1..=6 => {
            let euler = object
                .field("rot")
                .and_then(|field| field.as_f32_array::<3>())
                .unwrap_or([0.0, 0.0, 0.0]);
            euler_to_quaternion(rotmode, euler)
        }
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

fn write_glb(scene: &ExportScene, options: &ExportOptions) -> Result<Vec<u8>> {
    let mut buffer = BinaryBuffer::default();
    let mut accessors = Vec::<Value>::new();
    let mut buffer_views = Vec::<Value>::new();
    let mut images = Vec::<Value>::new();
    let mut textures = Vec::<Value>::new();
    let mut materials = Vec::<Value>::new();
    let mut meshes = Vec::<Value>::new();
    let mut cameras = Vec::<Value>::new();
    let mut nodes = Vec::<Value>::new();
    let mut animations = Vec::<Value>::new();
    let mut lights = Vec::<Value>::new();
    let mut extensions_used = Vec::<Value>::new();

    for image in &scene.images {
        let view_index = append_blob_view(&mut buffer, &mut buffer_views, &image.bytes, None);
        images.push(json!({
            "name": image.name,
            "bufferView": view_index,
            "mimeType": image.mime_type,
        }));
    }

    for texture in &scene.textures {
        textures.push(json!({
            "name": texture.name,
            "source": texture.image,
        }));
    }

    for material in &scene.materials {
        let mut pbr = Map::new();
        pbr.insert("baseColorFactor".to_owned(), json!(material.base_color_factor));
        pbr.insert(
            "metallicFactor".to_owned(),
            json!(material.metallic_factor.clamp(0.0, 1.0)),
        );
        pbr.insert(
            "roughnessFactor".to_owned(),
            json!(material.roughness_factor.clamp(0.0, 1.0)),
        );
        if let Some(index) = material.base_color_texture {
            pbr.insert("baseColorTexture".to_owned(), json!({ "index": index }));
        }

        let mut value = Map::new();
        value.insert("name".to_owned(), json!(material.name));
        value.insert("pbrMetallicRoughness".to_owned(), Value::Object(pbr));
        if let Some(index) = material.normal_texture {
            value.insert("normalTexture".to_owned(), json!({ "index": index }));
        }
        if let Some(index) = material.emissive_texture {
            value.insert("emissiveTexture".to_owned(), json!({ "index": index }));
        }
        if material.emissive_factor != [0.0, 0.0, 0.0] {
            value.insert("emissiveFactor".to_owned(), json!(material.emissive_factor));
        }
        if !matches!(material.alpha_mode, AlphaMode::Opaque) {
            value.insert("alphaMode".to_owned(), json!(material.alpha_mode));
        }
        if let Some(alpha_cutoff) = material.alpha_cutoff {
            value.insert("alphaCutoff".to_owned(), json!(alpha_cutoff));
        }
        materials.push(Value::Object(value));
    }

    for mesh in &scene.meshes {
        let mut primitives = Vec::new();
        for primitive in &mesh.primitives {
            let position_accessor =
                append_vec3_accessor(&mut buffer, &mut buffer_views, &mut accessors, &primitive.positions);
            let normal_accessor =
                append_vec3_accessor(&mut buffer, &mut buffer_views, &mut accessors, &primitive.normals);
            let texcoord_accessor = primitive.texcoords0.as_ref().map(|uvs| {
                append_vec2_accessor(&mut buffer, &mut buffer_views, &mut accessors, uvs)
            });
            let index_accessor =
                append_indices_accessor(&mut buffer, &mut buffer_views, &mut accessors, &primitive.indices);

            let mut attributes = Map::new();
            attributes.insert("POSITION".to_owned(), json!(position_accessor));
            attributes.insert("NORMAL".to_owned(), json!(normal_accessor));
            if let Some(texcoord_accessor) = texcoord_accessor {
                attributes.insert("TEXCOORD_0".to_owned(), json!(texcoord_accessor));
            }

            let mut primitive_value = Map::new();
            primitive_value.insert("attributes".to_owned(), Value::Object(attributes));
            primitive_value.insert("indices".to_owned(), json!(index_accessor));
            primitive_value.insert("mode".to_owned(), json!(4));
            if let Some(material) = primitive.material {
                primitive_value.insert("material".to_owned(), json!(material));
            }
            primitives.push(Value::Object(primitive_value));
        }

        meshes.push(json!({
            "name": mesh.name,
            "primitives": primitives,
        }));
    }

    for camera in scene.nodes.iter().filter_map(|node| node.camera.as_ref()) {
        match camera.kind {
            CameraKind::Perspective => {
                let mut perspective = Map::new();
                perspective.insert("yfov".to_owned(), json!(camera.yfov.unwrap_or(0.7853982)));
                perspective.insert("znear".to_owned(), json!(camera.znear));
                perspective.insert("zfar".to_owned(), json!(camera.zfar));
                if let Some(aspect_ratio) = camera.aspect_ratio {
                    perspective.insert("aspectRatio".to_owned(), json!(aspect_ratio));
                }
                cameras.push(json!({
                    "type": "perspective",
                    "perspective": perspective,
                }));
            }
            CameraKind::Orthographic => {
                cameras.push(json!({
                    "type": "orthographic",
                    "orthographic": {
                        "xmag": camera.xmag.unwrap_or(1.0),
                        "ymag": camera.ymag.unwrap_or(1.0),
                        "znear": camera.znear,
                        "zfar": camera.zfar,
                    }
                }));
            }
        }
    }

    for light in &scene.lights {
        let mut value = Map::new();
        value.insert("name".to_owned(), json!(light.name));
        value.insert("type".to_owned(), json!(light.kind));
        value.insert("color".to_owned(), json!(light.color));
        value.insert("intensity".to_owned(), json!(light.intensity));
        if let Some(range) = light.range {
            value.insert("range".to_owned(), json!(range));
        }
        if let Some(outer) = light.outer_cone_angle {
            value.insert(
                "spot".to_owned(),
                json!({
                    "innerConeAngle": light.inner_cone_angle.unwrap_or(0.0),
                    "outerConeAngle": outer,
                }),
            );
        }
        lights.push(Value::Object(value));
    }
    if !lights.is_empty() {
        extensions_used.push(json!("KHR_lights_punctual"));
    }

    let mut camera_index = 0usize;
    for node in &scene.nodes {
        let mut value = Map::new();
        value.insert("name".to_owned(), json!(node.name));
        if node.translation != [0.0, 0.0, 0.0] {
            value.insert("translation".to_owned(), json!(node.translation));
        }
        if node.rotation != [0.0, 0.0, 0.0, 1.0] {
            value.insert("rotation".to_owned(), json!(node.rotation));
        }
        if node.scale != [1.0, 1.0, 1.0] {
            value.insert("scale".to_owned(), json!(node.scale));
        }
        if !node.children.is_empty() {
            value.insert("children".to_owned(), json!(node.children));
        }
        if let Some(mesh) = node.mesh {
            value.insert("mesh".to_owned(), json!(mesh));
        }
        if node.camera.is_some() {
            value.insert("camera".to_owned(), json!(camera_index));
            camera_index += 1;
        }
        if let Some(light_index) = node.light {
            value.insert(
                "extensions".to_owned(),
                json!({
                    "KHR_lights_punctual": {
                        "light": light_index
                    }
                }),
            );
        }
        nodes.push(Value::Object(value));
    }

    let top_level_scene_nodes = if matches!(options.axis_mode, AxisMode::BlenderGltfCompatible)
        && !scene.scene_nodes.is_empty()
    {
        nodes.push(json!({
            "name": "BlendIOAxisRoot",
            "rotation": [-FRAC_1_SQRT_2, 0.0, 0.0, FRAC_1_SQRT_2],
            "children": scene.scene_nodes,
        }));
        vec![nodes.len() - 1]
    } else {
        scene.scene_nodes.clone()
    };

    for animation in &scene.animations {
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for channel in &animation.channels {
            let input_values = channel
                .keyframes
                .iter()
                .map(|frame| frame.time_seconds)
                .collect::<Vec<_>>();
            let output_values = channel
                .keyframes
                .iter()
                .flat_map(|frame| frame.values.iter().copied())
                .collect::<Vec<_>>();
            let input_accessor =
                append_scalar_accessor(&mut buffer, &mut buffer_views, &mut accessors, &input_values);
            let output_accessor = match channel.path {
                AnimationPath::Rotation => append_vec4_accessor(
                    &mut buffer,
                    &mut buffer_views,
                    &mut accessors,
                    &chunk_vec4(&output_values),
                ),
                AnimationPath::Translation | AnimationPath::Scale => append_vec3_accessor(
                    &mut buffer,
                    &mut buffer_views,
                    &mut accessors,
                    &chunk_vec3(&output_values),
                ),
            };
            let sampler_index = samplers.len();
            samplers.push(json!({
                "input": input_accessor,
                "output": output_accessor,
                "interpolation": "LINEAR",
            }));
            channels.push(json!({
                "sampler": sampler_index,
                "target": {
                    "node": channel.node,
                    "path": channel.path,
                }
            }));
        }
        animations.push(json!({
            "name": animation.name,
            "samplers": samplers,
            "channels": channels,
        }));
    }

    let mut root = Map::new();
    root.insert(
        "asset".to_owned(),
        json!({
            "version": "2.0",
            "generator": "blendio blend2glb",
        }),
    );
    root.insert(
        "buffers".to_owned(),
        json!([{
            "byteLength": buffer.bytes.len(),
        }]),
    );
    if !buffer_views.is_empty() {
        root.insert("bufferViews".to_owned(), Value::Array(buffer_views));
    }
    if !accessors.is_empty() {
        root.insert("accessors".to_owned(), Value::Array(accessors));
    }
    if !images.is_empty() {
        root.insert("images".to_owned(), Value::Array(images));
    }
    if !textures.is_empty() {
        root.insert("textures".to_owned(), Value::Array(textures));
    }
    if !materials.is_empty() {
        root.insert("materials".to_owned(), Value::Array(materials));
    }
    if !meshes.is_empty() {
        root.insert("meshes".to_owned(), Value::Array(meshes));
    }
    if !cameras.is_empty() {
        root.insert("cameras".to_owned(), Value::Array(cameras));
    }
    if !nodes.is_empty() {
        root.insert("nodes".to_owned(), Value::Array(nodes));
    }
    if !animations.is_empty() {
        root.insert("animations".to_owned(), Value::Array(animations));
    }
    root.insert("scenes".to_owned(), json!([{ "nodes": top_level_scene_nodes }]));
    root.insert("scene".to_owned(), json!(0));
    if !lights.is_empty() {
        root.insert(
            "extensions".to_owned(),
            json!({
                "KHR_lights_punctual": {
                    "lights": lights,
                }
            }),
        );
    }
    if !extensions_used.is_empty() {
        root.insert("extensionsUsed".to_owned(), Value::Array(extensions_used));
    }

    let json_bytes = serde_json::to_vec(&Value::Object(root))?;
    Ok(pack_glb(&json_bytes, &buffer.bytes))
}

#[derive(Default)]
struct BinaryBuffer {
    bytes: Vec<u8>,
}

impl BinaryBuffer {
    fn append_bytes(&mut self, bytes: &[u8], alignment: usize) -> (usize, usize) {
        while self.bytes.len() % alignment != 0 {
            self.bytes.push(0);
        }
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        (offset, bytes.len())
    }
}

fn append_blob_view(
    buffer: &mut BinaryBuffer,
    buffer_views: &mut Vec<Value>,
    bytes: &[u8],
    target: Option<u32>,
) -> usize {
    let (offset, length) = buffer.append_bytes(bytes, 4);
    let mut view = Map::new();
    view.insert("buffer".to_owned(), json!(0));
    view.insert("byteOffset".to_owned(), json!(offset));
    view.insert("byteLength".to_owned(), json!(length));
    if let Some(target) = target {
        view.insert("target".to_owned(), json!(target));
    }
    buffer_views.push(Value::Object(view));
    buffer_views.len() - 1
}

fn append_scalar_accessor(
    buffer: &mut BinaryBuffer,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    values: &[f32],
) -> usize {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let view_index = append_blob_view(buffer, buffer_views, &bytes, None);
    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    accessors.push(json!({
        "bufferView": view_index,
        "componentType": 5126,
        "count": values.len(),
        "type": "SCALAR",
        "min": [min],
        "max": [max],
    }));
    accessors.len() - 1
}

fn append_indices_accessor(
    buffer: &mut BinaryBuffer,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    values: &[u32],
) -> usize {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let view_index = append_blob_view(buffer, buffer_views, &bytes, Some(34963));
    let min = values.iter().copied().min().unwrap_or(0);
    let max = values.iter().copied().max().unwrap_or(0);
    accessors.push(json!({
        "bufferView": view_index,
        "componentType": 5125,
        "count": values.len(),
        "type": "SCALAR",
        "min": [min],
        "max": [max],
    }));
    accessors.len() - 1
}

fn append_vec2_accessor(
    buffer: &mut BinaryBuffer,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    values: &[[f32; 2]],
) -> usize {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for value in values {
        min[0] = min[0].min(value[0]);
        min[1] = min[1].min(value[1]);
        max[0] = max[0].max(value[0]);
        max[1] = max[1].max(value[1]);
        bytes.extend_from_slice(&value[0].to_le_bytes());
        bytes.extend_from_slice(&value[1].to_le_bytes());
    }
    let view_index = append_blob_view(buffer, buffer_views, &bytes, Some(34962));
    accessors.push(json!({
        "bufferView": view_index,
        "componentType": 5126,
        "count": values.len(),
        "type": "VEC2",
        "min": min,
        "max": max,
    }));
    accessors.len() - 1
}

fn append_vec3_accessor(
    buffer: &mut BinaryBuffer,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    values: &[[f32; 3]],
) -> usize {
    let mut bytes = Vec::with_capacity(values.len() * 12);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for value in values {
        for index in 0..3 {
            min[index] = min[index].min(value[index]);
            max[index] = max[index].max(value[index]);
            bytes.extend_from_slice(&value[index].to_le_bytes());
        }
    }
    let view_index = append_blob_view(buffer, buffer_views, &bytes, Some(34962));
    accessors.push(json!({
        "bufferView": view_index,
        "componentType": 5126,
        "count": values.len(),
        "type": "VEC3",
        "min": min,
        "max": max,
    }));
    accessors.len() - 1
}

fn append_vec4_accessor(
    buffer: &mut BinaryBuffer,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    values: &[[f32; 4]],
) -> usize {
    let mut bytes = Vec::with_capacity(values.len() * 16);
    let mut min = [f32::INFINITY; 4];
    let mut max = [f32::NEG_INFINITY; 4];
    for value in values {
        for index in 0..4 {
            min[index] = min[index].min(value[index]);
            max[index] = max[index].max(value[index]);
            bytes.extend_from_slice(&value[index].to_le_bytes());
        }
    }
    let view_index = append_blob_view(buffer, buffer_views, &bytes, None);
    accessors.push(json!({
        "bufferView": view_index,
        "componentType": 5126,
        "count": values.len(),
        "type": "VEC4",
        "min": min,
        "max": max,
    }));
    accessors.len() - 1
}

fn pack_glb(json_bytes: &[u8], bin_bytes: &[u8]) -> Vec<u8> {
    let json_padded_len = (json_bytes.len() + 3) & !3;
    let bin_padded_len = (bin_bytes.len() + 3) & !3;
    let total_length = 12 + 8 + json_padded_len + 8 + bin_padded_len;

    let mut output = Vec::with_capacity(total_length);
    output.extend_from_slice(b"glTF");
    output.extend_from_slice(&2_u32.to_le_bytes());
    output.extend_from_slice(&(total_length as u32).to_le_bytes());
    output.extend_from_slice(&(json_padded_len as u32).to_le_bytes());
    output.extend_from_slice(&0x4E4F534A_u32.to_le_bytes());
    output.extend_from_slice(json_bytes);
    while output.len() % 4 != 0 {
        output.push(b' ');
    }
    output.extend_from_slice(&(bin_padded_len as u32).to_le_bytes());
    output.extend_from_slice(&0x004E4942_u32.to_le_bytes());
    output.extend_from_slice(bin_bytes);
    while output.len() % 4 != 0 {
        output.push(0);
    }
    output
}

fn chunk_vec3(values: &[f32]) -> Vec<[f32; 3]> {
    values
        .chunks_exact(3)
        .map(|chunk| [chunk[0], chunk[1], chunk[2]])
        .collect()
}

fn chunk_vec4(values: &[f32]) -> Vec<[f32; 4]> {
    values
        .chunks_exact(4)
        .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
        .collect()
}

pub(crate) fn normalize_quaternion(quat: [f32; 4]) -> [f32; 4] {
    let len = (quat[0] * quat[0] + quat[1] * quat[1] + quat[2] * quat[2] + quat[3] * quat[3])
        .sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [quat[0] / len, quat[1] / len, quat[2] / len, quat[3] / len]
    }
}

pub(crate) fn quaternion_from_axis_angle(axis: [f32; 3], angle: f32) -> [f32; 4] {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if len <= f32::EPSILON {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let half = angle * 0.5;
    let sin = half.sin() / len;
    normalize_quaternion([axis[0] * sin, axis[1] * sin, axis[2] * sin, half.cos()])
}

pub(crate) fn quaternion_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    normalize_quaternion([
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ])
}

pub(crate) fn euler_to_quaternion(rotmode: i16, euler: [f32; 3]) -> [f32; 4] {
    let qx = quaternion_from_axis_angle([1.0, 0.0, 0.0], euler[0]);
    let qy = quaternion_from_axis_angle([0.0, 1.0, 0.0], euler[1]);
    let qz = quaternion_from_axis_angle([0.0, 0.0, 1.0], euler[2]);
    match rotmode {
        1 => quaternion_mul(quaternion_mul(qz, qy), qx),
        2 => quaternion_mul(quaternion_mul(qy, qz), qx),
        3 => quaternion_mul(quaternion_mul(qz, qx), qy),
        4 => quaternion_mul(quaternion_mul(qx, qz), qy),
        5 => quaternion_mul(quaternion_mul(qy, qx), qz),
        6 => quaternion_mul(quaternion_mul(qx, qy), qz),
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}
