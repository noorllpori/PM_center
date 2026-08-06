use super::{ComponentRuntimeError, ComponentRuntimeErrorCode, ComponentRuntimeManager};
use pmc_platform::{ComponentDependency, ComponentManifestV1, FileHandlerContribution};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

const MAX_ROUTE_HISTORY: usize = 200;
const PROJECT_BINDINGS_FILE: &str = "file-associations.json";

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileRoutingScopeContext {
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
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
    pub candidates: Vec<FileRouteCandidate>,
    pub binding: Option<FileRouteBindingResolution>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRouteDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRouteCandidate {
    pub handler_id: String,
    pub handler_name: String,
    pub component_id: String,
    pub component_name: String,
    pub priority: i32,
    pub workspace_target: Option<String>,
    pub eligible: bool,
    pub selected: bool,
    pub reasons: Vec<FileRouteDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRouteBindingResolution {
    pub id: String,
    pub scope: String,
    pub handler: String,
    pub behavior: String,
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
    pub storage_paths: Vec<FileRoutingStoragePath>,
    pub context: FileRoutingScopeContext,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRoutingStoragePath {
    pub scope: String,
    pub path: String,
    pub available: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFileAssociationBindingRequest {
    pub binding: FileAssociationBinding,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveFileAssociationBindingRequest {
    pub binding_id: String,
    #[serde(default)]
    pub context: FileRoutingScopeContext,
}

#[derive(Debug)]
pub struct FileRoutingStore {
    app_data_dir: PathBuf,
    global_path: PathBuf,
    global_bindings: Mutex<Vec<FileAssociationBinding>>,
    route_history: Mutex<VecDeque<FileRoutePlan>>,
}

impl FileRoutingStore {
    pub fn new(app_data_dir: &Path) -> Self {
        let global_path = app_data_dir
            .join("file-routing")
            .join("global-bindings.json");
        let global_bindings = read_bindings(&global_path).unwrap_or_default();
        Self {
            app_data_dir: app_data_dir.to_path_buf(),
            global_path,
            global_bindings: Mutex::new(global_bindings),
            route_history: Mutex::new(VecDeque::new()),
        }
    }

    pub fn snapshot(
        &self,
        context: FileRoutingScopeContext,
    ) -> Result<FileRoutingSnapshot, ComponentRuntimeError> {
        let bindings = self.bindings_for_context(&context)?;
        let storage_paths = self.storage_paths(&context)?;
        Ok(FileRoutingSnapshot {
            bindings,
            storage_path: self.global_path.to_string_lossy().into_owned(),
            storage_paths,
            context,
        })
    }

    pub fn set(&self, mut binding: FileAssociationBinding) -> Result<(), ComponentRuntimeError> {
        normalize_binding(&mut binding)?;
        let path = self.path_for_binding(&binding)?;
        if binding.scope == "global" {
            let mut bindings = self
                .global_bindings
                .lock()
                .expect("file routing mutex poisoned");
            bindings.retain(|item| item.id != binding.id);
            bindings.push(binding);
            sort_bindings(&mut bindings);
            return persist_bindings(&path, &bindings);
        }
        let mut bindings = read_bindings(&path)?;
        bindings.retain(|item| item.id != binding.id);
        bindings.push(binding);
        sort_bindings(&mut bindings);
        persist_bindings(&path, &bindings)
    }

    pub fn remove(
        &self,
        request: RemoveFileAssociationBindingRequest,
    ) -> Result<(), ComponentRuntimeError> {
        let binding_id = request.binding_id.trim();
        if binding_id.is_empty() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "文件绑定缺少 ID",
            ));
        }
        let locations = self.binding_locations(&request.context)?;
        let mut removed = false;
        for (scope, path) in locations {
            if scope == "global" {
                let mut bindings = self
                    .global_bindings
                    .lock()
                    .expect("file routing mutex poisoned");
                let original_len = bindings.len();
                bindings.retain(|item| item.id != binding_id);
                if bindings.len() != original_len {
                    persist_bindings(&path, &bindings)?;
                    removed = true;
                    break;
                }
                continue;
            }
            let mut bindings = read_bindings(&path)?;
            let original_len = bindings.len();
            bindings.retain(|item| item.id != binding_id);
            if bindings.len() != original_len {
                persist_bindings(&path, &bindings)?;
                removed = true;
                break;
            }
        }
        if !removed {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("没有找到文件绑定: {binding_id}"),
            ));
        }
        Ok(())
    }

    pub fn find(
        &self,
        extension: &str,
        mime: Option<&str>,
        intent: FileIntent,
        request: &FileRouteRequest,
    ) -> BindingLookup {
        let context = FileRoutingScopeContext {
            project_path: request.project_path.clone(),
            profile_id: request.profile_id.clone(),
        };
        let mut diagnostics = Vec::new();
        for (scope, path) in self.binding_locations(&context).unwrap_or_default() {
            let bindings = if scope == "global" {
                self.global_bindings
                    .lock()
                    .expect("file routing mutex poisoned")
                    .clone()
            } else {
                match read_bindings(&path) {
                    Ok(bindings) => bindings,
                    Err(error) => {
                        diagnostics.push(FileRouteDiagnostic {
                            code: "BINDING_STORE_INVALID".into(),
                            message: format!("{scope} 文件关联无法读取，已忽略: {}", error.message),
                        });
                        continue;
                    }
                }
            };
            if let Some(binding) = matching_binding(&bindings, extension, mime, intent) {
                diagnostics.push(FileRouteDiagnostic {
                    code: "BINDING_APPLIED".into(),
                    message: format!("已应用{}文件关联: {}", scope_label(&scope), binding.id),
                });
                return BindingLookup {
                    binding: Some(binding),
                    diagnostics,
                };
            }
        }
        BindingLookup {
            binding: None,
            diagnostics,
        }
    }

    pub fn record_plan(&self, plan: FileRoutePlan) {
        let mut history = self
            .route_history
            .lock()
            .expect("file route history mutex poisoned");
        history.push_front(plan);
        history.truncate(MAX_ROUTE_HISTORY);
    }

    pub fn route_trace(&self, route_id: &str) -> Option<FileRoutePlan> {
        self.route_history
            .lock()
            .expect("file route history mutex poisoned")
            .iter()
            .find(|plan| plan.route_id == route_id)
            .cloned()
    }

    fn bindings_for_context(
        &self,
        context: &FileRoutingScopeContext,
    ) -> Result<Vec<FileAssociationBinding>, ComponentRuntimeError> {
        let mut bindings = Vec::new();
        for (scope, path) in self.binding_locations(context)? {
            if scope == "global" {
                bindings.extend(
                    self.global_bindings
                        .lock()
                        .expect("file routing mutex poisoned")
                        .clone(),
                );
            } else if path.exists() {
                bindings.extend(read_bindings(&path)?);
            }
        }
        sort_bindings(&mut bindings);
        Ok(bindings)
    }

    fn storage_paths(
        &self,
        context: &FileRoutingScopeContext,
    ) -> Result<Vec<FileRoutingStoragePath>, ComponentRuntimeError> {
        Ok(self
            .binding_locations(context)?
            .into_iter()
            .map(|(scope, path)| FileRoutingStoragePath {
                scope,
                available: path.exists() || path.parent().is_some_and(Path::exists),
                path: path.to_string_lossy().into_owned(),
            })
            .collect())
    }

    fn binding_locations(
        &self,
        context: &FileRoutingScopeContext,
    ) -> Result<Vec<(String, PathBuf)>, ComponentRuntimeError> {
        let mut locations = Vec::new();
        if let Some(project_path) = context
            .project_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            locations.push(("project".into(), project_bindings_path(project_path)?));
        }
        if let Some(profile_id) = context
            .profile_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            locations.push(("profile".into(), self.profile_bindings_path(profile_id)?));
        }
        locations.push(("global".into(), self.global_path.clone()));
        Ok(locations)
    }

    fn path_for_binding(
        &self,
        binding: &FileAssociationBinding,
    ) -> Result<PathBuf, ComponentRuntimeError> {
        match binding.scope.as_str() {
            "global" => Ok(self.global_path.clone()),
            "profile" => self.profile_bindings_path(
                binding
                    .profile_id
                    .as_deref()
                    .ok_or_else(|| invalid_binding("Profile 文件关联缺少 profileId"))?,
            ),
            "project" => project_bindings_path(
                binding
                    .project_path
                    .as_deref()
                    .ok_or_else(|| invalid_binding("项目文件关联缺少 projectPath"))?,
            ),
            _ => Err(invalid_binding(
                "文件关联 scope 必须是 global、profile 或 project",
            )),
        }
    }

    fn profile_bindings_path(&self, profile_id: &str) -> Result<PathBuf, ComponentRuntimeError> {
        let profile_id = safe_storage_id(profile_id, "profileId")?;
        Ok(self
            .app_data_dir
            .join("profiles")
            .join("local-bindings")
            .join(format!("{profile_id}.json")))
    }
}

