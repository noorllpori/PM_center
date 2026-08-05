use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Runtime,
};

mod automation_runtime;
mod cache_manager;
mod db;
mod file_details;
mod fs;
mod icon_extractor;
mod link_preview;
mod local_web_console;
mod p2p;
mod platform;
mod plugin;
mod process_utils;
mod project_resources;
mod python;
mod python_env;
mod render_center;
mod smart_clipboard;
mod task;
mod thumbnail_cache;
mod tools;
mod tree_cache;
mod watcher;

use db::FileChange;
use db::{
    Collection, CollectionMemberRemoval, CollectionMemberUpdate, Database, FileMetadata, Tag,
};
use file_details::{
    get_blender_external_data, get_blender_preview_png, get_file_details,
    update_blender_scene_render,
};
use p2p::{
    cancel_lan_transfer, cancel_server_transfer, clear_lan_conversation, clear_lan_history,
    connect_pmc_server, create_lan_transfer_staging_path, discard_lan_transfer_staging_file,
    disconnect_pmc_server, get_lan_collaboration_snapshot, init_p2p, initialize_lan_collaboration,
    mark_lan_conversation_read, offer_lan_files, offer_server_files, remove_lan_contact,
    respond_lan_transfer, respond_server_transfer, send_lan_message, send_p2p_message,
    send_server_message, set_lan_avatar, start_lan_discovery, start_p2p_discovery,
    stop_lan_discovery, stop_p2p_discovery, test_pmc_server_speed, update_lan_profile,
    update_lan_receive_directory, update_p2p_user, update_pmc_server_settings,
};
use plugin::{
    get_plugin_dirs, inspect_plugin_dependencies, install_plugin_dependencies, list_plugins,
    refresh_plugins, remove_plugin_dependencies, reset_plugin_settings, run_plugin_action,
    set_plugin_enabled, update_plugin_settings, validate_plugin,
};
use process_utils::std_command;
use python::{
    detect_python_envs, get_blender_file_info, pip_install, run_python_file, run_python_script,
};
use python_env::{
    create_venv, delete_venv, detect_system_python, pip_install_package, pip_list_packages,
    pip_uninstall_package, scan_app_venvs,
};
use tauri_plugin_global_shortcut::ShortcutState;
use tools::{inspect_blender_executable, inspect_tool_paths, scan_blender_installations};

type DbState = project_resources::ProjectDatabaseState;

async fn get_or_create_db(
    db_state: &tauri::State<'_, DbState>,
    project_path: &str,
) -> Result<Database, String> {
    project_resources::get_or_create_database(db_state.inner(), project_path).await
}

fn ensure_project_support_files(project_path: &str) -> Result<(), String> {
    use std::fs;

    let pm_center_dir = PathBuf::from(project_path).join(".pm_center");
    let scripts_dir = pm_center_dir.join("scripts");
    let plugins_dir = pm_center_dir.join("plugins");
    let thumbnails_dir = pm_center_dir.join("thumbnails");
    fs::create_dir_all(&scripts_dir).map_err(|e| format!("创建 scripts 目录失败: {}", e))?;
    fs::create_dir_all(&plugins_dir).map_err(|e| format!("创建 plugins 目录失败: {}", e))?;
    fs::create_dir_all(&thumbnails_dir)
        .map_err(|e| format!("鍒涘缓 thumbnails 鐩綍澶辫触: {}", e))?;
    ensure_project_default_scripts(&scripts_dir)?;

    Ok(())
}

#[tauri::command]
async fn init_project(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    exclude_patterns: Option<Vec<String>>,
) -> Result<(), String> {
    project_resources::wait_for_module_enabled(std::time::Duration::from_secs(15)).await?;
    ensure_project_support_files(&project_path)?;
    render_center::init_project_storage(&project_path)?;
    open_project_resources(
        db_state.inner(),
        &project_path,
        exclude_patterns.as_deref().unwrap_or(&[]),
    )
    .await
}

#[tauri::command]
async fn activate_project(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    exclude_patterns: Option<Vec<String>>,
) -> Result<(), String> {
    project_resources::wait_for_module_enabled(std::time::Duration::from_secs(15)).await?;
    render_center::init_project_storage(&project_path)?;
    open_project_resources(
        db_state.inner(),
        &project_path,
        exclude_patterns.as_deref().unwrap_or(&[]),
    )
    .await
}

async fn open_project_resources(
    db_state: &DbState,
    project_path: &str,
    exclude_patterns: &[String],
) -> Result<(), String> {
    if !PathBuf::from(project_path).is_dir() {
        return Err("项目目录不存在或不可访问".into());
    }

    let already_registered = project_resources::register_project(project_path, exclude_patterns)?;
    let result = async {
        let db = project_resources::get_or_create_database(db_state, project_path).await?;
        let _ = tree_cache::get_or_create_project_cache(project_path)?;
        project_resources::clear_active_project();
        watcher::set_active_project(project_path, &db, exclude_patterns)?;
        project_resources::mark_active_project(project_path)?;
        Ok(())
    }
    .await;

    if result.is_err() && !already_registered {
        project_resources::unregister_project(project_path);
        let _ = project_resources::release_project_handles(db_state, project_path).await;
    }
    result
}

