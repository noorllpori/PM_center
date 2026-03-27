use serde_json;
use tokio::fs;
use uuid::Uuid;

use super::{run_python_file, EnvType};

const JSON_START_MARKER: &str = "__PM_CENTER_BLEND_INFO_START__";
const JSON_END_MARKER: &str = "__PM_CENTER_BLEND_INFO_END__";

// 获取 Blender 文件信息（专用命令）
#[tauri::command]
pub async fn get_blender_file_info(
    blender_path: String,
    blend_file: String,
) -> Result<serde_json::Value, String> {
    let temp_script_path = std::env::temp_dir()
        .join(format!("pm_center_blender_info_{}.py", Uuid::new_v4()));
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

print("{start}")
print(json.dumps(info, ensure_ascii=False))
print("{end}")
"#,
        blend_file.replace("\\", "\\\\").replace("\"", "\\\""),
        start = JSON_START_MARKER,
        end = JSON_END_MARKER,
    );

    fs::write(&temp_script_path, script)
        .await
        .map_err(|e| format!("Failed to write temporary Blender script: {}", e))?;

    let result = run_python_file(
        EnvType::Blender,
        blender_path,
        temp_script_path.to_string_lossy().to_string(),
        vec![],
        None,
    ).await;

    let _ = fs::remove_file(&temp_script_path).await;
    let result = result?;

    if result.success {
        let start = result.stdout.find(JSON_START_MARKER)
            .ok_or("No JSON start marker")?;
        let end = result.stdout.find(JSON_END_MARKER)
            .ok_or("No JSON end marker")?;

        if end <= start {
            return Err("Invalid JSON marker order".to_string());
        }

        let json_str = result.stdout[start + JSON_START_MARKER.len()..end].trim();
        
        serde_json::from_str(json_str)
            .map_err(|e| format!("JSON parse error: {}", e))
    } else {
        let err = result.stderr.clone();
        Err(format!("Blender error: {}", err))
    }
}