#[derive(Debug)]
pub struct BindingLookup {
    binding: Option<FileAssociationBinding>,
    diagnostics: Vec<FileRouteDiagnostic>,
}

pub fn route_file(
    manager: &ComponentRuntimeManager,
    routing: &FileRoutingStore,
    effective_component_ids: &BTreeSet<String>,
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
    let mut lookup = if let Some(handler) = request
        .preferred_handler_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        BindingLookup {
            binding: Some(FileAssociationBinding {
                id: "request-preference".into(),
                scope: "request".into(),
                extension: Some(extension.clone()),
                mime_type: mime.clone(),
                intent,
                handler: handler.trim().to_string(),
                behavior: "fallback".into(),
                project_path: None,
                profile_id: None,
            }),
            diagnostics: vec![FileRouteDiagnostic {
                code: "PREFERRED_HANDLER_APPLIED".into(),
                message: format!("本次操作优先使用处理器: {}", handler.trim()),
            }],
        }
    } else {
        routing.find(&extension, mime.as_deref(), intent, &request)
    };
    let binding_resolution = lookup.binding.as_ref().map(binding_resolution);
    let mut diagnostics = std::mem::take(&mut lookup.diagnostics);
    let manifests = manager.manifests();
    let installed = manifests
        .iter()
        .map(|manifest| (manifest.id.clone(), manifest.clone()))
        .collect::<BTreeMap<_, _>>();
    let effective_manifests = manifests
        .iter()
        .filter(|manifest| effective_component_ids.contains(&manifest.id))
        .cloned()
        .collect::<Vec<_>>();
    let mut candidates = build_candidates(
        &effective_manifests,
        &installed,
        &extension,
        mime.as_deref(),
        is_directory,
        intent,
    );
    let mut eligible_indexes = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| candidate.eligible.then_some(index))
        .collect::<Vec<_>>();
    eligible_indexes
        .sort_by(|left, right| compare_candidates(&candidates[*left], &candidates[*right]));

    if let Some(binding) = lookup.binding.as_ref() {
        if binding.handler == "system" {
            if intent.allows_system_fallback() && binding.behavior != "strict" {
                diagnostics.push(FileRouteDiagnostic {
                    code: "BOUND_TO_SYSTEM".into(),
                    message: "按文件关联交给系统默认程序".into(),
                });
                return finish_plan(
                    routing,
                    system_fallback_plan(path, intent, diagnostics, candidates, binding_resolution),
                );
            }
            diagnostics.push(FileRouteDiagnostic {
                code: "STRICT_SYSTEM_NOT_ALLOWED".into(),
                message: "该意图不允许使用系统程序作为成功结果".into(),
            });
            return finish_plan(
                routing,
                no_handler_plan(path, intent, diagnostics, candidates, binding_resolution),
            );
        }
        if let Some(index) = eligible_indexes
            .iter()
            .copied()
            .find(|index| candidates[*index].handler_id == binding.handler)
        {
            candidates[index].selected = true;
            for other_index in &eligible_indexes {
                if *other_index != index {
                    candidates[*other_index].reasons.push(FileRouteDiagnostic {
                        code: "BINDING_PREFERRED_OTHER_HANDLER".into(),
                        message: format!("文件关联优先选择 {}", binding.handler),
                    });
                }
            }
            let selected = candidates[index].clone();
            return finish_plan(
                routing,
                selected_plan(
                    path,
                    intent,
                    &selected,
                    diagnostics,
                    candidates,
                    binding_resolution,
                ),
            );
        }
        diagnostics.push(FileRouteDiagnostic {
            code: "BOUND_HANDLER_MISSING".into(),
            message: format!("绑定的处理器不属于当前装配或已经不可用: {}", binding.handler),
        });
        let handler_belongs_to_current_profile = candidates
            .iter()
            .any(|candidate| candidate.handler_id == binding.handler);
        if handler_belongs_to_current_profile && binding.behavior == "strict" {
            return finish_plan(
                routing,
                no_handler_plan(path, intent, diagnostics, candidates, binding_resolution),
            );
        }
    }

    if let Some(index) = eligible_indexes.first().copied() {
        candidates[index].selected = true;
        for other_index in eligible_indexes.iter().skip(1) {
            candidates[*other_index].reasons.push(FileRouteDiagnostic {
                code: "LOWER_PRIORITY".into(),
                message: "优先级或稳定 ID 排序靠后".into(),
            });
        }
        let selected = candidates[index].clone();
        return finish_plan(
            routing,
            selected_plan(
                path,
                intent,
                &selected,
                diagnostics,
                candidates,
                binding_resolution,
            ),
        );
    }

    if intent.allows_system_fallback() {
        diagnostics.push(FileRouteDiagnostic {
            code: "SYSTEM_FALLBACK".into(),
            message: "没有已启用的内部文件处理器，将交给系统默认程序".into(),
        });
        return finish_plan(
            routing,
            system_fallback_plan(path, intent, diagnostics, candidates, binding_resolution),
        );
    }

    diagnostics.push(FileRouteDiagnostic {
        code: "NO_HANDLER".into(),
        message: "没有能够处理该文件意图的已启用组件".into(),
    });
    finish_plan(
        routing,
        no_handler_plan(path, intent, diagnostics, candidates, binding_resolution),
    )
}

