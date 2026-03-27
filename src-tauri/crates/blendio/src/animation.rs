use std::collections::HashMap;

use crate::array_view::{iter_listbase, read_struct_array};
use crate::error::Result;
use crate::gltf_export::{
    AnimationPath, ExportAnimation, ExportAnimationChannel, ExportScene, KeyframeValue,
    euler_to_quaternion, normalize_quaternion, quaternion_from_axis_angle,
};
use crate::report::{ExportOptions, ExportReport, ExportWarningKind};
use crate::view::BlendFile;

pub fn extract_object_animations(
    file: &BlendFile,
    scene: &ExportScene,
    options: &ExportOptions,
    report: &mut ExportReport,
) -> Result<Vec<ExportAnimation>> {
    let node_map = scene
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.old_ptr, index))
        .collect::<HashMap<_, _>>();
    let mut animations = Vec::new();

    for block in file.ids() {
        if block.header().code.as_string() != "OB" {
            continue;
        }
        let Some(node_index) = node_map.get(&block.header().old_ptr).copied() else {
            continue;
        };
        let view = block.struct_view()?;
        let Some(adt_ptr) = view.field("adt").and_then(|field| field.as_pointer()) else {
            continue;
        };
        let Some(adt) = file.view_old_ptr_as_struct(adt_ptr, "AnimData")? else {
            continue;
        };
        let Some(action_ptr) = adt.field("action").and_then(|field| field.as_pointer()) else {
            continue;
        };
        let Some(action) = file.view_old_ptr_as_struct(action_ptr, "bAction")? else {
            continue;
        };
        let Some(curves) = action.field("curves").and_then(|field| field.as_struct_view()) else {
            continue;
        };

        let mut translation = HashMap::<usize, Vec<(f32, f32)>>::new();
        let mut scale = HashMap::<usize, Vec<(f32, f32)>>::new();
        let mut rotation_euler = HashMap::<usize, Vec<(f32, f32)>>::new();
        let mut rotation_quat = HashMap::<usize, Vec<(f32, f32)>>::new();
        let mut rotation_axis_angle = HashMap::<usize, Vec<(f32, f32)>>::new();

        for block in iter_listbase(file, &curves)? {
            let curve = block.view_as("FCurve")?;
            let path = curve
                .field("rna_path")
                .and_then(|field| field.as_pointer())
                .map(|ptr| file.read_c_string_at_ptr(ptr))
                .transpose()?
                .flatten()
                .unwrap_or_default();
            let array_index = curve
                .field("array_index")
                .and_then(|field| field.as_i32())
                .unwrap_or_default()
                .max(0) as usize;
            let totvert = curve
                .field("totvert")
                .and_then(|field| field.as_i32())
                .unwrap_or_default()
                .max(0) as usize;
            if totvert == 0 {
                continue;
            }
            let Some(bezt_ptr) = curve.field("bezt").and_then(|field| field.as_pointer()) else {
                continue;
            };
            let Some(keyframes) = read_struct_array(file, bezt_ptr, "BezTriple", totvert)? else {
                continue;
            };
            let samples = keyframes
                .iter()
                .filter_map(|view| {
                    let values = view.field("vec").and_then(|field| field.as_f32_array::<9>())?;
                    Some((values[3], values[4]))
                })
                .collect::<Vec<_>>();
            if samples.is_empty() {
                continue;
            }

            match path.as_str() {
                "location" => {
                    translation.insert(array_index, samples);
                }
                "scale" => {
                    scale.insert(array_index, samples);
                }
                "rotation_euler" => {
                    rotation_euler.insert(array_index, samples);
                }
                "rotation_quaternion" => {
                    rotation_quat.insert(array_index, samples);
                }
                "rotation_axis_angle" => {
                    rotation_axis_angle.insert(array_index, samples);
                }
                _ => {
                    if path.starts_with("rotation")
                        || path == "location"
                        || path == "scale"
                        || path.contains("location")
                        || path.contains("rotation")
                    {
                        report.warn(
                            options,
                            ExportWarningKind::UnsupportedAnimation,
                            Some(&scene.nodes[node_index].name),
                            format!("animation path {path} is not exported"),
                        )?;
                    }
                }
            }
        }

        let mut channels = Vec::new();
        if let Some(channel) = build_vec3_channel(
            node_index,
            AnimationPath::Translation,
            &translation,
            scene.nodes[node_index].translation,
        ) {
            channels.push(channel);
        }
        if let Some(channel) = build_vec3_channel(
            node_index,
            AnimationPath::Scale,
            &scale,
            scene.nodes[node_index].scale,
        ) {
            channels.push(channel);
        }
        if let Some(channel) = build_rotation_channel(
            &scene.nodes[node_index],
            node_index,
            &rotation_euler,
            &rotation_quat,
            &rotation_axis_angle,
            view.field("rotmode").and_then(|field| field.as_i16()).unwrap_or(1),
        ) {
            channels.push(channel);
        }

        if !channels.is_empty() {
            animations.push(ExportAnimation {
                name: scene.nodes[node_index].name.clone(),
                channels,
            });
        }
    }

    Ok(animations)
}

