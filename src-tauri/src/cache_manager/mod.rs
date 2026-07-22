use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

use crate::thumbnail_cache::{self, ThumbnailSource};
use crate::tree_cache;

const PROGRESS_EVENT: &str = "pm-center:cache-maintenance-progress";
const CANCELLED_MESSAGE: &str = "缓存维护已取消";

lazy_static::lazy_static! {
    static ref ACTIVE_PROJECTS: Arc<Mutex<HashMap<String, String>>> =
        Arc::new(Mutex::new(HashMap::new()));
    static ref CANCELLED_OPERATIONS: Arc<Mutex<HashSet<String>>> =
        Arc::new(Mutex::new(HashSet::new()));
    static ref QUEUED_CACHE_EVENTS: Arc<Mutex<HashMap<String, HashMap<String, QueuedCacheEvent>>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheCategoryReport {
    pub id: String,
    pub label: String,
    pub physical_bytes: u64,
    pub logical_bytes: Option<u64>,
    pub entry_count: u64,
    pub anomaly_count: u64,
    pub status: String,
    pub protected: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TreeCacheSummary {
    pub entry_count: u64,
    pub directory_count: u64,
    pub dirty_directory_count: u64,
    pub file_details_count: u64,
    pub expired_file_details_count: u64,
    pub missing_file_details_sources: u64,
    pub file_details_logical_bytes: u64,
    pub last_full_scan_ts: Option<i64>,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheReport {
    pub project_path: String,
    pub pm_center_path: String,
    pub generated_at: i64,
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
    pub protected_bytes: u64,
    pub health_status: String,
    pub health_message: String,
    pub categories: Vec<CacheCategoryReport>,
    pub tree: TreeCacheSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheCheckIssue {
    pub code: String,
    pub category: String,
    pub severity: String,
    pub message: String,
    pub suggested_action: Option<String>,
    pub affected_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheCheckReport {
    pub mode: String,
    pub status: String,
    pub checked_at: i64,
    pub issues: Vec<CacheCheckIssue>,
    pub scanned_items: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheCheckMode {
    Quick,
    Deep,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheAction {
    ClearThumbnails,
    ClearFileDetails,
    RebuildTree,
    ClearReclaimable,
    ResetAndRepair,
}

impl CacheAction {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ClearThumbnails => "clearThumbnails",
            Self::ClearFileDetails => "clearFileDetails",
            Self::RebuildTree => "rebuildTree",
            Self::ClearReclaimable => "clearReclaimable",
            Self::ResetAndRepair => "resetAndRepair",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheActionResult {
    pub action: String,
    pub bytes_reclaimed: u64,
    pub affected_items: u64,
    pub report: CacheReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheMaintenanceProgress {
    operation_id: String,
    project_path: String,
    action: String,
    phase: String,
    processed_items: u64,
    total_items: Option<u64>,
    current_path: Option<String>,
    cancellable: bool,
}

#[derive(Debug, Clone)]
struct QueuedCacheEvent {
    path: String,
    is_dir: bool,
    affects_tree: bool,
}

struct MaintenanceGuard {
    project_path: String,
    operation_id: String,
}

impl MaintenanceGuard {
    fn acquire(project_path: &str, operation_id: &str) -> Result<Self, String> {
        let project_key = tree_cache::normalize_path_key(project_path);
        {
            let mut active = ACTIVE_PROJECTS.lock().map_err(|error| error.to_string())?;
            if let Some(existing) = active.get(&project_key) {
                return Err(format!("当前项目已有缓存维护任务：{}", existing));
            }
            active.insert(project_key, operation_id.to_string());
        }

        if !thumbnail_cache::begin_project_maintenance(project_path)? {
            let mut active = ACTIVE_PROJECTS.lock().map_err(|error| error.to_string())?;
            active.remove(&tree_cache::normalize_path_key(project_path));
            return Err("当前项目正在进行缓存维护".to_string());
        }

        if let Ok(mut cancelled) = CANCELLED_OPERATIONS.lock() {
            cancelled.remove(operation_id);
        }

        Ok(Self {
            project_path: project_path.to_string(),
            operation_id: operation_id.to_string(),
        })
    }
}

impl Drop for MaintenanceGuard {
    fn drop(&mut self) {
        thumbnail_cache::end_project_maintenance(&self.project_path);
        replay_queued_cache_events(&self.project_path);
        if let Ok(mut active) = ACTIVE_PROJECTS.lock() {
            active.remove(&tree_cache::normalize_path_key(&self.project_path));
        }
        if let Ok(mut cancelled) = CANCELLED_OPERATIONS.lock() {
            cancelled.remove(&self.operation_id);
        }
    }
}

pub fn is_project_under_maintenance(project_path: &str) -> bool {
    ACTIVE_PROJECTS
        .lock()
        .map(|active| active.contains_key(&tree_cache::normalize_path_key(project_path)))
        .unwrap_or(false)
}

pub fn queue_cache_event(project_path: &str, path: &str, is_dir: bool, affects_tree: bool) {
    let project_key = tree_cache::normalize_path_key(project_path);
    let path_key = tree_cache::normalize_path_key(path);
    if let Ok(mut projects) = QUEUED_CACHE_EVENTS.lock() {
        let events = projects.entry(project_key).or_default();
        events
            .entry(path_key)
            .and_modify(|event| {
                event.is_dir |= is_dir;
                event.affects_tree |= affects_tree;
            })
            .or_insert_with(|| QueuedCacheEvent {
                path: path.to_string(),
                is_dir,
                affects_tree,
            });
    }
}

fn replay_queued_cache_events(project_path: &str) {
    let events = QUEUED_CACHE_EVENTS
        .lock()
        .ok()
        .and_then(|mut projects| projects.remove(&tree_cache::normalize_path_key(project_path)))
        .map(|events| events.into_values().collect::<Vec<_>>())
        .unwrap_or_default();

    if events.is_empty() {
        return;
    }

    if let Ok(cache) = tree_cache::get_or_create_project_cache(project_path) {
        for event in events {
            let path = PathBuf::from(&event.path);
            if let Some(parent) = path.parent() {
                let _ = cache.mark_dir_dirty(&parent.to_string_lossy());
            }
            if event.is_dir {
                let _ = cache.mark_dir_dirty(&event.path);
                let _ = cache.invalidate_file_details_by_prefix(&event.path);
            } else {
                let _ = cache.invalidate_file_details(&event.path);
            }
            if event.affects_tree {
                let _ = cache.mark_tree_dirty();
            }
        }
    }
}

fn validate_project_path(project_path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(project_path);
    if !root.exists() || !root.is_dir() {
        return Err("项目目录不存在或不可访问".to_string());
    }
    Ok(root)
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn path_stats(path: &Path) -> (u64, u64) {
    if !path.exists() {
        return (0, 0);
    }
    if path.is_file() {
        return (
            1,
            fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0),
        );
    }

    let mut count = 0u64;
    let mut bytes = 0u64;
    for entry in walkdir::WalkDir::new(path)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            count += 1;
            bytes =
                bytes.saturating_add(entry.metadata().map(|metadata| metadata.len()).unwrap_or(0));
        }
    }
    (count, bytes)
}

fn database_group_stats(path: &Path) -> (u64, u64) {
    let mut count = 0u64;
    let mut bytes = 0u64;
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.to_string_lossy())),
        PathBuf::from(format!("{}-shm", path.to_string_lossy())),
        PathBuf::from(format!("{}-journal", path.to_string_lossy())),
    ] {
        if let Ok(metadata) = fs::metadata(candidate) {
            count += 1;
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    (count, bytes)
}

fn open_readonly_database(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())
}

fn database_quick_check(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("数据库尚未创建".to_string());
    }
    let conn = open_readonly_database(path)?;
    let result: String = conn
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(result)
    }
}

fn required_tables_exist(path: &Path, tables: &[&str]) -> Result<(), String> {
    let conn = open_readonly_database(path)?;
    for table in tables {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
                [table],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            return Err(format!("缺少数据表 {}", table));
        }
    }
    Ok(())
}

fn read_tree_summary(tree_db_path: &Path) -> Result<TreeCacheSummary, String> {
    if !tree_db_path.exists() {
        return Ok(TreeCacheSummary {
            is_dirty: true,
            ..Default::default()
        });
    }

    let conn = open_readonly_database(tree_db_path)?;
    let now = now_ts();
    let entry_count = conn
        .query_row("SELECT COUNT(*) FROM fs_entries", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| error.to_string())?;
    let directory_count = conn
        .query_row(
            "SELECT COUNT(*) FROM fs_entries WHERE is_dir = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
    let dirty_directory_count = conn
        .query_row(
            "SELECT COUNT(*) FROM dir_state WHERE is_dirty = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
    let (file_details_count, expired_count, logical_bytes): (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), SUM(CASE WHEN expires_at <= ?1 THEN 1 ELSE 0 END), COALESCE(SUM(LENGTH(payload_json)), 0) FROM file_details_cache",
            [now],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| error.to_string())?;
    let mut missing_sources = 0u64;
    {
        let mut statement = conn
            .prepare("SELECT path FROM file_details_cache")
            .map_err(|error| error.to_string())?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        for path in paths.flatten() {
            if !PathBuf::from(path).exists() {
                missing_sources += 1;
            }
        }
    }
    let (is_dirty_value, last_scan): (i64, Option<i64>) = conn
        .query_row(
            "SELECT is_dirty, last_full_scan_ts FROM tree_state WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or((1, None));

    Ok(TreeCacheSummary {
        entry_count: entry_count.max(0) as u64,
        directory_count: directory_count.max(0) as u64,
        dirty_directory_count: dirty_directory_count.max(0) as u64,
        file_details_count: file_details_count.max(0) as u64,
        expired_file_details_count: expired_count.max(0) as u64,
        missing_file_details_sources: missing_sources,
        file_details_logical_bytes: logical_bytes.max(0) as u64,
        last_full_scan_ts: last_scan,
        is_dirty: is_dirty_value != 0,
    })
}

fn is_valid_thumbnail_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let components = relative.components().collect::<Vec<_>>();
    if components.len() != 2 {
        return false;
    }
    let bucket = components[0].as_os_str().to_string_lossy();
    let file_name = components[1].as_os_str().to_string_lossy();
    bucket.len() == 2
        && bucket.chars().all(|ch| ch.is_ascii_hexdigit())
        && file_name.len() == 37
        && file_name.ends_with(".png")
        && file_name[..16].chars().all(|ch| ch.is_ascii_hexdigit())
        && file_name.as_bytes().get(16) == Some(&b'-')
        && file_name[17..33].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn thumbnail_anomalies(root: &Path) -> (u64, Vec<PathBuf>) {
    let mut anomalies = 0u64;
    let mut files = Vec::new();
    if !root.exists() {
        return (0, files);
    }
    for entry in walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.into_path();
        let is_zero = fs::metadata(&path)
            .map(|metadata| metadata.len() == 0)
            .unwrap_or(true);
        if is_zero || !is_valid_thumbnail_path(root, &path) {
            anomalies += 1;
        }
        files.push(path);
    }
    (anomalies, files)
}

fn compute_cache_report(project_path: &str) -> Result<CacheReport, String> {
    let project_root = validate_project_path(project_path)?;
    let pm_center = project_root.join(".pm_center");
    let thumbnails = pm_center.join("thumbnails");
    let tree_db = pm_center.join("tree_cache.db");
    let data_db = pm_center.join("data.db");
    let scripts = pm_center.join("scripts");
    let plugins = pm_center.join("plugins");

    let (total_files, total_bytes) = path_stats(&pm_center);
    let (thumbnail_count, thumbnail_bytes) = path_stats(&thumbnails);
    let (thumbnail_anomaly_count, _) = thumbnail_anomalies(&thumbnails);
    let (tree_file_count, tree_bytes) = database_group_stats(&tree_db);
    let (data_file_count, data_bytes) = database_group_stats(&data_db);
    let (script_count, script_bytes) = path_stats(&scripts);
    let (plugin_count, plugin_bytes) = path_stats(&plugins);

    let tree_result = read_tree_summary(&tree_db);
    let tree = tree_result.clone().unwrap_or_else(|_| TreeCacheSummary {
        is_dirty: true,
        ..Default::default()
    });

    let known_bytes = thumbnail_bytes
        .saturating_add(tree_bytes)
        .saturating_add(data_bytes)
        .saturating_add(script_bytes)
        .saturating_add(plugin_bytes);
    let known_files = thumbnail_count
        .saturating_add(tree_file_count)
        .saturating_add(data_file_count)
        .saturating_add(script_count)
        .saturating_add(plugin_count);
    let other_bytes = total_bytes.saturating_sub(known_bytes);
    let other_count = total_files.saturating_sub(known_files);

    let data_integrity = database_quick_check(&data_db);
    let tree_integrity = if tree_db.exists() {
        database_quick_check(&tree_db)
    } else {
        Err("目录树缓存尚未建立".to_string())
    };

    let mut health_status = "healthy".to_string();
    let mut health_message = "缓存状态正常".to_string();
    if data_integrity.is_err() || (tree_db.exists() && tree_integrity.is_err()) {
        health_status = "error".to_string();
        health_message = "检测到数据库完整性问题".to_string();
    } else if tree.is_dirty
        || tree.dirty_directory_count > 0
        || tree.expired_file_details_count > 0
        || tree.missing_file_details_sources > 0
        || thumbnail_anomaly_count > 0
        || !tree_db.exists()
    {
        health_status = "warning".to_string();
        health_message = "存在待修复或可清理的缓存".to_string();
    }

    let protected_bytes = data_bytes
        .saturating_add(script_bytes)
        .saturating_add(plugin_bytes)
        .saturating_add(other_bytes);
    let reclaimable_bytes = thumbnail_bytes.saturating_add(tree.file_details_logical_bytes);
    let categories = vec![
        CacheCategoryReport {
            id: "thumbnails".to_string(),
            label: "缩略图".to_string(),
            physical_bytes: thumbnail_bytes,
            logical_bytes: None,
            entry_count: thumbnail_count,
            anomaly_count: thumbnail_anomaly_count,
            status: if thumbnail_anomaly_count > 0 {
                "warning"
            } else {
                "healthy"
            }
            .to_string(),
            protected: false,
            description: "浏览文件时按需生成的 PNG 预览".to_string(),
        },
        CacheCategoryReport {
            id: "treeIndex".to_string(),
            label: "目录树索引".to_string(),
            physical_bytes: tree_bytes,
            logical_bytes: None,
            entry_count: tree.entry_count,
            anomaly_count: tree.dirty_directory_count + u64::from(tree.is_dirty),
            status: if tree_integrity.is_err() {
                "error"
            } else if tree.is_dirty || tree.dirty_directory_count > 0 {
                "warning"
            } else {
                "healthy"
            }
            .to_string(),
            protected: false,
            description: "tree_cache.db 物理占用，包含 WAL/SHM".to_string(),
        },
        CacheCategoryReport {
            id: "fileDetails".to_string(),
            label: "解析缓存".to_string(),
            physical_bytes: 0,
            logical_bytes: Some(tree.file_details_logical_bytes),
            entry_count: tree.file_details_count,
            anomaly_count: tree.expired_file_details_count + tree.missing_file_details_sources,
            status: if tree.expired_file_details_count + tree.missing_file_details_sources > 0 {
                "warning"
            } else {
                "healthy"
            }
            .to_string(),
            protected: false,
            description: "与目录索引共享 tree_cache.db，大小为 payload 逻辑估算".to_string(),
        },
        CacheCategoryReport {
            id: "projectData".to_string(),
            label: "项目数据".to_string(),
            physical_bytes: data_bytes,
            logical_bytes: None,
            entry_count: data_file_count,
            anomaly_count: u64::from(data_integrity.is_err()),
            status: if data_integrity.is_err() {
                "error"
            } else {
                "healthy"
            }
            .to_string(),
            protected: true,
            description: "标签、集合、元数据和变更记录，任何清理操作都不会删除".to_string(),
        },
        CacheCategoryReport {
            id: "extensions".to_string(),
            label: "脚本与插件".to_string(),
            physical_bytes: script_bytes.saturating_add(plugin_bytes),
            logical_bytes: None,
            entry_count: script_count.saturating_add(plugin_count),
            anomaly_count: 0,
            status: "healthy".to_string(),
            protected: true,
            description: "项目脚本与插件，受保护且不会清理".to_string(),
        },
        CacheCategoryReport {
            id: "other".to_string(),
            label: "其他文件".to_string(),
            physical_bytes: other_bytes,
            logical_bytes: None,
            entry_count: other_count,
            anomaly_count: 0,
            status: if other_count > 0 { "info" } else { "healthy" }.to_string(),
            protected: true,
            description: "无法归类的内容，仅展示，不自动删除".to_string(),
        },
    ];

    Ok(CacheReport {
        project_path: project_path.to_string(),
        pm_center_path: pm_center.to_string_lossy().to_string(),
        generated_at: now_ts(),
        total_bytes,
        reclaimable_bytes,
        protected_bytes,
        health_status,
        health_message,
        categories,
        tree,
    })
}

fn add_issue(
    issues: &mut Vec<CacheCheckIssue>,
    code: &str,
    category: &str,
    severity: &str,
    message: impl Into<String>,
    suggested_action: Option<&str>,
    affected_count: Option<u64>,
) {
    issues.push(CacheCheckIssue {
        code: code.to_string(),
        category: category.to_string(),
        severity: severity.to_string(),
        message: message.into(),
        suggested_action: suggested_action.map(str::to_string),
        affected_count,
    });
}

fn quick_check(project_path: &str) -> Result<CacheCheckReport, String> {
    let root = validate_project_path(project_path)?;
    let pm_center = root.join(".pm_center");
    let tree_db = pm_center.join("tree_cache.db");
    let data_db = pm_center.join("data.db");
    let thumbnails = pm_center.join("thumbnails");
    let mut issues = Vec::new();

    fs::create_dir_all(&pm_center).map_err(|error| error.to_string())?;
    let write_test = pm_center.join(format!(".cache-write-test-{}", now_ts()));
    if fs::write(&write_test, b"ok").is_err() {
        add_issue(
            &mut issues,
            "PM_CENTER_NOT_WRITABLE",
            "storage",
            "error",
            ".pm_center 目录不可写",
            None,
            None,
        );
    } else {
        let _ = fs::remove_file(write_test);
    }

    match database_quick_check(&data_db) {
        Ok(()) => {
            if let Err(error) = required_tables_exist(
                &data_db,
                &["tags", "file_metadata", "collections", "collection_items"],
            ) {
                add_issue(
                    &mut issues,
                    "DATA_DB_SCHEMA",
                    "projectData",
                    "error",
                    error,
                    None,
                    None,
                );
            }
        }
        Err(error) => add_issue(
            &mut issues,
            "DATA_DB_INTEGRITY",
            "projectData",
            "error",
            error,
            None,
            None,
        ),
    }

    if tree_db.exists() {
        match database_quick_check(&tree_db) {
            Ok(()) => {
                if let Err(error) = required_tables_exist(
                    &tree_db,
                    &[
                        "fs_entries",
                        "dir_state",
                        "tree_state",
                        "file_details_cache",
                    ],
                ) {
                    add_issue(
                        &mut issues,
                        "TREE_DB_SCHEMA",
                        "treeIndex",
                        "error",
                        error,
                        Some("rebuildTree"),
                        None,
                    );
                }
            }
            Err(error) => add_issue(
                &mut issues,
                "TREE_DB_INTEGRITY",
                "treeIndex",
                "error",
                error,
                Some("rebuildTree"),
                None,
            ),
        }
    } else {
        add_issue(
            &mut issues,
            "TREE_DB_MISSING",
            "treeIndex",
            "warning",
            "目录树缓存尚未建立",
            Some("rebuildTree"),
            None,
        );
    }

    if let Ok(tree) = read_tree_summary(&tree_db) {
        if tree.is_dirty || tree.dirty_directory_count > 0 {
            add_issue(
                &mut issues,
                "TREE_DIRTY",
                "treeIndex",
                "warning",
                "目录树包含待刷新的目录",
                Some("rebuildTree"),
                Some(tree.dirty_directory_count),
            );
        }
        if tree.expired_file_details_count > 0 {
            add_issue(
                &mut issues,
                "DETAILS_EXPIRED",
                "fileDetails",
                "warning",
                "存在已过期的解析缓存",
                Some("clearFileDetails"),
                Some(tree.expired_file_details_count),
            );
        }
        if tree.missing_file_details_sources > 0 {
            add_issue(
                &mut issues,
                "DETAILS_SOURCE_MISSING",
                "fileDetails",
                "warning",
                "解析缓存对应的源文件已不存在",
                Some("clearFileDetails"),
                Some(tree.missing_file_details_sources),
            );
        }
    }

    let (thumbnail_anomaly_count, _) = thumbnail_anomalies(&thumbnails);
    if thumbnail_anomaly_count > 0 {
        add_issue(
            &mut issues,
            "THUMBNAIL_OBVIOUS_INVALID",
            "thumbnails",
            "warning",
            "存在空文件或命名异常的缩略图",
            Some("clearThumbnails"),
            Some(thumbnail_anomaly_count),
        );
    }

    let status = check_status(&issues);
    Ok(CacheCheckReport {
        mode: "quick".to_string(),
        status,
        checked_at: now_ts(),
        issues,
        scanned_items: 0,
    })
}

fn check_status(issues: &[CacheCheckIssue]) -> String {
    if issues.iter().any(|issue| issue.severity == "error") {
        "error".to_string()
    } else if issues.iter().any(|issue| issue.severity == "warning") {
        "warning".to_string()
    } else {
        "healthy".to_string()
    }
}

fn is_cancelled(operation_id: &str) -> bool {
    CANCELLED_OPERATIONS
        .lock()
        .map(|cancelled| cancelled.contains(operation_id))
        .unwrap_or(false)
}

fn throw_if_cancelled(operation_id: &str) -> Result<(), String> {
    if is_cancelled(operation_id) {
        Err(CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

fn emit_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    project_path: &str,
    action: &str,
    phase: &str,
    processed_items: u64,
    total_items: Option<u64>,
    current_path: Option<String>,
    cancellable: bool,
) {
    let _ = app.emit(
        PROGRESS_EVENT,
        CacheMaintenanceProgress {
            operation_id: operation_id.to_string(),
            project_path: project_path.to_string(),
            action: action.to_string(),
            phase: phase.to_string(),
            processed_items,
            total_items,
            current_path,
            cancellable,
        },
    );
}

fn deep_check(
    app: &tauri::AppHandle,
    project_path: &str,
    operation_id: &str,
    exclude_patterns: &[String],
) -> Result<CacheCheckReport, String> {
    let _guard = MaintenanceGuard::acquire(project_path, operation_id)?;
    let root = validate_project_path(project_path)?;
    let tree_db = root.join(".pm_center").join("tree_cache.db");
    let thumbnail_root = root.join(".pm_center").join("thumbnails");
    let mut report = quick_check(project_path)?;
    report.mode = "deep".to_string();
    let mut disk_paths = HashSet::new();
    let mut expected_thumbnails = HashSet::new();
    let mut scanned = 0u64;

    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !tree_cache::should_skip_path_with_patterns(&root, entry.path(), exclude_patterns)
        })
        .filter_map(Result::ok)
    {
        throw_if_cancelled(operation_id)?;
        if entry.path() == root {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        disk_paths.insert(tree_cache::normalize_path_key(&path));
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                let extension = entry
                    .path()
                    .extension()
                    .map(|value| value.to_string_lossy().to_string().to_lowercase());
                let modified_ts = metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64);
                let source = ThumbnailSource {
                    path: path.clone(),
                    is_dir: false,
                    size: metadata.len(),
                    modified_ts,
                    extension,
                };
                if let Some(expected) =
                    thumbnail_cache::expected_thumbnail_path(project_path, &source)
                {
                    expected_thumbnails
                        .insert(tree_cache::normalize_path_key(&expected.to_string_lossy()));
                }
            }
        }
        scanned += 1;
        if scanned == 1 || scanned % 100 == 0 {
            emit_progress(
                app,
                operation_id,
                project_path,
                "deepCheck",
                "scanningProject",
                scanned,
                None,
                Some(path),
                true,
            );
        }
    }

    if tree_db.exists() {
        let conn = open_readonly_database(&tree_db)?;
        let mut statement = conn
            .prepare("SELECT path_key FROM fs_entries")
            .map_err(|error| error.to_string())?;
        let indexed_paths = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .flatten()
            .collect::<HashSet<_>>();
        let missing_count = disk_paths.difference(&indexed_paths).count() as u64;
        let stale_count = indexed_paths.difference(&disk_paths).count() as u64;
        if missing_count > 0 {
            add_issue(
                &mut report.issues,
                "TREE_MISSING_PATHS",
                "treeIndex",
                "warning",
                "磁盘中存在尚未进入目录索引的路径",
                Some("rebuildTree"),
                Some(missing_count),
            );
        }
        if stale_count > 0 {
            add_issue(
                &mut report.issues,
                "TREE_STALE_PATHS",
                "treeIndex",
                "warning",
                "目录索引中存在已经删除的路径",
                Some("rebuildTree"),
                Some(stale_count),
            );
        }
    }

    let (_, thumbnail_files) = thumbnail_anomalies(&thumbnail_root);
    let total_thumbnails = thumbnail_files.len() as u64;
    let mut corrupt_thumbnails = 0u64;
    let mut stale_thumbnails = 0u64;
    for (index, path) in thumbnail_files.into_iter().enumerate() {
        throw_if_cancelled(operation_id)?;
        let path_key = tree_cache::normalize_path_key(&path.to_string_lossy());
        if !expected_thumbnails.contains(&path_key) {
            stale_thumbnails += 1;
        }
        if image::io::Reader::open(&path)
            .and_then(|reader| reader.with_guessed_format())
            .ok()
            .and_then(|reader| reader.decode().ok())
            .is_none()
        {
            corrupt_thumbnails += 1;
        }
        if index == 0 || (index + 1) % 50 == 0 {
            emit_progress(
                app,
                operation_id,
                project_path,
                "deepCheck",
                "checkingThumbnails",
                (index + 1) as u64,
                Some(total_thumbnails),
                Some(path.to_string_lossy().to_string()),
                true,
            );
        }
    }
    if stale_thumbnails > 0 {
        add_issue(
            &mut report.issues,
            "THUMBNAIL_STALE",
            "thumbnails",
            "warning",
            "存在签名已过期或源文件已删除的缩略图",
            Some("clearThumbnails"),
            Some(stale_thumbnails),
        );
    }
    if corrupt_thumbnails > 0 {
        add_issue(
            &mut report.issues,
            "THUMBNAIL_CORRUPT",
            "thumbnails",
            "warning",
            "存在无法解码的缩略图",
            Some("clearThumbnails"),
            Some(corrupt_thumbnails),
        );
    }

    report.scanned_items = scanned.saturating_add(total_thumbnails);
    report.checked_at = now_ts();
    report.status = check_status(&report.issues);
    emit_progress(
        app,
        operation_id,
        project_path,
        "deepCheck",
        "completed",
        report.scanned_items,
        Some(report.scanned_items),
        None,
        false,
    );
    Ok(report)
}

fn wait_for_thumbnail_jobs(operation_id: &str, project_path: &str) -> Result<(), String> {
    while thumbnail_cache::project_in_flight_count(project_path) > 0 {
        throw_if_cancelled(operation_id)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn clear_thumbnails(operation_id: &str, project_path: &str) -> Result<(u64, u64), String> {
    wait_for_thumbnail_jobs(operation_id, project_path)?;
    let (count, bytes) = thumbnail_cache::clear_project_thumbnail_cache(project_path)?;
    Ok((count as u64, bytes))
}

fn clear_file_details(project_path: &str) -> Result<u64, String> {
    let cache = tree_cache::get_or_create_project_cache(project_path)?;
    cache
        .clear_file_details_cache_and_compact()
        .map(|count| count as u64)
}

fn run_action(
    app: &tauri::AppHandle,
    project_path: &str,
    action: CacheAction,
    operation_id: &str,
    exclude_patterns: &[String],
) -> Result<CacheActionResult, String> {
    let _guard = MaintenanceGuard::acquire(project_path, operation_id)?;
    let before = compute_cache_report(project_path)?;
    let action_name = action.as_str();
    emit_progress(
        app,
        operation_id,
        project_path,
        action_name,
        "preparing",
        0,
        None,
        None,
        true,
    );
    throw_if_cancelled(operation_id)?;

    let affected = match action {
        CacheAction::ClearThumbnails => {
            emit_progress(
                app,
                operation_id,
                project_path,
                action_name,
                "clearingThumbnails",
                0,
                Some(before.categories[0].entry_count),
                None,
                true,
            );
            let (count, _) = clear_thumbnails(operation_id, project_path)?;
            count
        }
        CacheAction::ClearFileDetails => {
            emit_progress(
                app,
                operation_id,
                project_path,
                action_name,
                "clearingFileDetails",
                0,
                Some(before.tree.file_details_count),
                None,
                false,
            );
            clear_file_details(project_path)?
        }
        CacheAction::RebuildTree => tree_cache::rebuild_project_tree_cache_with_control(
            project_path,
            exclude_patterns,
            || is_cancelled(operation_id),
            |processed, path| {
                emit_progress(
                    app,
                    operation_id,
                    project_path,
                    action_name,
                    "rebuildingTree",
                    processed as u64,
                    None,
                    Some(path.to_string_lossy().to_string()),
                    true,
                );
            },
        )? as u64,
        CacheAction::ClearReclaimable => {
            emit_progress(
                app,
                operation_id,
                project_path,
                action_name,
                "clearingThumbnails",
                0,
                Some(before.categories[0].entry_count),
                None,
                true,
            );
            let (thumbnail_count, _) = clear_thumbnails(operation_id, project_path)?;
            throw_if_cancelled(operation_id)?;
            emit_progress(
                app,
                operation_id,
                project_path,
                action_name,
                "clearingFileDetails",
                0,
                Some(before.tree.file_details_count),
                None,
                false,
            );
            thumbnail_count.saturating_add(clear_file_details(project_path)?)
        }
        CacheAction::ResetAndRepair => {
            emit_progress(
                app,
                operation_id,
                project_path,
                action_name,
                "clearingThumbnails",
                0,
                Some(before.categories[0].entry_count),
                None,
                true,
            );
            let (thumbnail_count, _) = clear_thumbnails(operation_id, project_path)?;
            throw_if_cancelled(operation_id)?;
            emit_progress(
                app,
                operation_id,
                project_path,
                action_name,
                "clearingFileDetails",
                0,
                Some(before.tree.file_details_count),
                None,
                false,
            );
            let details_count = clear_file_details(project_path)?;
            thumbnail_count
                .saturating_add(details_count)
                .saturating_add(tree_cache::rebuild_project_tree_cache_with_control(
                    project_path,
                    exclude_patterns,
                    || is_cancelled(operation_id),
                    |processed, path| {
                        emit_progress(
                            app,
                            operation_id,
                            project_path,
                            action_name,
                            "rebuildingTree",
                            processed as u64,
                            None,
                            Some(path.to_string_lossy().to_string()),
                            true,
                        );
                    },
                )? as u64)
        }
    };

    throw_if_cancelled(operation_id)?;
    let report = compute_cache_report(project_path)?;
    let bytes_reclaimed = before.total_bytes.saturating_sub(report.total_bytes);
    emit_progress(
        app,
        operation_id,
        project_path,
        action_name,
        "completed",
        affected,
        Some(affected),
        None,
        false,
    );
    Ok(CacheActionResult {
        action: action_name.to_string(),
        bytes_reclaimed,
        affected_items: affected,
        report,
    })
}

#[tauri::command]
pub async fn get_project_cache_report(project_path: String) -> Result<CacheReport, String> {
    tokio::task::spawn_blocking(move || compute_cache_report(&project_path))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn check_project_cache(
    app: tauri::AppHandle,
    project_path: String,
    mode: CacheCheckMode,
    operation_id: Option<String>,
    exclude_patterns: Option<Vec<String>>,
) -> Result<CacheCheckReport, String> {
    tokio::task::spawn_blocking(move || match mode {
        CacheCheckMode::Quick => quick_check(&project_path),
        CacheCheckMode::Deep => {
            let operation_id = operation_id
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "深度检查缺少 operationId".to_string())?;
            deep_check(
                &app,
                &project_path,
                &operation_id,
                exclude_patterns.as_deref().unwrap_or(&[]),
            )
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn run_project_cache_action(
    app: tauri::AppHandle,
    project_path: String,
    action: CacheAction,
    operation_id: String,
    exclude_patterns: Option<Vec<String>>,
) -> Result<CacheActionResult, String> {
    if operation_id.trim().is_empty() {
        return Err("缓存维护缺少 operationId".to_string());
    }
    tokio::task::spawn_blocking(move || {
        run_action(
            &app,
            &project_path,
            action,
            &operation_id,
            exclude_patterns.as_deref().unwrap_or(&[]),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn cancel_cache_maintenance(operation_id: String) -> Result<(), String> {
    if operation_id.trim().is_empty() {
        return Ok(());
    }
    let mut cancelled = CANCELLED_OPERATIONS
        .lock()
        .map_err(|error| error.to_string())?;
    cancelled.insert(operation_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("pm-center-cache-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join(".pm_center/thumbnails/ab")).unwrap();
        fs::create_dir_all(root.join(".pm_center/scripts")).unwrap();
        fs::create_dir_all(root.join(".pm_center/plugins")).unwrap();
        fs::write(root.join("scene.blend"), b"blend").unwrap();
        root
    }

    #[test]
    fn cache_report_keeps_project_data_protected() {
        let root = test_project();
        let _ = crate::db::Database::new(&root.to_string_lossy()).unwrap();
        let report = compute_cache_report(&root.to_string_lossy()).unwrap();
        let project_data = report
            .categories
            .iter()
            .find(|item| item.id == "projectData")
            .unwrap();
        assert!(project_data.protected);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clear_thumbnails_does_not_touch_protected_files() {
        let root = test_project();
        let data_path = root.join(".pm_center/data.db");
        fs::write(&data_path, b"protected").unwrap();
        fs::write(root.join(".pm_center/thumbnails/ab/file.png"), b"cache").unwrap();
        thumbnail_cache::begin_project_maintenance(&root.to_string_lossy()).unwrap();
        let result =
            thumbnail_cache::clear_project_thumbnail_cache(&root.to_string_lossy()).unwrap();
        thumbnail_cache::end_project_maintenance(&root.to_string_lossy());
        assert_eq!(result.0, 1);
        assert_eq!(fs::read(data_path).unwrap(), b"protected");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rebuild_matches_disk_and_respects_excludes() {
        let root = test_project();
        fs::create_dir_all(root.join("assets/nested")).unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        fs::write(root.join("assets/nested/scene.txt"), b"scene").unwrap();
        fs::write(root.join("ignored/secret.txt"), b"secret").unwrap();

        let project_path = root.to_string_lossy().to_string();
        let count = tree_cache::rebuild_project_tree_cache_with_control(
            &project_path,
            &["ignored".to_string()],
            || false,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(count, 4);

        let cache = tree_cache::get_or_create_project_cache(&project_path).unwrap();
        let root_entries = cache.get_directory_entries(&project_path).unwrap();
        assert_eq!(root_entries.len(), 2);
        assert!(root_entries.iter().any(|entry| entry.name == "assets"));
        assert!(root_entries.iter().any(|entry| entry.name == "scene.blend"));
        assert!(!root_entries.iter().any(|entry| entry.name == ".pm_center"));
        assert!(!root_entries.iter().any(|entry| entry.name == "ignored"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancelled_rebuild_preserves_previous_snapshot() {
        let root = test_project();
        let old_path = root.join("old.txt");
        fs::write(&old_path, b"old").unwrap();
        let project_path = root.to_string_lossy().to_string();
        tree_cache::rebuild_project_tree_cache(&project_path).unwrap();

        fs::remove_file(&old_path).unwrap();
        fs::write(root.join("new.txt"), b"new").unwrap();
        let result = tree_cache::rebuild_project_tree_cache_with_control(
            &project_path,
            &[],
            || true,
            |_, _| {},
        );
        assert_eq!(
            result.unwrap_err(),
            tree_cache::TREE_REBUILD_CANCELLED_MESSAGE
        );

        let cache = tree_cache::get_or_create_project_cache(&project_path).unwrap();
        let entries = cache.get_directory_entries(&project_path).unwrap();
        assert!(entries.iter().any(|entry| entry.name == "old.txt"));
        assert!(!entries.iter().any(|entry| entry.name == "new.txt"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_project_database_is_reported_without_modification() {
        let root = test_project();
        let project_path = root.to_string_lossy().to_string();
        let _ = tree_cache::get_or_create_project_cache(&project_path).unwrap();
        let data_path = root.join(".pm_center/data.db");
        let corrupt_bytes = b"not a sqlite database";
        fs::write(&data_path, corrupt_bytes).unwrap();

        let report = compute_cache_report(&project_path).unwrap();
        assert_eq!(report.health_status, "error");
        let check = quick_check(&project_path).unwrap();
        assert!(check
            .issues
            .iter()
            .any(|issue| issue.code == "DATA_DB_INTEGRITY"));
        assert_eq!(fs::read(&data_path).unwrap(), corrupt_bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_details_cleanup_preserves_project_data_scripts_and_plugins() {
        let root = test_project();
        let project_path = root.to_string_lossy().to_string();
        let _ = crate::db::Database::new(&project_path).unwrap();
        let script_path = root.join(".pm_center/scripts/custom.py");
        let plugin_path = root.join(".pm_center/plugins/custom.json");
        fs::write(&script_path, b"print('protected')").unwrap();
        fs::write(&plugin_path, b"{\"protected\":true}").unwrap();
        let data_before = fs::read(root.join(".pm_center/data.db")).unwrap();
        let script_before = fs::read(&script_path).unwrap();
        let plugin_before = fs::read(&plugin_path).unwrap();

        let cache = tree_cache::get_or_create_project_cache(&project_path).unwrap();
        let missing_source = root.join("missing.blend").to_string_lossy().to_string();
        cache
            .upsert_file_details_payload(&missing_source, "sig", "{\"value\":1}", -1)
            .unwrap();
        let before = compute_cache_report(&project_path).unwrap();
        assert_eq!(before.tree.expired_file_details_count, 1);
        assert_eq!(before.tree.missing_file_details_sources, 1);
        assert_eq!(clear_file_details(&project_path).unwrap(), 1);

        assert_eq!(
            fs::read(root.join(".pm_center/data.db")).unwrap(),
            data_before
        );
        assert_eq!(fs::read(&script_path).unwrap(), script_before);
        assert_eq!(fs::read(&plugin_path).unwrap(), plugin_before);
        let _ = fs::remove_dir_all(root);
    }
}