fn finish_plan(
    routing: &FileRoutingStore,
    plan: FileRoutePlan,
) -> Result<FileRoutePlan, ComponentRuntimeError> {
    routing.record_plan(plan.clone());
    Ok(plan)
}

fn build_candidates(
    manifests: &[ComponentManifestV1],
    installed: &BTreeMap<String, ComponentManifestV1>,
    extension: &str,
    mime: Option<&str>,
    is_directory: bool,
    intent: FileIntent,
) -> Vec<FileRouteCandidate> {
    let mut candidates = Vec::new();
    for manifest in manifests {
        let dependency_issues = dependency_issues(manifest, installed);
        for handler in &manifest.contributes.file_handlers {
            candidates.push(build_candidate(
                manifest,
                handler,
                &dependency_issues,
                extension,
                mime,
                is_directory,
                intent,
            ));
        }
    }
    candidates.sort_by(|left, right| {
        left.component_id
            .cmp(&right.component_id)
            .then_with(|| left.handler_id.cmp(&right.handler_id))
    });
    candidates
}

fn build_candidate(
    manifest: &ComponentManifestV1,
    handler: &FileHandlerContribution,
    dependency_issues: &[FileRouteDiagnostic],
    extension: &str,
    mime: Option<&str>,
    is_directory: bool,
    intent: FileIntent,
) -> FileRouteCandidate {
    let mut reasons = dependency_issues.to_vec();
    if !handler.intents.iter().any(|value| value == intent.as_str()) {
        reasons.push(FileRouteDiagnostic {
            code: "INTENT_NOT_SUPPORTED".into(),
            message: format!("处理器未声明 {} 意图", intent.as_str()),
        });
    }
    let kind_matches = handler.file_kinds.is_empty()
        || handler.file_kinds.iter().any(|kind| {
            (is_directory && kind.eq_ignore_ascii_case("directory"))
                || (!is_directory && kind.eq_ignore_ascii_case("file"))
        });
    if !kind_matches {
        reasons.push(FileRouteDiagnostic {
            code: "FILE_KIND_NOT_SUPPORTED".into(),
            message: if is_directory {
                "处理器不支持目录".into()
            } else {
                "处理器不支持普通文件".into()
            },
        });
    }
    if !is_directory {
        let extension_matches = handler.extensions.is_empty()
            || handler
                .extensions
                .iter()
                .any(|value| value.eq_ignore_ascii_case(extension));
        let mime_matches = handler.mime_types.is_empty()
            || mime.is_some_and(|value| {
                handler
                    .mime_types
                    .iter()
                    .any(|item| mime_matches(item, value))
            });
        if !extension_matches && !mime_matches {
            reasons.push(FileRouteDiagnostic {
                code: "TYPE_NOT_MATCHED".into(),
                message: format!(
                    "后缀 .{} 与 MIME {} 不匹配",
                    if extension.is_empty() {
                        "(无)"
                    } else {
                        extension
                    },
                    mime.unwrap_or("未知")
                ),
            });
        }
    }
    FileRouteCandidate {
        handler_id: handler.id.clone(),
        handler_name: handler.name.clone(),
        component_id: manifest.id.clone(),
        component_name: manifest.name.clone(),
        priority: handler.priority,
        workspace_target: handler.workspace_target.clone(),
        eligible: reasons.is_empty(),
        selected: false,
        reasons,
    }
}

