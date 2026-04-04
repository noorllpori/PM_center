#[cfg(windows)]
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::future::Future;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::process_utils::std_command;
use crate::tree_cache::{self, FsEntrySnapshot, TreeCacheDb};

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{BOOL, HWND};
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    SHFileOperationW, FILEOPERATION_FLAGS, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
    FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
};

pub const FILE_CONFLICT_ERROR_PREFIX: &str = "PM_CONFLICT:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<String>,
    pub created: Option<String>,
    pub extension: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemClipboardStatus {
    #[serde(rename = "hasFiles")]
    pub has_files: bool,
    #[serde(rename = "hasImage")]
    pub has_image: bool,
}

#[cfg(windows)]
fn read_system_clipboard_status() -> Result<SystemClipboardStatus, String> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$result = @{
  hasFiles = [System.Windows.Forms.Clipboard]::ContainsFileDropList()
  hasImage = [System.Windows.Forms.Clipboard]::ContainsImage()
}
ConvertTo-Json -InputObject $result -Compress
"#;

    let output = std_command("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|e| format!("读取系统剪贴板失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "读取系统剪贴板失败".to_string()
        } else {
            stderr
        });
    }

    serde_json::from_slice::<SystemClipboardStatus>(&output.stdout)
        .map_err(|e| format!("解析系统剪贴板状态失败: {}", e))
}

#[cfg(not(windows))]
fn read_system_clipboard_status() -> Result<SystemClipboardStatus, String> {
    Ok(SystemClipboardStatus {
        has_files: false,
        has_image: false,
    })
}

#[cfg(windows)]
fn read_system_clipboard_file_list() -> Result<Vec<PathBuf>, String> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
if (-not [System.Windows.Forms.Clipboard]::ContainsFileDropList()) {
  '[]'
  exit 0
}
$files = @(
  [System.Windows.Forms.Clipboard]::GetFileDropList() |
    ForEach-Object { [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($_)) }
)
ConvertTo-Json -InputObject @($files) -Compress
"#;

    let output = std_command("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|e| format!("读取系统剪贴板文件失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "读取系统剪贴板文件失败".to_string()
        } else {
            stderr
        });
    }

    let encoded_paths = serde_json::from_slice::<Vec<String>>(&output.stdout)
        .map_err(|e| format!("解析系统剪贴板文件失败: {}", e))?;

    encoded_paths
        .into_iter()
        .map(|encoded_path| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded_path)
                .map_err(|e| format!("解码系统剪贴板文件路径失败: {}", e))?;
            let path = String::from_utf8(bytes)
                .map_err(|e| format!("系统剪贴板文件路径编码无效: {}", e))?;
            Ok(PathBuf::from(path))
        })
        .collect()
}

#[cfg(not(windows))]
fn read_system_clipboard_file_list() -> Result<Vec<PathBuf>, String> {
    Ok(Vec::new())
}

#[cfg(windows)]
fn save_system_clipboard_image(target_path: &PathBuf) -> Result<(), String> {
    let output = std_command("powershell")
        .env(
            "PM_CENTER_CLIPBOARD_IMAGE_PATH",
            target_path.to_string_lossy().to_string(),
        )
        .args([
            "-NoProfile",
            "-STA",
            "-Command",
            r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$outputPath = $env:PM_CENTER_CLIPBOARD_IMAGE_PATH
if ([string]::IsNullOrWhiteSpace($outputPath)) {
  throw '缺少剪贴板图片输出路径'
}
if (-not [System.Windows.Forms.Clipboard]::ContainsImage()) {
  throw '剪贴板中没有图片'
}
$image = [System.Windows.Forms.Clipboard]::GetImage()
if ($null -eq $image) {
  throw '读取剪贴板图片失败'
}
try {
  $image.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
  $image.Dispose()
}
"#,
        ])
        .output()
        .map_err(|e| format!("保存剪贴板图片失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "保存剪贴板图片失败".to_string()
        } else {
            stderr
        });
    }

    Ok(())
}

#[cfg(not(windows))]
fn save_system_clipboard_image(_target_path: &PathBuf) -> Result<(), String> {
    Err("当前平台暂不支持系统剪贴板图片粘贴".to_string())
}

