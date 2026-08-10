//! Public, host-owned file operations component.
//!
//! The component is intentionally the only place that turns a project-relative
//! path or a scoped external absolute path into an operating-system path. This
//! keeps TreeCache invalidation and external-directory grants out of component
//! code and out of the Python SDK implementation.

use super::component_runtime::{ComponentRuntimeError, ComponentRuntimeErrorCode, ProcessResponse};
use base64::Engine;
use chrono::Utc;
use pmc_platform::Capability;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;
use uuid::Uuid;
use walkdir::WalkDir;

pub const FILE_OPERATIONS_COMPONENT_ID: &str = "nexora.file-operations";
const MAX_CONTENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_STREAM_CHUNK_BYTES: usize = 1024 * 1024;
const MAX_BATCH_OPERATIONS: usize = 100;
const STREAM_TTL_MS: i64 = 15 * 60 * 1000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileLocation {
    space: String,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    grant_id: Option<String>,
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum GrantLifetime {
    Once,
    Session,
    Persistent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExternalPathGrant {
    id: String,
    component_id: String,
    root_path: String,
    access: String,
    lifetime: GrantLifetime,
    created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct ResolvedLocation {
    path: PathBuf,
    project_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocationAccess {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct FileStream {
    component_id: String,
    mode: String,
    path: PathBuf,
    temporary_path: Option<PathBuf>,
    project_root: Option<PathBuf>,
    grant_id: Option<String>,
    access: LocationAccess,
    offset: u64,
    expires_at: i64,
}

lazy_static::lazy_static! {
    static ref SESSION_GRANTS: Mutex<HashMap<String, ExternalPathGrant>> = Mutex::new(HashMap::new());
    static ref STREAMS: Mutex<HashMap<String, FileStream>> = Mutex::new(HashMap::new());
}

pub(crate) fn release_component_streams(component_id: &str) {
    if let Ok(mut streams) = STREAMS.lock() {
        let released = streams
            .iter()
            .filter(|(_, stream)| stream.component_id == component_id)
            .map(|(id, stream)| (id.clone(), stream.temporary_path.clone()))
            .collect::<Vec<_>>();
        for (id, temporary) in released {
            streams.remove(&id);
            if let Some(temporary) = temporary {
                let _ = fs::remove_file(temporary);
            }
        }
    }
}

/// Project leases are revoked when their outer tab closes. Drop matching streams
/// immediately so temporary write files cannot outlive that lease.
pub(crate) fn release_project_streams(project_path: &str) {
    let project_key = crate::tree_cache::normalize_path_key(project_path);
    release_streams_matching(|stream| {
        stream
            .project_root
            .as_ref()
            .is_some_and(|root| crate::tree_cache::normalize_path_key(&root.to_string_lossy()) == project_key)
    });
}

fn release_grant_streams(grant_id: &str) {
    release_streams_matching(|stream| stream.grant_id.as_deref() == Some(grant_id));
}

fn release_streams_matching(predicate: impl Fn(&FileStream) -> bool) {
    if let Ok(mut streams) = STREAMS.lock() {
        let released = streams
            .iter()
            .filter(|(_, stream)| predicate(stream))
            .map(|(id, stream)| (id.clone(), stream.temporary_path.clone()))
            .collect::<Vec<_>>();
        for (id, temporary) in released {
            streams.remove(&id);
            if let Some(temporary) = temporary {
                let _ = fs::remove_file(temporary);
            }
        }
    }
}

/// Returns the smallest capability set for a concrete public command request.
/// The component manifest advertises every capability a command may require;
/// the bridge uses this function to request only the spaces actually touched.
pub(crate) fn required_capabilities_for_command(
    command: &str,
    request: &Value,
) -> Result<Vec<Capability>, String> {
    let location_capability = |key: &str, access: LocationAccess| -> Result<Capability, String> {
        let location: FileLocation = serde_json::from_value(
            request.get(key).cloned().ok_or_else(|| format!("缺少 {key}"))?,
        ).map_err(|error| format!("{key} 无效: {error}"))?;
        match (location.space.as_str(), access) {
            ("project", LocationAccess::Read) => Ok(Capability::ProjectFilesRead),
            ("project", LocationAccess::Write) => Ok(Capability::ProjectFilesWrite),
            ("external", LocationAccess::Read) => Ok(Capability::FilesystemExternalRead),
            ("external", LocationAccess::Write) => Ok(Capability::FilesystemExternalWrite),
            _ => Err("location.space 只支持 project 或 external".into()),
        }
    };
    let one_location = |access| -> Result<Vec<Capability>, String> {
        if command == "directory.list" && request.get("location").is_none() {
            return Ok(vec![Capability::ProjectFilesRead]);
        }
        Ok(vec![location_capability("location", access)?])
    };
    let mut capabilities = match command {
        "project.describe" => vec![Capability::ProjectFilesRead],
        "directory.list" | "entry.stat" | "entry.exists" | "entry.search" | "file.read"
        | "file.hash" | "stream.open-read" => one_location(LocationAccess::Read)?,
        "file.write" | "directory.create" | "stream.open-write" => one_location(LocationAccess::Write)?,
        "entry.copy" | "external.import" | "external.export" => vec![
            location_capability("source", LocationAccess::Read)?,
            location_capability("target", LocationAccess::Write)?,
        ],
        "entry.move" => vec![
            location_capability("source", LocationAccess::Write)?,
            location_capability("target", LocationAccess::Write)?,
        ],
        "entry.rename" | "entry.delete" => one_location(LocationAccess::Write)?,
        "batch.execute" => {
            let operations = request
                .get("operations")
                .and_then(Value::as_array)
                .ok_or("批量操作缺少 operations")?;
            let mut all = Vec::new();
            for operation in operations {
                let nested_command = operation
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or("批量项目缺少 command")?;
                if nested_command == "batch.execute" {
                    return Err("批量操作不能递归调用 batch.execute".into());
                }
                let nested_input = operation.get("input").cloned().unwrap_or(Value::Null);
                all.extend(required_capabilities_for_command(nested_command, &nested_input)?);
            }
            all
        }
        "stream.read" | "stream.write" | "stream.commit" | "stream.abort" => Vec::new(),
        "external.select" | "external.grant-directory" => vec![Capability::FilesystemDialogOpen],
        "external.list-grants" | "external.revoke-grant" => vec![Capability::FilesystemExternalRead],
        "cache.status" | "cache.query" | "watcher.status" => vec![Capability::CacheInspect],
        "cache.invalidate" | "cache.refresh-directory" | "cache.rebuild-project" => {
            vec![Capability::CacheMaintain]
        }
        other => return Err(format!("文件操作不支持命令 {other}")),
    };
    capabilities.dedup();
    Ok(capabilities)
}

pub(crate) async fn invoke(
    app_handle: &AppHandle,
    component_root: &Path,
    payload: Value,
) -> Result<ProcessResponse, ComponentRuntimeError> {
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_error("文件操作缺少 command"))?;
    let input = payload.get("input").cloned().unwrap_or(Value::Null);
    let request = input.get("request").cloned().unwrap_or_else(|| input.clone());
    let context = input.get("nexora").cloned().unwrap_or(Value::Null);
    let caller_component_id = context
        .get("callerComponentId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(FILE_OPERATIONS_COMPONENT_ID)
        .to_string();
    let project_root = context
        .get("projectPath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);

    purge_expired_streams();
    let output = execute_command(
        app_handle,
        component_root,
        command,
        &request,
        project_root.as_deref(),
        &caller_component_id,
    )
    .await?;
    Ok(ProcessResponse {
        output,
        logs: vec![format!("文件操作 {command} 已完成")],
    })
}

async fn execute_command(
    app_handle: &AppHandle,
    component_root: &Path,
    command: &str,
    request: &Value,
    current_project_root: Option<&Path>,
    caller_component_id: &str,
) -> Result<Value, ComponentRuntimeError> {
    match command {
        "project.describe" => describe_project(current_project_root),
        "directory.list" => list_directory(request, current_project_root, component_root, caller_component_id),
        "entry.stat" => stat_entry(request, current_project_root, component_root, caller_component_id),
        "entry.exists" => exists_entry(request, current_project_root, component_root, caller_component_id),
        "entry.search" => search_entries(request, current_project_root, component_root, caller_component_id),
        "file.read" => read_file(request, current_project_root, component_root, caller_component_id),
        "file.hash" => hash_file(request, current_project_root, component_root, caller_component_id),
        "file.write" => write_file(request, current_project_root, component_root, caller_component_id),
        "directory.create" => create_directory(request, current_project_root, component_root, caller_component_id),
        "entry.copy" | "external.import" | "external.export" => {
            copy_entry(app_handle, request, current_project_root, component_root, caller_component_id).await
        }
        "entry.move" => move_entry(request, current_project_root, component_root, caller_component_id).await,
        "entry.rename" => rename_entry(request, current_project_root, component_root, caller_component_id).await,
        "entry.delete" => delete_entry(request, current_project_root, component_root, caller_component_id).await,
        "batch.execute" => batch_execute(app_handle, component_root, request, current_project_root, caller_component_id).await,
        "stream.open-read" => stream_open_read(request, current_project_root, component_root, caller_component_id),
        "stream.read" => stream_read(request, component_root, caller_component_id),
        "stream.open-write" => stream_open_write(request, current_project_root, component_root, caller_component_id),
        "stream.write" => stream_write(request, component_root, caller_component_id),
        "stream.commit" => stream_commit(request, component_root, caller_component_id),
        "stream.abort" => stream_abort(request, caller_component_id),
        "external.select" => {
            select_external_directory(app_handle, component_root, request, caller_component_id, true).await
        }
        "external.grant-directory" => {
            select_external_directory(app_handle, component_root, request, caller_component_id, false).await
        }
        "external.list-grants" => list_external_grants(component_root, caller_component_id),
        "external.revoke-grant" => revoke_external_grant(component_root, request, caller_component_id),
        "cache.status" => cache_status(current_project_root),
        "cache.query" => cache_query(request, current_project_root),
        "cache.invalidate" => cache_invalidate(current_project_root),
        "cache.refresh-directory" => cache_refresh(request, current_project_root),
        "cache.rebuild-project" => cache_rebuild(current_project_root),
        "watcher.status" => watcher_status(current_project_root),
        other => Err(protocol_error(format!("文件操作不支持命令 {other}"))),
    }
}

fn describe_project(current_project_root: Option<&Path>) -> Result<Value, ComponentRuntimeError> {
    let root = require_project_root(current_project_root)?;
    Ok(json!({
        "projectId": project_id(root),
        "rootPath": root.to_string_lossy(),
        "name": root.file_name().and_then(|value| value.to_str()).unwrap_or("project"),
    }))
}

fn list_directory(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = request.get("location").is_some().then(|| required_location(request, "location")).transpose()?;
    let resolved = if let Some(location) = location {
        resolve_location(&location, current_root, component_root, caller, false, LocationAccess::Read)?
    } else {
        let root = require_project_root(current_root)
            .map_err(|_| protocol_error("目录列表需要项目路径 location 或有效项目上下文"))?;
        let relative = request.get("path").and_then(Value::as_str).unwrap_or("");
        ResolvedLocation { path: resolve_project_path(root, relative, false)?, project_root: Some(root.to_path_buf()) }
    };
    let source = request.get("source").and_then(Value::as_str).unwrap_or("auto");
    let directory = resolved.path;
    if !directory.is_dir() {
        return Err(protocol_error("目录列表目标不是目录"));
    }
    let cached = if source != "disk" && resolved.project_root.is_some() {
        let root = resolved.project_root.as_deref().expect("project root checked");
        crate::tree_cache::get_or_create_project_cache(&root.to_string_lossy())
            .ok()
            .and_then(|cache| cache.get_directory_entries(&directory.to_string_lossy()).ok())
    } else {
        None
    };
    let cache_missing = cached.is_none();
    let entries = if let Some(entries) = cached {
        entries
            .into_iter()
            .map(|entry| json!({
                "path": project_relative(resolved.project_root.as_deref().expect("project cache root"), Path::new(&entry.path)),
                "name": entry.name,
                "kind": if entry.is_dir { "directory" } else { "file" },
                "sizeBytes": entry.size,
                "modifiedAt": entry.modified_ts,
                "extension": entry.extension,
            }))
            .collect::<Vec<_>>()
    } else {
        let mut entries = fs::read_dir(&directory)
            .map_err(|error| io_error("读取目录失败", error))?
            .filter_map(Result::ok)
            .filter_map(|entry| entry_metadata_value(resolved.project_root.as_deref(), &entry.path()).ok())
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left["kind"].as_str().cmp(&right["kind"].as_str())
                .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
        });
        entries
    };
    if source == "disk" {
        if let Some(root) = resolved.project_root.as_deref() {
            if let Ok(cache) = crate::tree_cache::get_or_create_project_cache(&root.to_string_lossy()) {
                if let Ok(scanned) = crate::tree_cache::scan_directory_entries_from_disk(
                    &root.to_string_lossy(),
                    &directory.to_string_lossy(),
                ) {
                    let _ = cache.replace_directory_entries(&directory.to_string_lossy(), &scanned);
                }
            }
        }
    }
    let cursor = request.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = request.get("limit").and_then(Value::as_u64).unwrap_or(200).clamp(1, 500) as usize;
    let page = entries.iter().skip(cursor).take(limit).cloned().collect::<Vec<_>>();
    Ok(json!({
        "entries": page,
        "nextCursor": if cursor + page.len() < entries.len() { Some(cursor + page.len()) } else { None },
        "source": if source == "cache" { "cache" } else if source == "disk" { "disk" } else { "auto" },
        "stale": source == "cache" && cache_missing,
    }))
}

fn stat_entry(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, false, LocationAccess::Read)?;
    entry_metadata_value(resolved.project_root.as_deref(), &resolved.path)
}

fn exists_entry(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, true, LocationAccess::Read)?;
    Ok(json!({"exists": resolved.path.exists()}))
}

fn search_entries(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, false, LocationAccess::Read)?;
    let query = request.get("query").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    let extension = request.get("extension").and_then(Value::as_str).map(|value| value.trim_start_matches('.').to_lowercase());
    let kind = request.get("kind").and_then(Value::as_str);
    let limit = request.get("limit").and_then(Value::as_u64).unwrap_or(200).clamp(1, 500) as usize;
    let root = resolved.path;
    let results = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != root)
        .filter(|entry| {
            let path = entry.path();
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("").to_lowercase();
            let extension_matches = extension.as_ref().is_none_or(|expected| {
                path.extension().and_then(|value| value.to_str()).is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
            });
            let kind_matches = kind.is_none_or(|expected| {
                (expected == "directory" && entry.file_type().is_dir()) || (expected == "file" && entry.file_type().is_file())
            });
            (query.is_empty() || name.contains(&query) || path.to_string_lossy().to_lowercase().contains(&query))
                && extension_matches && kind_matches
        })
        .filter_map(|entry| entry_metadata_value(resolved.project_root.as_deref(), entry.path()).ok())
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({"entries": results, "truncated": results.len() == limit}))
}