fn dependency_issues(
    manifest: &ComponentManifestV1,
    installed: &BTreeMap<String, ComponentManifestV1>,
) -> Vec<FileRouteDiagnostic> {
    manifest
        .requires_components
        .iter()
        .filter_map(|dependency| dependency_issue(dependency, installed))
        .collect()
}

fn dependency_issue(
    dependency: &ComponentDependency,
    installed: &BTreeMap<String, ComponentManifestV1>,
) -> Option<FileRouteDiagnostic> {
    let Some(component) = installed.get(&dependency.id) else {
        return Some(FileRouteDiagnostic {
            code: "REQUIRED_COMPONENT_MISSING".into(),
            message: format!("缺少必需组件: {}", dependency.id),
        });
    };
    let requirement = VersionReq::parse(&dependency.version_requirement).ok()?;
    let version = Version::parse(&component.version).ok()?;
    (!requirement.matches(&version)).then(|| FileRouteDiagnostic {
        code: "REQUIRED_COMPONENT_INCOMPATIBLE".into(),
        message: format!(
            "组件 {} 版本 {} 不满足 {}",
            dependency.id, component.version, dependency.version_requirement
        ),
    })
}

fn compare_candidates(left: &FileRouteCandidate, right: &FileRouteCandidate) -> std::cmp::Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| left.component_id.cmp(&right.component_id))
        .then_with(|| left.handler_id.cmp(&right.handler_id))
}

