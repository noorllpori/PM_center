use base64::Engine;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let request = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(error) => {
                emit(json!({"ok": false, "error": format!("请求不是有效 JSON: {error}")}));
                continue;
            }
        };
        let response = handle(request).unwrap_or_else(|error| json!({"ok": false, "error": error}));
        emit(response);
    }
}

fn handle(request: Value) -> Result<Value, String> {
    let command = request
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    let path = input
        .get("path")
        .or_else(|| input.get("file"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| "BlenderIO 命令缺少 path/file 参数".to_string())?;
    let path = PathBuf::from(path);
    let result = match command {
        "inspect" | "read-render-settings" => {
            let file = blendio::BlendFile::open(&path).map_err(|error| error.to_string())?;
            let summary = blendio::summarize(&file).map_err(|error| error.to_string())?;
            let external = blendio::collect_external_data_with_base(&file, Some(&path))
                .map_err(|error| error.to_string())?;
            json!({ "summary": summary, "externalData": external })
        }
        "collect-external-data" => {
            let file = blendio::BlendFile::open(&path).map_err(|error| error.to_string())?;
            let external = blendio::collect_external_data_with_base(&file, Some(&path))
                .map_err(|error| error.to_string())?;
            serde_json::to_value(external).map_err(|error| error.to_string())?
        }
        "extract-preview" => {
            let preview =
                blendio::extract_preview_from_path(&path).map_err(|error| error.to_string())?;
            let Some(preview) = preview else {
                return Ok(json!({ "ok": true, "result": Value::Null }));
            };
            let image = image::RgbaImage::from_raw(preview.width, preview.height, preview.rgba)
                .ok_or_else(|| "Blender 预览像素无效".to_string())?;
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image)
                .write_to(&mut bytes, image::ImageFormat::Png)
                .map_err(|error| error.to_string())?;
            json!({
                "width": preview.width,
                "height": preview.height,
                "pngBase64": base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()),
            })
        }
        "edit-render-settings" => {
            let scene_selector = serde_json::from_value::<blendio::SceneSelector>(
                input.get("sceneSelector").cloned().unwrap_or(Value::Null),
            )
            .map_err(|error| error.to_string())?;
            let edit = serde_json::from_value::<blendio::SceneRenderEdit>(
                input.get("edit").cloned().unwrap_or(Value::Null),
            )
            .map_err(|error| error.to_string())?;
            let options = input
                .get("options")
                .cloned()
                .map(serde_json::from_value::<blendio::WriteOptions>)
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or_default();
            let mut session =
                blendio::BlendEditSession::open(&path).map_err(|error| error.to_string())?;
            session
                .edit_scene_render(scene_selector, edit)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(session.commit(options).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?
        }
        _ => return Err(format!("BlenderIO 不支持命令 {command}")),
    };
    Ok(json!({ "ok": true, "result": result }))
}

fn emit(value: Value) {
    let _ = writeln!(io::stdout(), "{value}");
    let _ = io::stdout().flush();
}