fn read_file(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, false, LocationAccess::Read)?;
    if !resolved.path.is_file() { return Err(protocol_error("读取目标不是文件")); }
    let offset = request.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let length = request.get("length").and_then(Value::as_u64).unwrap_or(MAX_CONTENT_BYTES as u64).clamp(1, MAX_CONTENT_BYTES as u64) as usize;
    let mut file = fs::File::open(&resolved.path).map_err(|error| io_error("打开文件失败", error))?;
    file.seek(SeekFrom::Start(offset)).map_err(|error| io_error("定位文件失败", error))?;
    let mut bytes = vec![0; length];
    let count = file.read(&mut bytes).map_err(|error| io_error("读取文件失败", error))?;
    bytes.truncate(count);
    let encoding = request.get("encoding").and_then(Value::as_str).unwrap_or("base64");
    let data = if encoding == "text" {
        String::from_utf8(bytes).map(Value::String).map_err(|_| protocol_error("文件内容不是 UTF-8 文本"))?
    } else {
        Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
    };
    Ok(json!({"data": data, "encoding": encoding, "offset": offset, "bytesRead": count, "eof": count < length}))
}

fn hash_file(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, false, LocationAccess::Read)?;
    let mut file = fs::File::open(&resolved.path).map_err(|error| io_error("打开文件失败", error))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| io_error("读取文件失败", error))?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(json!({"algorithm": "blake3", "hash": hasher.finalize().to_hex().to_string()}))
}

