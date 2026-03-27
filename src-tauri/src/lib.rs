use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Runtime,
};

mod fs;
mod db;
mod python;
mod watcher;
mod icon_extractor;
mod task;
mod p2p;
mod python_env;

use fs::{read_directory, get_directory_tree, search_files, get_file_info, create_directory, show_in_folder, reveal_in_explorer, open_file, open_path, delete_file, move_file, copy_file, rename_file, get_file_property, read_file};
use task::{run_task, cancel_task};
use db::{Database, Tag, FileMetadata};
use python::{detect_python_envs, run_python_script, run_python_file, pip_install, get_blender_file_info};
use db::FileChange;
use p2p::{init_p2p, update_p2p_user, start_p2p_discovery, stop_p2p_discovery, send_p2p_message};
use python_env::{detect_system_python, scan_app_venvs, create_venv, delete_venv, pip_install_package, pip_uninstall_package, pip_list_packages};

// 全局状态
type DbState = Arc<Mutex<Option<Database>>>;

#[tauri::command]
async fn init_project(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
) -> Result<(), String> {
    use std::fs;
    use std::path::PathBuf;
    
    // 初始化数据库
    let db = Database::new(&project_path)
        .map_err(|e| e.to_string())?;
    
    // 创建 scripts 目录
    let scripts_dir = PathBuf::from(&project_path).join(".pm_center").join("scripts");
    if !scripts_dir.exists() {
        fs::create_dir_all(&scripts_dir)
            .map_err(|e| format!("创建 scripts 目录失败: {}", e))?;
        
        // 创建示例脚本
        create_example_scripts(&scripts_dir)?;
    }
    
    // 保存数据库状态
    {
        let mut guard = db_state.lock().await;
        *guard = Some(db.clone());
    }
    
    // 简化 watcher 初始化 - 只初始化活跃项目
    let _ = watcher::init_project(project_path.clone(), true);
    let _ = watcher::set_active_project(&project_path, &db);
    
    Ok(())
}

// 创建示例脚本
fn create_example_scripts(scripts_dir: &std::path::Path) -> Result<(), String> {
    use std::fs;
    
    // Python 进度示例
    let python_script = r#"# -*- coding: utf-8 -*-
# @name: Python进度示例
# @desc: 演示如何使用 /***N*/ 格式报告进度

import time
import sys

print("Starting Python task...", flush=True)

for i in range(0, 101, 10):
    print(f"progress /***{i}*/", flush=True)
    time.sleep(0.2)

print("Task completed successfully!", flush=True)
sys.exit(0)
"#;
    fs::write(scripts_dir.join("example_progress.py"), python_script)
        .map_err(|e| format!("创建示例脚本失败: {}", e))?;
    
    // Python Blender 渲染示例
    let blender_script = r#"# -*- coding: utf-8 -*-
# @name: Blender渲染示例
# @desc: 使用 Blender Python API 渲染场景

import sys
import os

print("Starting Blender render task...", flush=True)

try:
    import bpy
    
    # 获取当前场景
    scene = bpy.context.scene
    
    # 设置渲染引擎
    scene.render.engine = 'CYCLES'
    
    # 获取帧范围
    start_frame = scene.frame_start
    end_frame = scene.frame_end
    total_frames = end_frame - start_frame + 1
    
    print(f"Rendering {total_frames} frames...", flush=True)
    
    # 逐帧渲染
    for frame in range(start_frame, end_frame + 1):
        scene.frame_set(frame)
        scene.render.filepath = f"//render/frame_{frame:04d}.png"
        
        # 渲染
        bpy.ops.render.render(write_file=True)
        
        # 报告进度
        progress = int((frame - start_frame + 1) / total_frames * 100)
        print(f"progress /***{progress}*/", flush=True)
    
    print("Render completed!", flush=True)
    
except ImportError:
    print("Error: Blender Python module not available", flush=True)
    print("This script should be run within Blender", flush=True)
    sys.exit(1)

sys.exit(0)
"#;
    fs::write(scripts_dir.join("example_blender_render.py"), blender_script)
        .map_err(|e| format!("创建示例脚本失败: {}", e))?;
    
    // Python 文件处理示例
    let file_script = r#"# -*- coding: utf-8 -*-
# @name: 文件批处理示例
# @desc: 批量处理项目文件

import os
import sys

print("Starting file batch processing...", flush=True)

# 获取当前工作目录（项目根目录）
project_dir = os.getcwd()
print(f"Project directory: {project_dir}", flush=True)

# 统计文件
extensions = {}
file_count = 0

for root, dirs, files in os.walk(project_dir):
    # 跳过 .pm_center 目录
    if '.pm_center' in root:
        continue
    
    for file in files:
        file_count += 1
        ext = os.path.splitext(file)[1].lower()
        extensions[ext] = extensions.get(ext, 0) + 1
    
    # 报告进度
    if file_count % 100 == 0:
        progress = min(90, int(file_count / 10))
        print(f"progress /***{progress}*/", flush=True)

# 输出统计结果
print(f"\nTotal files: {file_count}", flush=True)
print("File types:", flush=True)
for ext, count in sorted(extensions.items(), key=lambda x: -x[1])[:10]:
    print(f"  {ext or '(no ext)'}: {count}", flush=True)

print("\nprogress /***100*/", flush=True)
print("Processing completed!", flush=True)

sys.exit(0)
"#;
    fs::write(scripts_dir.join("example_file_batch.py"), file_script)
        .map_err(|e| format!("创建示例脚本失败: {}", e))?;
    
    Ok(())
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
}

