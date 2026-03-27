use crate::array_view::read_struct_array;
use crate::error::Result;
use crate::gltf_export::{ExportMesh, ExportPrimitive};
use crate::material::{MaterialResolver, resolve_material_slots};
use crate::modifier::apply_supported_modifiers;
use crate::report::{ExportOptions, ExportReport, ExportWarningKind};
use crate::view::{BlendFile, StructView};

#[derive(Clone)]
pub(crate) struct RawMesh {
    pub name: String,
    pub vertices: Vec<[f32; 3]>,
    pub loops: Vec<RawLoop>,
    pub polys: Vec<RawPoly>,
    pub material_slots: Vec<Option<usize>>,
}

#[derive(Clone, Copy)]
pub(crate) struct RawLoop {
    pub vertex: usize,
    pub uv: Option<[f32; 2]>,
}

#[derive(Clone, Copy)]
pub(crate) struct RawPoly {
    pub loopstart: usize,
    pub totloop: usize,
    pub material_index: usize,
}

pub fn build_object_mesh(
    file: &BlendFile,
    object: &StructView<'_>,
    object_name: &str,
    options: &ExportOptions,
    materials: &mut MaterialResolver<'_>,
    report: &mut ExportReport,
) -> Result<Option<ExportMesh>> {
    let Some(mesh_ptr) = object.field("data").and_then(|field| field.as_pointer()) else {
        return Ok(None);
    };
    let Some(mesh_view) = file.view_old_ptr_as_struct(mesh_ptr, "Mesh")? else {
        return Ok(None);
    };

    let raw = load_raw_mesh(file, &mesh_view, object, object_name, options, materials, report)?;
    let raw = apply_supported_modifiers(file, object, object_name, raw, options, report)?;
    if raw.vertices.is_empty() || raw.polys.is_empty() {
        return Ok(None);
    }
    Ok(Some(triangulate_mesh(raw)))
}