fn write_file(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, true, LocationAccess::Write)?;
    let bytes = decode_request_bytes(request)?;
    if bytes.len() > MAX_CONTENT_BYTES { return Err(protocol_error("单次写入不能超过 4 MiB，请使用 stream.*")); }
    if let Some(expected_hash) = request.get("expectedHash").and_then(Value::as_str) {
        if resolved.path.exists() {
            let actual = hash_path(&resolved.path)?;
            if actual != expected_hash { return Err(protocol_error("文件已被其他操作修改，expectedHash 不匹配")); }
        }
    }
    let mode = request.get("mode").and_then(Value::as_str).unwrap_or("overwrite");
    if mode == "append" {
        if let Some(parent) = resolved.path.parent() { fs::create_dir_all(parent).map_err(|error| io_error("创建父目录失败", error))?; }
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&resolved.path).map_err(|error| io_error("追加文件失败", error))?;
        file.write_all(&bytes).map_err(|error| io_error("写入文件失败", error))?;
    } else {
        atomic_write(&resolved.path, &bytes)?;
    }
    mark_project_dirty(resolved.project_root.as_deref());
    entry_metadata_value(resolved.project_root.as_deref(), &resolved.path)
}

fn create_directory(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, true, LocationAccess::Write)?;
    fs::create_dir_all(&resolved.path).map_err(|error| io_error("创建目录失败", error))?;
    mark_project_dirty(resolved.project_root.as_deref());
    entry_metadata_value(resolved.project_root.as_deref(), &resolved.path)
}

async fn copy_entry(app: &AppHandle, request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let source = resolve_location(&required_location(request, "source")?, current_root, component_root, caller, false, LocationAccess::Read)?;
    let target = resolve_location(&required_location(request, "target")?, current_root, component_root, caller, true, LocationAccess::Write)?;
    let target_root = target.project_root.clone();
    let target = resolve_conflict_target(target.path, request.get("conflict").and_then(Value::as_str).unwrap_or("error"))?;
    if source.path == target { return Err(protocol_error("源和目标不能相同")); }
    crate::fs::copy_path_to_target(app.clone(), source.path.to_string_lossy().to_string(), target.to_string_lossy().to_string(), None).await.map_err(protocol_error)?;
    mark_project_dirty(source.project_root.as_deref());
    mark_project_dirty(target_root.as_deref());
    entry_metadata_value(target_root.as_deref(), &target)
}