// 格式化时间为 ISO 8601 字符串
fn format_time(time: std::io::Result<SystemTime>) -> Option<String> {
    time.ok().map(|t| {
        let duration = t.duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = duration.as_secs() as i64;
        let _nanos = duration.subsec_nanos();

        // Convert to date components (simplified, not accounting for leap seconds)
        let days = secs / 86400;
        let remaining_secs = secs % 86400;
        let hours = remaining_secs / 3600;
        let mins = (remaining_secs % 3600) / 60;
        let secs = remaining_secs % 60;

        // Approximate date calculation (days since 1970-01-01)
        let year = 1970 + (days / 365) as i32;
        let day_of_year = days % 365;

        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year,
            (day_of_year / 30 + 1).min(12),
            (day_of_year % 30 + 1).min(31),
            hours,
            mins,
            secs
        )
    })
}

// 读取目录内容
#[tauri::command]
pub async fn read_directory(
    path: String,
    project_path: Option<String>,
    force_refresh: Option<bool>,
) -> Result<Vec<FileInfo>, String> {
    let force_refresh = force_refresh.unwrap_or(false);

    if let Some(project_path_value) = project_path.clone() {
        if let Ok(cache_db) = tree_cache::get_or_create_project_cache(&project_path_value) {
            if force_refresh {
                let fresh_entries =
                    tree_cache::scan_directory_entries_from_disk(&project_path_value, &path)?;
                let _ = cache_db.replace_directory_entries(&path, &fresh_entries);
                return Ok(convert_snapshots_to_file_infos(fresh_entries));
            }

            let has_snapshot = cache_db.has_directory_snapshot(&path).unwrap_or(false);
            if has_snapshot {
                if let Ok(cached_entries) = cache_db.get_directory_entries(&path) {
                    if cache_db.is_dir_dirty(&path).unwrap_or(false) {
                        let project_for_refresh = project_path_value.clone();
                        let path_for_refresh = path.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Ok(cache) =
                                tree_cache::get_or_create_project_cache(&project_for_refresh)
                            {
                                if let Ok(fresh_entries) =
                                    tree_cache::scan_directory_entries_from_disk(
                                        &project_for_refresh,
                                        &path_for_refresh,
                                    )
                                {
                                    let _ = cache.replace_directory_entries(
                                        &path_for_refresh,
                                        &fresh_entries,
                                    );
                                }
                            }
                        });
                    }
                    return Ok(convert_snapshots_to_file_infos(cached_entries));
                }
            }

            let fresh_entries =
                tree_cache::scan_directory_entries_from_disk(&project_path_value, &path)?;
            let _ = cache_db.replace_directory_entries(&path, &fresh_entries);
            return Ok(convert_snapshots_to_file_infos(fresh_entries));
        }
    }

    read_directory_from_disk(&path, project_path.as_deref()).await
}

// 获取目录树
#[tauri::command]
pub async fn get_directory_tree(
    path: String,
    project_path: Option<String>,
    force_refresh: Option<bool>,
) -> Result<TreeNode, String> {
    let force_refresh = force_refresh.unwrap_or(false);

    if let Some(project_path_value) = project_path {
        if let Ok(cache_db) = tree_cache::get_or_create_project_cache(&project_path_value) {
            let need_full_refresh =
                force_refresh || !cache_db.has_full_tree_snapshot().unwrap_or(false);
            if need_full_refresh {
                let project_for_refresh = project_path_value.clone();
                let refresh_result = tokio::task::spawn_blocking(move || {
                    tree_cache::rebuild_project_tree_cache(&project_for_refresh)
                })
                .await
                .map_err(|error| error.to_string());
                if !matches!(refresh_result, Ok(Ok(()))) {
                    return build_tree_node(&PathBuf::from(path)).await;
                }
                if let Ok(tree) = build_tree_from_cache(&cache_db, &path) {
                    return Ok(tree);
                }
                return build_tree_node(&PathBuf::from(path)).await;
            }

            if let Ok(tree) = build_tree_from_cache(&cache_db, &path) {
                if cache_db.is_tree_dirty().unwrap_or(false) {
                    let project_for_refresh = project_path_value.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = tokio::task::spawn_blocking(move || {
                            tree_cache::rebuild_project_tree_cache(&project_for_refresh)
                        })
                        .await;
                    });
                }
                return Ok(tree);
            }
        }
    }

    build_tree_node(&PathBuf::from(path)).await
}