fn selected_plan(
    path: &str,
    intent: FileIntent,
    selected: &FileRouteCandidate,
    diagnostics: Vec<FileRouteDiagnostic>,
    candidates: Vec<FileRouteCandidate>,
    binding: Option<FileRouteBindingResolution>,
) -> FileRoutePlan {
    let workspace_target = selected.workspace_target.clone();
    FileRoutePlan {
        route_id: Uuid::new_v4().to_string(),
        path: path.to_string(),
        intent,
        accepted: true,
        fallback_to_system: false,
        handler_id: Some(selected.handler_id.clone()),
        component_id: Some(selected.component_id.clone()),
        handler_name: Some(selected.handler_name.clone()),
        target: if workspace_target.is_some() {
            "workspace"
        } else {
            "component"
        }
        .into(),
        workspace_target,
        diagnostics,
        candidates,
        binding,
    }
}

fn system_fallback_plan(
    path: &str,
    intent: FileIntent,
    diagnostics: Vec<FileRouteDiagnostic>,
    candidates: Vec<FileRouteCandidate>,
    binding: Option<FileRouteBindingResolution>,
) -> FileRoutePlan {
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
        candidates,
        binding,
    }
}

fn no_handler_plan(
    path: &str,
    intent: FileIntent,
    diagnostics: Vec<FileRouteDiagnostic>,
    candidates: Vec<FileRouteCandidate>,
    binding: Option<FileRouteBindingResolution>,
) -> FileRoutePlan {
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
        candidates,
        binding,
    }
}

fn binding_resolution(binding: &FileAssociationBinding) -> FileRouteBindingResolution {
    FileRouteBindingResolution {
        id: binding.id.clone(),
        scope: binding.scope.clone(),
        handler: binding.handler.clone(),
        behavior: if binding.behavior.trim().is_empty() {
            "fallback".into()
        } else {
            binding.behavior.clone()
        },
    }
}