async fn move_entry(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let source = resolve_location(&required_location(request, "source")?, current_root, component_root, caller, false, LocationAccess::Write)?;
    let target = resolve_location(&required_location(request, "target")?, current_root, component_root, caller, true, LocationAccess::Write)?;
    let target_root = target.project_root.clone();
    let parent = target.path.parent().ok_or_else(|| protocol_error("移动目标缺少父目录"))?.to_path_buf();
    let name = target.path.file_name().and_then(|value| value.to_str()).map(str::to_string);
    let conflict = request.get("conflict").and_then(Value::as_str).unwrap_or("error");
    let moved = crate::fs::move_path_with_strategy(source.path.clone(), parent, conflict, name).await.map_err(protocol_error)?;
    mark_project_dirty(source.project_root.as_deref());
    mark_project_dirty(target_root.as_deref());
    entry_metadata_value(target_root.as_deref(), &moved)
}

async fn rename_entry(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let source = resolve_location(&required_location(request, "source")?, current_root, component_root, caller, false, LocationAccess::Write)?;
    let name = request.get("name").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| protocol_error("重命名缺少 name"))?;
    if Path::new(name).components().count() != 1 { return Err(protocol_error("文件名不能包含目录分隔符")); }
    let target = source.path.parent().ok_or_else(|| protocol_error("重命名目标无父目录"))?.join(name);
    let target = resolve_conflict_target(target, request.get("conflict").and_then(Value::as_str).unwrap_or("error"))?;
    fs::rename(&source.path, &target).map_err(|error| io_error("重命名失败", error))?;
    mark_project_dirty(source.project_root.as_deref());
    entry_metadata_value(source.project_root.as_deref(), &target)
}

async fn delete_entry(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, false, LocationAccess::Write)?;
    let permanent = request.get("permanent").and_then(Value::as_bool).unwrap_or(false);
    if permanent {
        if resolved.path.is_dir() { tokio::fs::remove_dir_all(&resolved.path).await.map_err(|error| io_error("永久删除目录失败", error))?; }
        else { tokio::fs::remove_file(&resolved.path).await.map_err(|error| io_error("永久删除文件失败", error))?; }
    } else {
        crate::fs::delete_paths(vec![resolved.path.to_string_lossy().to_string()]).await.map_err(protocol_error)?;
    }
    mark_project_dirty(resolved.project_root.as_deref());
    Ok(json!({"deleted": true, "permanent": permanent}))
}

async fn batch_execute(app: &AppHandle, component_root: &Path, request: &Value, current_root: Option<&Path>, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let operations = request.get("operations").and_then(Value::as_array).ok_or_else(|| protocol_error("批量操作缺少 operations"))?;
    if operations.is_empty() || operations.len() > MAX_BATCH_OPERATIONS { return Err(protocol_error("批量操作数量必须在 1 到 100 之间")); }
    let mut results = Vec::new();
    for operation in operations {
        let command = operation.get("command").and_then(Value::as_str).ok_or_else(|| protocol_error("批量项目缺少 command"))?;
        if command == "batch.execute" { return Err(protocol_error("批量操作不能递归调用 batch.execute")); }
        let input = operation.get("input").cloned().unwrap_or(Value::Null);
        match Box::pin(execute_command(app, component_root, command, &input, current_root, caller)).await {
            Ok(output) => results.push(json!({"ok": true, "command": command, "output": output})),
            Err(error) => results.push(json!({"ok": false, "command": command, "error": {"code": format!("{:?}", error.code), "message": error.message}})),
        }
    }
    Ok(json!({"results": results}))
}

fn stream_open_read(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, false, LocationAccess::Read)?;
    let size = fs::metadata(&resolved.path).map_err(|error| io_error("读取文件元数据失败", error))?.len();
    let id = Uuid::new_v4().to_string();
    STREAMS.lock().map_err(|_| protocol_error("流状态不可用"))?.insert(id.clone(), FileStream {
        component_id: caller.into(), mode: "read".into(), path: resolved.path,
        temporary_path: None, project_root: resolved.project_root,
        grant_id: (location.space == "external").then(|| location.grant_id).flatten(),
        access: LocationAccess::Read, offset: 0,
        expires_at: Utc::now().timestamp_millis() + STREAM_TTL_MS,
    });
    Ok(json!({"streamId": id, "sizeBytes": size, "chunkLimit": MAX_STREAM_CHUNK_BYTES}))
}

fn stream_read(request: &Value, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let id = required_string(request, "streamId")?;
    let length = request.get("length").and_then(Value::as_u64).unwrap_or(MAX_STREAM_CHUNK_BYTES as u64).clamp(1, MAX_STREAM_CHUNK_BYTES as u64) as usize;
    let mut streams = STREAMS.lock().map_err(|_| protocol_error("流状态不可用"))?;
    let stream = streams.get_mut(id).ok_or_else(|| protocol_error("读取流不存在或已过期"))?;
    if stream.component_id != caller || stream.mode != "read" { return Err(protocol_error("读取流不属于当前组件")); }
    validate_stream_scope(stream, component_root, caller)?;
    let mut file = fs::File::open(&stream.path).map_err(|error| io_error("打开读取流失败", error))?;
    file.seek(SeekFrom::Start(stream.offset)).map_err(|error| io_error("定位读取流失败", error))?;
    let mut buffer = vec![0; length];
    let count = file.read(&mut buffer).map_err(|error| io_error("读取流失败", error))?;
    buffer.truncate(count);
    stream.offset += count as u64;
    stream.expires_at = Utc::now().timestamp_millis() + STREAM_TTL_MS;
    Ok(json!({"dataBase64": base64::engine::general_purpose::STANDARD.encode(buffer), "bytesRead": count, "offset": stream.offset, "eof": count < length}))
}

