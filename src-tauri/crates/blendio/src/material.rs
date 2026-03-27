use std::collections::HashMap;
use std::path::Path;

use crate::array_view::{iter_listbase, read_pointer_array};
use crate::error::{BlendError, Result};
use crate::gltf_export::{AlphaMode, ExportImage, ExportMaterial, ExportTexture};
use crate::report::{ExportOptions, ExportReport, ExportWarningKind};
use crate::view::{BlendFile, StructView};

pub struct MaterialResolver<'a> {
    file: &'a BlendFile,
    options: &'a ExportOptions,
    materials: Vec<ExportMaterial>,
    textures: Vec<ExportTexture>,
    images: Vec<ExportImage>,
    material_cache: HashMap<u64, usize>,
    image_cache: HashMap<u64, usize>,
}

#[derive(Clone)]
struct NodeInfo {
    idname: String,
    inputs: Vec<SocketInfo>,
    linked_id: Option<u64>,
}

#[derive(Clone)]
struct SocketInfo {
    old_ptr: u64,
    name: String,
    identifier: String,
    default_value: Option<u64>,
}

#[derive(Clone)]
struct LinkInfo {
    from_node: u64,
    to_socket: u64,
}

impl<'a> MaterialResolver<'a> {
    pub fn new(file: &'a BlendFile, options: &'a ExportOptions) -> Self {
        Self {
            file,
            options,
            materials: Vec::new(),
            textures: Vec::new(),
            images: Vec::new(),
            material_cache: HashMap::new(),
            image_cache: HashMap::new(),
        }
    }

    pub fn resolve_material_pointer(
        &mut self,
        old_ptr: u64,
        object_name: &str,
        report: &mut ExportReport,
    ) -> Result<Option<usize>> {
        if old_ptr == 0 {
            return Ok(None);
        }
        if let Some(index) = self.material_cache.get(&old_ptr).copied() {
            return Ok(Some(index));
        }

        let Some(material_view) = self.file.view_old_ptr_as_struct(old_ptr, "Material")? else {
            return Ok(None);
        };
        let material = self.extract_material(&material_view, object_name, report)?;
        let index = self.materials.len();
        self.materials.push(material);
        self.material_cache.insert(old_ptr, index);
        Ok(Some(index))
    }

    pub fn into_parts(self) -> (Vec<ExportMaterial>, Vec<ExportTexture>, Vec<ExportImage>) {
        (self.materials, self.textures, self.images)
    }

    fn extract_material(
        &mut self,
        material: &StructView<'_>,
        object_name: &str,
        report: &mut ExportReport,
    ) -> Result<ExportMaterial> {
        let name = raw_id_name(material)
            .map(|value| strip_id_prefix(&value))
            .unwrap_or_else(|| "Material".to_owned());
        let mut result = ExportMaterial {
            name,
            base_color_factor: [
                material.field("r").and_then(|field| field.as_f32()).unwrap_or(0.8),
                material.field("g").and_then(|field| field.as_f32()).unwrap_or(0.8),
                material.field("b").and_then(|field| field.as_f32()).unwrap_or(0.8),
                material
                    .field("alpha")
                    .and_then(|field| field.as_f32())
                    .or_else(|| material.field("a").and_then(|field| field.as_f32()))
                    .unwrap_or(1.0),
            ],
            metallic_factor: material
                .field("metallic")
                .and_then(|field| field.as_f32())
                .unwrap_or(0.0)
                .clamp(0.0, 1.0),
            roughness_factor: material
                .field("roughness")
                .and_then(|field| field.as_f32())
                .unwrap_or(0.5)
                .clamp(0.0, 1.0),
            emissive_factor: [0.0, 0.0, 0.0],
            alpha_mode: alpha_mode_from_material(material),
            alpha_cutoff: material.field("alpha_threshold").and_then(|field| field.as_f32()),
            base_color_texture: None,
            normal_texture: None,
            emissive_texture: None,
        };

        let use_nodes = material
            .field("use_nodes")
            .and_then(|field| field.as_u8())
            .unwrap_or(0)
            != 0;
        if use_nodes {
            let nodetree_ptr = material
                .field("nodetree")
                .and_then(|field| field.as_pointer())
                .unwrap_or(0);
            if nodetree_ptr != 0 {
                self.apply_node_tree(nodetree_ptr, &mut result, object_name, report)?;
            }
        }

        Ok(result)
    }

