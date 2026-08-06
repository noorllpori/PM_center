use super::{ComponentRuntimeError, ComponentRuntimeErrorCode, ComponentRuntimeManager};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
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
    #[serde(default)]
    pub preferred_handler_id: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAssociationBinding {
    pub id: String,
    pub scope: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub intent: FileIntent,
    pub handler: String,
    #[serde(default)]
    pub behavior: String,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRoutingSnapshot {
    pub bindings: Vec<FileAssociationBinding>,
    pub storage_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFileAssociationBindingRequest {
    pub binding: FileAssociationBinding,
}

#[derive(Debug)]
pub struct FileRoutingStore {
    path: PathBuf,
    bindings: Mutex<Vec<FileAssociationBinding>>,
}

impl FileRoutingStore {
    pub fn new(app_data_dir: &std::path::Path) -> Self {
        let path = app_data_dir.join("file-routing").join("global-bindings.json");
        let bindings = fs::read_to_string(&path)
            .ok()
            .and_then(|value| serde_json::from_str::<Vec<FileAssociationBinding>>(&value).ok())
            .unwrap_or_default();
        Self { path, bindings: Mutex::new(bindings) }
    }

    pub fn snapshot(&self) -> FileRoutingSnapshot {
        FileRoutingSnapshot {
            bindings: self.bindings.lock().expect("file routing mutex poisoned").clone(),
            storage_path: self.path.to_string_lossy().into_owned(),
        }
    }

    pub fn set(&self, binding: FileAssociationBinding) -> Result<(), ComponentRuntimeError> {
        let mut bindings = self.bindings.lock().expect("file routing mutex poisoned");
        bindings.retain(|item| item.id != binding.id);
        bindings.push(binding);
        bindings.sort_by(|left, right| left.id.cmp(&right.id));
        persist_bindings(&self.path, &bindings)
    }

    pub fn remove(&self, binding_id: &str) -> Result<(), ComponentRuntimeError> {
        let mut bindings = self.bindings.lock().expect("file routing mutex poisoned");
        bindings.retain(|item| item.id != binding_id);
        persist_bindings(&self.path, &bindings)
    }

    fn find(&self, extension: &str, mime: Option<&str>, intent: FileIntent, request: &FileRouteRequest) -> Option<FileAssociationBinding> {
        let bindings = self.bindings.lock().expect("file routing mutex poisoned");
        bindings.iter().rev().find(|binding| {
            binding.intent == intent
                && binding.scope_matches(request)
                && binding.extension.as_deref().map(|value| value.eq_ignore_ascii_case(extension)).unwrap_or(true)
                && binding.mime_type.as_deref().map(|value| mime == Some(value)).unwrap_or(true)
        }).cloned()
    }
}

impl FileAssociationBinding {
    fn scope_matches(&self, request: &FileRouteRequest) -> bool {
        match self.scope.as_str() {
            "project" => self.project_path.as_deref() == request.project_path.as_deref(),
            "profile" => self.profile_id.as_deref() == request.profile_id.as_deref(),
            _ => true,
        }
    }
}

fn persist_bindings(path: &PathBuf, bindings: &[FileAssociationBinding]) -> Result<(), ComponentRuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ComponentRuntimeError::new(ComponentRuntimeErrorCode::ComponentIoError, format!("创建文件绑定目录失败: {error}")))?;
    }
    let value = serde_json::to_vec_pretty(bindings).map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentIoError,
            format!("序列化文件绑定失败: {error}"),
        )
    })?;
    fs::write(path, value).map_err(|error| ComponentRuntimeError::new(ComponentRuntimeErrorCode::ComponentIoError, format!("保存文件绑定失败: {error}")))
}

