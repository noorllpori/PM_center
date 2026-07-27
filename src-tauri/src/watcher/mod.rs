use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

use crate::cache_manager;
use crate::db::{Database, FileChange};
use crate::tree_cache::{
    get_or_create_project_cache, normalize_path_key, should_skip_path_with_patterns,
};

const EVENT_BATCH_WINDOW: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeType {
    Created,
    Modified,
    Deleted,
}

impl ChangeType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailCacheUpdatedEvent {
    pub project_path: String,
    pub directory_path: String,
    pub updated_count: usize,
}

#[derive(Debug)]
struct PendingEvent {
    path: PathBuf,
    change_type: ChangeType,
    is_dir: bool,
    is_rename: bool,
}

lazy_static::lazy_static! {
    static ref ACTIVE_WATCHER: Arc<Mutex<Option<RecommendedWatcher>>> =
        Arc::new(Mutex::new(None));
    static ref ACTIVE_WATCHER_WORKER: Arc<Mutex<Option<JoinHandle<()>>>> =
        Arc::new(Mutex::new(None));
    static ref ACTIVE_PROJECT: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    static ref WATCHER_STATE_LOCK: Mutex<()> = Mutex::new(());
    static ref APP_HANDLE: Arc<Mutex<Option<tauri::AppHandle>>> = Arc::new(Mutex::new(None));
}

pub fn set_app_handle(app_handle: tauri::AppHandle) {
    if let Ok(mut handle) = APP_HANDLE.lock() {
        *handle = Some(app_handle);
    }
}

pub fn set_active_project(
    path: &str,
    db: &Database,
    exclude_patterns: &[String],
) -> Result<(), String> {
    let project_root = PathBuf::from(path);
    if !project_root.is_dir() {
        return Err("项目目录不存在或不可访问".to_string());
    }

    let _state_guard = WATCHER_STATE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    stop_active_watcher();

    let (sender, receiver) = mpsc::channel::<Result<Event, notify::Error>>();
    let callback_sender = sender.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result| {
            let _ = callback_sender.send(result);
        },
        Config::default(),
    )
    .map_err(|error| error.to_string())?;

    watcher
        .watch(&project_root, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;

    let project_path = path.to_string();
    let worker_project_path = project_path.clone();
    let worker_db = db.clone();
    let worker_excludes = exclude_patterns.to_vec();
    let worker = std::thread::Builder::new()
        .name("pm-center-watcher".to_string())
        .spawn(move || {
            run_event_worker(receiver, &worker_project_path, &worker_db, &worker_excludes)
        })
        .map_err(|error| error.to_string())?;

    drop(sender);
    if let Ok(mut active_project) = ACTIVE_PROJECT.lock() {
        *active_project = Some(project_path);
    }
    let mut active_watcher = ACTIVE_WATCHER.lock().map_err(|error| error.to_string())?;
    *active_watcher = Some(watcher);
    let mut active_worker = ACTIVE_WATCHER_WORKER
        .lock()
        .map_err(|error| error.to_string())?;
    *active_worker = Some(worker);
    Ok(())
}

pub fn get_active_project_path() -> Option<String> {
    ACTIVE_PROJECT.lock().ok().and_then(|path| path.clone())
}

/// Stops the active watcher only when it belongs to the project being closed.
/// This avoids a late cleanup from a closed tab stopping a watcher that was
/// already switched to another project.
pub fn clear_active_project_if_matches(project_path: &str) -> bool {
    let expected_key = normalize_path_key(project_path);
    let _state_guard = match WATCHER_STATE_LOCK.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    let matches_project = ACTIVE_PROJECT
        .lock()
        .ok()
        .and_then(|active| {
            active
                .as_ref()
                .map(|path| normalize_path_key(path) == expected_key)
        })
        .unwrap_or(false);

    if !matches_project {
        return false;
    }

    stop_active_watcher();
    true
}

fn stop_active_watcher() {
    let watcher = ACTIVE_WATCHER
        .lock()
        .ok()
        .and_then(|mut value| value.take());
    if let Ok(mut active_project) = ACTIVE_PROJECT.lock() {
        *active_project = None;
    }

    // Dropping notify's watcher disconnects its event channel. Join the worker
    // before releasing cache handles so it cannot flush a final stale batch.
    drop(watcher);
    let worker = ACTIVE_WATCHER_WORKER
        .lock()
        .ok()
        .and_then(|mut value| value.take());
    if let Some(worker) = worker {
        let _ = worker.join();
    }
}

fn run_event_worker(
    receiver: mpsc::Receiver<Result<Event, notify::Error>>,
    project_path: &str,
    db: &Database,
    exclude_patterns: &[String],
) {
    let mut batch = Vec::new();

    loop {
        match receiver.recv_timeout(EVENT_BATCH_WINDOW) {
            Ok(Ok(event)) => batch.push(event),
            Ok(Err(_)) => mark_tree_dirty(project_path),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush_events(project_path, db, exclude_patterns, &mut batch)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush_events(project_path, db, exclude_patterns, &mut batch);
                break;
            }
        }
    }
}

