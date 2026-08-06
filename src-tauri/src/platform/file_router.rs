use super::{ComponentRuntimeError, ComponentRuntimeErrorCode, ComponentRuntimeManager};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileIntent {
    Open,
    OpenInternal,
    OpenSystem,
    Preview,
    Edit,
    Inspect,
    Thumbnail,
    ExtractMetadata,
}

impl FileIntent {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::OpenInternal => "open-internal",
            Self::OpenSystem => "open-system",
            Self::Preview => "preview",
            Self::Edit => "edit",
            Self::Inspect => "inspect",
            Self::Thumbnail => "thumbnail",
            Self::ExtractMetadata => "extract-metadata",
        }
    }

    fn allows_system_fallback(self) -> bool {
        matches!(self, Self::Open | Self::OpenSystem)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRouteRequest {
    pub path: String,
    #[serde(default = "default_file_intent")]
    pub intent: FileIntent,
}

fn default_file_intent() -> FileIntent {
    FileIntent::Open
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRoutePlan {
    pub route_id: String,
    pub path: String,
    pub intent: FileIntent,
    pub accepted: bool,
    pub fallback_to_system: bool,
    pub handler_id: Option<String>,
    pub component_id: Option<String>,
    pub handler_name: Option<String>,
    pub target: String,
    pub workspace_target: Option<String>,
    pub diagnostics: Vec<FileRouteDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRouteDiagnostic {
    pub code: String,
    pub message: String,
}

pub fn route_file(
    manager: &ComponentRuntimeManager,
    request: FileRouteRequest,
) -> Result<FileRoutePlan, ComponentRuntimeError> {
    let path = request.path.trim();
    if path.is_empty() {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "文件路由缺少路径",
        ));
    }
    let file_path = Path::new(path);
    if !file_path.is_file() {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("文件不存在或不可读取: {path}"),
        ));
    }

    let extension = file_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let mime = mime_guess::from_path(file_path)
        .first_raw()
        .map(str::to_ascii_lowercase);
    let intent = request.intent;
    let mut diagnostics = Vec::new();

    let mut candidates = manager
        .manifests()
        .into_iter()
        .flat_map(|manifest| {
            let extension = extension.clone();
            let mime = mime.clone();
            manifest
                .contributes
                .file_handlers
                .clone()
                .into_iter()
                .filter_map(move |handler| {
                    let intent_matches = handler.intents.iter().any(|value| value == intent.as_str());
                    let extension_matches = handler.extensions.is_empty()
                        || handler
                            .extensions
                            .iter()
                            .any(|value| value.eq_ignore_ascii_case(&extension));
                    let mime_matches = handler.mime_types.is_empty()
                        || mime
                            .as_deref()
                            .map(|value| handler.mime_types.iter().any(|item| item.eq_ignore_ascii_case(value)))
                            .unwrap_or(false);
                    if intent_matches && (extension_matches || mime_matches) {
                        Some((manifest.clone(), handler))
                    } else {
                        None
                    }
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .1
            .priority
            .cmp(&left.1.priority)
            .then_with(|| left.0.id.cmp(&right.0.id))
            .then_with(|| left.1.id.cmp(&right.1.id))
    });

    if let Some((manifest, handler)) = candidates.into_iter().next() {
        return Ok(FileRoutePlan {
            route_id: Uuid::new_v4().to_string(),
            path: path.to_string(),
            intent,
            accepted: true,
            fallback_to_system: false,
            handler_id: Some(handler.id),
            component_id: Some(manifest.id),
            handler_name: Some(handler.name),
            target: if handler.workspace_target.is_some() { "workspace" } else { "component" }.into(),
            workspace_target: handler.workspace_target,
            diagnostics,
        });
    }

    // Core file-manager adapters retain the existing image/text/video tabs.
    let core_target = match extension.as_str() {
        "blend" => None,
        _ if is_image(&extension) => Some(("builtin.file-manager.image", "image", "图像预览")),
        _ if is_video(&extension) => Some(("builtin.file-manager.video", "video", "视频播放器")),
        _ if is_text(&extension) => Some(("builtin.file-manager.text", "text", "文本编辑器")),
        _ => None,
    };
    if let Some((handler_id, target, name)) = core_target {
        return Ok(FileRoutePlan {
            route_id: Uuid::new_v4().to_string(),
            path: path.to_string(),
            intent,
            accepted: true,
            fallback_to_system: false,
            handler_id: Some(handler_id.into()),
            component_id: None,
            handler_name: Some(name.into()),
            target: "workspace".into(),
            workspace_target: Some(target.into()),
            diagnostics,
        });
    }

    if intent.allows_system_fallback() {
        diagnostics.push(FileRouteDiagnostic {
            code: "SYSTEM_FALLBACK".into(),
            message: "没有已启用的内部文件处理器，将交给系统默认程序".into(),
        });
        return Ok(FileRoutePlan {
            route_id: Uuid::new_v4().to_string(),
            path: path.to_string(),
            intent,
            accepted: true,
            fallback_to_system: true,
            handler_id: None,
            component_id: None,
            handler_name: Some("系统默认程序".into()),
            target: "system".into(),
            workspace_target: None,
            diagnostics,
        });
    }

    diagnostics.push(FileRouteDiagnostic {
        code: "NO_HANDLER".into(),
        message: "没有能够处理该文件意图的已启用组件".into(),
    });
    Ok(FileRoutePlan {
        route_id: Uuid::new_v4().to_string(),
        path: path.to_string(),
        intent,
        accepted: false,
        fallback_to_system: false,
        handler_id: None,
        component_id: None,
        handler_name: None,
        target: "none".into(),
        workspace_target: None,
        diagnostics,
    })
}

fn is_image(extension: &str) -> bool {
    matches!(extension, "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "svg" | "ico" | "avif")
}

fn is_video(extension: &str) -> bool {
    matches!(extension, "mp4" | "m4v" | "mov" | "avi" | "mkv" | "webm" | "wmv" | "flv" | "mpeg" | "mpg" | "m2ts")
}

fn is_text(extension: &str) -> bool {
    matches!(extension, "txt" | "md" | "markdown" | "mdx" | "json" | "jsonc" | "js" | "ts" | "tsx" | "jsx" | "html" | "htm" | "css" | "scss" | "py" | "rs" | "c" | "h" | "cpp" | "hpp" | "java" | "go" | "php" | "rb" | "xml" | "yml" | "yaml" | "toml" | "ini" | "conf" | "log" | "csv" | "tsv")
}