/// Releases process-local resources after an outer project tab is closed.
/// Project data is kept on disk; reopening the project creates fresh handles.
#[tauri::command]
async fn release_project_resources(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
) -> Result<(), String> {
    // Revoke the project lease before dropping handles so late async work cannot
    // recreate the SQLite connections after the outer project tab has closed.
    project_resources::unregister_project(&project_path);
    project_resources::release_project_handles(db_state.inner(), &project_path).await
}

fn ensure_project_default_scripts(scripts_dir: &std::path::Path) -> Result<(), String> {
    let showcase_script = r#"# -*- coding: utf-8 -*-
# @name: 项目任务示例（完整功能）
# @desc: 演示项目任务脚本的常用能力：工作目录、文件扫描、进度输出和结果汇总

import os
import sys
from collections import Counter

project_dir = os.getcwd()
print(f"Project directory: {project_dir}", flush=True)
print("progress /***5*/", flush=True)

all_files = []
extensions = Counter()

for root, dirs, files in os.walk(project_dir):
    dirs[:] = [name for name in dirs if name != '.pm_center']

    for filename in files:
        all_files.append(os.path.join(root, filename))
        ext = os.path.splitext(filename)[1].lower() or '(no ext)'
        extensions[ext] += 1

    if all_files and len(all_files) % 100 == 0:
        progress = min(80, 5 + len(all_files) // 20)
        print(f"progress /***{progress}*/", flush=True)

total_size = 0
for path in all_files:
    try:
        total_size += os.path.getsize(path)
    except OSError:
        pass

print(f"Scanned files: {len(all_files)}", flush=True)
print(f"Total size: {total_size} bytes", flush=True)
print("Top file types:", flush=True)
for ext, count in extensions.most_common(8):
    print(f"  {ext}: {count}", flush=True)

print("progress /***100*/", flush=True)
print("Showcase script completed.", flush=True)
sys.exit(0)
"#;

    write_file_if_missing(
        &scripts_dir.join("project_showcase_task.py"),
        showcase_script,
        "项目默认脚本",
    )
}

fn ensure_global_task_scripts(scripts_dir: &std::path::Path) -> Result<(), String> {
    let cleanup_blender_backups = r#"# -*- coding: utf-8 -*-
# @name: 清理 Blender 备份文件
# @desc: 查找项目下所有 .blend1/.blend2/... 备份文件，统计大小、输出位置并删除

import os
import re
import sys

BACKUP_PATTERN = re.compile(r".*\.blend\d+$", re.IGNORECASE)


def format_size(size):
    units = ["B", "KB", "MB", "GB", "TB"]
    value = float(size)
    index = 0
    while value >= 1024 and index < len(units) - 1:
        value /= 1024
        index += 1
    return f"{value:.1f} {units[index]}"


project_dir = os.getcwd()
print(f"Project directory: {project_dir}", flush=True)
print("Scanning Blender backup files...", flush=True)
print("progress /***5*/", flush=True)

matches = []

for root, dirs, files in os.walk(project_dir):
    dirs[:] = [name for name in dirs if name != '.pm_center']

    for filename in files:
        if not BACKUP_PATTERN.match(filename):
            continue

        path = os.path.join(root, filename)
        try:
            size = os.path.getsize(path)
        except OSError:
            size = 0
        matches.append((path, size))

if not matches:
    print("No Blender backup files found.", flush=True)
    print("progress /***100*/", flush=True)
    sys.exit(0)

matches.sort(key=lambda item: item[0].lower())
total_size = sum(size for _, size in matches)

print(f"Found {len(matches)} backup files.", flush=True)
print(f"Total size: {format_size(total_size)} ({total_size} bytes)", flush=True)
print("Backup file list:", flush=True)

for index, (path, size) in enumerate(matches, 1):
    print(f"  [{index}/{len(matches)}] {format_size(size)}  {path}", flush=True)

print("progress /***50*/", flush=True)
print("Deleting backup files...", flush=True)

deleted = 0
for index, (path, _) in enumerate(matches, 1):
    try:
        os.remove(path)
        deleted += 1
        print(f"Deleted: {path}", flush=True)
    except OSError as exc:
        print(f"Failed: {path} -> {exc}", flush=True)

    progress = 50 + int(index / len(matches) * 50)
    print(f"progress /***{progress}*/", flush=True)

print(f"Cleanup finished. Deleted {deleted}/{len(matches)} files.", flush=True)
sys.exit(0)
"#;

    let scan_project_files = r#"# -*- coding: utf-8 -*-
# @name: 统计项目文件类型
# @desc: 统计项目目录中的文件总数、大小和主要扩展名分布

import os
import sys
from collections import Counter

project_dir = os.getcwd()
print(f"Project directory: {project_dir}", flush=True)
print("progress /***5*/", flush=True)

counter = Counter()
total_files = 0
total_size = 0

for root, dirs, files in os.walk(project_dir):
    dirs[:] = [name for name in dirs if name != '.pm_center']

    for filename in files:
        path = os.path.join(root, filename)
        total_files += 1
        counter[os.path.splitext(filename)[1].lower() or '(no ext)'] += 1
        try:
            total_size += os.path.getsize(path)
        except OSError:
            pass

    if total_files and total_files % 100 == 0:
        progress = min(85, 5 + total_files // 20)
        print(f"progress /***{progress}*/", flush=True)

print(f"Total files: {total_files}", flush=True)
print(f"Total size: {total_size} bytes", flush=True)
print("Top file types:", flush=True)
for ext, count in counter.most_common(10):
    print(f"  {ext}: {count}", flush=True)

print("progress /***100*/", flush=True)
print("Statistics completed.", flush=True)
sys.exit(0)
"#;

    write_file_if_missing(
        &scripts_dir.join("cleanup_blender_backups.py"),
        cleanup_blender_backups,
        "全局脚本",
    )?;
    write_file_if_missing(
        &scripts_dir.join("scan_project_file_types.py"),
        scan_project_files,
        "全局脚本",
    )?;

    Ok(())
}

fn write_file_if_missing(path: &std::path::Path, content: &str, label: &str) -> Result<(), String> {
    use std::fs;

    if path.exists() {
        return Ok(());
    }

    fs::write(path, content).map_err(|e| format!("创建{}失败: {}", label, e))
}

fn get_global_task_scripts_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    Ok(app_data_dir.join("task_scripts"))
}

#[tauri::command]
async fn get_global_task_scripts_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    use std::fs;

    automation_runtime::wait_until_running().await?;
    let scripts_dir = get_global_task_scripts_dir(&app_handle)?;
    fs::create_dir_all(&scripts_dir).map_err(|e| format!("创建全局脚本目录失败: {}", e))?;
    ensure_global_task_scripts(&scripts_dir)?;

    Ok(scripts_dir.to_string_lossy().to_string())
}