fn build_vec3_channel(
    node_index: usize,
    path: AnimationPath,
    curves: &HashMap<usize, Vec<(f32, f32)>>,
    default: [f32; 3],
) -> Option<ExportAnimationChannel> {
    let times = collect_times(curves);
    if times.is_empty() {
        return None;
    }
    Some(ExportAnimationChannel {
        node: node_index,
        path,
        keyframes: times
            .into_iter()
            .map(|time| KeyframeValue {
                time_seconds: time,
                values: vec![
                    sample_curve(curves.get(&0), time, default[0]),
                    sample_curve(curves.get(&1), time, default[1]),
                    sample_curve(curves.get(&2), time, default[2]),
                ],
            })
            .collect(),
    })
}

fn build_rotation_channel(
    node: &crate::gltf_export::ExportNode,
    node_index: usize,
    euler_curves: &HashMap<usize, Vec<(f32, f32)>>,
    quat_curves: &HashMap<usize, Vec<(f32, f32)>>,
    axis_angle_curves: &HashMap<usize, Vec<(f32, f32)>>,
    rotmode: i16,
) -> Option<ExportAnimationChannel> {
    let curves = if !quat_curves.is_empty() {
        RotationCurves::Quaternion(quat_curves)
    } else if !euler_curves.is_empty() {
        RotationCurves::Euler(euler_curves, rotmode)
    } else if !axis_angle_curves.is_empty() {
        RotationCurves::AxisAngle(axis_angle_curves)
    } else {
        return None;
    };
    let times = match curves {
        RotationCurves::Quaternion(curves) => collect_times(curves),
        RotationCurves::Euler(curves, _) => collect_times(curves),
        RotationCurves::AxisAngle(curves) => collect_times(curves),
    };
    if times.is_empty() {
        return None;
    }

    Some(ExportAnimationChannel {
        node: node_index,
        path: AnimationPath::Rotation,
        keyframes: times
            .into_iter()
            .map(|time| {
                let quat = match curves {
                    RotationCurves::Quaternion(curves) => normalize_quaternion([
                        sample_curve(curves.get(&1), time, node.rotation[0]),
                        sample_curve(curves.get(&2), time, node.rotation[1]),
                        sample_curve(curves.get(&3), time, node.rotation[2]),
                        sample_curve(curves.get(&0), time, node.rotation[3]),
                    ]),
                    RotationCurves::Euler(curves, rotmode) => euler_to_quaternion(
                        rotmode,
                        [
                            sample_curve(curves.get(&0), time, 0.0),
                            sample_curve(curves.get(&1), time, 0.0),
                            sample_curve(curves.get(&2), time, 0.0),
                        ],
                    ),
                    RotationCurves::AxisAngle(curves) => quaternion_from_axis_angle(
                        [
                            sample_curve(curves.get(&1), time, 0.0),
                            sample_curve(curves.get(&2), time, 0.0),
                            sample_curve(curves.get(&3), time, 1.0),
                        ],
                        sample_curve(curves.get(&0), time, 0.0),
                    ),
                };
                KeyframeValue {
                    time_seconds: time,
                    values: quat.to_vec(),
                }
            })
            .collect(),
    })
}

enum RotationCurves<'a> {
    Quaternion(&'a HashMap<usize, Vec<(f32, f32)>>),
    Euler(&'a HashMap<usize, Vec<(f32, f32)>>, i16),
    AxisAngle(&'a HashMap<usize, Vec<(f32, f32)>>),
}

fn collect_times(curves: &HashMap<usize, Vec<(f32, f32)>>) -> Vec<f32> {
    let mut times = curves
        .values()
        .flat_map(|curve| curve.iter().map(|(time, _)| *time))
        .collect::<Vec<_>>();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() <= f32::EPSILON);
    times
}

fn sample_curve(curve: Option<&Vec<(f32, f32)>>, time: f32, default: f32) -> f32 {
    let Some(curve) = curve else {
        return default;
    };
    if curve.is_empty() {
        return default;
    }
    if time <= curve[0].0 {
        return curve[0].1;
    }
    if time >= curve[curve.len() - 1].0 {
        return curve[curve.len() - 1].1;
    }
    for pair in curve.windows(2) {
        let (t0, v0) = pair[0];
        let (t1, v1) = pair[1];
        if time >= t0 && time <= t1 {
            let span = (t1 - t0).max(f32::EPSILON);
            let factor = (time - t0) / span;
            return v0 + (v1 - v0) * factor;
        }
    }
    default
}