    fn apply_node_tree(
        &mut self,
        nodetree_ptr: u64,
        material: &mut ExportMaterial,
        object_name: &str,
        report: &mut ExportReport,
    ) -> Result<()> {
        let Some(tree) = self.file.view_old_ptr_as_struct(nodetree_ptr, "bNodeTree")? else {
            return Ok(());
        };
        let nodes_list = tree.field("nodes").and_then(|field| field.as_struct_view());
        let links_list = tree.field("links").and_then(|field| field.as_struct_view());
        let Some(nodes_list) = nodes_list else {
            return Ok(());
        };
        let Some(links_list) = links_list else {
            return Ok(());
        };

        let mut nodes = HashMap::<u64, NodeInfo>::new();
        for block in iter_listbase(self.file, &nodes_list)? {
            let view = block.view_as("bNode")?;
            let inputs = view
                .field("inputs")
                .and_then(|field| field.as_struct_view())
                .map(|list| self.read_sockets(&list))
                .transpose()?
                .unwrap_or_default();
            let _outputs = view
                .field("outputs")
                .and_then(|field| field.as_struct_view())
                .map(|list| self.read_sockets(&list))
                .transpose()?
                .unwrap_or_default();
            nodes.insert(
                block.header().old_ptr,
                NodeInfo {
                    idname: view
                        .field("idname")
                        .and_then(|field| field.as_c_string())
                        .unwrap_or_default(),
                    inputs,
                    linked_id: view.field("id").and_then(|field| field.as_pointer()).filter(|ptr| *ptr != 0),
                },
            );
        }

        let mut links = Vec::new();
        for block in iter_listbase(self.file, &links_list)? {
            let view = block.view_as("bNodeLink")?;
            links.push(LinkInfo {
                from_node: view.field("fromnode").and_then(|field| field.as_pointer()).unwrap_or(0),
                to_socket: view.field("tosock").and_then(|field| field.as_pointer()).unwrap_or(0),
            });
        }

        let Some(output) = nodes
            .values()
            .find(|node| node.idname == "ShaderNodeOutputMaterial")
            .cloned()
        else {
            report.warn(
                self.options,
                ExportWarningKind::UnsupportedMaterialNode,
                Some(object_name),
                "material node tree has no Material Output node; using fallback values",
            )?;
            return Ok(());
        };

        let Some(surface_socket) = output.inputs.iter().find(|socket| socket.name == "Surface") else {
            return Ok(());
        };
        let Some(surface_link) = links.iter().find(|link| link.to_socket == surface_socket.old_ptr) else {
            return Ok(());
        };
        let Some(principled) = nodes.get(&surface_link.from_node) else {
            return Ok(());
        };
        if principled.idname != "ShaderNodeBsdfPrincipled" {
            report.warn(
                self.options,
                ExportWarningKind::UnsupportedMaterialNode,
                Some(object_name),
                format!(
                    "material output is driven by {}, falling back to basic material values",
                    principled.idname
                ),
            )?;
            return Ok(());
        }

        self.apply_principled_inputs(principled, &nodes, &links, material, object_name, report)
    }

    fn read_sockets(&self, list: &StructView<'_>) -> Result<Vec<SocketInfo>> {
        let mut sockets = Vec::new();
        for block in iter_listbase(self.file, list)? {
            let view = block.view_as("bNodeSocket")?;
            sockets.push(SocketInfo {
                old_ptr: block.header().old_ptr,
                name: view.field("name").and_then(|field| field.as_c_string()).unwrap_or_default(),
                identifier: view
                    .field("identifier")
                    .and_then(|field| field.as_c_string())
                    .unwrap_or_default(),
                default_value: view
                    .field("default_value")
                    .and_then(|field| field.as_pointer())
                    .filter(|ptr| *ptr != 0),
            });
        }
        Ok(sockets)
    }

