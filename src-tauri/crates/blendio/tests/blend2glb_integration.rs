use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gltf::animation::Property;
use image::{Rgba, RgbaImage};
use serde_json::Value;

fn blender_exe() -> PathBuf {
    std::env::var_os("BLENDER_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"D:\Blender_4.5\blender.exe"))
}

fn fresh_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "blend2glb-tests-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn python_string(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("'{}'", escaped)
}

fn make_blend(expr: &str, output: &Path) {
    let script = format!(
        "import bpy; {expr}; bpy.ops.wm.save_as_mainfile(filepath={}, compress=False)",
        python_string(output)
    );
    let output = Command::new(blender_exe())
        .arg("--background")
        .arg("--factory-startup")
        .arg("--python-expr")
        .arg(script)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "failed to generate Blender file\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_blend2glb(args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_blend2glb"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn exports_basic_scene_to_glb() {
    let root = fresh_dir("basic");
    let blend = root.join("basic.blend");
    let glb = root.join("basic.glb");
    let report = root.join("basic_report.json");

    make_blend(
        "bpy.data.objects['Cube'].location = (1.5, 0.0, 0.0)",
        &blend,
    );
    let output = run_blend2glb(&[
        blend.as_os_str().to_os_string(),
        glb.as_os_str().to_os_string(),
        OsString::from("--report-json"),
        report.as_os_str().to_os_string(),
    ]);
    assert!(
        output.status.success(),
        "blend2glb failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("GLB Export Report"));
    assert!(text.contains("Meshes: 1"));

    let report_json: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(report_json["exported_mesh_count"], 1);
    assert_eq!(report_json["exported_material_count"], 1);

    let (document, buffers, _images) = gltf::import(&glb).unwrap();
    assert_eq!(buffers.len(), 1);
    assert!(document.meshes().count() >= 1);
    let node_names = document
        .nodes()
        .filter_map(|node| node.name().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(node_names.iter().any(|name| name == "Cube"));
    assert!(node_names.iter().any(|name| name == "Camera"));
    assert!(node_names.iter().any(|name| name == "Light"));
}

#[test]
fn exports_object_translation_animation() {
    let root = fresh_dir("anim");
    let blend = root.join("anim.blend");
    let glb = root.join("anim.glb");

    make_blend(
        "cube = bpy.data.objects['Cube']; \
         cube.location = (0.0, 0.0, 0.0); \
         cube.keyframe_insert(data_path='location', frame=1); \
         cube.location = (2.0, 0.0, 0.0); \
         cube.keyframe_insert(data_path='location', frame=10)",
        &blend,
    );
    let output = run_blend2glb(&[
        blend.as_os_str().to_os_string(),
        glb.as_os_str().to_os_string(),
    ]);
    assert!(
        output.status.success(),
        "blend2glb failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let (document, _buffers, _images) = gltf::import(&glb).unwrap();
    let animations = document.animations().collect::<Vec<_>>();
    assert!(!animations.is_empty());
    assert!(animations.iter().any(|animation| {
        animation
            .channels()
            .any(|channel| channel.target().property() == Property::Translation)
    }));
}

#[test]
fn exports_packed_base_color_texture() {
    let root = fresh_dir("packed");
    let blend = root.join("packed.blend");
    let glb = root.join("packed.glb");
    let texture = root.join("packed.png");

    let mut image = RgbaImage::new(2, 2);
    image.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    image.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
    image.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
    image.put_pixel(1, 1, Rgba([255, 255, 0, 255]));
    image.save(&texture).unwrap();

    let expr = format!(
        "cube = bpy.data.objects['Cube']; \
         bpy.context.view_layer.objects.active = cube; \
         cube.select_set(True); \
         bpy.ops.object.mode_set(mode='EDIT'); \
         bpy.ops.mesh.select_all(action='SELECT'); \
         bpy.ops.uv.smart_project(); \
         bpy.ops.object.mode_set(mode='OBJECT'); \
         mat = bpy.data.materials['Material']; \
         mat.use_nodes = True; \
         nodes = mat.node_tree.nodes; \
         links = mat.node_tree.links; \
         bsdf = next(node for node in nodes if node.bl_idname == 'ShaderNodeBsdfPrincipled'); \
         tex = nodes.new(type='ShaderNodeTexImage'); \
         img = bpy.data.images.load(filepath={}); \
         img.pack(); \
         tex.image = img; \
         links.new(tex.outputs['Color'], bsdf.inputs['Base Color']); \
         cube.data.materials.clear(); \
         cube.data.materials.append(mat)",
        python_string(&texture)
    );
    make_blend(&expr, &blend);

    let output = run_blend2glb(&[
        blend.as_os_str().to_os_string(),
        glb.as_os_str().to_os_string(),
    ]);
    assert!(
        output.status.success(),
        "blend2glb failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let (document, _buffers, images) = gltf::import(&glb).unwrap();
    assert!(!images.is_empty());
    let material = document.materials().next().unwrap();
    assert!(material.pbr_metallic_roughness().base_color_texture().is_some());
}

#[test]
fn strict_mode_fails_on_unsupported_modifier() {
    let root = fresh_dir("strict");
    let blend = root.join("strict.blend");
    let glb = root.join("strict.glb");

    make_blend(
        "cube = bpy.data.objects['Cube']; \
         cube.modifiers.new(name='Subsurf', type='SUBSURF')",
        &blend,
    );

    let output = run_blend2glb(&[
        blend.as_os_str().to_os_string(),
        glb.as_os_str().to_os_string(),
        OsString::from("--strict"),
    ]);
    assert!(
        !output.status.success(),
        "blend2glb unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
