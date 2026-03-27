use crate::array_view::iter_listbase;
use crate::error::Result;
use crate::mesh::{RawLoop, RawMesh, RawPoly};
use crate::report::{ExportOptions, ExportReport, ExportWarningKind};
use crate::view::{BlendFile, StructView};

const MODIFIER_MIRROR: i32 = 5;
const MODIFIER_ARRAY: i32 = 12;
const MODIFIER_TRIANGULATE: i32 = 44;

const MOD_ARR_FIXEDCOUNT: i32 = 0;
const MOD_ARR_OFF_CONST: i32 = 1 << 0;
const MOD_ARR_OFF_RELATIVE: i32 = 1 << 1;
const MOD_ARR_OFF_OBJ: i32 = 1 << 2;
const MOD_ARR_MERGE: i32 = 1 << 0;
const MOD_ARR_MERGEFINAL: i32 = 1 << 1;

const MOD_MIR_AXIS_X: i16 = 1 << 3;
const MOD_MIR_AXIS_Y: i16 = 1 << 4;
const MOD_MIR_AXIS_Z: i16 = 1 << 5;
const MOD_MIR_NO_MERGE: i16 = 1 << 7;

pub(crate) fn apply_supported_modifiers(
    file: &BlendFile,
    object: &StructView<'_>,
    object_name: &str,
    raw: RawMesh,
    options: &ExportOptions,
    report: &mut ExportReport,
) -> Result<RawMesh> {
    let Some(modifiers) = object.field("modifiers").and_then(|field| field.as_struct_view()) else {
        return Ok(raw);
    };
    let mut current = raw;
    for block in iter_listbase(file, &modifiers)? {
        let base = block.view_as("ModifierData")?;
        let modifier_type = base.field("type").and_then(|field| field.as_i32()).unwrap_or_default();
        match modifier_type {
            MODIFIER_MIRROR => {
                let view = block.view_as("MirrorModifierData")?;
                current = apply_mirror_modifier(&view, current, report, options, object_name)?;
            }
            MODIFIER_ARRAY => {
                let view = block.view_as("ArrayModifierData")?;
                current = apply_array_modifier(&view, current, report, options, object_name)?;
            }
            MODIFIER_TRIANGULATE => {}
            other => {
                report.warn(
                    options,
                    ExportWarningKind::UnsupportedModifier,
                    Some(object_name),
                    format!(
                        "modifier {} is not evaluated and will be skipped",
                        base.field("name")
                            .and_then(|field| field.as_c_string())
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| other.to_string())
                    ),
                )?;
                report.add_unsupported_feature(format!("modifier:{object_name}:{other}"));
            }
        }
    }
    Ok(current)
}

fn apply_mirror_modifier(
    modifier: &StructView<'_>,
    mut raw: RawMesh,
    report: &mut ExportReport,
    options: &ExportOptions,
    object_name: &str,
) -> Result<RawMesh> {
    if modifier
        .field("mirror_ob")
        .and_then(|field| field.as_pointer())
        .unwrap_or(0)
        != 0
    {
        report.warn(
            options,
            ExportWarningKind::UnsupportedModifier,
            Some(object_name),
            "mirror modifier with mirror object is approximated around local origin",
        )?;
    }

    let flag = modifier.field("flag").and_then(|field| field.as_i16()).unwrap_or_default();
    let legacy_axis = modifier.field("axis").and_then(|field| field.as_i16()).unwrap_or_default();
    let axis_flags = if flag & (MOD_MIR_AXIS_X | MOD_MIR_AXIS_Y | MOD_MIR_AXIS_Z) != 0 {
        flag
    } else {
        legacy_axis << 3
    };
    if flag & !MOD_MIR_NO_MERGE == 0 {
        return Ok(raw);
    }
    if flag & MOD_MIR_NO_MERGE == 0 {
        report.warn(
            options,
            ExportWarningKind::UnsupportedModifier,
            Some(object_name),
            "mirror merge behaviour is not replicated exactly in this first version",
        )?;
    }

    for axis in [
        (MOD_MIR_AXIS_X, 0usize),
        (MOD_MIR_AXIS_Y, 1usize),
        (MOD_MIR_AXIS_Z, 2usize),
    ] {
        if axis_flags & axis.0 != 0 {
            raw = mirror_axis(raw, axis.1);
        }
    }
    Ok(raw)
}