fn matching_binding(
    bindings: &[FileAssociationBinding],
    extension: &str,
    mime: Option<&str>,
    intent: FileIntent,
) -> Option<FileAssociationBinding> {
    let mut candidates = bindings
        .iter()
        .filter(|binding| {
            binding.intent == intent
                && binding
                    .extension
                    .as_deref()
                    .map(|value| value.eq_ignore_ascii_case(extension))
                    .unwrap_or(true)
                && binding
                    .mime_type
                    .as_deref()
                    .map(|value| mime.is_some_and(|mime| value.eq_ignore_ascii_case(mime)))
                    .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        binding_specificity(right)
            .cmp(&binding_specificity(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    candidates.into_iter().next()
}

fn binding_specificity(binding: &FileAssociationBinding) -> u8 {
    u8::from(binding.extension.is_some()) + u8::from(binding.mime_type.is_some())
}

fn mime_matches(pattern: &str, mime: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        mime.starts_with(&format!("{prefix}/"))
    } else {
        pattern.eq_ignore_ascii_case(mime)
    }
}

fn normalize_binding(binding: &mut FileAssociationBinding) -> Result<(), ComponentRuntimeError> {
    binding.id = if binding.id.trim().is_empty() {
        format!("file-association-{}", Uuid::new_v4().simple())
    } else {
        safe_storage_id(binding.id.trim(), "文件绑定 ID")?
    };
    binding.scope = binding.scope.trim().to_ascii_lowercase();
    if !matches!(binding.scope.as_str(), "global" | "profile" | "project") {
        return Err(invalid_binding(
            "文件关联 scope 必须是 global、profile 或 project",
        ));
    }
    binding.extension = binding
        .extension
        .as_deref()
        .map(normalize_extension)
        .transpose()?;
    binding.mime_type = binding
        .mime_type
        .as_deref()
        .map(normalize_mime)
        .transpose()?;
    if binding.extension.is_none() && binding.mime_type.is_none() {
        return Err(invalid_binding("文件关联至少需要后缀或 MIME 类型"));
    }
    binding.handler = binding.handler.trim().to_string();
    if binding.handler.is_empty() {
        return Err(invalid_binding("文件关联缺少处理器"));
    }
    binding.behavior = if binding.behavior.trim().is_empty() {
        "fallback".into()
    } else {
        binding.behavior.trim().to_ascii_lowercase()
    };
    if !matches!(binding.behavior.as_str(), "fallback" | "strict") {
        return Err(invalid_binding(
            "文件关联 behavior 必须是 fallback 或 strict",
        ));
    }
    match binding.scope.as_str() {
        "global" => {
            binding.project_path = None;
            binding.profile_id = None;
        }
        "profile" => {
            binding.project_path = None;
            binding.profile_id = Some(safe_storage_id(
                binding
                    .profile_id
                    .as_deref()
                    .ok_or_else(|| invalid_binding("Profile 文件关联缺少 profileId"))?,
                "profileId",
            )?);
        }
        "project" => {
            binding.profile_id = None;
            let project_path = binding
                .project_path
                .as_deref()
                .ok_or_else(|| invalid_binding("项目文件关联缺少 projectPath"))?;
            if !Path::new(project_path).is_dir() {
                return Err(invalid_binding(
                    "项目文件关联的 projectPath 不存在或不是目录",
                ));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn normalize_extension(value: &str) -> Result<String, ComponentRuntimeError> {
    let value = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if value.is_empty() || value.contains(['.', '/', '\\']) {
        return Err(invalid_binding("文件后缀必须是不带点的名称"));
    }
    Ok(value)
}

fn normalize_mime(value: &str) -> Result<String, ComponentRuntimeError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() || !value.contains('/') || value.contains(['\\', ' ']) {
        return Err(invalid_binding("MIME 类型无效"));
    }
    Ok(value)
}

fn project_bindings_path(project_path: &str) -> Result<PathBuf, ComponentRuntimeError> {
    let root = Path::new(project_path);
    if project_path.trim().is_empty() || !root.is_dir() {
        return Err(invalid_binding("项目路径不存在或不是目录"));
    }
    Ok(root.join(".pm_center").join(PROJECT_BINDINGS_FILE))
}

fn safe_storage_id(value: &str, label: &str) -> Result<String, ComponentRuntimeError> {
    if value.is_empty()
        || value.len() > 160
        || value.chars().any(|character| {
            !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-' | '_')
        })
    {
        return Err(invalid_binding(format!("{label} 包含不安全字符")));
    }
    Ok(value.to_string())
}

fn read_bindings(path: &Path) -> Result<Vec<FileAssociationBinding>, ComponentRuntimeError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = fs::read_to_string(path).map_err(|error| io_error("读取文件关联失败", error))?;
    let mut bindings =
        serde_json::from_str::<Vec<FileAssociationBinding>>(&source).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentIoError,
                format!("文件关联内容无效: {error}"),
            )
        })?;
    for binding in &mut bindings {
        normalize_binding(binding)?;
    }
    sort_bindings(&mut bindings);
    Ok(bindings)
}

fn persist_bindings(
    path: &Path,
    bindings: &[FileAssociationBinding],
) -> Result<(), ComponentRuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error("创建文件绑定目录失败", error))?;
    }
    let value = serde_json::to_vec_pretty(bindings).map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentIoError,
            format!("序列化文件绑定失败: {error}"),
        )
    })?;
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
    fs::write(&temporary, value).map_err(|error| io_error("保存文件绑定失败", error))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(io_error("提交文件绑定失败", error));
    }
    Ok(())
}