async fn read_directory_from_disk(
    path: &str,
    project_path: Option<&str>,
) -> Result<Vec<FileInfo>, String> {
    let mut entries = Vec::new();

    let mut dir = tokio::fs::read_dir(path).await.map_err(|e| e.to_string())?;

    let project_root = project_path.map(PathBuf::from);

    while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
        let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
        let entry_path = entry.path();

        if let Some(project_root_path) = project_root.as_ref() {
            if should_skip_pm_center(project_root_path, &entry_path) {
                continue;
            }
        }

        let name = entry.file_name().to_string_lossy().to_string();

        let extension = entry_path
            .extension()
            .map(|e| e.to_string_lossy().to_string().to_lowercase());

        entries.push(FileInfo {
            name: name.clone(),
            path: entry_path.to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: format_time(metadata.modified()),
            created: format_time(metadata.created()),
            extension,
            thumbnail: None,
        });
    }

    // 目录在前，文件在后，按名称排序
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

// Box::pin 递归 async 函数
fn build_tree_node(
    path: &PathBuf,
) -> Pin<Box<dyn Future<Output = Result<TreeNode, String>> + Send + '_>> {
    Box::pin(async move {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let is_dir = path.is_dir();
        let mut children = Vec::new();

        if is_dir {
            if let Ok(mut dir) = tokio::fs::read_dir(path).await {
                while let Ok(Some(entry)) = dir.next_entry().await {
                    let child_path = entry.path();

                    if child_path.is_dir() {
                        match build_tree_node(&child_path).await {
                            Ok(node) => children.push(node),
                            Err(_) => continue,
                        }
                    }
                }
            }
        }

        Ok(TreeNode {
            name,
            path: path.to_string_lossy().to_string(),
            is_dir,
            children,
        })
    })
}

fn build_tree_from_cache(cache_db: &TreeCacheDb, root_path: &str) -> Result<TreeNode, String> {
    let mut visited = HashSet::new();
    build_tree_from_cache_inner(cache_db, root_path, &mut visited)
}

fn build_tree_from_cache_inner(
    cache_db: &TreeCacheDb,
    current_path: &str,
    visited: &mut HashSet<String>,
) -> Result<TreeNode, String> {
    let current_key = tree_cache::normalize_path_key(current_path);
    if !visited.insert(current_key) {
        return Ok(TreeNode {
            name: PathBuf::from(current_path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| current_path.to_string()),
            path: current_path.to_string(),
            is_dir: true,
            children: Vec::new(),
        });
    }

    let mut children = Vec::new();
    let child_dirs = cache_db.get_cached_child_dirs(current_path)?;
    for child_dir in child_dirs {
        children.push(build_tree_from_cache_inner(cache_db, &child_dir, visited)?);
    }

    Ok(TreeNode {
        name: PathBuf::from(current_path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| current_path.to_string()),
        path: current_path.to_string(),
        is_dir: true,
        children,
    })
}

fn convert_snapshots_to_file_infos(entries: Vec<FsEntrySnapshot>) -> Vec<FileInfo> {
    entries
        .into_iter()
        .map(|entry| FileInfo {
            name: entry.name,
            path: entry.path,
            is_dir: entry.is_dir,
            size: entry.size,
            modified: format_cache_timestamp(entry.modified_ts),
            created: format_cache_timestamp(entry.created_ts),
            extension: entry.extension,
            thumbnail: None,
        })
        .collect()
}

fn format_cache_timestamp(timestamp: Option<i64>) -> Option<String> {
    timestamp.map(|value| {
        let days = value / 86400;
        let remaining_secs = value % 86400;
        let hours = remaining_secs / 3600;
        let mins = (remaining_secs % 3600) / 60;
        let secs = remaining_secs % 60;

        let year = 1970 + (days / 365) as i32;
        let day_of_year = days % 365;

        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year,
            (day_of_year / 30 + 1).min(12),
            (day_of_year % 30 + 1).min(31),
            hours,
            mins,
            secs
        )
    })
}

fn should_skip_pm_center(project_root: &PathBuf, entry_path: &PathBuf) -> bool {
    if !entry_path.starts_with(project_root) {
        return false;
    }

    match entry_path.strip_prefix(project_root) {
        Ok(relative) => relative.components().any(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".pm_center")
        }),
        Err(_) => false,
    }
}