fn load_raw_mesh(
    file: &BlendFile,
    mesh: &StructView<'_>,
    object: &StructView<'_>,
    object_name: &str,
    options: &ExportOptions,
    materials: &mut MaterialResolver<'_>,
    report: &mut ExportReport,
) -> Result<RawMesh> {
    let name = raw_id_name(mesh)
        .map(|value| strip_id_prefix(&value))
        .unwrap_or_else(|| object_name.to_owned());
    let totvert = mesh
        .field("totvert")
        .and_then(|field| field.as_i32())
        .unwrap_or_default()
        .max(0) as usize;
    let totloop = mesh
        .field("totloop")
        .and_then(|field| field.as_i32())
        .unwrap_or_default()
        .max(0) as usize;
    let totpoly = mesh
        .field("totpoly")
        .and_then(|field| field.as_i32())
        .unwrap_or_default()
        .max(0) as usize;

    let vertices = mesh
        .field("mvert")
        .and_then(|field| field.as_pointer())
        .map(|ptr| read_struct_array(file, ptr, "MVert", totvert))
        .transpose()?
        .flatten()
        .map(|array| {
            array
                .iter()
                .map(|view| {
                    view.field("co")
                        .and_then(|field| field.as_f32_array::<3>())
                        .unwrap_or([0.0, 0.0, 0.0])
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let vertices = if vertices.is_empty() && totvert > 0 {
        load_positions_from_attributes(file, mesh, totvert)?
    } else {
        vertices
    };

    let uv_layer = load_uv_layer(file, mesh, totloop, object_name, options, report)?;
    let loops = mesh
        .field("mloop")
        .and_then(|field| field.as_pointer())
        .map(|ptr| read_struct_array(file, ptr, "MLoop", totloop))
        .transpose()?
        .flatten()
        .map(|array| {
            array
                .iter()
                .enumerate()
                .map(|(index, view)| RawLoop {
                    vertex: view.field("v").and_then(|field| field.as_u32()).unwrap_or_default()
                        as usize,
                    uv: uv_layer.get(index).copied().flatten(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let loops = if loops.is_empty() && totloop > 0 {
        load_loops_from_attributes(file, mesh, totloop)?
            .into_iter()
            .enumerate()
            .map(|(index, vertex)| RawLoop {
                vertex,
                uv: uv_layer.get(index).copied().flatten(),
            })
            .collect()
    } else {
        loops
    };

    let polys = mesh
        .field("mpoly")
        .and_then(|field| field.as_pointer())
        .map(|ptr| read_struct_array(file, ptr, "MPoly", totpoly))
        .transpose()?
        .flatten()
        .map(|array| {
            array
                .iter()
                .map(|view| RawPoly {
                    loopstart: view
                        .field("loopstart")
                        .and_then(|field| field.as_i32())
                        .unwrap_or_default()
                        .max(0) as usize,
                    totloop: view
                        .field("totloop")
                        .and_then(|field| field.as_i32())
                        .unwrap_or_default()
                        .max(0) as usize,
                    material_index: view
                        .field("mat_nr")
                        .and_then(|field| field.as_i16())
                        .unwrap_or_default()
                        .max(0) as usize,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let polys = if polys.is_empty() && totpoly > 0 {
        load_polys_from_offsets(file, mesh, totpoly)?
    } else {
        polys
    };

    let material_slots = resolve_material_slots(file, mesh, object, object_name, materials, report)?;

    Ok(RawMesh {
        name,
        vertices,
        loops,
        polys,
        material_slots,
    })
}

fn load_uv_layer(
    file: &BlendFile,
    mesh: &StructView<'_>,
    totloop: usize,
    object_name: &str,
    options: &ExportOptions,
    report: &mut ExportReport,
) -> Result<Vec<Option<[f32; 2]>>> {
    let Some(ldata) = mesh.field("ldata").and_then(|field| field.as_struct_view()) else {
        return Ok(vec![None; totloop]);
    };
    let layer_count = ldata
        .field("totlayer")
        .and_then(|field| field.as_i32())
        .unwrap_or_default()
        .max(0) as usize;
    let Some(layer_ptr) = ldata.field("layers").and_then(|field| field.as_pointer()) else {
        return Ok(vec![None; totloop]);
    };
    let Some(layers) = read_struct_array(file, layer_ptr, "CustomDataLayer", layer_count)? else {
        return Ok(vec![None; totloop]);
    };
    for layer in layers.iter() {
        let Some(data_ptr) = layer.field("data").and_then(|field| field.as_pointer()) else {
            continue;
        };
        let Some(block) = file.resolve_old_ptr(data_ptr) else {
            continue;
        };
        if let Some(struct_def) = block.struct_def() {
            if struct_def.type_name == "MLoopUV" {
                if let Some(uvs) = read_struct_array(file, data_ptr, "MLoopUV", totloop)? {
                    return Ok(uvs
                        .iter()
                        .map(|view| {
                            view.field("uv").and_then(|field| field.as_f32_array::<2>()).map(|uv| {
                                [uv[0], 1.0 - uv[1]]
                            })
                        })
                        .collect());
                }
            }
            if struct_def.type_name == "vec2f" {
                if let Some(uvs) = read_struct_array(file, data_ptr, "vec2f", totloop)? {
                    return Ok(uvs
                        .iter()
                        .map(|view| {
                            Some([
                                view.field("x").and_then(|field| field.as_f32()).unwrap_or(0.0),
                                1.0 - view.field("y").and_then(|field| field.as_f32()).unwrap_or(0.0),
                            ])
                        })
                        .collect());
                }
            }
        }

        if block.bytes().len() >= totloop * 8 {
            report.warn(
                options,
                ExportWarningKind::MeshDataFallbackUsed,
                Some(object_name),
                "using fallback UV decoding from raw float2 CustomData layer",
            )?;
            return Ok(
                block.bytes()[..totloop * 8]
                    .chunks_exact(8)
                    .map(|chunk| {
                        Some([
                            f32::from_le_bytes(chunk[0..4].try_into().unwrap()),
                            1.0 - f32::from_le_bytes(chunk[4..8].try_into().unwrap()),
                        ])
                    })
                    .collect(),
            );
        }
    }
    Ok(vec![None; totloop])
}

fn load_positions_from_attributes(
    file: &BlendFile,
    mesh: &StructView<'_>,
    totvert: usize,
) -> Result<Vec<[f32; 3]>> {
    let Some(vdata) = mesh.field("vdata").and_then(|field| field.as_struct_view()) else {
        return Ok(Vec::new());
    };
    let Some(layer) = find_named_layer(file, &vdata, "position")? else {
        return Ok(Vec::new());
    };
    if let Some(data_ptr) = layer.field("data").and_then(|field| field.as_pointer()) {
        if let Some(array) = read_struct_array(file, data_ptr, "vec3f", totvert)? {
            return Ok(array
                .iter()
                .map(|view| {
                    [
                        view.field("x").and_then(|field| field.as_f32()).unwrap_or(0.0),
                        view.field("y").and_then(|field| field.as_f32()).unwrap_or(0.0),
                        view.field("z").and_then(|field| field.as_f32()).unwrap_or(0.0),
                    ]
                })
                .collect());
        }
    }
    Ok(Vec::new())
}

fn load_loops_from_attributes(
    file: &BlendFile,
    mesh: &StructView<'_>,
    totloop: usize,
) -> Result<Vec<usize>> {
    let Some(ldata) = mesh.field("ldata").and_then(|field| field.as_struct_view()) else {
        return Ok(Vec::new());
    };
    let Some(layer) = find_named_layer(file, &ldata, ".corner_vert")? else {
        return Ok(Vec::new());
    };
    let Some(data_ptr) = layer.field("data").and_then(|field| field.as_pointer()) else {
        return Ok(Vec::new());
    };
    if let Some(array) = read_struct_array(file, data_ptr, "MIntProperty", totloop)? {
        return Ok(array
            .iter()
            .map(|view| {
                view.field("i")
                    .and_then(|field| field.as_i32())
                    .unwrap_or_default()
                    .max(0) as usize
            })
            .collect());
    }

    let Some(block) = file.resolve_old_ptr(data_ptr) else {
        return Ok(Vec::new());
    };
    if block.bytes().len() >= totloop * 4 {
        return Ok(block.bytes()[..totloop * 4]
            .chunks_exact(4)
            .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()).max(0) as usize)
            .collect());
    }
    Ok(Vec::new())
}

fn load_polys_from_offsets(
    file: &BlendFile,
    mesh: &StructView<'_>,
    totpoly: usize,
) -> Result<Vec<RawPoly>> {
    let Some(offset_ptr) = mesh.field("poly_offset_indices").and_then(|field| field.as_pointer()) else {
        return Ok(Vec::new());
    };
    let Some(offset_block) = file.resolve_old_ptr(offset_ptr) else {
        return Ok(Vec::new());
    };
    let offset_count = totpoly + 1;
    if offset_block.bytes().len() < offset_count * 4 {
        return Ok(Vec::new());
    }
    let offsets = offset_block.bytes()[..offset_count * 4]
        .chunks_exact(4)
        .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()).max(0) as usize)
        .collect::<Vec<_>>();

    let material_indices = load_poly_material_indices(file, mesh, totpoly)?;
    Ok((0..totpoly)
        .map(|index| RawPoly {
            loopstart: offsets[index],
            totloop: offsets[index + 1].saturating_sub(offsets[index]),
            material_index: material_indices.get(index).copied().unwrap_or(0),
        })
        .collect())
}

fn load_poly_material_indices(
    file: &BlendFile,
    mesh: &StructView<'_>,
    totpoly: usize,
) -> Result<Vec<usize>> {
    let Some(pdata) = mesh.field("pdata").and_then(|field| field.as_struct_view()) else {
        return Ok(vec![0; totpoly]);
    };
    let Some(layer) = find_named_layer(file, &pdata, "material_index")?
        .or_else(|| find_named_layer(file, &pdata, ".material_index").ok().flatten())
    else {
        return Ok(vec![0; totpoly]);
    };
    let Some(data_ptr) = layer.field("data").and_then(|field| field.as_pointer()) else {
        return Ok(vec![0; totpoly]);
    };
    if let Some(array) = read_struct_array(file, data_ptr, "MIntProperty", totpoly)? {
        return Ok(array
            .iter()
            .map(|view| {
                view.field("i")
                    .and_then(|field| field.as_i32())
                    .unwrap_or_default()
                    .max(0) as usize
            })
            .collect());
    }
    let Some(block) = file.resolve_old_ptr(data_ptr) else {
        return Ok(vec![0; totpoly]);
    };
    if block.bytes().len() >= totpoly * 4 {
        return Ok(block.bytes()[..totpoly * 4]
            .chunks_exact(4)
            .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()).max(0) as usize)
            .collect());
    }
    Ok(vec![0; totpoly])
}

fn find_named_layer<'a>(
    file: &'a BlendFile,
    custom_data: &StructView<'a>,
    name: &str,
) -> Result<Option<StructView<'a>>> {
    let layer_count = custom_data
        .field("totlayer")
        .and_then(|field| field.as_i32())
        .unwrap_or_default()
        .max(0) as usize;
    let Some(layer_ptr) = custom_data.field("layers").and_then(|field| field.as_pointer()) else {
        return Ok(None);
    };
    let Some(layers) = read_struct_array(file, layer_ptr, "CustomDataLayer", layer_count)? else {
        return Ok(None);
    };
    for layer in layers.iter() {
        if layer
            .field("name")
            .and_then(|field| field.as_c_string())
            .as_deref()
            == Some(name)
        {
            return Ok(Some(layer));
        }
    }
    Ok(None)
}

fn triangulate_mesh(raw: RawMesh) -> ExportMesh {
    let primitive_count = raw
        .material_slots
        .len()
        .max(
            raw.polys
                .iter()
                .map(|poly| poly.material_index + 1)
                .max()
                .unwrap_or(0),
        )
        .max(1);
    let mut builders = (0..primitive_count)
        .map(|index| PrimitiveBuilder {
            material: raw.material_slots.get(index).copied().flatten(),
            positions: Vec::new(),
            normals: Vec::new(),
            texcoords0: Vec::new(),
            has_uv: false,
            indices: Vec::new(),
        })
        .collect::<Vec<_>>();

    for poly in &raw.polys {
        if poly.totloop < 3 {
            continue;
        }
        let builder_index = poly.material_index.min(builders.len() - 1);
        let builder = &mut builders[builder_index];
        let loop_range = poly.loopstart..(poly.loopstart + poly.totloop).min(raw.loops.len());
        let polygon_loops = raw.loops[loop_range].to_vec();
        if polygon_loops.len() < 3 {
            continue;
        }
        let origin = polygon_loops[0];
        for triangle in 1..polygon_loops.len() - 1 {
            let corners = [origin, polygon_loops[triangle], polygon_loops[triangle + 1]];
            let positions = corners.map(|corner| raw.vertices.get(corner.vertex).copied().unwrap_or([0.0, 0.0, 0.0]));
            let normal = face_normal(positions[0], positions[1], positions[2]);
            for (position, corner) in positions.into_iter().zip(corners.into_iter()) {
                builder.indices.push(builder.positions.len() as u32);
                builder.positions.push(position);
                builder.normals.push(normal);
                if let Some(uv) = corner.uv {
                    builder.has_uv = true;
                    builder.texcoords0.push(uv);
                } else {
                    builder.texcoords0.push([0.0, 0.0]);
                }
            }
        }
    }

    ExportMesh {
        name: raw.name,
        primitives: builders
            .into_iter()
            .filter(|builder| !builder.indices.is_empty())
            .map(|builder| ExportPrimitive {
                positions: builder.positions,
                normals: builder.normals,
                texcoords0: builder.has_uv.then_some(builder.texcoords0),
                indices: builder.indices,
                material: builder.material,
            })
            .collect(),
    }
}

#[derive(Default)]
struct PrimitiveBuilder {
    material: Option<usize>,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    texcoords0: Vec<[f32; 2]>,
    has_uv: bool,
    indices: Vec<u32>,
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [cross[0] / len, cross[1] / len, cross[2] / len]
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
