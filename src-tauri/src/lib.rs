use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Runtime,
};
use tokio::sync::Mutex;

mod db;
mod file_details;
mod fs;
mod icon_extractor;
mod p2p;
mod plugin;
mod process_utils;
mod python;
mod python_env;
mod task;
mod tools;
mod tree_cache;
mod watcher;

use db::FileChange;
use db::{Database, FileMetadata, Tag};
use file_details::get_file_details;
use p2p::{init_p2p, send_p2p_message, start_p2p_discovery, stop_p2p_discovery, update_p2p_user};
use plugin::{
    get_plugin_dirs, inspect_plugin_dependencies, install_plugin_dependencies, list_plugins,
    refresh_plugins, remove_plugin_dependencies, run_plugin_action, set_plugin_enabled,
    validate_plugin,
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
use tools::inspect_tool_paths;

#[derive(Default)]
struct DbStateInner {
    databases: HashMap<String, Database>,
}

type DbState = Arc<Mutex<DbStateInner>>;

async fn get_or_create_db(
    db_state: &tauri::State<'_, DbState>,
    project_path: &str,
) -> Result<Database, String> {
    let mut guard = db_state.lock().await;

    if let Some(db) = guard.databases.get(project_path) {
        return Ok(db.clone());
    }

    let db = Database::new(project_path).map_err(|e| e.to_string())?;
    guard.databases.insert(project_path.to_string(), db.clone());

    Ok(db)
}

fn ensure_project_support_files(project_path: &str) -> Result<(), String> {
    use std::fs;

    let pm_center_dir = PathBuf::from(project_path).join(".pm_center");
    let scripts_dir = pm_center_dir.join("scripts");
    let plugins_dir = pm_center_dir.join("plugins");
    fs::create_dir_all(&scripts_dir).map_err(|e| format!("创建 scripts 目录失败: {}", e))?;
    fs::create_dir_all(&plugins_dir).map_err(|e| format!("创建 plugins 目录失败: {}", e))?;
    ensure_project_default_scripts(&scripts_dir)?;

    Ok(())
}

#[tauri::command]
async fn init_project(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
) -> Result<(), String> {
    ensure_project_support_files(&project_path)?;
    let db = get_or_create_db(&db_state, &project_path).await?;
    let _ = tree_cache::get_or_create_project_cache(&project_path)?;

    let _ = watcher::init_project(project_path.clone(), true);
    let _ = watcher::set_active_project(&project_path, &db);

    Ok(())
}

#[tauri::command]
async fn activate_project(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
) -> Result<(), String> {
    let db = get_or_create_db(&db_state, &project_path).await?;
    let _ = watcher::set_active_project(&project_path, &db);
    Ok(())
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
    init_project(db_state, project_path.to_string_lossy().to_string()).await?;

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

#[tauri::command]
fn exit_app(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
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
    let db_state: DbState = Arc::new(Mutex::new(DbStateInner::default()));
    let db_state_for_single = db_state.clone();

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
            let window = app.get_webview_window("main").unwrap();

            // 创建托盘菜单
            let show_i = MenuItem::with_id(app, "show", "显示", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "隐藏", true, None::<&str>)?;
            let quit_i =
                MenuItem::with_id(app, "quit", "退出后台进程", true, None::<&str>)?;
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
                        app.exit(0);
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

            // 启动后台任务：休眠项目扫描（低频率）
            let db_state_for_scan = db_state.clone();
            tauri::async_runtime::spawn(async move {
                // 等待数据库初始化
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await; // 5分钟

                    let databases = {
                        let guard = db_state_for_scan.lock().await;
                        guard.databases.clone()
                    };

                    if !databases.is_empty() {
                        watcher::run_dormant_scan(databases).await;
                    }
                }
            });

            // 启动后台任务：每天归档一次
            let db_state_for_archive = db_state.clone();
            tauri::async_runtime::spawn(async move {
                let mut archive_interval =
                    tokio::time::interval(tokio::time::Duration::from_secs(86400)); // 24小时

                loop {
                    archive_interval.tick().await;

                    let databases = {
                        let guard = db_state_for_archive.lock().await;
                        guard.databases.values().cloned().collect::<Vec<_>>()
                    };

                    for db in databases {
                        let _ = db.archive_old_changes();
                    }
                }
            });

            // 启动后台任务：高频增量修复缓存脏目录
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));

                loop {
                    interval.tick().await;
                    let _ = tree_cache::process_dirty_dirs(50);
                }
            });

            // 启动后台任务：低频全量树校验（活动项目）
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));

                loop {
                    interval.tick().await;
                    if let Some(active_project_path) = watcher::get_active_project_path() {
                        let _ = tree_cache::rebuild_project_tree_cache(&active_project_path);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            fs::read_directory,
            fs::get_directory_tree,
            fs::search_files,
            fs::get_file_info,
            fs::create_directory,
            fs::show_in_folder,
            fs::reveal_in_explorer,
            fs::open_file,
            fs::open_path,
            fs::delete_file,
            fs::delete_paths,
            fs::move_file,
            fs::copy_file,
            fs::start_external_file_drag,
            fs::show_system_context_menu,
            fs::rename_file,
            fs::path_exists,
            fs::get_system_clipboard_status,
            fs::paste_system_clipboard,
            fs::get_file_property,
            fs::read_file,
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
            get_tags,
            add_tag,
            delete_tag,
            get_file_tags,
            get_file_tags_batch,
            add_tag_to_file,
            remove_tag_from_file,
            get_file_metadata,
            update_file_metadata,
            detect_python_envs,
            run_python_script,
            run_python_file,
            pip_install,
            get_blender_file_info,
            get_file_changes,
            get_change_stats,
            archive_old_changes,
            get_global_task_scripts_path,
            get_project_scripts,
            scan_projects_root,
            create_project,
            init_p2p,
            update_p2p_user,
            start_p2p_discovery,
            stop_p2p_discovery,
            send_p2p_message,
            list_plugins,
            refresh_plugins,
            set_plugin_enabled,
            get_plugin_dirs,
            inspect_plugin_dependencies,
            install_plugin_dependencies,
            remove_plugin_dependencies,
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