fn stream_open_write(request: &Value, current_root: Option<&Path>, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let location = required_location(request, "location")?;
    let resolved = resolve_location(&location, current_root, component_root, caller, true, LocationAccess::Write)?;
    let parent = resolved.path.parent().ok_or_else(|| protocol_error("写入目标缺少父目录"))?;
    fs::create_dir_all(parent).map_err(|error| io_error("创建父目录失败", error))?;
    let temporary = parent.join(format!(".nexora-write-{}.tmp", Uuid::new_v4()));
    fs::File::create(&temporary).map_err(|error| io_error("创建临时文件失败", error))?;
    let id = Uuid::new_v4().to_string();
    STREAMS.lock().map_err(|_| protocol_error("流状态不可用"))?.insert(id.clone(), FileStream {
        component_id: caller.into(), mode: "write".into(), path: resolved.path,
        temporary_path: Some(temporary), project_root: resolved.project_root,
        grant_id: (location.space == "external").then(|| location.grant_id).flatten(),
        access: LocationAccess::Write, offset: 0,
        expires_at: Utc::now().timestamp_millis() + STREAM_TTL_MS,
    });
    Ok(json!({"streamId": id, "chunkLimit": MAX_STREAM_CHUNK_BYTES}))
}

fn stream_write(request: &Value, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let id = required_string(request, "streamId")?;
    let bytes = request.get("dataBase64").and_then(Value::as_str).ok_or_else(|| protocol_error("写入流缺少 dataBase64"))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(bytes).map_err(|_| protocol_error("dataBase64 无效"))?;
    if bytes.len() > MAX_STREAM_CHUNK_BYTES { return Err(protocol_error("流分块不能超过 1 MiB")); }
    let mut streams = STREAMS.lock().map_err(|_| protocol_error("流状态不可用"))?;
    let stream = streams.get_mut(id).ok_or_else(|| protocol_error("写入流不存在或已过期"))?;
    if stream.component_id != caller || stream.mode != "write" { return Err(protocol_error("写入流不属于当前组件")); }
    validate_stream_scope(stream, component_root, caller)?;
    let temporary = stream.temporary_path.as_ref().ok_or_else(|| protocol_error("写入流临时文件缺失"))?;
    let mut file = fs::OpenOptions::new().append(true).open(temporary).map_err(|error| io_error("打开写入流失败", error))?;
    file.write_all(&bytes).map_err(|error| io_error("写入流失败", error))?;
    stream.offset += bytes.len() as u64;
    stream.expires_at = Utc::now().timestamp_millis() + STREAM_TTL_MS;
    Ok(json!({"bytesWritten": bytes.len(), "offset": stream.offset}))
}

fn stream_commit(request: &Value, component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let id = required_string(request, "streamId")?;
    let mut streams = STREAMS.lock().map_err(|_| protocol_error("流状态不可用"))?;
    let pending = streams.get(id).cloned().ok_or_else(|| protocol_error("写入流不存在或已过期"))?;
    if pending.component_id != caller || pending.mode != "write" { return Err(protocol_error("写入流不属于当前组件")); }
    validate_stream_scope(&pending, component_root, caller)?;
    let stream = streams.remove(id).expect("stream remains registered after validation");
    let temporary = stream.temporary_path.ok_or_else(|| protocol_error("写入流临时文件缺失"))?;
    if stream.path.exists() { fs::remove_file(&stream.path).map_err(|error| io_error("替换目标文件失败", error))?; }
    fs::rename(&temporary, &stream.path).map_err(|error| io_error("提交写入流失败", error))?;
    Ok(json!({"committed": true, "sizeBytes": stream.offset}))
}

fn stream_abort(request: &Value, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let id = required_string(request, "streamId")?;
    let stream = STREAMS.lock().map_err(|_| protocol_error("流状态不可用"))?.remove(id).ok_or_else(|| protocol_error("流不存在或已过期"))?;
    if stream.component_id != caller { return Err(protocol_error("流不属于当前组件")); }
    if let Some(temporary) = stream.temporary_path { let _ = fs::remove_file(temporary); }
    Ok(json!({"aborted": true}))
}

fn validate_stream_scope(stream: &FileStream, component_root: &Path, caller: &str) -> Result<(), ComponentRuntimeError> {
    if stream.component_id != caller {
        return Err(protocol_error("流不属于当前组件"));
    }
    if let Some(root) = stream.project_root.as_deref() {
        require_project_root(Some(root))?;
    }
    if let Some(grant_id) = stream.grant_id.as_deref() {
        let grant = find_grant(component_root, grant_id, caller)?;
        ensure_grant_access(&grant, stream.access)?;
    }
    Ok(())
}

fn create_external_grant(component_root: &Path, request: &Value, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let root = PathBuf::from(required_string(request, "rootPath")?);
    if !root.is_absolute() || !root.is_dir() { return Err(protocol_error("外部授权目录必须是存在的绝对目录")); }
    let root = fs::canonicalize(root).map_err(|error| io_error("解析外部授权目录失败", error))?;
    let lifetime = match request.get("lifetime").and_then(Value::as_str).unwrap_or("once") {
        "persistent" => GrantLifetime::Persistent,
        "session" => GrantLifetime::Session,
        "once" => GrantLifetime::Once,
        _ => return Err(protocol_error("外部路径授权 lifetime 只支持 once、session 或 persistent")),
    };
    let access = request.get("access").and_then(Value::as_str).unwrap_or("read-write");
    if !matches!(access, "read" | "write" | "read-write") {
        return Err(protocol_error("外部路径授权 access 只支持 read、write 或 read-write"));
    }
    let grant = ExternalPathGrant {
        id: Uuid::new_v4().to_string(), component_id: caller.into(), root_path: root.to_string_lossy().to_string(),
        access: access.to_string(), lifetime,
        created_at: Utc::now().timestamp_millis(), expires_at: if lifetime == GrantLifetime::Once { Some(Utc::now().timestamp_millis() + 60_000) } else { None },
    };
    match lifetime {
        GrantLifetime::Persistent => { let mut grants = load_persistent_grants(component_root)?; grants.push(grant.clone()); save_persistent_grants(component_root, &grants)?; }
        _ => { SESSION_GRANTS.lock().map_err(|_| protocol_error("授权状态不可用"))?.insert(grant.id.clone(), grant.clone()); }
    }
    serde_json::to_value(grant).map_err(|error| protocol_error(error.to_string()))
}