fn flush_events(
    project_path: &str,
    db: &Database,
    exclude_patterns: &[String],
    events: &mut Vec<Event>,
) {
    if events.is_empty() {
        return;
    }

    let project_root = PathBuf::from(project_path);
    let mut pending = HashMap::<String, PendingEvent>::new();
    for event in events.drain(..) {
        let is_rename = matches!(
            event.kind,
            EventKind::Modify(notify::event::ModifyKind::Name(_))
        );
        let change_type = match event.kind {
            EventKind::Create(_) => ChangeType::Created,
            EventKind::Modify(_) => ChangeType::Modified,
            EventKind::Remove(_) => ChangeType::Deleted,
            EventKind::Other => {
                mark_tree_dirty(project_path);
                continue;
            }
            _ => continue,
        };

        for path in event.paths {
            if should_skip_path_with_patterns(&project_root, &path, exclude_patterns) {
                continue;
            }
            let is_dir = if change_type == ChangeType::Deleted {
                false
            } else {
                path.metadata()
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false)
            };
            if is_dir && change_type == ChangeType::Modified && !is_rename {
                continue;
            }

            pending.insert(
                normalize_path_key(&path.to_string_lossy()),
                PendingEvent {
                    path,
                    change_type,
                    is_dir,
                    is_rename,
                },
            );
        }
    }

    if pending.is_empty() {
        return;
    }

    let timestamp = current_timestamp();
    let mut db_changes = Vec::with_capacity(pending.len());
    for event in pending.into_values() {
        let path_string = event.path.to_string_lossy().to_string();
        let affects_tree = event.is_rename
            || matches!(event.change_type, ChangeType::Created | ChangeType::Deleted);

        if cache_manager::is_project_under_maintenance(project_path) {
            cache_manager::queue_cache_event(
                project_path,
                &path_string,
                event.is_dir,
                affects_tree,
            );
        } else {
            invalidate_cache_for_event(project_path, &event.path, event.is_dir, affects_tree);
        }

        let file_size = if event.change_type != ChangeType::Deleted && !event.is_dir {
            event
                .path
                .metadata()
                .ok()
                .map(|metadata| metadata.len() as i64)
        } else {
            None
        };
        db_changes.push(FileChange {
            id: 0,
            project_path: project_path.to_string(),
            file_path: path_string.clone(),
            change_type: event.change_type.as_str().to_string(),
            file_size,
            timestamp,
            depth: calculate_depth(&project_root, &event.path),
        });

        emit_project_fs_change(ProjectFsChangeEvent {
            project_path: project_path.to_string(),
            file_path: path_string,
            change_type: if event.is_rename {
                "renamed".to_string()
            } else {
                event.change_type.as_str().to_string()
            },
            is_dir: event.is_dir,
            is_rename: event.is_rename,
            timestamp,
        });
    }

    let _ = db.add_file_changes_batch(&db_changes);
}

fn invalidate_cache_for_event(project_path: &str, path: &Path, is_dir: bool, affects_tree: bool) {
    let Ok(cache) = get_or_create_project_cache(project_path) else {
        return;
    };
    let path_string = path.to_string_lossy().to_string();
    if let Some(parent) = path.parent() {
        let _ = cache.mark_dir_dirty(&parent.to_string_lossy());
    }
    if is_dir {
        let _ = cache.mark_dir_dirty(&path_string);
        let _ = cache.invalidate_file_details_by_prefix(&path_string);
    } else {
        let _ = cache.invalidate_file_details(&path_string);
    }
    if affects_tree {
        let _ = cache.mark_tree_dirty();
    }
}

fn mark_tree_dirty(project_path: &str) {
    if cache_manager::is_project_under_maintenance(project_path) {
        cache_manager::queue_cache_event(project_path, project_path, true, true);
        return;
    }
    if let Ok(cache) = get_or_create_project_cache(project_path) {
        let _ = cache.mark_tree_dirty();
    }
}

fn calculate_depth(project_root: &Path, path: &Path) -> i32 {
    path.components()
        .count()
        .saturating_sub(project_root.components().count()) as i32
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn emit_project_fs_change(payload: ProjectFsChangeEvent) {
    if let Ok(handle) = APP_HANDLE.lock() {
        if let Some(app_handle) = handle.as_ref() {
            let _ = app_handle.emit("pm-center:project-fs-change", payload);
        }
    }
}

pub fn emit_thumbnail_cache_updated(payload: ThumbnailCacheUpdatedEvent) {
    if let Ok(handle) = APP_HANDLE.lock() {
        if let Some(app_handle) = handle.as_ref() {
            let _ = app_handle.emit("pm-center:thumbnail-cache-updated", payload);
        }
    }
}