// 搜索文件
#[tauri::command]
pub async fn search_files(root_path: String, query: String) -> Result<Vec<FileInfo>, String> {
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();

    for entry in walkdir::WalkDir::new(&root_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let name = entry.file_name().to_string_lossy().to_string();

        if name.to_lowercase().contains(&query_lower) {
            if let Ok(metadata) = entry.metadata() {
                let path = entry.path();
                let extension = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string().to_lowercase());

                results.push(FileInfo {
                    name,
                    path: path.to_string_lossy().to_string(),
                    is_dir: metadata.is_dir(),
                    size: metadata.len(),
                    modified: format_time(Ok(metadata.modified().unwrap_or(UNIX_EPOCH))),
                    created: format_time(Ok(metadata.created().unwrap_or(UNIX_EPOCH))),
                    extension,
                    thumbnail: None,
                });
            }
        }
    }

    Ok(results)
}

// 获取文件详情
#[tauri::command]
pub async fn get_file_info(path: String) -> Result<FileInfo, String> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| e.to_string())?;

    let path_buf = PathBuf::from(&path);
    let name = path_buf
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let extension = path_buf
        .extension()
        .map(|e| e.to_string_lossy().to_string().to_lowercase());

    Ok(FileInfo {
        name,
        path,
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        modified: format_time(metadata.modified()),
        created: format_time(metadata.created()),
        extension,
        thumbnail: None,
    })
}

// 创建目录
#[tauri::command]
pub async fn create_directory(path: String) -> Result<(), String> {
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| e.to_string())
}