    fn apply_principled_inputs(
        &mut self,
        principled: &NodeInfo,
        nodes: &HashMap<u64, NodeInfo>,
        links: &[LinkInfo],
        material: &mut ExportMaterial,
        object_name: &str,
        report: &mut ExportReport,
    ) -> Result<()> {
        if let Some(socket) = principled
            .inputs
            .iter()
            .find(|socket| socket.identifier == "Base Color" || socket.name == "Base Color")
        {
            if let Some(link) = links.iter().find(|link| link.to_socket == socket.old_ptr) {
                if let Some(texture) = self.texture_from_link(link, nodes, links, object_name, report)? {
                    material.base_color_texture = Some(texture);
                }
            } else if let Some(default_ptr) = socket.default_value {
                if let Some(value) = self.file.view_old_ptr_as_struct(default_ptr, "bNodeSocketValueRGBA")? {
                    material.base_color_factor = value
                        .field("value")
                        .and_then(|field| field.as_f32_array::<4>())
                        .unwrap_or(material.base_color_factor);
                }
            }
        }

        for (label, target) in [
            ("Metallic", PrincipledScalarTarget::Metallic),
            ("Roughness", PrincipledScalarTarget::Roughness),
            ("Alpha", PrincipledScalarTarget::Alpha),
        ] {
            if let Some(socket) = principled
                .inputs
                .iter()
                .find(|socket| socket.identifier == label || socket.name == label)
            {
                if links.iter().any(|link| link.to_socket == socket.old_ptr) {
                    report.warn(
                        self.options,
                        ExportWarningKind::UnsupportedMaterialNode,
                        Some(object_name),
                        format!("{label} texture input is not exported in this first version"),
                    )?;
                } else if let Some(default_ptr) = socket.default_value {
                    if let Some(value) = self.file.view_old_ptr_as_struct(default_ptr, "bNodeSocketValueFloat")? {
                        if let Some(scalar) = value.field("value").and_then(|field| field.as_f32()) {
                            match target {
                                PrincipledScalarTarget::Metallic => {
                                    material.metallic_factor = scalar.clamp(0.0, 1.0)
                                }
                                PrincipledScalarTarget::Roughness => {
                                    material.roughness_factor = scalar.clamp(0.0, 1.0)
                                }
                                PrincipledScalarTarget::Alpha => {
                                    material.base_color_factor[3] = scalar.clamp(0.0, 1.0)
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(socket) = principled
            .inputs
            .iter()
            .find(|socket| socket.identifier == "Emission Color" || socket.name == "Emission Color")
        {
            if let Some(link) = links.iter().find(|link| link.to_socket == socket.old_ptr) {
                if let Some(texture) = self.texture_from_link(link, nodes, links, object_name, report)? {
                    material.emissive_texture = Some(texture);
                    material.emissive_factor = [1.0, 1.0, 1.0];
                }
            } else if let Some(default_ptr) = socket.default_value {
                if let Some(value) = self.file.view_old_ptr_as_struct(default_ptr, "bNodeSocketValueRGBA")? {
                    let emissive = value
                        .field("value")
                        .and_then(|field| field.as_f32_array::<4>())
                        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    material.emissive_factor = [emissive[0], emissive[1], emissive[2]];
                }
            }
        }

        if let Some(socket) = principled
            .inputs
            .iter()
            .find(|socket| socket.identifier == "Normal" || socket.name == "Normal")
        {
            if let Some(link) = links.iter().find(|link| link.to_socket == socket.old_ptr) {
                if let Some(texture) =
                    self.texture_from_normal_link(link, nodes, links, object_name, report)?
                {
                    material.normal_texture = Some(texture);
                }
            }
        }

        Ok(())
    }

    fn texture_from_normal_link(
        &mut self,
        link: &LinkInfo,
        nodes: &HashMap<u64, NodeInfo>,
        links: &[LinkInfo],
        object_name: &str,
        report: &mut ExportReport,
    ) -> Result<Option<usize>> {
        let Some(node) = nodes.get(&link.from_node) else {
            return Ok(None);
        };
        if node.idname == "ShaderNodeNormalMap" {
            let Some(color_socket) = node
                .inputs
                .iter()
                .find(|socket| socket.identifier == "Color" || socket.name == "Color")
            else {
                return Ok(None);
            };
            let Some(color_link) = links.iter().find(|item| item.to_socket == color_socket.old_ptr) else {
                return Ok(None);
            };
            return self.texture_from_link(color_link, nodes, links, object_name, report);
        }
        self.texture_from_link(link, nodes, links, object_name, report)
    }

    fn texture_from_link(
        &mut self,
        link: &LinkInfo,
        nodes: &HashMap<u64, NodeInfo>,
        _links: &[LinkInfo],
        object_name: &str,
        report: &mut ExportReport,
    ) -> Result<Option<usize>> {
        let Some(node) = nodes.get(&link.from_node) else {
            return Ok(None);
        };
        if node.idname != "ShaderNodeTexImage" {
            report.warn(
                self.options,
                ExportWarningKind::UnsupportedMaterialNode,
                Some(object_name),
                format!("unsupported texture source node {}", node.idname),
            )?;
            return Ok(None);
        }
        let Some(image_ptr) = node.linked_id else {
            return Ok(None);
        };
        self.resolve_image_pointer(image_ptr, object_name, report)
    }

    fn resolve_image_pointer(
        &mut self,
        old_ptr: u64,
        object_name: &str,
        report: &mut ExportReport,
    ) -> Result<Option<usize>> {
        if old_ptr == 0 {
            return Ok(None);
        }
        if let Some(index) = self.image_cache.get(&old_ptr).copied() {
            return Ok(Some(index));
        }
        let Some(image_view) = self.file.view_old_ptr_as_struct(old_ptr, "Image")? else {
            return Ok(None);
        };

        let (name, bytes) = if let Some(packed_ptr) = image_view.field("packedfile").and_then(|field| field.as_pointer()).filter(|ptr| *ptr != 0) {
            (image_name(&image_view), self.read_packed_file_bytes(packed_ptr)?)
        } else {
            let Some(packedfiles) = image_view.field("packedfiles").and_then(|field| field.as_struct_view()) else {
                report.warn(
                    self.options,
                    ExportWarningKind::MissingPackedTexture,
                    Some(object_name),
                    "image is external or unpacked; only packed textures are exported",
                )?;
                return Ok(None);
            };
            let Some(first) = iter_listbase(self.file, &packedfiles)?.into_iter().next() else {
                report.warn(
                    self.options,
                    ExportWarningKind::MissingPackedTexture,
                    Some(object_name),
                    "image is external or unpacked; only packed textures are exported",
                )?;
                return Ok(None);
            };
            let packed_view = first.view_as("ImagePackedFile")?;
            let packed_ptr = packed_view
                .field("packedfile")
                .and_then(|field| field.as_pointer())
                .unwrap_or(0);
            if packed_ptr == 0 {
                report.warn(
                    self.options,
                    ExportWarningKind::MissingPackedTexture,
                    Some(object_name),
                    "image packed file entry has no payload",
                )?;
                return Ok(None);
            }
            (
                packed_view
                    .field("filepath")
                    .and_then(|field| field.as_c_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| image_name(&image_view)),
                self.read_packed_file_bytes(packed_ptr)?,
            )
        };

        let Some(bytes) = bytes else {
            report.warn(
                self.options,
                ExportWarningKind::MissingPackedTexture,
                Some(object_name),
                "packed texture payload is missing",
            )?;
            return Ok(None);
        };

        let Some(mime_type) = infer_mime_type(&name, &bytes) else {
            report.warn(
                self.options,
                ExportWarningKind::MissingPackedTexture,
                Some(object_name),
                format!("unsupported packed image format for {name}"),
            )?;
            return Ok(None);
        };

        let image_index = self.images.len();
        self.images.push(ExportImage {
            name: Path::new(&name)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&name)
                .to_owned(),
            mime_type,
            bytes,
        });
        self.textures.push(ExportTexture {
            name: Path::new(&name)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Image")
                .to_owned(),
            image: image_index,
        });
        self.image_cache.insert(old_ptr, self.textures.len() - 1);
        Ok(Some(self.textures.len() - 1))
    }

    fn read_packed_file_bytes(&self, packed_ptr: u64) -> Result<Option<Vec<u8>>> {
        let Some(packed_view) = self.file.view_old_ptr_as_struct(packed_ptr, "PackedFile")? else {
            return Ok(None);
        };
        let size = packed_view
            .field("size")
            .and_then(|field| field.as_i32())
            .unwrap_or_default()
            .max(0) as usize;
        let Some(data_ptr) = packed_view.field("data").and_then(|field| field.as_pointer()) else {
            return Ok(None);
        };
        let block = self
            .file
            .resolve_old_ptr(data_ptr)
            .ok_or(BlendError::MissingOldPointer { ptr: data_ptr })?;
        if block.bytes().len() < size {
            return Err(BlendError::TruncatedBlock {
                offset: block.header().file_offset,
            });
        }
        Ok(Some(block.bytes()[..size].to_vec()))
    }
}

enum PrincipledScalarTarget {
    Metallic,
    Roughness,
    Alpha,
}

pub fn resolve_material_slots(
    file: &BlendFile,
    mesh_view: &StructView<'_>,
    object_view: &StructView<'_>,
    object_name: &str,
    materials: &mut MaterialResolver<'_>,
    report: &mut ExportReport,
) -> Result<Vec<Option<usize>>> {
    let object_count = object_view
        .field("totcol")
        .and_then(|field| field.as_i32())
        .unwrap_or_default()
        .max(0) as usize;
    let mesh_count = mesh_view
        .field("totcol")
        .and_then(|field| field.as_i16())
        .unwrap_or_default()
        .max(0) as usize;
    let (pointer, count) = match object_view
        .field("mat")
        .and_then(|field| field.as_pointer())
        .filter(|ptr| *ptr != 0)
    {
        Some(pointer) if object_count > 0 => (pointer, object_count),
        _ => match mesh_view
            .field("mat")
            .and_then(|field| field.as_pointer())
            .filter(|ptr| *ptr != 0)
        {
            Some(pointer) if mesh_count > 0 => (pointer, mesh_count),
            _ => return Ok(Vec::new()),
        },
    };

    if let Some(pointer_array) = read_pointer_array(file, pointer, count)? {
        let mut resolved = Vec::with_capacity(pointer_array.len());
        for ptr in pointer_array.iter() {
            resolved.push(materials.resolve_material_pointer(ptr, object_name, report)?);
        }
        if resolved.iter().any(|value| value.is_some()) {
            return Ok(resolved);
        }
    }

    let material_ids = file
        .ids()
        .into_iter()
        .filter(|block| block.header().code.as_string() == "MA")
        .map(|block| block.header().old_ptr)
        .collect::<Vec<_>>();
    if material_ids.is_empty() {
        return Ok(Vec::new());
    }

    let fallback_count = if material_ids.len() == count {
        count
    } else if material_ids.len() == 1 {
        count.min(1)
    } else {
        0
    };
    if fallback_count == 0 {
        return Ok(Vec::new());
    }

    let mut resolved = Vec::with_capacity(fallback_count);
    for old_ptr in material_ids.into_iter().take(fallback_count) {
        resolved.push(materials.resolve_material_pointer(old_ptr, object_name, report)?);
    }
    Ok(resolved)
}

fn alpha_mode_from_material(material: &StructView<'_>) -> AlphaMode {
    match material
        .field("blend_method")
        .and_then(|field| field.as_u8())
        .unwrap_or(0)
    {
        3 => AlphaMode::Mask,
        4 | 5 => AlphaMode::Blend,
        _ => AlphaMode::Opaque,
    }
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

fn image_name(image: &StructView<'_>) -> String {
    image
        .field("name")
        .and_then(|field| field.as_c_string())
        .filter(|value| !value.is_empty())
        .or_else(|| raw_id_name(image).map(|value| strip_id_prefix(&value)))
        .unwrap_or_else(|| "Image".to_owned())
}

fn infer_mime_type(name: &str, bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png".to_owned());
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg".to_owned());
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some("image/webp".to_owned());
    }

    match Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png".to_owned()),
        Some("jpg") | Some("jpeg") => Some("image/jpeg".to_owned()),
        Some("webp") => Some("image/webp".to_owned()),
        _ => None,
    }
}