// 脚本信息
#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ScriptInfo {
    id: String,
    name: String,
    description: String,
    filename: String,
    path: String,
    script_type: String,
    scope: String,
}

#[tauri::command]
async fn get_project_scripts(
    app_handle: tauri::AppHandle,
    project_path: String,
) -> Result<Vec<ScriptInfo>, String> {
    use std::fs;
    use std::path::PathBuf;

    automation_runtime::wait_until_running().await?;
    let project_scripts_dir = PathBuf::from(&project_path)
        .join(".pm_center")
        .join("scripts");
    fs::create_dir_all(&project_scripts_dir).map_err(|e| format!("创建项目脚本目录失败: {}", e))?;
    ensure_project_default_scripts(&project_scripts_dir)?;

    let global_scripts_dir = get_global_task_scripts_dir(&app_handle)?;
    fs::create_dir_all(&global_scripts_dir).map_err(|e| format!("创建全局脚本目录失败: {}", e))?;
    ensure_global_task_scripts(&global_scripts_dir)?;

    let mut scripts = vec![];

    fn scan_dir(
        dir: &std::path::Path,
        base_dir: &std::path::Path,
        scope: &str,
        scripts: &mut Vec<ScriptInfo>,
    ) -> Result<(), String> {
        let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                scan_dir(&path, base_dir, scope, scripts)?;
            } else {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if !filename.ends_with(".py") {
                    continue;
                }
                let script_type = "python";

                let content = fs::read_to_string(&path).unwrap_or_default();
                let (name, description) = parse_script_metadata(&content, filename);

                let relative_path = path
                    .strip_prefix(base_dir)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| filename.to_string());

                scripts.push(ScriptInfo {
                    id: format!("{}:{}", scope, relative_path),
                    name,
                    description,
                    filename: filename.to_string(),
                    path: path.to_string_lossy().to_string(),
                    script_type: script_type.to_string(),
                    scope: scope.to_string(),
                });
            }
        }

        Ok(())
    }

    scan_dir(
        &project_scripts_dir,
        &project_scripts_dir,
        "project",
        &mut scripts,
    )?;
    scan_dir(
        &global_scripts_dir,
        &global_scripts_dir,
        "global",
        &mut scripts,
    )?;

    scripts.sort_by(|a, b| {
        let scope_rank = |scope: &str| if scope == "project" { 0 } else { 1 };
        scope_rank(&a.scope)
            .cmp(&scope_rank(&b.scope))
            .then(a.name.cmp(&b.name))
    });

    Ok(scripts)
}

// 解析脚本元数据
fn parse_script_metadata(content: &str, filename: &str) -> (String, String) {
    let mut name = None;
    let mut description = None;

    for line in content.lines().take(20) {
        let line = line.trim();

        // Python 注释: # @name: 或 # @desc:
        if line.starts_with("# @name:") {
            name = Some(line[8..].trim().to_string());
        } else if line.starts_with("# @desc:") {
            description = Some(line[8..].trim().to_string());
        }
    }

    // 如果没有找到元数据，使用文件名（去掉扩展名）
    let name = name.unwrap_or_else(|| {
        filename
            .rfind('.')
            .map(|i| &filename[..i])
            .unwrap_or(filename)
            .to_string()
    });

    let description = description.unwrap_or_else(|| "Python 脚本".to_string());

    (name, description)
}