pub async fn path_exists_internal(path: &str) -> Result<bool, String> {
    match tokio::fs::metadata(path).await {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub async fn path_exists(path: String) -> Result<bool, String> {
    path_exists_internal(&path).await
}

// 在资源管理器中显示文件
#[tauri::command]
pub async fn show_in_folder(path: String) -> Result<(), String> {
    reveal_in_explorer(path).await
}

// 在资源管理器中显示文件（别名）
#[tauri::command]
pub async fn reveal_in_explorer(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std_command("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std_command("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std_command("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// 用系统默认程序打开文件
#[tauri::command]
pub async fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std_command("cmd")
            .args(["/C", "start", "", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std_command("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std_command("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

// 打开文件夹
#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std_command("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std_command("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std_command("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

// 删除文件或目录
#[tauri::command]
pub async fn delete_file(path: String) -> Result<(), String> {
    delete_paths(vec![path]).await.map(|_| ())
}

fn normalize_delete_targets(paths: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique_paths = paths
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .filter(|path| seen.insert(path.clone()))
        .collect::<Vec<_>>();

    unique_paths.sort_by_key(|path| path.len());
    let mut compact_paths = Vec::new();

    for candidate in unique_paths {
        let is_child_of_existing = compact_paths.iter().any(|existing: &String| {
            candidate.starts_with(existing)
                && candidate
                    .as_bytes()
                    .get(existing.len())
                    .is_some_and(|byte| *byte == b'\\' || *byte == b'/')
        });

        if !is_child_of_existing {
            compact_paths.push(candidate);
        }
    }

    compact_paths
}

#[cfg(not(windows))]
async fn delete_file_permanently(path: &str) -> Result<(), String> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| e.to_string())?;

    if metadata.is_dir() {
        tokio::fs::remove_dir_all(&path)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(windows)]
fn build_double_null_terminated_paths(paths: &[String]) -> Vec<u16> {
    let mut wide = Vec::new();

    for path in paths {
        wide.extend(OsStr::new(path).encode_wide());
        wide.push(0);
    }

    wide.push(0);
    wide
}

#[cfg(windows)]
fn move_paths_to_recycle_bin(paths: &[String]) -> Result<(), String> {
    let encoded_paths = build_double_null_terminated_paths(paths);
    let flags: FILEOPERATION_FLAGS =
        FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT;

    let mut file_op = SHFILEOPSTRUCTW {
        hwnd: HWND(0),
        wFunc: FO_DELETE,
        pFrom: PCWSTR(encoded_paths.as_ptr()),
        pTo: PCWSTR::null(),
        fFlags: flags.0 as u16,
        fAnyOperationsAborted: BOOL(0),
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: PCWSTR::null(),
    };

    let result = unsafe { SHFileOperationW(&mut file_op) };
    if result != 0 {
        return Err(format!("移到回收站失败，错误代码: {}", result));
    }

    if file_op.fAnyOperationsAborted != BOOL(0) {
        return Err("移到回收站已取消".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_paths(paths: Vec<String>) -> Result<usize, String> {
    let normalized_paths = normalize_delete_targets(paths)
        .into_iter()
        .filter(|path| PathBuf::from(path).exists())
        .collect::<Vec<_>>();

    if normalized_paths.is_empty() {
        return Ok(0);
    }

    #[cfg(windows)]
    {
        move_paths_to_recycle_bin(&normalized_paths)?;
        return Ok(normalized_paths.len());
    }

    #[cfg(not(windows))]
    {
        for path in &normalized_paths {
            delete_file_permanently(path).await?;
        }
        Ok(normalized_paths.len())
    }
}

// 移动文件或目录
async fn remove_path(path: &PathBuf) -> Result<(), String> {
    let metadata = tokio::fs::metadata(path).await.map_err(|e| e.to_string())?;

    if metadata.is_dir() {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn move_path_fallback(source: &PathBuf, target_path: &PathBuf) -> Result<(), String> {
    let metadata = tokio::fs::metadata(source)
        .await
        .map_err(|e| e.to_string())?;

    if metadata.is_dir() {
        copy_dir_recursive(source.clone(), target_path.clone()).await?;
        tokio::fs::remove_dir_all(source)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        tokio::fs::copy(source, target_path)
            .await
            .map_err(|e| e.to_string())?;
        tokio::fs::remove_file(source)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn build_target_path(source_path: &PathBuf, target_dir: &PathBuf) -> Result<PathBuf, String> {
    let file_name = source_path
        .file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();

    Ok(target_dir.join(file_name))
}

fn build_renamed_path(path: &PathBuf) -> PathBuf {
    let parent = path.parent().map(PathBuf::from).unwrap_or_default();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string());

    let stem = path
        .file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or(file_name.clone());
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_string());

    for index in 1.. {
        let candidate_name = if let Some(extension) = &extension {
            format!("{} ({}).{}", stem, index, extension)
        } else {
            format!("{} ({})", file_name, index)
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    path.clone()
}

pub async fn move_path_with_strategy(
    source: PathBuf,
    target_dir: PathBuf,
    conflict_strategy: &str,
) -> Result<PathBuf, String> {
    let mut target_path = build_target_path(&source, &target_dir)?;

    if source == target_path {
        return Err("不能移动到当前位置".to_string());
    }

    if target_path.exists() {
        match conflict_strategy {
            "overwrite" => {
                remove_path(&target_path).await?;
            }
            "rename" => {
                target_path = build_renamed_path(&target_path);
            }
            _ => {
                return Err(format!(
                    "{}{}",
                    FILE_CONFLICT_ERROR_PREFIX,
                    target_path.to_string_lossy()
                ));
            }
        }
    }

    match tokio::fs::rename(&source, &target_path).await {
        Ok(_) => Ok(target_path),
        Err(_) => {
            move_path_fallback(&source, &target_path).await?;
            Ok(target_path)
        }
    }
}

pub async fn rename_path(path: PathBuf, new_name: String) -> Result<PathBuf, String> {
    let parent = path.parent().ok_or("Invalid path")?.to_path_buf();

    let target_path = parent.join(&new_name);

    if target_path.exists() {
        return Err(format!(
            "{}{}",
            FILE_CONFLICT_ERROR_PREFIX,
            target_path.to_string_lossy()
        ));
    }

    tokio::fs::rename(&path, &target_path)
        .await
        .map_err(|e| e.to_string())?;

    Ok(target_path)
}

#[tauri::command]
pub async fn move_file(source: String, target: String) -> Result<(), String> {
    move_path_with_strategy(PathBuf::from(source), PathBuf::from(target), "error")
        .await
        .map(|_| ())
}

async fn copy_path_to_directory(
    source_path: PathBuf,
    target_dir: PathBuf,
    rename_on_conflict: bool,
) -> Result<PathBuf, String> {
    let file_name = source_path
        .file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();

    let mut target_path = target_dir.join(&file_name);

    if target_path.exists() {
        if rename_on_conflict {
            target_path = build_renamed_path(&target_path);
        } else {
            return Err("目标位置已存在同名文件".to_string());
        }
    }

    let metadata = tokio::fs::metadata(&source_path)
        .await
        .map_err(|e| e.to_string())?;

    if metadata.is_dir() {
        copy_dir_recursive(source_path, target_path.clone()).await?;
    } else {
        tokio::fs::copy(&source_path, &target_path)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(target_path)
}

// 复制文件或目录
#[tauri::command]
pub async fn copy_file(source: String, target: String) -> Result<(), String> {
    copy_path_to_directory(PathBuf::from(source), PathBuf::from(target), false).await?;

    Ok(())
}

#[tauri::command]
pub async fn get_system_clipboard_status() -> Result<SystemClipboardStatus, String> {
    read_system_clipboard_status()
}

#[tauri::command]
pub async fn paste_system_clipboard(target_dir: String) -> Result<Vec<String>, String> {
    let target_dir_path = PathBuf::from(&target_dir);
    let metadata = tokio::fs::metadata(&target_dir_path)
        .await
        .map_err(|e| e.to_string())?;

    if !metadata.is_dir() {
        return Err("目标位置不是文件夹".to_string());
    }

    let status = read_system_clipboard_status()?;
    let mut pasted_paths = Vec::new();

    if status.has_files {
        for source_path in read_system_clipboard_file_list()? {
            let pasted_path =
                copy_path_to_directory(source_path, target_dir_path.clone(), true).await?;
            pasted_paths.push(pasted_path.to_string_lossy().to_string());
        }
        return Ok(pasted_paths);
    }

    if status.has_image {
        let mut target_path = target_dir_path.join("粘贴的图像.png");
        if target_path.exists() {
            target_path = build_renamed_path(&target_path);
        }
        save_system_clipboard_image(&target_path)?;
        pasted_paths.push(target_path.to_string_lossy().to_string());
        return Ok(pasted_paths);
    }

    Err("系统剪贴板中没有可粘贴的文件或图片".to_string())
}

// 递归复制目录
fn copy_dir_recursive(
    source: PathBuf,
    target: PathBuf,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
    Box::pin(async move {
        tokio::fs::create_dir_all(&target)
            .await
            .map_err(|e| e.to_string())?;

        let mut entries = tokio::fs::read_dir(&source)
            .await
            .map_err(|e| e.to_string())?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let source_path = entry.path();
            let file_name = entry.file_name();
            let target_path = target.join(&file_name);

            let metadata = entry.metadata().await.map_err(|e| e.to_string())?;

            if metadata.is_dir() {
                copy_dir_recursive(source_path, target_path).await?;
            } else {
                tokio::fs::copy(&source_path, &target_path)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    })
}

// 重命名文件或目录
#[tauri::command]
pub async fn rename_file(path: String, new_name: String) -> Result<(), String> {
    rename_path(PathBuf::from(path), new_name).await.map(|_| ())
}

// 文件属性
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProperty {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub size_formatted: String,
    pub is_dir: bool,
    pub created: String,
    pub modified: String,
    pub accessed: String,
    pub readonly: bool,
    pub hidden: bool,
    pub extension: Option<String>,
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "-".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    while size >= 1024.0 && unit_index < units.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    format!("{:.1} {}", size, units[unit_index])
}

fn format_datetime(time: std::io::Result<SystemTime>) -> String {
    time.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| {
            let secs = d.as_secs();
            let days = secs / 86400;
            let remaining_secs = secs % 86400;
            let hours = remaining_secs / 3600;
            let mins = (remaining_secs % 3600) / 60;
            let secs = remaining_secs % 60;

            // 简单的日期格式化 (1970 + 天数/365 近似年份)
            let year = 1970 + (days / 365);
            let day_of_year = days % 365;
            let month = (day_of_year / 30 + 1).min(12);
            let day = (day_of_year % 30 + 1).min(31);

            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                year, month, day, hours, mins, secs
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

// 获取文件属性
#[tauri::command]
pub async fn get_file_property(path: String) -> Result<FileProperty, String> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| e.to_string())?;

    let path_buf = PathBuf::from(&path);
    let name = path_buf
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let extension = path_buf
        .extension()
        .map(|e| e.to_string_lossy().to_string().to_lowercase());

    // 检查是否隐藏文件（Windows）
    let hidden = {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            (metadata.file_attributes() & 0x2) != 0
        }
        #[cfg(not(windows))]
        {
            name.starts_with('.')
        }
    };

    // 检查是否只读
    let readonly = metadata.permissions().readonly();

    Ok(FileProperty {
        name,
        path: path.clone(),
        size: metadata.len(),
        size_formatted: format_size(metadata.len()),
        is_dir: metadata.is_dir(),
        created: format_datetime(metadata.created()),
        modified: format_datetime(metadata.modified()),
        accessed: format_datetime(metadata.accessed()),
        readonly,
        hidden,
        extension,
    })
}

// 读取文件内容为字符串
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取文件失败: {}", e))
}
