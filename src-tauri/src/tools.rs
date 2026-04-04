use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;

use crate::process_utils::std_command;
use crate::python::resolve_blender_path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolPathsInput {
    pub ffprobe: Option<String>,
    pub blender: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub label: String,
    pub configured_path: Option<String>,
    pub detected_path: Option<String>,
    pub resolved_path: Option<String>,
    pub source: String,
    pub status: String,
    pub version: Option<String>,
    pub message: Option<String>,
}

#[tauri::command]
pub async fn inspect_tool_paths(
    tool_paths: Option<ToolPathsInput>,
) -> Result<Vec<ToolStatus>, String> {
    Ok(vec![
        build_ffprobe_status(
            tool_paths
                .as_ref()
                .and_then(|paths| paths.ffprobe.as_deref()),
        ),
        build_blender_status(
            tool_paths
                .as_ref()
                .and_then(|paths| paths.blender.as_deref()),
        ),
    ])
}

pub fn resolve_ffprobe_path(configured_path: Option<&str>) -> Option<String> {
    configured_path
        .and_then(validate_tool_path)
        .or_else(detect_ffprobe_on_path)
}

static DETECTED_FFPROBE_PATH: OnceLock<Option<String>> = OnceLock::new();

fn build_ffprobe_status(configured_path: Option<&str>) -> ToolStatus {
    let configured_path = configured_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    let configured_valid = configured_path.as_deref().and_then(validate_tool_path);
    let detected_path = detect_ffprobe_on_path();
    let resolved_path = configured_valid.clone().or_else(|| detected_path.clone());

    let source = if configured_valid.is_some() {
        "configured"
    } else if detected_path.is_some() {
        "system"
    } else {
        "missing"
    };

    let message = if configured_path.is_some() && configured_valid.is_none() {
        Some("已配置的 ffprobe 路径无效，当前已回退到自动探测结果或基础信息模式".to_string())
    } else if resolved_path.is_none() {
        Some("未检测到 ffprobe，视频高级信息将只显示基础属性。你可以在设置中手动指定路径，或点击打开下载页。".to_string())
    } else {
        Some(match source {
            "configured" => "正在使用你指定的 ffprobe 路径".to_string(),
            _ => "正在使用系统环境中的 ffprobe".to_string(),
        })
    };

    ToolStatus {
        id: "ffprobe".to_string(),
        label: "FFprobe".to_string(),
        configured_path,
        detected_path,
        resolved_path: resolved_path.clone(),
        source: source.to_string(),
        status: if resolved_path.is_some() {
            "ready".to_string()
        } else {
            "missing".to_string()
        },
        version: resolved_path
            .as_deref()
            .and_then(|path| read_tool_version(path, "-version")),
        message,
    }
}

fn build_blender_status(configured_path: Option<&str>) -> ToolStatus {
    let configured_path = configured_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    let configured_valid = configured_path.as_deref().and_then(validate_tool_path);
    let detected_path = resolve_blender_path(None);
    let resolved_path = configured_valid.clone().or_else(|| detected_path.clone());

    let source = if configured_valid.is_some() {
        "configured"
    } else if detected_path.is_some() {
        "system"
    } else {
        "missing"
    };

    let message = if configured_path.is_some() && configured_valid.is_none() {
        Some("已配置的 Blender 路径无效；.blend 仍会优先使用内置 BlendIO，必要时再回退到自动探测到的 Blender。".to_string())
    } else if resolved_path.is_none() {
        Some("未检测到 Blender；.blend 文件会优先使用内置 BlendIO 解析，但遇到不兼容文件时将无法使用 Blender 兼容回退。".to_string())
    } else {
        Some(match source {
            "configured" => "正在使用你指定的 Blender 作为兼容回退".to_string(),
            _ => "正在使用自动探测到的 Blender 作为兼容回退".to_string(),
        })
    };

    ToolStatus {
        id: "blender".to_string(),
        label: "Blender 兼容回退".to_string(),
        configured_path,
        detected_path,
        resolved_path: resolved_path.clone(),
        source: source.to_string(),
        status: if resolved_path.is_some() {
            "ready".to_string()
        } else {
            "missing".to_string()
        },
        version: resolved_path
            .as_deref()
            .and_then(|path| read_tool_version(path, "--version")),
        message,
    }
}

fn validate_tool_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || !Path::new(trimmed).exists() {
        return None;
    }
    Some(trimmed.to_string())
}

fn detect_ffprobe_on_path() -> Option<String> {
    DETECTED_FFPROBE_PATH
        .get_or_init(detect_ffprobe_on_path_uncached)
        .clone()
}

fn detect_ffprobe_on_path_uncached() -> Option<String> {
    #[cfg(target_os = "windows")]
    let lookup_cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let lookup_cmd = "which";

    let output = std_command(lookup_cmd).arg("ffprobe").output().ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && Path::new(line).exists())
        .map(str::to_string)
}

fn read_tool_version(path: &str, version_arg: &str) -> Option<String> {
    let output = std_command(path).arg(version_arg).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}