#[tauri::command]
async fn get_project_scripts(project_path: String) -> Result<Vec<ScriptInfo>, String> {
    use std::fs;
    use std::path::PathBuf;
    
    let scripts_dir = PathBuf::from(&project_path).join(".pm_center").join("scripts");
    
    if !scripts_dir.exists() {
        return Ok(vec![]);
    }
    
    let mut scripts = vec![];
    
    fn scan_dir(dir: &std::path::Path, base_dir: &std::path::Path, scripts: &mut Vec<ScriptInfo>) -> Result<(), String> {
        let entries = fs::read_dir(dir)
            .map_err(|e| format!("读取目录失败: {}", e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
            let path = entry.path();
            
            if path.is_dir() {
                // 递归扫描子目录
                scan_dir(&path, base_dir, scripts)?;
            } else {
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                
                // 只识别 Python 脚本
                if !filename.ends_with(".py") {
                    continue;
                }
                let script_type = "python"; 
                
                // 跳过示例脚本（可选）
                // if filename.starts_with("example_") {
                //     continue;
                // }
                
                // 读取文件内容解析元数据
                let content = fs::read_to_string(&path).unwrap_or_default();
                let (name, description) = parse_script_metadata(&content, filename);
                
                // 计算相对路径作为 ID
                let relative_path = path.strip_prefix(base_dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| filename.to_string());
                
                scripts.push(ScriptInfo {
                    id: relative_path.clone(),
                    name,
                    description,
                    filename: filename.to_string(),
                    path: path.to_string_lossy().to_string(),
                    script_type: script_type.to_string(),
                });
            }
        }
        
        Ok(())
    }
    
    scan_dir(&scripts_dir, &scripts_dir, &mut scripts)?;
    
    // 按名称排序
    scripts.sort_by(|a, b| a.name.cmp(&b.name));
    
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
        filename.rfind('.')
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
    let entries = fs::read_dir(&root)
        .map_err(|e| format!("读取目录失败: {}", e))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        
        if !path.is_dir() {
            continue;
        }
        
        let dir_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
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
    fs::create_dir_all(&project_path)
        .map_err(|e| format!("创建项目目录失败: {}", e))?;
    
    // 初始化项目
    init_project(db_state, project_path.to_string_lossy().to_string()).await?;
    
    Ok(project_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn get_tags(db_state: tauri::State<'_, DbState>) -> Result<Vec<Tag>, String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.get_all_tags().map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_tag(
    db_state: tauri::State<'_, DbState>,
    id: String,
    name: String,
    color: String,
) -> Result<(), String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.add_tag(&id, &name, &color).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_tag(db_state: tauri::State<'_, DbState>, id: String) -> Result<(), String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.delete_tag(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_tags(
    db_state: tauri::State<'_, DbState>,
    file_path: String,
) -> Result<Vec<String>, String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.get_file_tags(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_tag_to_file(
    db_state: tauri::State<'_, DbState>,
    file_path: String,
    tag_id: String,
) -> Result<(), String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.add_tag_to_file(&file_path, &tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_tag_from_file(
    db_state: tauri::State<'_, DbState>,
    file_path: String,
    tag_id: String,
) -> Result<(), String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.remove_tag_from_file(&file_path, &tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_metadata(
    db_state: tauri::State<'_, DbState>,
    file_path: String,
) -> Result<Option<FileMetadata>, String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.get_file_metadata(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_file_metadata(
    db_state: tauri::State<'_, DbState>,
    metadata: FileMetadata,
) -> Result<(), String> {
    let guard = db_state.lock().await;
    let db = guard.as_ref().ok_or("Project not initialized")?;
    db.update_file_metadata(&metadata).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_file_changes(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    since: i64,
    change_type: Option<String>,
    limit: i64,
) -> Result<Vec<FileChange>, String> {
    let db_guard = db_state.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.get_file_changes(&project_path, since, change_type.as_deref(), limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_change_stats(
    db_state: tauri::State<'_, DbState>,
    project_path: String,
    since: i64,
) -> Result<serde_json::Value, String> {
    let db_guard = db_state.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.get_change_stats(&project_path, since)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn archive_old_changes(
    db_state: tauri::State<'_, DbState>,
) -> Result<usize, String> {
    let db_guard = db_state.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.archive_old_changes()
        .map_err(|e| e.to_string())
}

// 启动外部程序
#[tauri::command]
async fn launch_program(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &path])
            .spawn()
            .map_err(|e| format!("Failed to launch: {}", e))?;
    }
    
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to launch: {}", e))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new(&path)
            .spawn()
            .map_err(|e| format!("Failed to launch: {}", e))?;
    }
    
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
    let db_state: DbState = Arc::new(Mutex::new(None));
    let db_state_for_single = db_state.clone();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(move |app, _args, _cwd| {
            // 当检测到重复实例时，显示已存在的窗口
            let _ = show_window(app);
        }))
        .manage(db_state_for_single)
        .setup(move |app| {
            let window = app.get_webview_window("main").unwrap();
            
            // 创建托盘菜单
            let show_i = MenuItem::with_id(app, "show", "显示", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "隐藏", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
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
                    
                    let guard = db_state_for_scan.lock().await;
                    if let Some(db) = guard.as_ref() {
                        watcher::run_dormant_scan(db).await;
                    }
                }
            });
            
            // 启动后台任务：每天归档一次
            let db_state_for_archive = db_state.clone();
            tauri::async_runtime::spawn(async move {
                let mut archive_interval = tokio::time::interval(tokio::time::Duration::from_secs(86400)); // 24小时
                
                loop {
                    archive_interval.tick().await;
                    
                    let guard = db_state_for_archive.lock().await;
                    if let Some(db) = guard.as_ref() {
                        let _ = db.archive_old_changes();
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
            fs::move_file,
            fs::copy_file,
            fs::rename_file,
            fs::get_file_property,
            fs::read_file,
            task::run_task,
            task::cancel_task,
            launch_program,
            icon_extractor::extract_icon,
            init_project,
            get_tags,
            add_tag,
            delete_tag,
            get_file_tags,
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
            get_project_scripts,
            scan_projects_root,
            create_project,
            init_p2p,
            update_p2p_user,
            start_p2p_discovery,
            stop_p2p_discovery,
            send_p2p_message,
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