// 扫描到的项目信息
#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ScannedProject {
    path: String,
    name: String,
    has_pm_center: bool,
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectLocationIssue {
    code: String,
    severity: String,
    message: String,
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectLocationReport {
    project_path: String,
    pm_center_path: String,
    status: String,
    missing_items: Vec<String>,
    issues: Vec<ProjectLocationIssue>,
    can_initialize: bool,
    can_repair: bool,
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectLocationCandidate {
    path: String,
    name: String,
    match_reason: String,
}

fn project_location_issue(
    code: &str,
    severity: &str,
    message: impl Into<String>,
) -> ProjectLocationIssue {
    ProjectLocationIssue {
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.into(),
    }
}

fn inspect_sqlite_database(path: &std::path::Path, required_tables: &[&str]) -> Result<(), String> {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    let quick_check: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(format!("SQLite quick_check: {quick_check}"));
    }

    for table in required_tables {
        let exists = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                [*table],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
            return Err(format!("缺少必要数据表: {table}"));
        }
    }

    Ok(())
}

#[tauri::command]
async fn inspect_project_location(project_path: String) -> Result<ProjectLocationReport, String> {
    use std::fs;
    use std::path::PathBuf;

    let project_root = PathBuf::from(&project_path);
    let pm_center_path = project_root.join(".pm_center");
    let mut issues = Vec::new();
    let mut missing_items = Vec::new();

    if !project_root.exists() {
        issues.push(project_location_issue(
            "project-directory-missing",
            "error",
            "项目目录不存在，可能已移动、改名或所在磁盘尚未连接。",
        ));
        return Ok(ProjectLocationReport {
            project_path,
            pm_center_path: pm_center_path.to_string_lossy().to_string(),
            status: "missingDirectory".to_string(),
            missing_items,
            issues,
            can_initialize: false,
            can_repair: false,
        });
    }

    if !project_root.is_dir() {
        issues.push(project_location_issue(
            "project-path-not-directory",
            "error",
            "指定位置不是一个目录，不能作为项目根目录打开。",
        ));
        return Ok(ProjectLocationReport {
            project_path,
            pm_center_path: pm_center_path.to_string_lossy().to_string(),
            status: "notDirectory".to_string(),
            missing_items,
            issues,
            can_initialize: false,
            can_repair: false,
        });
    }

    if let Err(error) = fs::read_dir(&project_root) {
        issues.push(project_location_issue(
            "project-directory-unreadable",
            "error",
            format!("项目目录无法读取: {error}"),
        ));
        return Ok(ProjectLocationReport {
            project_path,
            pm_center_path: pm_center_path.to_string_lossy().to_string(),
            status: "unreadable".to_string(),
            missing_items,
            issues,
            can_initialize: false,
            can_repair: false,
        });
    }

    if !pm_center_path.exists() {
        missing_items.push(".pm_center".to_string());
        issues.push(project_location_issue(
            "pm-center-missing",
            "warning",
            "此目录尚未初始化为 PMC 项目。初始化只会创建 .pm_center，不会改动现有项目文件。",
        ));
        return Ok(ProjectLocationReport {
            project_path,
            pm_center_path: pm_center_path.to_string_lossy().to_string(),
            status: "missingPmCenter".to_string(),
            missing_items,
            issues,
            can_initialize: true,
            can_repair: false,
        });
    }

    if !pm_center_path.is_dir() {
        issues.push(project_location_issue(
            "pm-center-not-directory",
            "error",
            ".pm_center 应为目录，但当前位置是普通文件。请先备份并手动处理。",
        ));
        return Ok(ProjectLocationReport {
            project_path,
            pm_center_path: pm_center_path.to_string_lossy().to_string(),
            status: "invalidPmCenter".to_string(),
            missing_items,
            issues,
            can_initialize: false,
            can_repair: false,
        });
    }

    let data_db = pm_center_path.join("data.db");
    let tree_cache_db = pm_center_path.join("tree_cache.db");
    let support_dirs = ["thumbnails", "scripts", "plugins"];
    let mut has_data_db_error = false;
    let mut has_tree_cache_error = false;

    if !data_db.is_file() {
        missing_items.push("data.db".to_string());
    } else if let Err(error) = inspect_sqlite_database(
        &data_db,
        &["tags", "file_metadata", "collections", "file_changes"],
    ) {
        has_data_db_error = true;
        issues.push(project_location_issue(
            "data-db-invalid",
            "error",
            format!("data.db 无法通过完整性检查: {error}。请先备份 .pm_center。"),
        ));
    }

    if !tree_cache_db.is_file() {
        missing_items.push("tree_cache.db".to_string());
    } else if let Err(error) = inspect_sqlite_database(
        &tree_cache_db,
        &[
            "fs_entries",
            "dir_state",
            "tree_state",
            "file_details_cache",
        ],
    ) {
        has_tree_cache_error = true;
        issues.push(project_location_issue(
            "tree-cache-invalid",
            "warning",
            format!("tree_cache.db 无法通过完整性检查: {error}。可以安全重建目录缓存。"),
        ));
    }

    for directory in support_dirs {
        if !pm_center_path.join(directory).is_dir() {
            missing_items.push(directory.to_string());
        }
    }

    if has_data_db_error {
        return Ok(ProjectLocationReport {
            project_path,
            pm_center_path: pm_center_path.to_string_lossy().to_string(),
            status: "invalidDataDb".to_string(),
            missing_items,
            issues,
            can_initialize: false,
            can_repair: false,
        });
    }

    if !missing_items.is_empty() || has_tree_cache_error {
        if !missing_items.is_empty() {
            issues.push(project_location_issue(
                "pm-center-incomplete",
                "warning",
                format!(".pm_center 缺少: {}。", missing_items.join("、")),
            ));
        }
        return Ok(ProjectLocationReport {
            project_path,
            pm_center_path: pm_center_path.to_string_lossy().to_string(),
            status: "incompletePmCenter".to_string(),
            missing_items,
            issues,
            can_initialize: false,
            can_repair: true,
        });
    }

    Ok(ProjectLocationReport {
        project_path,
        pm_center_path: pm_center_path.to_string_lossy().to_string(),
        status: "ready".to_string(),
        missing_items,
        issues,
        can_initialize: false,
        can_repair: false,
    })
}