pub fn route_file(
    manager: &ComponentRuntimeManager,
    routing: &FileRoutingStore,
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
    let is_directory = file_path.is_dir();
    if !is_directory && !file_path.is_file() {
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
    let mime = if is_directory {
        Some("inode/directory".to_string())
    } else {
        mime_guess::from_path(file_path)
            .first_raw()
            .map(str::to_ascii_lowercase)
    };
    let intent = request.intent;
    let mut diagnostics = Vec::new();
    let binding = request
        .preferred_handler_id
        .clone()
        .map(|handler| FileAssociationBinding {
            id: "request-preference".into(),
            scope: "global".into(),
            extension: Some(extension.clone()),
            mime_type: mime.clone(),
            intent,
            handler,
            behavior: "fallback".into(),
            project_path: None,
            profile_id: None,
        })
        .or_else(|| routing.find(&extension, mime.as_deref(), intent, &request));

    let manifests = manager.manifests();
    let installed_ids = manifests
        .iter()
        .map(|manifest| manifest.id.clone())
        .collect::<BTreeSet<_>>();
    let mut candidates = manifests
        .into_iter()
        .flat_map(|manifest| {
            let extension = extension.clone();
            let mime = mime.clone();
            let dependencies_available = manifest
                .requires_components
                .iter()
                .all(|dependency| installed_ids.contains(&dependency.id));
            manifest
                .contributes
                .file_handlers
                .clone()
                .into_iter()
                .filter_map(move |handler| {
                    let intent_matches = handler.intents.iter().any(|value| value == intent.as_str());
                    let kind_matches = handler.file_kinds.is_empty()
                        || handler.file_kinds.iter().any(|kind| {
                            (is_directory && kind.eq_ignore_ascii_case("directory"))
                                || (!is_directory && kind.eq_ignore_ascii_case("file"))
                        });
                    let extension_matches = handler.extensions.is_empty()
                        || handler
                            .extensions
                            .iter()
                            .any(|value| value.eq_ignore_ascii_case(&extension));
                    let mime_matches = handler.mime_types.is_empty()
                        || mime.as_deref().map(|value| {
                            handler.mime_types.iter().any(|item| {
                                if let Some(prefix) = item.strip_suffix("/*") {
                                    value.starts_with(&format!("{prefix}/"))
                                } else {
                                    item.eq_ignore_ascii_case(value)
                                }
                            })
                        }).unwrap_or(false);
                    if dependencies_available
                        && kind_matches
                        && intent_matches
                        && (is_directory || extension_matches || mime_matches)
                    {
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

    if let Some(binding) = binding.as_ref() {
        if binding.handler == "system" {
            if intent.allows_system_fallback() && binding.behavior != "strict" {
                diagnostics.push(FileRouteDiagnostic {
                    code: "BOUND_TO_SYSTEM".into(),
                    message: "按文件绑定交给系统默认程序".into(),
                });
                return Ok(system_fallback_plan(path, intent, diagnostics));
            }
            diagnostics.push(FileRouteDiagnostic {
                code: "STRICT_SYSTEM_NOT_ALLOWED".into(),
                message: "该意图不允许使用系统程序作为成功结果".into(),
            });
            return Ok(no_handler_plan(path, intent, diagnostics));
        }
        if binding.behavior == "strict"
            && !candidates.iter().any(|(_, handler)| handler.id == binding.handler)
        {
            diagnostics.push(FileRouteDiagnostic {
                code: "BOUND_HANDLER_MISSING".into(),
                message: format!("绑定的处理器不可用: {}", binding.handler),
            });
            return Ok(no_handler_plan(path, intent, diagnostics));
        }
    }
    let selected = binding.as_ref().and_then(|binding| {
        candidates.iter().find(|(_, handler)| handler.id == binding.handler).cloned()
    });
    let selected = selected.or_else(|| candidates.into_iter().next());
    if let Some((manifest, handler)) = selected {
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

    if intent.allows_system_fallback() {
        diagnostics.push(FileRouteDiagnostic {
            code: "SYSTEM_FALLBACK".into(),
            message: "没有已启用的内部文件处理器，将交给系统默认程序".into(),
        });
        return Ok(system_fallback_plan(path, intent, diagnostics));
    }

    diagnostics.push(FileRouteDiagnostic {
        code: "NO_HANDLER".into(),
        message: "没有能够处理该文件意图的已启用组件".into(),
    });
    Ok(no_handler_plan(path, intent, diagnostics))
}

fn system_fallback_plan(path: &str, intent: FileIntent, diagnostics: Vec<FileRouteDiagnostic>) -> FileRoutePlan {
    FileRoutePlan {
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
    }
}

fn no_handler_plan(path: &str, intent: FileIntent, diagnostics: Vec<FileRouteDiagnostic>) -> FileRoutePlan {
    FileRoutePlan {
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
    }
}