fn sort_bindings(bindings: &mut [FileAssociationBinding]) {
    bindings.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then_with(|| left.intent.as_str().cmp(right.intent.as_str()))
            .then_with(|| left.extension.cmp(&right.extension))
            .then_with(|| left.mime_type.cmp(&right.mime_type))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn scope_label(scope: &str) -> &'static str {
    match scope {
        "project" => "项目级",
        "profile" => "装配方案级",
        "global" => "全局",
        _ => "本次",
    }
}

fn invalid_binding(message: impl Into<String>) -> ComponentRuntimeError {
    ComponentRuntimeError::new(ComponentRuntimeErrorCode::ComponentPackageInvalid, message)
}

fn io_error(prefix: &str, error: impl std::fmt::Display) -> ComponentRuntimeError {
    ComponentRuntimeError::new(
        ComponentRuntimeErrorCode::ComponentIoError,
        format!("{prefix}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(
        id: &str,
        scope: &str,
        extension: Option<&str>,
        mime: Option<&str>,
    ) -> FileAssociationBinding {
        FileAssociationBinding {
            id: id.into(),
            scope: scope.into(),
            extension: extension.map(str::to_string),
            mime_type: mime.map(str::to_string),
            intent: FileIntent::Open,
            handler: "nexora.file-handler.text".into(),
            behavior: "fallback".into(),
            project_path: None,
            profile_id: None,
        }
    }

    #[test]
    fn matching_binding_prefers_more_specific_then_stable_id() {
        let selected = matching_binding(
            &[
                binding("z-general", "global", Some("txt"), None),
                binding("a-specific", "global", Some("txt"), Some("text/plain")),
            ],
            "txt",
            Some("text/plain"),
            FileIntent::Open,
        )
        .unwrap();
        assert_eq!(selected.id, "a-specific");
    }

    #[test]
    fn scope_binding_normalization_rejects_unsafe_values() {
        let mut value = binding("../../bad", "global", Some("txt"), None);
        assert!(normalize_binding(&mut value).is_err());
        let mut extension = binding("valid", "global", Some(".tar.gz"), None);
        assert!(normalize_binding(&mut extension).is_err());
    }

    #[test]
    fn candidate_sort_is_deterministic() {
        let left = FileRouteCandidate {
            handler_id: "test.handler.b".into(),
            handler_name: "B".into(),
            component_id: "test.b".into(),
            component_name: "B".into(),
            priority: 10,
            workspace_target: None,
            eligible: true,
            selected: false,
            reasons: vec![],
        };
        let right = FileRouteCandidate {
            handler_id: "test.handler.a".into(),
            handler_name: "A".into(),
            component_id: "test.a".into(),
            component_name: "A".into(),
            priority: 10,
            workspace_target: None,
            eligible: true,
            selected: false,
            reasons: vec![],
        };
        assert_eq!(
            compare_candidates(&left, &right),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn inactive_component_manifests_do_not_build_route_candidates() {
        let manifests = crate::platform::builtin_components::builtin_component_manifests();
        let active = manifests
            .iter()
            .find(|manifest| manifest.id == "nexora.file-handler.text")
            .cloned()
            .unwrap();
        let installed = manifests
            .into_iter()
            .map(|manifest| (manifest.id.clone(), manifest))
            .collect::<BTreeMap<_, _>>();

        let candidates = build_candidates(
            &[active],
            &installed,
            "txt",
            Some("text/plain"),
            false,
            FileIntent::OpenInternal,
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].component_id, "nexora.file-handler.text");
    }
}
