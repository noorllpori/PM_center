use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

use crate::db::Database;
use crate::tree_cache::get_or_create_project_cache;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

impl ChangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeType::Created => "created",
            ChangeType::Modified => "modified",
            ChangeType::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    pub max_depth_active: i32,
    pub max_depth_dormant: i32,
    pub exclude_patterns: Vec<String>,
    pub dormant_scan_interval_sec: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            max_depth_active: 5,
            max_depth_dormant: 3,
            exclude_patterns: vec![
                ".pm_center".to_string(),
                ".git".to_string(),
                "temp".to_string(),
                "cache".to_string(),
                "*.tmp".to_string(),
                "*.temp".to_string(),
                "Thumbs.db".to_string(),
                ".DS_Store".to_string(),
            ],
            dormant_scan_interval_sec: 30,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProjectWatchState {
    Active,
    Dormant,
}

pub struct ProjectWatcherState {
    pub project_path: String,
    pub state: ProjectWatchState,
    pub config: WatchConfig,
    pub file_cache: HashMap<String, (u64, i64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFsChangeEvent {
    pub project_path: String,
    pub file_path: String,
    pub change_type: String,
    pub is_dir: bool,
    pub is_rename: bool,
    pub timestamp: i64,
}

// 全局状态
lazy_static::lazy_static! {
    static ref PROJECTS: Arc<Mutex<HashMap<String, ProjectWatcherState>>> = Arc::new(Mutex::new(HashMap::new()));
    static ref ACTIVE_WATCHER: Arc<Mutex<Option<RecommendedWatcher>>> = Arc::new(Mutex::new(None));
    static ref APP_HANDLE: Arc<Mutex<Option<tauri::AppHandle>>> = Arc::new(Mutex::new(None));
}

pub fn set_app_handle(app_handle: tauri::AppHandle) {
    let mut handle = APP_HANDLE.lock().unwrap();
    *handle = Some(app_handle);
}

// 初始化项目监控
pub fn init_project(path: String, is_active: bool) -> Result<(), String> {
    let mut cache = HashMap::new();
    let config = WatchConfig::default();
    scan_directory(
        &PathBuf::from(&path),
        0,
        config.max_depth_active,
        &config.exclude_patterns,
        &mut cache,
    );

    let state = if is_active {
        ProjectWatchState::Active
    } else {
        ProjectWatchState::Dormant
    };

    {
        let mut projects = PROJECTS.lock().unwrap();
        projects.insert(
            path.clone(),
            ProjectWatcherState {
                project_path: path,
                state,
                config,
                file_cache: cache,
            },
        );
    }

    Ok(())
}

// 设置活跃项目
pub fn set_active_project(path: &str, db: &Database) -> Result<(), String> {
    // 停止之前的 watcher
    {
        let mut watcher = ACTIVE_WATCHER.lock().unwrap();
        *watcher = None;
    }

    // 更新项目状态
    {
        let mut projects = PROJECTS.lock().unwrap();

        for (_, watcher) in projects.iter_mut() {
            watcher.state = ProjectWatchState::Dormant;
        }

        if let Some(watcher) = projects.get_mut(path) {
            watcher.state = ProjectWatchState::Active;
        } else {
            drop(projects);
            init_project(path.to_string(), true)?;
        }
    }

    // 启动 notify 监控
    start_notify_watcher(path, db)?;

    Ok(())
}

// 启动 notify 监控
fn start_notify_watcher(path: &str, db: &Database) -> Result<(), String> {
    let path = PathBuf::from(path);
    let db = db.clone();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                handle_notify_event(event, &db);
            }
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    watcher
        .watch(&path, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    {
        let mut active = ACTIVE_WATCHER.lock().unwrap();
        *active = Some(watcher);
    }

    Ok(())
}

// 处理 notify 事件
fn handle_notify_event(event: Event, db: &Database) {
    let projects_guard = PROJECTS.lock().unwrap();
    let is_rename_event = matches!(
        event.kind,
        notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
    );

    for file_path in &event.paths {
        let project_info = projects_guard
            .iter()
            .find(|(p, _)| file_path.starts_with(p));

        if let Some((project_path, watcher)) = project_info {
            if should_exclude(file_path, &watcher.config.exclude_patterns) {
                continue;
            }

            let depth = calculate_depth(project_path, file_path);
            if depth > watcher.config.max_depth_active {
                continue;
            }

            let change_type = match event.kind {
                notify::EventKind::Create(_) => ChangeType::Created,
                notify::EventKind::Modify(_) => ChangeType::Modified,
                notify::EventKind::Remove(_) => ChangeType::Deleted,
                _ => continue,
            };

            // 检查路径类型：如果是目录，只记录 Created/Deleted，不记录 Modified
            // 因为目录的 Modified 通常是由于内部文件变动导致的
            let is_dir = if change_type == ChangeType::Deleted {
                // 已删除的路径，通过缓存判断（如果无法确定，保守起见假设是文件）
                false
            } else {
                std::fs::metadata(file_path)
                    .ok()
                    .map(|m| m.is_dir())
                    .unwrap_or(false)
            };

            if is_dir && change_type == ChangeType::Modified {
                // 跳过目录的修改事件（只保留目录的创建和删除）
                continue;
            }

            let file_size = if change_type != ChangeType::Deleted && !is_dir {
                std::fs::metadata(file_path).ok().map(|m| m.len())
            } else {
                None
            };

            if let Ok(cache_db) = get_or_create_project_cache(project_path) {
                let changed_path = file_path.to_string_lossy().to_string();

                if let Some(parent) = file_path.parent() {
                    let parent_path = parent.to_string_lossy().to_string();
                    let _ = cache_db.mark_dir_dirty(&parent_path);
                }

                if is_dir {
                    let _ = cache_db.mark_dir_dirty(&changed_path);
                    let _ = cache_db.invalidate_file_details_by_prefix(&changed_path);
                } else {
                    let _ = cache_db.invalidate_file_details(&changed_path);
                }

                if is_rename_event
                    || matches!(change_type, ChangeType::Created | ChangeType::Deleted)
                {
                    let _ = cache_db.mark_tree_dirty();
                }
            }

            // 直接写入数据库
            let change = crate::db::FileChange {
                id: 0,
                project_path: project_path.clone(),
                file_path: file_path.to_string_lossy().to_string(),
                change_type: change_type.as_str().to_string(),
                file_size: file_size.map(|s| s as i64),
                timestamp: current_timestamp(),
                depth,
            };

            let _ = db.add_file_change(&change);

            println!(
                "[Watcher] {:?} - {} - {:?}",
                change_type,
                file_path.display(),
                file_size
            );

            emit_project_fs_change(ProjectFsChangeEvent {
                project_path: project_path.clone(),
                file_path: file_path.to_string_lossy().to_string(),
                change_type: if is_rename_event {
                    "renamed".to_string()
                } else {
                    change_type.as_str().to_string()
                },
                is_dir,
                is_rename: is_rename_event,
                timestamp: current_timestamp(),
            });
        }
    }
}

pub fn get_active_project_path() -> Option<String> {
    let projects = PROJECTS.lock().ok()?;
    projects
        .iter()
        .find(|(_, watcher)| matches!(watcher.state, ProjectWatchState::Active))
        .map(|(path, _)| path.clone())
}

fn emit_project_fs_change(payload: ProjectFsChangeEvent) {
    let handle = APP_HANDLE.lock().unwrap();
    if let Some(app_handle) = handle.as_ref() {
        let _ = app_handle.emit("pm-center:project-fs-change", payload);
    }
}

// 休眠项目轮询 - 降低频率到5分钟，减少CPU占用
pub async fn run_dormant_scan(databases: HashMap<String, Database>) {
    // 先收集需要处理的项目路径
    let dormant_projects: Vec<(String, WatchConfig)> = {
        let projects = PROJECTS.lock().unwrap();
        projects
            .iter()
            .filter(|(_, p)| matches!(p.state, ProjectWatchState::Dormant))
            .take(3) // 只处理3个
            .map(|(path, p)| (path.clone(), p.config.clone()))
            .collect()
    };

    // 逐个处理，不持有锁
    for (path, config) in dormant_projects {
        tokio::task::yield_now().await;

        if let Ok(changes) = scan_dormant_project(&path, &config) {
            if !changes.is_empty() {
                let Some(db) = databases.get(&path) else {
                    continue;
                };
                let db_changes: Vec<crate::db::FileChange> = changes
                    .into_iter()
                    .map(|c| crate::db::FileChange {
                        id: 0,
                        project_path: c.project_path,
                        file_path: c.file_path,
                        change_type: c.change_type,
                        file_size: c.file_size.map(|s| s as i64),
                        timestamp: c.timestamp,
                        depth: c.depth,
                    })
                    .collect();
                let _ = db.add_file_changes_batch(&db_changes);
            }
        }

        // 每个项目之间等待一下，避免阻塞
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// 变更数据结构
struct PendingChange {
    project_path: String,
    file_path: String,
    change_type: String,
    file_size: Option<u64>,
    timestamp: i64,
    depth: i32,
}

// 扫描休眠项目 - 限制处理数量
fn scan_dormant_project(path: &str, config: &WatchConfig) -> Result<Vec<PendingChange>, String> {
    let mut projects = PROJECTS.lock().unwrap();
    let watcher = projects.get_mut(path).ok_or("Project not found")?;

    let mut new_cache = HashMap::new();
    // 降低休眠项目扫描深度到2层，减少IO
    scan_directory_limited(
        &PathBuf::from(path),
        0,
        2, // 只扫描2层
        &config.exclude_patterns,
        &mut new_cache,
        1000, // 最多1000个文件
    );

    let mut changes = Vec::new();

    for (file_path, (new_size, new_time)) in &new_cache {
        match watcher.file_cache.get(file_path) {
            None => {
                changes.push(PendingChange {
                    project_path: path.to_string(),
                    file_path: file_path.clone(),
                    change_type: ChangeType::Created.as_str().to_string(),
                    file_size: Some(*new_size),
                    timestamp: *new_time,
                    depth: 0,
                });
            }
            Some((old_size, old_time)) => {
                if new_size != old_size || new_time != old_time {
                    changes.push(PendingChange {
                        project_path: path.to_string(),
                        file_path: file_path.clone(),
                        change_type: ChangeType::Modified.as_str().to_string(),
                        file_size: Some(*new_size),
                        timestamp: *new_time,
                        depth: 0,
                    });
                }
            }
        }
    }

    for (file_path, _) in &watcher.file_cache {
        if !new_cache.contains_key(file_path) {
            changes.push(PendingChange {
                project_path: path.to_string(),
                file_path: file_path.clone(),
                change_type: ChangeType::Deleted.as_str().to_string(),
                file_size: None,
                timestamp: current_timestamp(),
                depth: 0,
            });
        }
    }

    watcher.file_cache = new_cache;
    Ok(changes)
}

// 扫描目录
fn scan_directory(
    path: &PathBuf,
    depth: i32,
    max_depth: i32,
    exclude_patterns: &[String],
    cache: &mut HashMap<String, (u64, i64)>,
) {
    scan_directory_limited(path, depth, max_depth, exclude_patterns, cache, usize::MAX);
}

// 有限制的扫描目录
fn scan_directory_limited(
    path: &PathBuf,
    depth: i32,
    max_depth: i32,
    exclude_patterns: &[String],
    cache: &mut HashMap<String, (u64, i64)>,
    max_files: usize,
) {
    if depth > max_depth {
        return;
    }

    if should_exclude(path, exclude_patterns) {
        return;
    }

    if cache.len() >= max_files {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if cache.len() >= max_files {
                break;
            }

            let path = entry.path();

            if should_exclude(&path, exclude_patterns) {
                continue;
            }

            if let Ok(meta) = entry.metadata() {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                cache.insert(path.to_string_lossy().to_string(), (meta.len(), modified));

                if meta.is_dir() {
                    scan_directory_limited(
                        &path,
                        depth + 1,
                        max_depth,
                        exclude_patterns,
                        cache,
                        max_files,
                    );
                }
            }
        }
    }
}

fn calculate_depth(project_path: &str, file_path: &PathBuf) -> i32 {
    let project = PathBuf::from(project_path);
    file_path.components().count() as i32 - project.components().count() as i32
}

fn should_exclude(path: &PathBuf, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();

    for pattern in patterns {
        if pattern.contains('*') {
            let regex = pattern.replace(".", "\\.").replace("*", ".*");
            if let Ok(re) = regex::Regex::new(&regex) {
                if re.is_match(&file_name) {
                    return true;
                }
            }
        } else if path_str.contains(pattern) {
            return true;
        }
    }
    false
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// Tauri 命令 - 这些命令在 lib.rs 中定义，这里只保留内部函数
// 实际命令实现移到 lib.rs 以保持类型一致