fn apply_array_modifier(
    modifier: &StructView<'_>,
    raw: RawMesh,
    report: &mut ExportReport,
    options: &ExportOptions,
    object_name: &str,
) -> Result<RawMesh> {
    let fit_type = modifier
        .field("fit_type")
        .and_then(|field| field.as_i32())
        .unwrap_or(MOD_ARR_FIXEDCOUNT);
    if fit_type != MOD_ARR_FIXEDCOUNT {
        report.warn(
            options,
            ExportWarningKind::UnsupportedModifier,
            Some(object_name),
            "array modifier fit mode is not supported; using count directly",
        )?;
    }
    let count = modifier
        .field("count")
        .and_then(|field| field.as_i32())
        .unwrap_or(1)
        .max(1) as usize;
    if count <= 1 {
        return Ok(raw);
    }

    let offset_type = modifier
        .field("offset_type")
        .and_then(|field| field.as_i32())
        .unwrap_or(MOD_ARR_OFF_RELATIVE);
    let const_offset = modifier
        .field("offset")
        .and_then(|field| field.as_f32_array::<3>())
        .unwrap_or([0.0, 0.0, 0.0]);
    let relative_offset = modifier
        .field("scale")
        .and_then(|field| field.as_f32_array::<3>())
        .unwrap_or([0.0, 0.0, 0.0]);
    let bounds = bounds_size(&raw);
    let mut step = [0.0, 0.0, 0.0];
    if offset_type & MOD_ARR_OFF_CONST != 0 {
        for index in 0..3 {
            step[index] += const_offset[index];
        }
    }
    if offset_type & MOD_ARR_OFF_RELATIVE != 0 {
        for index in 0..3 {
            step[index] += relative_offset[index] * bounds[index];
        }
    }
    if offset_type & MOD_ARR_OFF_OBJ != 0 {
        report.warn(
            options,
            ExportWarningKind::UnsupportedModifier,
            Some(object_name),
            "array object offset is not supported in this first version",
        )?;
    }

    let flags = modifier.field("flags").and_then(|field| field.as_i32()).unwrap_or_default();
    if flags & (MOD_ARR_MERGE | MOD_ARR_MERGEFINAL) != 0 {
        report.warn(
            options,
            ExportWarningKind::UnsupportedModifier,
            Some(object_name),
            "array merge behaviour is not replicated exactly in this first version",
        )?;
    }

    Ok(array_repeat(raw, count, step))
}

fn mirror_axis(mut raw: RawMesh, axis: usize) -> RawMesh {
    let vertex_offset = raw.vertices.len();
    let loop_offset = raw.loops.len();
    let source_vertices = raw.vertices.clone();
    let source_loops = raw.loops.clone();
    let source_polys = raw.polys.clone();

    for mut vertex in source_vertices.iter().copied() {
        vertex[axis] = -vertex[axis];
        raw.vertices.push(vertex);
    }

    for poly in source_polys {
        let loop_indices = (poly.loopstart..poly.loopstart + poly.totloop)
            .filter_map(|index| source_loops.get(index).copied())
            .collect::<Vec<_>>();
        let new_loopstart = raw.loops.len();
        for loop_data in loop_indices.into_iter().rev() {
            raw.loops.push(RawLoop {
                vertex: loop_data.vertex + vertex_offset,
                uv: loop_data.uv,
            });
        }
        raw.polys.push(RawPoly {
            loopstart: new_loopstart,
            totloop: poly.totloop,
            material_index: poly.material_index,
        });
    }

    debug_assert_eq!(loop_offset + source_loops.len(), raw.loops.len());
    raw
}

fn array_repeat(raw: RawMesh, count: usize, step: [f32; 3]) -> RawMesh {
    let mut result = RawMesh {
        name: raw.name.clone(),
        vertices: Vec::new(),
        loops: Vec::new(),
        polys: Vec::new(),
        material_slots: raw.material_slots.clone(),
    };

    for instance in 0..count {
        let vertex_offset = result.vertices.len();
        let loop_offset = result.loops.len();
        let translation = [
            step[0] * instance as f32,
            step[1] * instance as f32,
            step[2] * instance as f32,
        ];
        for mut vertex in raw.vertices.iter().copied() {
            vertex[0] += translation[0];
            vertex[1] += translation[1];
            vertex[2] += translation[2];
            result.vertices.push(vertex);
        }
        for loop_data in &raw.loops {
            result.loops.push(RawLoop {
                vertex: loop_data.vertex + vertex_offset,
                uv: loop_data.uv,
            });
        }
        for poly in &raw.polys {
            result.polys.push(RawPoly {
                loopstart: poly.loopstart + loop_offset,
                totloop: poly.totloop,
                material_index: poly.material_index,
            });
        }
    }

    result
}

fn bounds_size(raw: &RawMesh) -> [f32; 3] {
    if raw.vertices.is_empty() {
        return [1.0, 1.0, 1.0];
    }
    let mut min = raw.vertices[0];
    let mut max = raw.vertices[0];
    for vertex in &raw.vertices[1..] {
        for index in 0..3 {
            min[index] = min[index].min(vertex[index]);
            max[index] = max[index].max(vertex[index]);
        }
    }
    [
        (max[0] - min[0]).max(1.0),
        (max[1] - min[1]).max(1.0),
        (max[2] - min[2]).max(1.0),
    ]
}
