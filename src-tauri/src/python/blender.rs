use serde_json;
use super::{run_python_script, EnvType};

// 获取 Blender 文件信息（专用命令）
#[tauri::command]
pub async fn get_blender_file_info(
    blender_path: String,
    blend_file: String,
) -> Result<serde_json::Value, String> {
    let script = format!(
        r#"import bpy
import json

bpy.ops.wm.open_mainfile(filepath=r"{}")

info = {{
    "scenes": [],
    "cameras": [],
    "objects": [],
    "materials": [],
    "frame_start": 1,
    "frame_end": 250,
    "resolution": [1920, 1080],
    "render_engine": "",
    "filepath": bpy.data.filepath,
    "version": list(bpy.app.version),
}}

for scene in bpy.data.scenes:
    scene_info = {{
        "name": scene.name,
        "frame_start": scene.frame_start,
        "frame_end": scene.frame_end,
        "resolution": [scene.render.resolution_x, scene.render.resolution_y],
        "fps": scene.render.fps,
    }}
    info["scenes"].append(scene_info)
    info["frame_start"] = scene.frame_start
    info["frame_end"] = scene.frame_end
    info["resolution"] = [scene.render.resolution_x, scene.render.resolution_y]
    info["render_engine"] = scene.render.engine

for cam in bpy.data.cameras:
    info["cameras"].append({{
        "name": cam.name,
        "type": cam.type,
    }})

info["objects"].append({{"count": len(bpy.data.objects)}})
info["materials"].append({{"count": len(bpy.data.materials)}})

print(json.dumps(info, ensure_ascii=False))
"#,
        blend_file.replace("\\", "\\\\").replace("\"", "\\\"")
    );

    let result = run_python_script(
        EnvType::Blender,
        blender_path,
        script,
        None,
        None,
    ).await?;

    if result.success {
        let json_start = result.stdout.find('{')
            .ok_or("No JSON output")?;
        let json_str = result.stdout[json_start..].trim();
        
        serde_json::from_str(json_str)
            .map_err(|e| format!("JSON parse error: {}", e))
    } else {
        let err = result.stderr.clone();
        Err(format!("Blender error: {}", err))
    }
}