async fn select_external_directory(
    app_handle: &AppHandle,
    component_root: &Path,
    request: &Value,
    caller: &str,
    force_once: bool,
) -> Result<Value, ComponentRuntimeError> {
    let (sender, receiver) = oneshot::channel();
    let title = request
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("选择要授权给组件的目录")
        .to_string();
    app_handle
        .dialog()
        .file()
        .set_title(title)
        .pick_folder(move |selected| {
            let result = selected
                .map(|path| path.into_path().map_err(|error| error.to_string()))
                .transpose();
            let _ = sender.send(result);
        });
    let selected = receiver
        .await
        .map_err(|_| protocol_error("外部目录选择器未返回结果"))?
        .map_err(protocol_error)?
        .ok_or_else(|| protocol_error("已取消外部目录选择"))?;
    let mut grant_request = request.clone();
    let object = grant_request
        .as_object_mut()
        .ok_or_else(|| protocol_error("外部目录选择请求必须是对象"))?;
    object.insert("rootPath".into(), Value::String(selected.to_string_lossy().into_owned()));
    if force_once {
        object.insert("lifetime".into(), Value::String("once".into()));
    }
    create_external_grant(component_root, &grant_request, caller)
}

fn list_external_grants(component_root: &Path, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let now = Utc::now().timestamp_millis();
    let mut persistent = load_persistent_grants(component_root)?;
    persistent.retain(|grant| grant.expires_at.is_none_or(|expires| expires > now));
    save_persistent_grants(component_root, &persistent)?;
    let mut grants = persistent;
    grants.extend(SESSION_GRANTS.lock().map_err(|_| protocol_error("授权状态不可用"))?.values().filter(|grant| grant.component_id == caller).cloned());
    grants.retain(|grant| grant.component_id == caller && grant.expires_at.is_none_or(|expires| expires > now));
    Ok(json!({"grants": grants}))
}

fn revoke_external_grant(component_root: &Path, request: &Value, caller: &str) -> Result<Value, ComponentRuntimeError> {
    let id = required_string(request, "grantId")?;
    SESSION_GRANTS.lock().map_err(|_| protocol_error("授权状态不可用"))?.remove(id);
    let mut grants = load_persistent_grants(component_root)?;
    let before = grants.len();
    grants.retain(|grant| !(grant.id == id && grant.component_id == caller));
    if grants.len() != before { save_persistent_grants(component_root, &grants)?; }
    // A grant revocation also invalidates any active read/write handles that
    // were opened under it. Write streams lose their temporary file here.
    release_grant_streams(id);
    Ok(json!({"revoked": true}))
}

fn cache_status(current_root: Option<&Path>) -> Result<Value, ComponentRuntimeError> {
    let root = require_project_root(current_root)?;
    let cache = crate::tree_cache::get_or_create_project_cache(&root.to_string_lossy()).map_err(protocol_error)?;
    Ok(json!({"projectId": project_id(root), "dirtyDirectories": cache.count_dirty_dirs().map_err(protocol_error)?, "registered": crate::tree_cache::project_cache_count()}))
}

fn cache_query(request: &Value, current_root: Option<&Path>) -> Result<Value, ComponentRuntimeError> {
    let root = require_project_root(current_root)?;
    let relative = request.get("path").and_then(Value::as_str).unwrap_or("");
    let dir = resolve_project_path(root, relative, false)?;
    let cache = crate::tree_cache::get_or_create_project_cache(&root.to_string_lossy()).map_err(protocol_error)?;
    let entries = cache.get_directory_entries(&dir.to_string_lossy()).map_err(protocol_error)?;
    Ok(json!({"entries": entries.into_iter().map(|entry| json!({"path": project_relative(root, Path::new(&entry.path)), "name": entry.name, "kind": if entry.is_dir { "directory" } else { "file" }, "sizeBytes": entry.size})).collect::<Vec<_>>()}))
}

fn cache_invalidate(current_root: Option<&Path>) -> Result<Value, ComponentRuntimeError> {
    let root = require_project_root(current_root)?;
    crate::tree_cache::mark_project_tree_dirty(&root.to_string_lossy());
    Ok(json!({"invalidated": true}))
}

fn cache_refresh(request: &Value, current_root: Option<&Path>) -> Result<Value, ComponentRuntimeError> {
    let root = require_project_root(current_root)?;
    let relative = request.get("path").and_then(Value::as_str).unwrap_or("");
    let dir = resolve_project_path(root, relative, false)?;
    let cache = crate::tree_cache::get_or_create_project_cache(&root.to_string_lossy()).map_err(protocol_error)?;
    let entries = crate::tree_cache::scan_directory_entries_from_disk(&root.to_string_lossy(), &dir.to_string_lossy()).map_err(protocol_error)?;
    cache.replace_directory_entries(&dir.to_string_lossy(), &entries).map_err(protocol_error)?;
    Ok(json!({"refreshed": true, "entryCount": entries.len()}))
}

fn cache_rebuild(current_root: Option<&Path>) -> Result<Value, ComponentRuntimeError> {
    let root = require_project_root(current_root)?;
    crate::tree_cache::rebuild_project_tree_cache(&root.to_string_lossy()).map_err(protocol_error)?;
    Ok(json!({"rebuilt": true}))
}

fn watcher_status(current_root: Option<&Path>) -> Result<Value, ComponentRuntimeError> {
    let root = require_project_root(current_root)?;
    Ok(json!({"projectId": project_id(root), "activeProject": crate::watcher::get_active_project_path(), "watcherActive": crate::watcher::get_active_project_path().as_deref() == Some(&root.to_string_lossy())}))
}

fn required_location(request: &Value, key: &str) -> Result<FileLocation, ComponentRuntimeError> {
    serde_json::from_value(request.get(key).cloned().ok_or_else(|| protocol_error(format!("缺少 {key}")))?).map_err(|error| protocol_error(format!("{key} 无效: {error}")))
}

fn required_string<'a>(request: &'a Value, key: &str) -> Result<&'a str, ComponentRuntimeError> { request.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| protocol_error(format!("缺少 {key}"))) }

