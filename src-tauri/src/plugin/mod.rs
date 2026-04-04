use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::process_utils::{std_command, tokio_command};

const PLUGIN_STATE_FILE: &str = "plugin-state.json";
const PLUGIN_API_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginValidationIssue {
    pub code: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionWhen {
    pub project_open: Option<bool>,
    pub selection_count: Option<String>,
    pub target_kind: Option<String>,
    pub extensions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAction {
    pub id: String,
    pub plugin_key: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub command_id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: String,
    pub scope: String,
    pub when: PluginActionWhen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDescriptor {
    pub key: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub runtime: String,
    pub entry: String,
    pub description: Option<String>,
    pub min_app_version: Option<String>,
    pub enabled: bool,
    pub enabled_by_default: bool,
    pub scope: String,
    pub path: String,
    pub entry_path: Option<String>,
    pub permissions: Vec<String>,
    pub actions: Vec<PluginAction>,
    pub validation_issues: Vec<PluginValidationIssue>,
    pub shadowed_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeInfo {
    pub status: String,
    pub resolved_path: Option<String>,
    pub sdk_path: Option<String>,
    pub source: String,
    pub version: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDirectories {
    pub global_path: String,
    pub project_path: Option<String>,
    pub runtime: PluginRuntimeInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionContextItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionContext {
    pub project_path: String,
    pub current_path: Option<String>,
    pub selected_items: Vec<PluginActionContextItem>,
    pub trigger: String,
    pub plugin_scope: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginControlMessage {
    pub r#type: String,
    pub value: Option<u8>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub tone: Option<String>,
    pub scope: Option<String>,
    pub path: Option<String>,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionRunRequest {
    pub plugin_key: String,
    pub command_id: String,
    pub context: PluginActionContext,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRunResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub controls: Vec<PluginControlMessage>,
}

#[derive(Debug, Clone)]
pub struct PreparedPluginExecution {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub env_vars: HashMap<String, String>,
    pub cleanup_paths: Vec<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PluginManifest {
    id: String,
    name: String,
    version: String,
    api_version: String,
    runtime: String,
    entry: String,
    description: Option<String>,
    min_app_version: Option<String>,
    enabled_by_default: Option<bool>,
    contributes: Option<PluginContributes>,
    permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PluginContributes {
    commands: Option<Vec<PluginCommandContribution>>,
    toolbar_actions: Option<Vec<PluginActionContribution>>,
    file_context_actions: Option<Vec<PluginActionContribution>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginCommandContribution {
    id: String,
    title: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginActionContribution {
    command: String,
    title: Option<String>,
    when: Option<PluginActionWhen>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginState {
    version: u8,
    enabled: HashMap<String, bool>,
}

#[derive(Debug)]
struct ScannedPlugin {
    descriptor: PluginDescriptor,
}

fn push_issue(
    issues: &mut Vec<PluginValidationIssue>,
    code: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(PluginValidationIssue {
        code: code.into(),
        message: message.into(),
        severity: "error".to_string(),
    });
}

fn version_greater_than(left: &str, right: &str) -> bool {
    let left_parts = left
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect::<Vec<_>>();
    let right_parts = right
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect::<Vec<_>>();
    let max_len = left_parts.len().max(right_parts.len());

    for index in 0..max_len {
        let left_value = *left_parts.get(index).unwrap_or(&0);
        let right_value = *right_parts.get(index).unwrap_or(&0);

        if left_value > right_value {
            return true;
        }
        if left_value < right_value {
            return false;
        }
    }

    false
}

fn normalize_extension(extension: &str) -> String {
    extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

fn validate_when(
    when: &PluginActionWhen,
    issues: &mut Vec<PluginValidationIssue>,
    action_label: &str,
) {
    if let Some(selection_count) = &when.selection_count {
        let valid = matches!(
            selection_count.as_str(),
            "any" | "none" | "single" | "multiple"
        );
        if !valid {
            push_issue(
                issues,
                "invalid_selection_count",
                format!("{action_label}: selectionCount 只支持 any/none/single/multiple"),
            );
        }
    }

    if let Some(target_kind) = &when.target_kind {
        let valid = matches!(target_kind.as_str(), "any" | "file" | "directory" | "mixed");
        if !valid {
            push_issue(
                issues,
                "invalid_target_kind",
                format!("{action_label}: targetKind 只支持 any/file/directory/mixed"),
            );
        }
    }

    if let Some(extensions) = &when.extensions {
        for extension in extensions {
            if normalize_extension(extension).is_empty() {
                push_issue(
                    issues,
                    "invalid_extension",
                    format!("{action_label}: extensions 不能包含空值"),
                );
                break;
            }
        }
    }
}

fn plugin_state_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取插件状态目录失败: {error}"))?;
    Ok(app_data_dir.join(PLUGIN_STATE_FILE))
}

fn load_plugin_state(app_handle: &AppHandle) -> Result<PluginState, String> {
    let path = plugin_state_path(app_handle)?;
    if !path.exists() {
        return Ok(PluginState {
            version: 1,
            enabled: HashMap::new(),
        });
    }

    let content =
        fs::read_to_string(&path).map_err(|error| format!("读取插件状态失败: {error}"))?;

    serde_json::from_str::<PluginState>(&content)
        .map_err(|error| format!("解析插件状态失败: {error}"))
}

fn save_plugin_state(app_handle: &AppHandle, state: &PluginState) -> Result<(), String> {
    let path = plugin_state_path(app_handle)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建插件状态目录失败: {error}"))?;
    }

    let content = serde_json::to_string_pretty(state)
        .map_err(|error| format!("序列化插件状态失败: {error}"))?;

    fs::write(&path, content).map_err(|error| format!("写入插件状态失败: {error}"))
}

pub fn get_global_plugins_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取全局插件目录失败: {error}"))?;
    Ok(app_data_dir.join("plugins"))
}

pub fn get_project_plugins_dir(project_path: &str) -> PathBuf {
    PathBuf::from(project_path)
        .join(".pm_center")
        .join("plugins")
}

fn descriptor_key(scope: &str, id: &str, path: &Path) -> String {
    format!("{scope}::{id}::{}", path.to_string_lossy())
}

fn runtime_version(program: &Path) -> Option<String> {
    std_command(program)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let value = if stdout.is_empty() { stderr } else { stdout };
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
}

fn resolve_resource_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    app_handle
        .path()
        .resource_dir()
        .ok()
        .or_else(|| Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")))
}

fn resolve_plugin_sdk_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    let resource_dir = resolve_resource_dir(app_handle)?;
    let candidates = [
        resource_dir.join("plugin-sdk"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("plugin-sdk"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

pub fn resolve_plugin_runtime(app_handle: &AppHandle) -> PluginRuntimeInfo {
    let mut candidates = Vec::new();

    if let Some(resource_dir) = resolve_resource_dir(app_handle) {
        candidates.push(
            resource_dir
                .join("plugin-python")
                .join("windows-x64")
                .join("python.exe"),
        );
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("plugin-python")
            .join("windows-x64")
            .join("python.exe"),
    );

    if let Some(path) = candidates.into_iter().find(|path| path.exists()) {
        return PluginRuntimeInfo {
            status: "ready".to_string(),
            resolved_path: Some(path.to_string_lossy().to_string()),
            sdk_path: resolve_plugin_sdk_dir(app_handle)
                .map(|path| path.to_string_lossy().to_string()),
            source: "embedded".to_string(),
            version: runtime_version(&path),
            message: None,
        };
    }

    if std::env::var("PMC_ALLOW_PLUGIN_SYSTEM_PYTHON")
        .map(|value| value == "1")
        .unwrap_or(false)
    {
        if let Ok(output) = std_command("python").arg("--version").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let version = if stdout.is_empty() { stderr } else { stdout };

                return PluginRuntimeInfo {
                    status: "ready".to_string(),
                    resolved_path: Some("python".to_string()),
                    sdk_path: resolve_plugin_sdk_dir(app_handle)
                        .map(|path| path.to_string_lossy().to_string()),
                    source: "dev-fallback".to_string(),
                    version: if version.is_empty() {
                        None
                    } else {
                        Some(version)
                    },
                    message: Some("当前使用开发模式系统 Python 回退，仅建议本地调试。".to_string()),
                };
            }
        }
    }

    PluginRuntimeInfo {
        status: "missing".to_string(),
        resolved_path: None,
        sdk_path: resolve_plugin_sdk_dir(app_handle).map(|path| path.to_string_lossy().to_string()),
        source: "missing".to_string(),
        version: None,
        message: Some("未找到内置插件 Python 运行时。请先执行插件运行时准备脚本。".to_string()),
    }
}

fn manifest_from_dir(plugin_dir: &Path) -> Result<PluginManifest, String> {
    let manifest_path = plugin_dir.join("plugin.json");
    let content = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("读取 {} 失败: {error}", manifest_path.to_string_lossy()))?;
    serde_json::from_str::<PluginManifest>(&content)
        .map_err(|error| format!("解析 {} 失败: {error}", manifest_path.to_string_lossy()))
}

fn scan_plugin_dir(
    app_handle: &AppHandle,
    plugin_dir: &Path,
    scope: &str,
    enabled_overrides: &HashMap<String, bool>,
) -> ScannedPlugin {
    let directory_name = plugin_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("plugin")
        .to_string();
    let runtime = resolve_plugin_runtime(app_handle);
    let mut issues = Vec::new();
    let manifest = match manifest_from_dir(plugin_dir) {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            push_issue(&mut issues, "manifest_error", error);
            None
        }
    };

    let mut descriptor = PluginDescriptor {
        key: descriptor_key(scope, &directory_name, plugin_dir),
        id: directory_name.clone(),
        name: directory_name.clone(),
        version: "0.0.0".to_string(),
        api_version: PLUGIN_API_VERSION.to_string(),
        runtime: "python".to_string(),
        entry: "main.py".to_string(),
        description: None,
        min_app_version: None,
        enabled: false,
        enabled_by_default: true,
        scope: scope.to_string(),
        path: plugin_dir.to_string_lossy().to_string(),
        entry_path: None,
        permissions: Vec::new(),
        actions: Vec::new(),
        validation_issues: Vec::new(),
        shadowed_by: None,
    };

    if let Some(manifest) = manifest {
        descriptor.id = if manifest.id.trim().is_empty() {
            push_issue(
                &mut issues,
                "missing_plugin_id",
                "plugin.json 缺少有效的 id",
            );
            directory_name.clone()
        } else {
            manifest.id.trim().to_string()
        };
        descriptor.key = descriptor_key(scope, &descriptor.id, plugin_dir);
        descriptor.name = if manifest.name.trim().is_empty() {
            push_issue(
                &mut issues,
                "missing_plugin_name",
                "plugin.json 缺少有效的 name",
            );
            directory_name.clone()
        } else {
            manifest.name.trim().to_string()
        };
        descriptor.version = if manifest.version.trim().is_empty() {
            push_issue(
                &mut issues,
                "missing_plugin_version",
                "plugin.json 缺少有效的 version",
            );
            "0.0.0".to_string()
        } else {
            manifest.version.trim().to_string()
        };
        descriptor.api_version = if manifest.api_version.trim().is_empty() {
            PLUGIN_API_VERSION.to_string()
        } else {
            manifest.api_version.trim().to_string()
        };
        descriptor.runtime = if manifest.runtime.trim().is_empty() {
            "python".to_string()
        } else {
            manifest.runtime.trim().to_string()
        };
        descriptor.entry = if manifest.entry.trim().is_empty() {
            "main.py".to_string()
        } else {
            manifest.entry.trim().replace('\\', "/")
        };
        descriptor.description = manifest
            .description
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        descriptor.min_app_version = manifest
            .min_app_version
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        descriptor.enabled_by_default = manifest.enabled_by_default.unwrap_or(true);
        descriptor.permissions = manifest.permissions.unwrap_or_default();

        if descriptor.runtime != "python" {
            push_issue(
                &mut issues,
                "unsupported_runtime",
                format!(
                    "插件 {} 仅支持 runtime=python，当前为 {}",
                    descriptor.id, descriptor.runtime
                ),
            );
        }

        if descriptor.api_version != PLUGIN_API_VERSION {
            push_issue(
                &mut issues,
                "unsupported_api_version",
                format!(
                    "插件 {} 的 apiVersion={} 与宿主支持的 {} 不匹配",
                    descriptor.id, descriptor.api_version, PLUGIN_API_VERSION
                ),
            );
        }

        if let Some(min_app_version) = &descriptor.min_app_version {
            if version_greater_than(min_app_version, env!("CARGO_PKG_VERSION")) {
                push_issue(
                    &mut issues,
                    "min_app_version_not_met",
                    format!(
                        "插件 {} 需要应用版本 >= {}，当前为 {}",
                        descriptor.id,
                        min_app_version,
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            }
        }

        let entry_path = plugin_dir.join(&descriptor.entry);
        if entry_path.exists() {
            descriptor.entry_path = Some(entry_path.to_string_lossy().to_string());
        } else {
            push_issue(
                &mut issues,
                "missing_entry",
                format!("插件入口不存在: {}", entry_path.to_string_lossy()),
            );
        }

        if runtime.status != "ready" {
            push_issue(
                &mut issues,
                "runtime_unavailable",
                runtime
                    .message
                    .clone()
                    .unwrap_or_else(|| "插件运行时不可用".to_string()),
            );
        }

        let contributes = manifest.contributes.unwrap_or_default();
        let mut command_map = HashMap::new();
        let mut command_ids = HashSet::new();

        for command in contributes.commands.unwrap_or_default() {
            if command.id.trim().is_empty() {
                push_issue(
                    &mut issues,
                    "missing_command_id",
                    "commands 中存在缺少 id 的项",
                );
                continue;
            }

            if !command_ids.insert(command.id.clone()) {
                push_issue(
                    &mut issues,
                    "duplicate_command_id",
                    format!(
                        "插件 {} 中存在重复 command id: {}",
                        descriptor.id, command.id
                    ),
                );
                continue;
            }

            command_map.insert(command.id.clone(), command);
        }

        let mut build_action = |location: &str, contribution: PluginActionContribution| {
            let Some(command) = command_map.get(&contribution.command) else {
                push_issue(
                    &mut issues,
                    "missing_command_reference",
                    format!(
                        "插件 {} 的 {} 引用了不存在的 command: {}",
                        descriptor.id, location, contribution.command
                    ),
                );
                return;
            };

            let when = contribution.when.unwrap_or_default();
            validate_when(
                &when,
                &mut issues,
                &format!("插件 {} 的 {}", descriptor.id, contribution.command),
            );

            descriptor.actions.push(PluginAction {
                id: format!("{}:{}:{}", descriptor.id, contribution.command, location),
                plugin_key: descriptor.key.clone(),
                plugin_id: descriptor.id.clone(),
                plugin_name: descriptor.name.clone(),
                command_id: contribution.command.clone(),
                title: contribution.title.unwrap_or_else(|| command.title.clone()),
                description: command.description.clone(),
                location: location.to_string(),
                scope: descriptor.scope.clone(),
                when,
            });
        };

        for contribution in contributes.toolbar_actions.unwrap_or_default() {
            build_action("toolbar", contribution);
        }

        for contribution in contributes.file_context_actions.unwrap_or_default() {
            build_action("file-context", contribution);
        }
    }

    descriptor.validation_issues = issues;
    let configured_enabled = enabled_overrides
        .get(&descriptor.key)
        .copied()
        .unwrap_or(descriptor.enabled_by_default);
    descriptor.enabled = configured_enabled && descriptor.validation_issues.is_empty();

    ScannedPlugin { descriptor }
}

fn collect_plugin_dirs(root: &Path) -> Vec<PathBuf> {
    if !root.exists() {
        return Vec::new();
    }

    match fs::read_dir(root) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn scan_plugins_internal(
    app_handle: &AppHandle,
    project_path: Option<&str>,
) -> Result<Vec<PluginDescriptor>, String> {
    let state = load_plugin_state(app_handle)?;
    let global_dir = get_global_plugins_dir(app_handle)?;
    fs::create_dir_all(&global_dir).map_err(|error| format!("创建全局插件目录失败: {error}"))?;

    let mut descriptors = Vec::new();

    for plugin_dir in collect_plugin_dirs(&global_dir) {
        descriptors
            .push(scan_plugin_dir(app_handle, &plugin_dir, "global", &state.enabled).descriptor);
    }

    if let Some(project_path) = project_path {
        let project_dir = get_project_plugins_dir(project_path);
        fs::create_dir_all(&project_dir)
            .map_err(|error| format!("创建项目插件目录失败: {error}"))?;

        for plugin_dir in collect_plugin_dirs(&project_dir) {
            descriptors.push(
                scan_plugin_dir(app_handle, &plugin_dir, "project", &state.enabled).descriptor,
            );
        }
    }

    let mut grouped_by_id: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, descriptor) in descriptors.iter().enumerate() {
        grouped_by_id
            .entry(descriptor.id.clone())
            .or_default()
            .push(index);
    }

    for indexes in grouped_by_id.values() {
        let project_indexes = indexes
            .iter()
            .copied()
            .filter(|index| descriptors[*index].scope == "project")
            .collect::<Vec<_>>();
        let global_indexes = indexes
            .iter()
            .copied()
            .filter(|index| descriptors[*index].scope == "global")
            .collect::<Vec<_>>();

        if project_indexes.len() > 1 || global_indexes.len() > 1 {
            for index in indexes {
                let plugin_id = descriptors[*index].id.clone();
                push_issue(
                    &mut descriptors[*index].validation_issues,
                    "duplicate_plugin_id",
                    format!("检测到重复插件 id: {}", plugin_id),
                );
            }
        }

        if !project_indexes.is_empty() {
            for index in global_indexes {
                descriptors[index].shadowed_by = Some("project".to_string());
                descriptors[index].enabled = false;
            }
        }
    }

    for descriptor in &mut descriptors {
        if !descriptor.validation_issues.is_empty() || descriptor.shadowed_by.is_some() {
            descriptor.enabled = false;
        }
    }

    descriptors.sort_by(|left, right| {
        let scope_rank = |scope: &str| if scope == "project" { 0 } else { 1 };
        scope_rank(&left.scope)
            .cmp(&scope_rank(&right.scope))
            .then(left.name.cmp(&right.name))
    });

    Ok(descriptors)
}

pub fn parse_plugin_control_message(line: &str) -> Option<PluginControlMessage> {
    let payload = line.strip_prefix("@pmc ")?;
    serde_json::from_str::<PluginControlMessage>(payload.trim()).ok()
}

async fn load_plugin_descriptors(
    app_handle: AppHandle,
    project_path: Option<String>,
) -> Result<Vec<PluginDescriptor>, String> {
    scan_plugins_internal(&app_handle, project_path.as_deref())
}

#[tauri::command]
pub async fn refresh_plugins(
    app_handle: AppHandle,
    project_path: Option<String>,
) -> Result<Vec<PluginDescriptor>, String> {
    load_plugin_descriptors(app_handle, project_path).await
}

#[tauri::command]
pub async fn list_plugins(
    app_handle: AppHandle,
    project_path: Option<String>,
) -> Result<Vec<PluginDescriptor>, String> {
    load_plugin_descriptors(app_handle, project_path).await
}

#[tauri::command]
pub async fn set_plugin_enabled(
    app_handle: AppHandle,
    plugin_key: String,
    enabled: bool,
) -> Result<(), String> {
    let mut state = load_plugin_state(&app_handle)?;
    state.version = 1;
    state.enabled.insert(plugin_key, enabled);
    save_plugin_state(&app_handle, &state)
}

#[tauri::command]
pub async fn get_plugin_dirs(
    app_handle: AppHandle,
    project_path: Option<String>,
) -> Result<PluginDirectories, String> {
    let global_path = get_global_plugins_dir(&app_handle)?;
    fs::create_dir_all(&global_path).map_err(|error| format!("创建全局插件目录失败: {error}"))?;

    let project_path = project_path.map(|path| {
        let project_plugins_dir = get_project_plugins_dir(&path);
        let _ = fs::create_dir_all(&project_plugins_dir);
        project_plugins_dir.to_string_lossy().to_string()
    });

    Ok(PluginDirectories {
        global_path: global_path.to_string_lossy().to_string(),
        project_path,
        runtime: resolve_plugin_runtime(&app_handle),
    })
}

#[tauri::command]
pub async fn validate_plugin(
    app_handle: AppHandle,
    plugin_path: String,
    scope: Option<String>,
) -> Result<PluginDescriptor, String> {
    let plugin_dir = PathBuf::from(&plugin_path);
    if !plugin_dir.exists() {
        return Err(format!("插件目录不存在: {plugin_path}"));
    }

    let state = load_plugin_state(&app_handle)?;
    Ok(scan_plugin_dir(
        &app_handle,
        &plugin_dir,
        scope.as_deref().unwrap_or("global"),
        &state.enabled,
    )
    .descriptor)
}

fn build_python_path(runtime: &PluginRuntimeInfo, plugin_dir: &Path) -> Option<String> {
    runtime.resolved_path.clone().or_else(|| {
        let fallback = plugin_dir.join("python.exe");
        if fallback.exists() {
            Some(fallback.to_string_lossy().to_string())
        } else {
            None
        }
    })
}

pub fn prepare_plugin_execution(
    app_handle: &AppHandle,
    request: &PluginActionRunRequest,
) -> Result<PreparedPluginExecution, String> {
    let descriptors = scan_plugins_internal(app_handle, Some(&request.context.project_path))?;
    let descriptor = descriptors
        .into_iter()
        .find(|descriptor| descriptor.key == request.plugin_key)
        .ok_or_else(|| format!("未找到插件: {}", request.plugin_key))?;

    if !descriptor.enabled {
        return Err(format!("插件 {} 当前不可用或未启用", descriptor.name));
    }

    let action = descriptor
        .actions
        .iter()
        .find(|action| action.command_id == request.command_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "插件 {} 中不存在动作 {}",
                descriptor.name, request.command_id
            )
        })?;

    let runtime = resolve_plugin_runtime(app_handle);
    if runtime.status != "ready" {
        return Err(runtime
            .message
            .unwrap_or_else(|| "插件运行时不可用".to_string()));
    }

    let plugin_dir = PathBuf::from(&descriptor.path);
    let entry_path = descriptor
        .entry_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| format!("插件 {} 缺少有效入口", descriptor.name))?;
    let request_path = std::env::temp_dir().join(format!(
        "pmc_plugin_request_{}_{}.json",
        descriptor.id,
        uuid::Uuid::new_v4()
    ));

    let request_payload = serde_json::json!({
        "apiVersion": PLUGIN_API_VERSION,
        "pluginId": descriptor.id,
        "pluginName": descriptor.name,
        "commandId": action.command_id,
        "commandTitle": action.title,
        "trigger": request.context.trigger,
        "projectPath": request.context.project_path,
        "currentPath": request.context.current_path,
        "selectedItems": request.context.selected_items,
        "pluginScope": descriptor.scope,
        "appVersion": env!("CARGO_PKG_VERSION"),
        "permissions": descriptor.permissions,
    });

    fs::write(
        &request_path,
        serde_json::to_string_pretty(&request_payload)
            .map_err(|error| format!("序列化插件请求失败: {error}"))?,
    )
    .map_err(|error| format!("写入插件请求文件失败: {error}"))?;

    let mut python_path_entries = Vec::new();
    if let Some(sdk_path) = runtime.sdk_path.clone() {
        python_path_entries.push(sdk_path);
    }
    python_path_entries.push(plugin_dir.to_string_lossy().to_string());

    let vendor_dir = plugin_dir.join("vendor");
    if vendor_dir.exists() {
        python_path_entries.push(vendor_dir.to_string_lossy().to_string());
    }

    let program = build_python_path(&runtime, &plugin_dir)
        .ok_or_else(|| "无法解析插件 Python 运行时路径".to_string())?;

    let mut env_vars = HashMap::new();
    env_vars.insert("PYTHONIOENCODING".to_string(), "utf-8".to_string());
    env_vars.insert("PYTHONUTF8".to_string(), "1".to_string());
    env_vars.insert(
        "PMC_PLUGIN_DIR".to_string(),
        plugin_dir.to_string_lossy().to_string(),
    );
    env_vars.insert("PMC_PLUGIN_ID".to_string(), descriptor.id.clone());
    env_vars.insert("PMC_PLUGIN_SCOPE".to_string(), descriptor.scope.clone());
    env_vars.insert(
        "PYTHONPATH".to_string(),
        python_path_entries.join(if cfg!(windows) { ";" } else { ":" }),
    );

    Ok(PreparedPluginExecution {
        program,
        args: vec![
            entry_path.to_string_lossy().to_string(),
            "--pmc-request".to_string(),
            request_path.to_string_lossy().to_string(),
        ],
        working_dir: request
            .context
            .current_path
            .clone()
            .or_else(|| Some(request.context.project_path.clone())),
        env_vars,
        cleanup_paths: vec![request_path],
    })
}

#[tauri::command]
pub async fn run_plugin_action(
    app_handle: AppHandle,
    request: PluginActionRunRequest,
) -> Result<PluginRunResult, String> {
    let prepared = prepare_plugin_execution(&app_handle, &request)?;
    let mut command = tokio_command(&prepared.program);
    command.args(&prepared.args);
    if let Some(working_dir) = &prepared.working_dir {
        command.current_dir(working_dir);
    }
    for (key, value) in &prepared.env_vars {
        command.env(key, value);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("启动插件动作失败: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取插件 stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取插件 stderr".to_string())?;

    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut lines = Vec::new();
        let mut controls = Vec::new();

        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).await.unwrap_or(0);
            if read == 0 {
                break;
            }

            let line = line.trim_end_matches(['\r', '\n']).to_string();
            if let Some(control) = parse_plugin_control_message(&line) {
                controls.push(control);
            }
            lines.push(line);
        }

        (lines.join("\n"), controls)
    });

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut lines = Vec::new();

        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).await.unwrap_or(0);
            if read == 0 {
                break;
            }
            lines.push(line.trim_end_matches(['\r', '\n']).to_string());
        }

        lines.join("\n")
    });

    let status = child
        .wait()
        .await
        .map_err(|error| format!("等待插件动作结束失败: {error}"))?;
    let (stdout, controls) = stdout_handle
        .await
        .map_err(|error| format!("读取插件 stdout 失败: {error}"))?;
    let stderr = stderr_handle
        .await
        .map_err(|error| format!("读取插件 stderr 失败: {error}"))?;

    for path in prepared.cleanup_paths {
        let _ = fs::remove_file(path);
    }

    Ok(PluginRunResult {
        success: status.success(),
        stdout,
        stderr,
        exit_code: status.code(),
        controls,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_extension, parse_plugin_control_message, validate_when, version_greater_than,
        PluginActionWhen,
    };

    #[test]
    fn parses_control_messages_from_stdout_prefix() {
        let control = parse_plugin_control_message(
            r#"@pmc {"type":"progress","value":42,"message":"hello"}"#,
        )
        .expect("control message should parse");

        assert_eq!(control.r#type, "progress");
        assert_eq!(control.value, Some(42));
        assert_eq!(control.message.as_deref(), Some("hello"));
    }

    #[test]
    fn ignores_non_control_lines() {
        assert!(parse_plugin_control_message("normal log line").is_none());
        assert!(parse_plugin_control_message("@pmc not-json").is_none());
    }

    #[test]
    fn normalizes_extensions_without_leading_dot() {
        assert_eq!(normalize_extension(".Py"), "py");
        assert_eq!(normalize_extension("TXT"), "txt");
        assert_eq!(normalize_extension(""), "");
    }

    #[test]
    fn compares_versions_semantically() {
        assert!(version_greater_than("1.10.0", "1.9.9"));
        assert!(!version_greater_than("1.5.2", "1.5.2"));
        assert!(!version_greater_than("1.5.1", "1.5.2"));
    }

    #[test]
    fn reports_invalid_when_values() {
        let mut issues = Vec::new();
        validate_when(
            &PluginActionWhen {
                project_open: Some(true),
                selection_count: Some("two".to_string()),
                target_kind: Some("archive".to_string()),
                extensions: Some(vec!["".to_string()]),
            },
            &mut issues,
            "test-action",
        );

        let codes = issues.iter().map(|issue| issue.code.as_str()).collect::<Vec<_>>();
        assert!(codes.contains(&"invalid_selection_count"));
        assert!(codes.contains(&"invalid_target_kind"));
        assert!(codes.contains(&"invalid_extension"));
    }
}
