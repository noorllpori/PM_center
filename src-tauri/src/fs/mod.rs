use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};
use std::future::Future;

use crate::process_utils::std_command;

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
        
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", 
            year, 
            (day_of_year / 30 + 1).min(12),
            (day_of_year % 30 + 1).min(31),
            hours, mins, secs
        )
    })
}

// 读取目录内容
#[tauri::command]
pub async fn read_directory(path: String) -> Result<Vec<FileInfo>, String> {
    let mut entries = Vec::new();
    
    let mut dir = tokio::fs::read_dir(&path)
        .await
        .map_err(|e| e.to_string())?;
    
    while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
        let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        let extension = path.extension()
            .map(|e| e.to_string_lossy().to_string().to_lowercase());
        
        entries.push(FileInfo {
            name: name.clone(),
            path: path.to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: format_time(metadata.modified()),
            created: format_time(metadata.created()),
            extension,
            thumbnail: None,
        });
    }
    
    // 目录在前，文件在后，按名称排序
    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    
    Ok(entries)
}

// 获取目录树
#[tauri::command]
pub async fn get_directory_tree(path: String) -> Result<TreeNode, String> {
    build_tree_node(&PathBuf::from(path)).await
}

// Box::pin 递归 async 函数
fn build_tree_node(
    path: &PathBuf,
) -> Pin<Box<dyn Future<Output = Result<TreeNode, String>> + Send + '_>> {
    Box::pin(async move {
        let name = path.file_name()
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
                let extension = path.extension()
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
    let name = path_buf.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    
    let extension = path_buf.extension()
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

// 移动文件或目录
async fn remove_path(path: &PathBuf) -> Result<(), String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| e.to_string())?;

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
    let file_name = source_path.file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();

    Ok(target_dir.join(file_name))
}

fn build_renamed_path(path: &PathBuf) -> PathBuf {
    let parent = path.parent().map(PathBuf::from).unwrap_or_default();
    let file_name = path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string());

    let stem = path.file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or(file_name.clone());
    let extension = path.extension()
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

pub async fn move_path_with_strategy(source: PathBuf, target_dir: PathBuf, conflict_strategy: &str) -> Result<PathBuf, String> {
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
                return Err(format!("{}{}", FILE_CONFLICT_ERROR_PREFIX, target_path.to_string_lossy()));
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
    let parent = path.parent()
        .ok_or("Invalid path")?
        .to_path_buf();

    let target_path = parent.join(&new_name);

    if target_path.exists() {
        return Err(format!("{}{}", FILE_CONFLICT_ERROR_PREFIX, target_path.to_string_lossy()));
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

// 复制文件或目录
#[tauri::command]
pub async fn copy_file(source: String, target: String) -> Result<(), String> {
    let source_path = PathBuf::from(&source);
    let file_name = source_path.file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();
    
    let target_path = PathBuf::from(&target).join(&file_name);
    
    // 检查目标是否已存在
    if target_path.exists() {
        return Err("目标位置已存在同名文件".to_string());
    }
    
    let metadata = tokio::fs::metadata(&source)
        .await
        .map_err(|e| e.to_string())?;
    
    if metadata.is_dir() {
        // 递归复制目录
        copy_dir_recursive(source_path, target_path).await?;
    } else {
        // 复制文件
        tokio::fs::copy(&source, &target_path)
            .await
            .map_err(|e| e.to_string())?;
    }
    
    Ok(())
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
    rename_path(PathBuf::from(path), new_name)
        .await
        .map(|_| ())
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
            
            format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hours, mins, secs)
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
    let name = path_buf.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    
    let extension = path_buf.extension()
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