fn resolve_location(location: &FileLocation, current_root: Option<&Path>, component_root: &Path, caller: &str, allow_missing: bool, access: LocationAccess) -> Result<ResolvedLocation, ComponentRuntimeError> {
    match location.space.as_str() {
        "project" => {
            let root = resolve_project_context_root(current_root, location.project_id.as_deref())?;
            let path = resolve_project_path(&root, &location.path, allow_missing)?;
            Ok(ResolvedLocation { path, project_root: Some(root) })
        }
        "external" => {
            let grant_id = location.grant_id.as_deref().ok_or_else(|| protocol_error("外部路径缺少 grantId"))?;
            let grant = find_grant(component_root, grant_id, caller)?;
            ensure_grant_access(&grant, access)?;
            let path = resolve_external_path(&grant, &location.path, allow_missing)?;
            Ok(ResolvedLocation { path, project_root: None })
        }
        _ => Err(protocol_error("location.space 只支持 project 或 external")),
    }
}

fn resolve_project_context_root(
    current_root: Option<&Path>,
    requested_project_id: Option<&str>,
) -> Result<PathBuf, ComponentRuntimeError> {
    let current = current_root.map(Path::to_path_buf);
    if let Some(project_id) = requested_project_id {
        if current.as_deref().is_some_and(|root| project_id_for(root) == project_id) {
            return current
                .filter(|root| root.is_dir())
                .ok_or_else(|| protocol_error("项目目录已关闭或不可访问"));
        }
        let registered = crate::project_resources::registrations()
            .into_iter()
            .map(|registration| PathBuf::from(registration.project_path))
            .find(|root| project_id_for(root) == project_id)
            .ok_or_else(|| protocol_error("指定项目未打开或资源已经释放"))?;
        require_project_root(Some(&registered))?;
        return Ok(registered);
    }
    let root = require_project_root(current.as_deref())?;
    Ok(root.to_path_buf())
}

fn ensure_grant_access(grant: &ExternalPathGrant, requested: LocationAccess) -> Result<(), ComponentRuntimeError> {
    let permitted = match (grant.access.as_str(), requested) {
        ("read-write", _) | ("write-read", _) => true,
        ("read", LocationAccess::Read) | ("write", LocationAccess::Write) => true,
        _ => false,
    };
    if permitted {
        Ok(())
    } else {
        let action = match requested { LocationAccess::Read => "读取", LocationAccess::Write => "写入" };
        Err(protocol_error(format!("外部路径授权未允许{action}")))
    }
}

fn require_project_root(root: Option<&Path>) -> Result<&Path, ComponentRuntimeError> {
    let root = root.ok_or_else(|| protocol_error("此操作需要有效项目上下文"))?;
    if !root.is_dir() { return Err(protocol_error("项目目录已关闭或不可访问")); }
    crate::project_resources::ensure_project_access(&root.to_string_lossy()).map_err(protocol_error)?;
    Ok(root)
}

fn resolve_project_path(root: &Path, relative: &str, allow_missing: bool) -> Result<PathBuf, ComponentRuntimeError> {
    let relative = Path::new(relative);
    if relative.is_absolute() || relative.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))) { return Err(protocol_error("项目路径必须是安全相对路径")); }
    if relative.components().any(|component| component.as_os_str().eq_ignore_ascii_case(".pm_center")) { return Err(protocol_error("组件不能直接访问 .pm_center")); }
    resolve_contained(root, &root.join(relative), allow_missing)
}

fn resolve_external_path(grant: &ExternalPathGrant, raw: &str, allow_missing: bool) -> Result<PathBuf, ComponentRuntimeError> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() { return Err(protocol_error("外部路径必须是绝对路径")); }
    let root = PathBuf::from(&grant.root_path);
    resolve_contained(&root, &path, allow_missing)
}

fn resolve_contained(root: &Path, path: &Path, allow_missing: bool) -> Result<PathBuf, ComponentRuntimeError> {
    let canonical_root = fs::canonicalize(root).map_err(|error| io_error("解析授权根目录失败", error))?;
    let candidate = if path.exists() { fs::canonicalize(path).map_err(|error| io_error("解析路径失败", error))? } else if allow_missing {
        let mut ancestor = path;
        while !ancestor.exists() { ancestor = ancestor.parent().ok_or_else(|| protocol_error("路径没有有效父目录"))?; }
        let canonical_ancestor = fs::canonicalize(ancestor).map_err(|error| io_error("解析父目录失败", error))?;
        let suffix = path.strip_prefix(ancestor).map_err(|_| protocol_error("路径解析失败"))?;
        canonical_ancestor.join(suffix)
    } else { return Err(protocol_error("路径不存在")); };
    if !candidate.starts_with(&canonical_root) { return Err(protocol_error("路径超出项目或外部授权范围")); }
    Ok(candidate)
}

fn find_grant(component_root: &Path, id: &str, caller: &str) -> Result<ExternalPathGrant, ComponentRuntimeError> {
    let now = Utc::now().timestamp_millis();
    if let Some(grant) = SESSION_GRANTS.lock().map_err(|_| protocol_error("授权状态不可用"))?.get(id).cloned() {
        if grant.component_id == caller && grant.expires_at.is_none_or(|expires| expires > now) { return Ok(grant); }
    }
    load_persistent_grants(component_root)?.into_iter().find(|grant| grant.id == id && grant.component_id == caller).ok_or_else(|| protocol_error("外部路径授权不存在、已过期或不属于当前组件"))
}

fn load_persistent_grants(root: &Path) -> Result<Vec<ExternalPathGrant>, ComponentRuntimeError> {
    let path = root.join("file-operation-grants.json");
    if !path.exists() { return Ok(Vec::new()); }
    serde_json::from_slice(&fs::read(path).map_err(|error| io_error("读取外部路径授权失败", error))?).map_err(|error| protocol_error(format!("外部路径授权格式无效: {error}")))
}

fn save_persistent_grants(root: &Path, grants: &[ExternalPathGrant]) -> Result<(), ComponentRuntimeError> {
    fs::create_dir_all(root).map_err(|error| io_error("创建授权目录失败", error))?;
    atomic_write(&root.join("file-operation-grants.json"), &serde_json::to_vec_pretty(grants).map_err(|error| protocol_error(error.to_string()))?)
}