fn project_location_key(path: &std::path::Path) -> String {
    tree_cache::normalize_path_key(&path.to_string_lossy())
}

fn is_safe_auto_search_root(path: &std::path::Path) -> bool {
    path.parent().is_some()
        && path
            .file_name()
            .map(|name| !name.is_empty())
            .unwrap_or(false)
}

#[tauri::command]
async fn find_project_location_candidates(
    project_path: String,
    search_roots: Vec<String>,
) -> Result<Vec<ProjectLocationCandidate>, String> {
    use std::collections::HashSet;
    use std::path::PathBuf;
    use walkdir::WalkDir;

    let requested = PathBuf::from(&project_path);
    let requested_name = requested
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut roots = Vec::<(PathBuf, bool)>::new();

    let mut ancestor = requested.clone();
    while !ancestor.exists() {
        if !ancestor.pop() {
            break;
        }
    }
    if ancestor.is_dir() && is_safe_auto_search_root(&ancestor) {
        roots.push((ancestor, false));
    }
    for root in search_roots {
        let root = PathBuf::from(root);
        if root.is_dir() {
            roots.push((root, true));
        }
    }

    let mut seen_roots = HashSet::new();
    roots.retain(|(root, _)| seen_roots.insert(project_location_key(root)));

    let mut seen_candidates = HashSet::new();
    let mut candidates = Vec::<(i32, ProjectLocationCandidate)>::new();
    for (root, is_configured_root) in roots {
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .max_depth(3)
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let name = entry.file_name().to_string_lossy();
                !matches!(name.as_ref(), ".pm_center" | "node_modules" | "renders")
            });

        for entry in walker.filter_map(Result::ok) {
            if !entry.file_type().is_dir() {
                continue;
            }
            let candidate_path = entry.path();
            if !candidate_path.join(".pm_center").is_dir() {
                continue;
            }
            let key = project_location_key(candidate_path);
            if !seen_candidates.insert(key) {
                continue;
            }

            let name = candidate_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| candidate_path.to_string_lossy().to_string());
            let name_matches =
                !requested_name.is_empty() && name.eq_ignore_ascii_case(&requested_name);
            let score = if name_matches { 0 } else { 1 };
            let match_reason = if name_matches {
                "目录名与原项目一致".to_string()
            } else if is_configured_root {
                "在已配置的项目根目录中找到".to_string()
            } else {
                "在原项目路径附近找到".to_string()
            };
            candidates.push((
                score,
                ProjectLocationCandidate {
                    path: candidate_path.to_string_lossy().to_string(),
                    name,
                    match_reason,
                },
            ));
        }
    }

    candidates.sort_by(|(left_score, left), (right_score, right)| {
        left_score
            .cmp(right_score)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(candidates
        .into_iter()
        .take(20)
        .map(|(_, candidate)| candidate)
        .collect())
}

/// 扫描项目根目录，查找带 .pm_center 的项目（2级深度）
#[tauri::command]
async fn scan_projects_root(root_path: String) -> Result<Vec<ScannedProject>, String> {
    use std::fs;
    use std::path::PathBuf;

    let root = PathBuf::from(&root_path);
    if !root.exists() {
        return Err("目录不存在".to_string());
    }

    let mut projects = vec![];

    // 读取根目录下的子目录（只扫描第1级）
    let entries = fs::read_dir(&root).map_err(|e| format!("读取目录失败: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // 跳过隐藏目录
        if dir_name.starts_with('.') {
            continue;
        }

        // 检查是否有 .pm_center
        let pm_center_path = path.join(".pm_center");
        let has_pm_center = pm_center_path.exists();

        // 添加所有文件夹（包括已初始化和未初始化的）
        projects.push(ScannedProject {
            path: path.to_string_lossy().to_string(),
            name: dir_name.to_string(),
            has_pm_center,
        });
    }

    // 按名称排序
    projects.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(projects)
}

/// 创建新项目（创建目录并初始化）
#[tauri::command]
async fn create_project(
    db_state: tauri::State<'_, DbState>,
    parent_path: String,
    project_name: String,
) -> Result<String, String> {
    use std::fs;
    use std::path::PathBuf;

    let project_path = PathBuf::from(&parent_path).join(&project_name);

    // 检查是否已存在
    if project_path.exists() {
        return Err("项目目录已存在".to_string());
    }

    // 创建目录
    fs::create_dir_all(&project_path).map_err(|e| format!("创建项目目录失败: {}", e))?;

    // 初始化项目
    init_project(db_state, project_path.to_string_lossy().to_string(), None).await?;

    Ok(project_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn move_project_entry(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    source: String,
    target: String,
    conflict_strategy: String,
    target_name: Option<String>,
) -> Result<String, String> {
    let final_path = fs::move_path_with_strategy(
        PathBuf::from(&source),
        PathBuf::from(&target),
        &conflict_strategy,
        target_name,
    )
    .await?;

    let final_path_str = final_path.to_string_lossy().to_string();
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.move_path_references(&source, &final_path_str)
        .map_err(|e| e.to_string())?;

    Ok(final_path_str)
}

#[tauri::command]
async fn rename_project_entry(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    path: String,
    new_name: String,
) -> Result<String, String> {
    let final_path = fs::rename_path(PathBuf::from(&path), new_name).await?;
    let final_path_str = final_path.to_string_lossy().to_string();
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.move_path_references(&path, &final_path_str)
        .map_err(|e| e.to_string())?;

    Ok(final_path_str)
}

#[tauri::command]
async fn get_tags(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
) -> Result<Vec<Tag>, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.get_all_tags().map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_tag(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    id: String,
    name: String,
    color: String,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.add_tag(&id, &name, &color).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_tag(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    id: String,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.delete_tag(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_tags(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    file_path: String,
) -> Result<Vec<String>, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.get_file_tags(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_tags_batch(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    file_paths: Vec<String>,
) -> Result<HashMap<String, Vec<String>>, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.get_file_tags_batch(&file_paths)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_tag_to_file(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    file_path: String,
    tag_id: String,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.add_tag_to_file(&file_path, &tag_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_tag_from_file(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    file_path: String,
    tag_id: String,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.remove_tag_from_file(&file_path, &tag_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_metadata(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    file_path: String,
) -> Result<Option<FileMetadata>, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.get_file_metadata(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_file_metadata(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    metadata: FileMetadata,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.update_file_metadata(&metadata)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_collection(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    name: String,
    member_paths: Vec<String>,
) -> Result<Collection, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.create_collection(&project_path, &name, &member_paths)
        .map_err(|e| {
            let message = e.to_string();
            if message.contains("UNIQUE constraint failed") {
                "项目根目录已存在同名集合".to_string()
            } else {
                message
            }
        })
}

#[tauri::command]
async fn list_project_collections(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
) -> Result<Vec<Collection>, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.list_all_collections().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_collection_items(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    collection_id: String,
) -> Result<Vec<fs::FileInfo>, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    let item_paths = db
        .get_collection_item_paths(&collection_id)
        .map_err(|e| e.to_string())?;
    let mut items = Vec::new();

    for item_path in item_paths {
        if let Ok(file_info) = fs::get_file_info_internal(&item_path).await {
            items.push(file_info);
        }
    }

    Ok(items)
}

#[tauri::command]
async fn add_collection_items(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    collection_id: String,
    member_paths: Vec<String>,
) -> Result<CollectionMemberUpdate, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.add_collection_items(&collection_id, &member_paths)
        .map_err(|error| {
            if matches!(&error, rusqlite::Error::QueryReturnedNoRows) {
                "集合不存在或已被删除".to_string()
            } else {
                error.to_string()
            }
        })
}

#[tauri::command]
async fn remove_collection_items(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    collection_id: String,
    member_paths: Vec<String>,
) -> Result<CollectionMemberRemoval, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.remove_collection_items(&collection_id, &member_paths)
        .map_err(|error| {
            if matches!(&error, rusqlite::Error::QueryReturnedNoRows) {
                "集合不存在或已被删除".to_string()
            } else {
                error.to_string()
            }
        })
}

#[tauri::command]
async fn rename_collection(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    collection_id: String,
    name: String,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.rename_collection(&collection_id, &name).map_err(|e| {
        let message = e.to_string();
        if message.contains("UNIQUE constraint failed") {
            "当前目录已存在同名集合".to_string()
        } else {
            message
        }
    })
}

#[tauri::command]
async fn delete_collection(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    collection_id: String,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.delete_collection(&collection_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_changes(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    since: i64,
    change_type: Option<String>,
    limit: i64,
) -> Result<Vec<FileChange>, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.get_file_changes(&project_path, since, change_type.as_deref(), limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_change_stats(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    since: i64,
) -> Result<serde_json::Value, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.get_change_stats(&project_path, since)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn archive_old_changes(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
) -> Result<usize, String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    db.archive_old_changes().map_err(|e| e.to_string())
}

// 启动外部程序
#[tauri::command]
async fn launch_program(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std_command("cmd")
            .args(["/c", "start", "", &path])
            .spawn()
            .map_err(|e| format!("Failed to launch: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std_command("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to launch: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std_command(&path)
            .spawn()
            .map_err(|e| format!("Failed to launch: {}", e))?;
    }

    Ok(())
}

async fn shutdown_application(app: tauri::AppHandle) {
    if let Some(runtime) = app.try_state::<platform::PlatformRuntime>() {
        let manager = runtime.manager.clone();
        for error in manager.shutdown_all().await {
            eprintln!("[platform] 退出清理警告: {error}");
        }
    }
    render_center::shutdown_all();
    app.exit(0);
}

async fn restart_application(app: tauri::AppHandle) {
    if let Some(runtime) = app.try_state::<platform::PlatformRuntime>() {
        let manager = runtime.manager.clone();
        for error in manager.shutdown_all().await {
            eprintln!("[platform] 重启清理警告: {error}");
        }
    }
    render_center::shutdown_all();
    app.restart();
}

#[tauri::command]
async fn exit_app(app: tauri::AppHandle) -> Result<(), String> {
    shutdown_application(app).await;
    Ok(())
}

fn toggle_window_visibility<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

// 显示窗口（用于单实例检测）
fn show_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.set_focus()?;
        // 如果窗口被最小化，恢复它
        if window.is_minimized()? {
            window.unminimize()?;
        }
    }
    Ok(())
}

pub fn run() {
    let db_state: DbState = project_resources::new_database_state();
    let db_state_for_single = db_state.clone();
    let db_state_for_platform = db_state.clone();

    let global_shortcut_plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_shortcut("Ctrl+Alt+S")
        .expect("Failed to register global shortcut Ctrl+Alt+S")
        .with_handler(|app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let _ = show_window(app);
        })
        .build();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("PM Center")
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(global_shortcut_plugin)
        .plugin(tauri_plugin_single_instance::init(
            move |app, _args, _cwd| {
                // 当检测到重复实例时，显示已存在的窗口
                let _ = show_window(app);
            },
        ))
        .manage(db_state_for_single)
        .setup(move |app| {
            watcher::set_app_handle(app.handle().clone());
            match app.path().app_data_dir() {
                Ok(app_data_dir) => {
                    let runtime = platform::PlatformRuntime::initialize(
                        &app_data_dir,
                        app.handle().clone(),
                        db_state_for_platform.clone(),
                    )?;
                    app.manage(runtime);
                }
                Err(error) => eprintln!("[platform] 获取应用数据目录失败: {error}"),
            }
            let window = app.get_webview_window("main").unwrap();

            // 创建托盘菜单
            let show_i = MenuItem::with_id(app, "show", "显示", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "隐藏", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出后台进程", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &hide_i, &quit_i])?;

            let app_handle = app.handle().clone();

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    "quit" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            shutdown_application(app).await;
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        toggle_window_visibility(app);
                    }
                })
                .build(app)?;

            // 窗口关闭时隐藏
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            });

            // 初始显示窗口
            let _ = window.show();
            let _ = window.set_focus();

            // 关闭开发者工具（开发模式下默认打开）
            #[cfg(debug_assertions)]
            window.close_devtools();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cache_manager::get_project_cache_report,
            cache_manager::check_project_cache,
            cache_manager::run_project_cache_action,
            cache_manager::cancel_cache_maintenance,
            platform::list_platform_modules,
            platform::initialize_workspace_profile_runtime,
            platform::get_workspace_profile_runtime,
            platform::get_workspace_profile_document,
            platform::validate_workspace_profile_draft,
            platform::create_workspace_profile,
            platform::save_workspace_profile,
            platform::preview_workspace_profile_switch,
            platform::switch_workspace_profile,
            platform::finalize_workspace_profile_switch,
            platform::rollback_workspace_profile_switch,
            platform::get_platform_module,
            platform::preview_disable_platform_module,
            platform::enable_platform_module,
            platform::disable_platform_module,
            platform::restart_platform_module,
            platform::set_local_web_console_enabled,
            platform::run_platform_module_health_check,
            platform::configure_platform_module_failure,
            platform::get_platform_module_failure_injections,
            platform::run_platform_module_diagnostic,
            platform::get_capability_gateway_overview,
            platform::request_platform_capability,
            platform::decide_platform_capability,
            platform::revoke_platform_capability_grant,
            platform::run_platform_capability_operation,
            platform::run_platform_capability_diagnostic,
            local_web_console::get_local_web_console_status,
            local_web_console::update_local_web_console_config,
            local_web_console::open_local_web_console,
            smart_clipboard::open_smart_clipboard,
            link_preview::get_link_preview,
            render_center::inspect_render_sources,
            render_center::create_render_batch,
            render_center::list_render_jobs,
            render_center::get_render_job,
            render_center::update_render_job,
            render_center::pause_render_job,
            render_center::resume_render_job,
            render_center::cancel_render_job,
            render_center::pause_render_queue,
            render_center::resume_render_queue,
            render_center::retry_render_frames,
            render_center::queue_render_frames,
            render_center::skip_render_frames,
            render_center::reorder_render_job,
            render_center::reorder_render_batch,
            render_center::resolve_render_source_change,
            render_center::archive_render_job,
            render_center::list_render_presets,
            render_center::save_render_preset,
            render_center::delete_render_preset,
            render_center::get_render_scheduler_settings,
            render_center::set_render_scheduler_settings,
            render_center::package_render_batch,
            render_center::package_render_job,
            render_center::open_render_output,
            fs::read_directory,
            fs::get_directory_tree,
            fs::search_files,
            fs::get_file_info,
            fs::store_cached_thumbnail,
            fs::create_directory,
            fs::show_in_folder,
            fs::reveal_in_explorer,
            fs::open_file,
            fs::open_file_with_program,
            fs::open_path,
            fs::delete_file,
            fs::delete_paths,
            fs::move_file,
            fs::copy_file,
            fs::copy_path_to_target,
            fs::cancel_file_copy,
            fs::start_external_file_drag,
            fs::show_system_context_menu,
            fs::rename_file,
            fs::path_exists,
            fs::get_system_clipboard_status,
            fs::get_system_clipboard_files,
            fs::paste_system_clipboard,
            fs::get_file_property,
            fs::read_file,
            get_blender_external_data,
            get_blender_preview_png,
            update_blender_scene_render,
            get_file_details,
            inspect_tool_paths,
            move_project_entry,
            rename_project_entry,
            task::run_task,
            task::cancel_task,
            launch_program,
            exit_app,
            icon_extractor::extract_icon,
            init_project,
            activate_project,
            release_project_resources,
            get_tags,
            add_tag,
            delete_tag,
            get_file_tags,
            get_file_tags_batch,
            add_tag_to_file,
            remove_tag_from_file,
            get_file_metadata,
            update_file_metadata,
            create_collection,
            list_project_collections,
            get_collection_items,
            add_collection_items,
            remove_collection_items,
            rename_collection,
            delete_collection,
            detect_python_envs,
            run_python_script,
            run_python_file,
            pip_install,
            get_blender_file_info,
            inspect_blender_executable,
            scan_blender_installations,
            get_file_changes,
            get_change_stats,
            archive_old_changes,
            get_global_task_scripts_path,
            get_project_scripts,
            scan_projects_root,
            inspect_project_location,
            find_project_location_candidates,
            create_project,
            init_p2p,
            update_p2p_user,
            start_p2p_discovery,
            stop_p2p_discovery,
            send_p2p_message,
            initialize_lan_collaboration,
            get_lan_collaboration_snapshot,
            update_lan_profile,
            update_lan_receive_directory,
            set_lan_avatar,
            start_lan_discovery,
            stop_lan_discovery,
            send_lan_message,
            send_server_message,
            update_pmc_server_settings,
            connect_pmc_server,
            disconnect_pmc_server,
            test_pmc_server_speed,
            offer_server_files,
            respond_server_transfer,
            cancel_server_transfer,
            offer_lan_files,
            respond_lan_transfer,
            cancel_lan_transfer,
            create_lan_transfer_staging_path,
            discard_lan_transfer_staging_file,
            mark_lan_conversation_read,
            clear_lan_conversation,
            clear_lan_history,
            remove_lan_contact,
            list_plugins,
            refresh_plugins,
            set_plugin_enabled,
            get_plugin_dirs,
            inspect_plugin_dependencies,
            install_plugin_dependencies,
            remove_plugin_dependencies,
            update_plugin_settings,
            reset_plugin_settings,
            run_plugin_action,
            validate_plugin,
            detect_system_python,
            scan_app_venvs,
            create_venv,
            delete_venv,
            pip_install_package,
            pip_uninstall_package,
            pip_list_packages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod project_location_tests {
    use super::*;
    use std::fs;

    fn temporary_project_root(name: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("pm-center-{name}-{unique}"))
    }

    #[tokio::test]
    async fn project_location_report_distinguishes_missing_incomplete_and_ready_storage() {
        let project_root = temporary_project_root("location-report");
        let project_path = project_root.to_string_lossy().to_string();

        let missing_directory = inspect_project_location(project_path.clone())
            .await
            .unwrap();
        assert_eq!(missing_directory.status, "missingDirectory");

        fs::create_dir_all(&project_root).unwrap();
        let missing_pm_center = inspect_project_location(project_path.clone())
            .await
            .unwrap();
        assert_eq!(missing_pm_center.status, "missingPmCenter");
        assert!(missing_pm_center.can_initialize);

        fs::create_dir_all(project_root.join(".pm_center")).unwrap();
        let incomplete = inspect_project_location(project_path.clone())
            .await
            .unwrap();
        assert_eq!(incomplete.status, "incompletePmCenter");
        assert!(incomplete.can_repair);

        let database = Database::new(&project_path).unwrap();
        for name in ["thumbnails", "scripts", "plugins"] {
            fs::create_dir_all(project_root.join(".pm_center").join(name)).unwrap();
        }
        tree_cache::get_or_create_project_cache(&project_path).unwrap();
        let ready = inspect_project_location(project_path.clone())
            .await
            .unwrap();
        assert_eq!(ready.status, "ready");

        drop(database);
        tree_cache::release_project_cache(&project_path).unwrap();
        fs::remove_dir_all(project_root).unwrap();
    }

    #[tokio::test]
    async fn project_location_search_finds_project_under_configured_root() {
        let root = temporary_project_root("location-search");
        let project = root.join("moved-project");
        fs::create_dir_all(project.join(".pm_center")).unwrap();

        let missing_path = root.join("missing-project").to_string_lossy().to_string();
        let candidates = find_project_location_candidates(
            missing_path,
            vec![root.to_string_lossy().to_string()],
        )
        .await
        .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, project.to_string_lossy());
        fs::remove_dir_all(root).unwrap();
    }
}