fn decode_request_bytes(request: &Value) -> Result<Vec<u8>, ComponentRuntimeError> {
    let encoding = request.get("encoding").and_then(Value::as_str).unwrap_or("base64");
    let data = request.get("data").and_then(Value::as_str).ok_or_else(|| protocol_error("写入操作缺少 data"))?;
    if encoding == "text" { Ok(data.as_bytes().to_vec()) } else { base64::engine::general_purpose::STANDARD.decode(data).map_err(|_| protocol_error("data 不是有效 Base64")) }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ComponentRuntimeError> {
    let parent = path.parent().ok_or_else(|| protocol_error("写入目标缺少父目录"))?;
    fs::create_dir_all(parent).map_err(|error| io_error("创建父目录失败", error))?;
    let temporary = parent.join(format!(".nexora-write-{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<(), ComponentRuntimeError> {
        let mut file = fs::File::create(&temporary).map_err(|error| io_error("创建临时文件失败", error))?;
        file.write_all(bytes).map_err(|error| io_error("写入临时文件失败", error))?;
        file.sync_all().map_err(|error| io_error("同步临时文件失败", error))?;
        if path.exists() { fs::remove_file(path).map_err(|error| io_error("替换目标文件失败", error))?; }
        fs::rename(&temporary, path).map_err(|error| io_error("提交文件失败", error))?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

fn resolve_conflict_target(target: PathBuf, conflict: &str) -> Result<PathBuf, ComponentRuntimeError> {
    if !target.exists() { return Ok(target); }
    match conflict {
        "overwrite" => { if target.is_dir() { fs::remove_dir_all(&target).map_err(|error| io_error("覆盖目标目录失败", error))?; } else { fs::remove_file(&target).map_err(|error| io_error("覆盖目标文件失败", error))?; } Ok(target) }
        "rename" => { let parent = target.parent().ok_or_else(|| protocol_error("冲突目标缺少父目录"))?.to_path_buf(); let name = target.file_name().and_then(|value| value.to_str()).unwrap_or("item"); let stem = target.file_stem().and_then(|value| value.to_str()).unwrap_or(name); let extension = target.extension().and_then(|value| value.to_str()); for index in 1..10000 { let candidate = parent.join(if let Some(extension) = extension { format!("{stem} ({index}).{extension}") } else { format!("{name} ({index})") }); if !candidate.exists() { return Ok(candidate); } } Err(protocol_error("无法生成不冲突的文件名")) }
        "skip" => Err(protocol_error("目标已存在，按 skip 策略跳过")),
        _ => Err(protocol_error("目标已存在")),
    }
}

fn entry_metadata_value(project_root: Option<&Path>, path: &Path) -> Result<Value, ComponentRuntimeError> {
    let metadata = fs::metadata(path).map_err(|error| io_error("读取文件元数据失败", error))?;
    Ok(json!({"path": project_root.map(|root| project_relative(root, path)).unwrap_or_else(|| path.to_string_lossy().to_string()), "name": path.file_name().and_then(|value| value.to_str()).unwrap_or(""), "kind": if metadata.is_dir() { "directory" } else { "file" }, "sizeBytes": metadata.len(), "extension": path.extension().and_then(|value| value.to_str()), "modifiedAt": metadata.modified().ok().and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok()).map(|value| value.as_secs() as i64)}))
}

fn project_relative(root: &Path, path: &Path) -> String { path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/") }
fn project_id(root: &Path) -> String { project_id_for(root) }
fn project_id_for(root: &Path) -> String { blake3::hash(root.to_string_lossy().replace('\\', "/").to_lowercase().as_bytes()).to_hex()[..16].to_string() }
fn hash_path(path: &Path) -> Result<String, ComponentRuntimeError> { let mut file = fs::File::open(path).map_err(|error| io_error("读取文件失败", error))?; let mut hasher = blake3::Hasher::new(); let mut buffer = [0u8; 64 * 1024]; loop { let count = file.read(&mut buffer).map_err(|error| io_error("读取文件失败", error))?; if count == 0 { break; } hasher.update(&buffer[..count]); } Ok(hasher.finalize().to_hex().to_string()) }
fn mark_project_dirty(root: Option<&Path>) { if let Some(root) = root { crate::tree_cache::mark_project_tree_dirty(&root.to_string_lossy()); } }
fn purge_expired_streams() { let now = Utc::now().timestamp_millis(); if let Ok(mut streams) = STREAMS.lock() { let expired = streams.iter().filter(|(_, stream)| stream.expires_at <= now).map(|(id, stream)| (id.clone(), stream.temporary_path.clone())).collect::<Vec<_>>(); for (id, temporary) in expired { streams.remove(&id); if let Some(temporary) = temporary { let _ = fs::remove_file(temporary); } } } }
fn protocol_error(message: impl Into<String>) -> ComponentRuntimeError { ComponentRuntimeError::new(ComponentRuntimeErrorCode::ComponentProtocolError, message) }
fn io_error(label: &str, error: std::io::Error) -> ComponentRuntimeError { ComponentRuntimeError::new(ComponentRuntimeErrorCode::ComponentIoError, format!("{label}: {error}")) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_project_path_traversal() {
        let root = std::env::temp_dir().join(format!("nexora-file-ops-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create test root");
        let result = resolve_project_path(&root, "../outside.txt", true);
        let _ = fs::remove_dir_all(&root);
        assert!(result.is_err());
    }

    #[test]
    fn external_grants_enforce_their_access_mode() {
        let grant = ExternalPathGrant {
            id: "grant".into(),
            component_id: "com.example.component".into(),
            root_path: r"D:\\Media".into(),
            access: "read".into(),
            lifetime: GrantLifetime::Session,
            created_at: 0,
            expires_at: None,
        };
        assert!(ensure_grant_access(&grant, LocationAccess::Read).is_ok());
        assert!(ensure_grant_access(&grant, LocationAccess::Write).is_err());
    }

    #[test]
    fn derives_cross_space_copy_capabilities() {
        let request = json!({
            "source": {"space": "project", "path": "assets/source.png"},
            "target": {"space": "external", "grantId": "grant", "path": r"D:\\Media\\source.png"}
        });
        assert_eq!(
            required_capabilities_for_command("entry.copy", &request).expect("derive capabilities"),
            vec![Capability::ProjectFilesRead, Capability::FilesystemExternalWrite],
        );
    }
}
