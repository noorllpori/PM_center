//! Global media-library storage.
//!
//! A media library is an ordinary directory chosen by the user.  Its catalogue
//! lives at `<library>/.pm_center/media_catalog.db`; this is deliberately
//! separate from a project `data.db` and does not register a project watcher.

use blake3::Hasher;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::SystemTime;
use tauri::Manager;
use uuid::Uuid;

pub const MEDIA_LIBRARY_MODULE_ID: &str = "builtin.media-library";
pub const MEDIA_LIBRARY_DISABLED: &str = "MEDIA_LIBRARY_MODULE_DISABLED";
pub const MEDIA_LIBRARY_STARTING: &str = "MEDIA_LIBRARY_MODULE_STARTING";
pub const MEDIA_LIBRARY_STOPPING: &str = "MEDIA_LIBRARY_MODULE_STOPPING";

const MEDIA_CATALOG_DIRECTORY: &str = ".pm_center";
const MEDIA_CATALOG_FILE: &str = "media_catalog.db";
const MEDIA_ARCHIVE_DIRECTORY: &str = "media";
const REGISTRY_FILE: &str = "media-library-registry.json";
const PHASE_DISABLED: u8 = 0;
const PHASE_STARTING: u8 = 1;
const PHASE_RUNNING: u8 = 2;
const PHASE_STOPPING: u8 = 3;

static PHASE: AtomicU8 = AtomicU8::new(PHASE_DISABLED);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLibraryBookmark {
    pub root_path: String,
    pub display_name: String,
    pub last_opened_at: i64,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLibraryInfo {
    pub root_path: String,
    pub display_name: String,
    pub catalog_path: String,
    pub archive_path: String,
    pub item_count: i64,
    pub duplicate_group_count: i64,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCatalogItem {
    pub id: String,
    pub name: String,
    pub media_kind: String,
    pub status: String,
    pub primary_path: String,
    pub size: i64,
    pub modified_at: Option<i64>,
    pub imported_at: i64,
    pub updated_at: i64,
    pub rating: i64,
    pub note: String,
    pub tags: Vec<String>,
    pub location_count: i64,
    pub duplicate_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCollection {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub item_count: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub item_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLibrarySnapshot {
    pub library: MediaLibraryInfo,
    pub items: Vec<MediaCatalogItem>,
    pub collections: Vec<MediaCollection>,
    pub tags: Vec<MediaTag>,
    pub total_items: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLibraryQuery {
    pub library_path: String,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub tag_id: Option<String>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMediaItemsRequest {
    pub library_path: String,
    pub paths: Vec<String>,
    /// `reference` does not change the file; `copy` and `move` place new
    /// content under the media-library archive directory.
    pub mode: String,
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub tag_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaImportItemResult {
    pub source_path: String,
    pub item_id: Option<String>,
    pub outcome: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaImportResult {
    pub imported: usize,
    pub duplicates_linked: usize,
    pub failed: usize,
    pub items: Vec<MediaImportItemResult>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMediaCollectionRequest {
    pub library_path: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMediaAnnotationRequest {
    pub library_path: String,
    pub item_id: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub rating: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMediaItemTagsRequest {
    pub library_path: String,
    pub item_id: String,
    #[serde(default)]
    pub tag_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaLibraryRegistry {
    schema_version: u16,
    libraries: Vec<MediaLibraryBookmark>,
}

impl Default for MediaLibraryRegistry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            libraries: Vec::new(),
        }
    }
}

pub fn initialize_lifecycle_control() {
    PHASE.store(PHASE_DISABLED, Ordering::SeqCst);
}

pub fn set_initial_desired_enabled(desired_enabled: bool) {
    PHASE.store(
        if desired_enabled {
            PHASE_STARTING
        } else {
            PHASE_DISABLED
        },
        Ordering::SeqCst,
    );
}

pub fn start_runtime() {
    PHASE.store(PHASE_RUNNING, Ordering::SeqCst);
}

pub async fn stop_runtime() -> Result<(), String> {
    PHASE.store(PHASE_STOPPING, Ordering::SeqCst);
    // Connections are request-scoped, so there are no workers or filesystem
    // handles to retain after the module stops.
    PHASE.store(PHASE_DISABLED, Ordering::SeqCst);
    Ok(())
}

pub fn is_running() -> bool {
    PHASE.load(Ordering::SeqCst) == PHASE_RUNNING
}

pub fn ensure_running() -> Result<(), String> {
    match PHASE.load(Ordering::SeqCst) {
        PHASE_RUNNING => Ok(()),
        PHASE_STARTING => Err(format!("{MEDIA_LIBRARY_STARTING}: 媒体资料库正在启动")),
        PHASE_STOPPING => Err(format!("{MEDIA_LIBRARY_STOPPING}: 媒体资料库正在停止")),
        _ => Err(format!("{MEDIA_LIBRARY_DISABLED}: 媒体资料库已停用")),
    }
}

pub async fn wait_until_running() -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match PHASE.load(Ordering::SeqCst) {
            PHASE_RUNNING => return Ok(()),
            PHASE_STARTING if std::time::Instant::now() < deadline => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            _ => return ensure_running(),
        }
    }
}

fn now() -> i64 {
    Utc::now().timestamp_millis()
}

fn normalize_root(path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err("媒体资料库必须是一个存在的目录".into());
    }
    root.canonicalize()
        .map_err(|error| format!("无法解析媒体资料库目录: {error}"))
}

fn catalog_path(root: &Path) -> PathBuf {
    root.join(MEDIA_CATALOG_DIRECTORY).join(MEDIA_CATALOG_FILE)
}

fn archive_path(root: &Path) -> PathBuf {
    root.join(MEDIA_ARCHIVE_DIRECTORY)
}

fn open_catalog(root: &Path) -> Result<Connection, String> {
    let catalog = catalog_path(root);
    let parent = catalog
        .parent()
        .ok_or_else(|| "媒体资料库数据库路径无效".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建媒体资料库目录失败: {error}"))?;
    fs::create_dir_all(archive_path(root))
        .map_err(|error| format!("创建媒体归档目录失败: {error}"))?;
    let connection =
        Connection::open(&catalog).map_err(|error| format!("打开媒体资料库失败: {error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|error| format!("初始化媒体资料库失败: {error}"))?;
    initialize_schema(&connection).map_err(|error| format!("初始化媒体资料库表失败: {error}"))?;
    Ok(connection)
}

fn initialize_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS media_items (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          media_kind TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'active',
          created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS media_locations (
          id TEXT PRIMARY KEY,
          item_id TEXT NOT NULL,
          path TEXT NOT NULL UNIQUE,
          original_path TEXT,
          ingest_mode TEXT NOT NULL,
          is_primary INTEGER NOT NULL DEFAULT 0,
          exists_on_disk INTEGER NOT NULL DEFAULT 1,
          size INTEGER NOT NULL DEFAULT 0,
          modified_at INTEGER,
          added_at INTEGER NOT NULL,
          FOREIGN KEY(item_id) REFERENCES media_items(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS media_hashes (
          item_id TEXT PRIMARY KEY,
          blake3 TEXT NOT NULL UNIQUE,
          file_size INTEGER NOT NULL,
          modified_at INTEGER,
          FOREIGN KEY(item_id) REFERENCES media_items(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS media_collections (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL COLLATE NOCASE UNIQUE,
          color TEXT,
          created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS media_collection_members (
          collection_id TEXT NOT NULL,
          item_id TEXT NOT NULL,
          added_at INTEGER NOT NULL,
          PRIMARY KEY(collection_id, item_id),
          FOREIGN KEY(collection_id) REFERENCES media_collections(id) ON DELETE CASCADE,
          FOREIGN KEY(item_id) REFERENCES media_items(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS media_tags (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL COLLATE NOCASE UNIQUE,
          color TEXT,
          created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS media_item_tags (
          item_id TEXT NOT NULL,
          tag_id TEXT NOT NULL,
          PRIMARY KEY(item_id, tag_id),
          FOREIGN KEY(item_id) REFERENCES media_items(id) ON DELETE CASCADE,
          FOREIGN KEY(tag_id) REFERENCES media_tags(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS media_annotations (
          item_id TEXT PRIMARY KEY,
          note TEXT NOT NULL DEFAULT '',
          rating INTEGER NOT NULL DEFAULT 0 CHECK(rating >= 0 AND rating <= 5),
          source TEXT,
          owner TEXT,
          copyright TEXT,
          updated_at INTEGER NOT NULL,
          FOREIGN KEY(item_id) REFERENCES media_items(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS media_archive_events (
          id TEXT PRIMARY KEY,
          item_id TEXT NOT NULL,
          event_kind TEXT NOT NULL,
          source_path TEXT,
          destination_path TEXT,
          details_json TEXT NOT NULL DEFAULT '{}',
          created_at INTEGER NOT NULL,
          FOREIGN KEY(item_id) REFERENCES media_items(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS media_derived_assets (
          id TEXT PRIMARY KEY,
          item_id TEXT NOT NULL,
          asset_kind TEXT NOT NULL,
          path TEXT NOT NULL,
          created_at INTEGER NOT NULL,
          FOREIGN KEY(item_id) REFERENCES media_items(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_media_items_updated ON media_items(updated_at DESC, id);
        CREATE INDEX IF NOT EXISTS idx_media_locations_item ON media_locations(item_id, is_primary DESC);
        CREATE INDEX IF NOT EXISTS idx_media_hashes_blake3 ON media_hashes(blake3);
        CREATE INDEX IF NOT EXISTS idx_media_collection_members_item ON media_collection_members(item_id);
        CREATE INDEX IF NOT EXISTS idx_media_item_tags_tag ON media_item_tags(tag_id, item_id);
        CREATE INDEX IF NOT EXISTS idx_media_archive_events_item ON media_archive_events(item_id, created_at DESC);
        "#,
    )
}

fn file_modified_at(metadata: &fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
}

fn hash_file(path: &Path) -> Result<(String, i64, Option<i64>), String> {
    let metadata = fs::metadata(path).map_err(|error| format!("读取媒体文件失败: {error}"))?;
    if !metadata.is_file() {
        return Err("只能收集普通文件，目录请先在系统中展开后选择文件".into());
    }
    let mut file = fs::File::open(path).map_err(|error| format!("打开媒体文件失败: {error}"))?;
    let mut hasher = Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("读取媒体文件失败: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok((
        hasher.finalize().to_hex().to_string(),
        metadata.len().min(i64::MAX as u64) as i64,
        file_modified_at(&metadata),
    ))
}

fn media_kind(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif" | "tif" | "tiff" | "exr" | "hdr" => {
            Some("image")
        }
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" => Some("video"),
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" => Some("audio"),
        "blend" | "psd" | "ai" | "pdf" => Some("reference"),
        _ => None,
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn archive_destination(root: &Path, item_id: &str, source: &Path) -> Result<PathBuf, String> {
    let directory = archive_path(root).join(&item_id[..8]);
    fs::create_dir_all(&directory).map_err(|error| format!("创建媒体归档目录失败: {error}"))?;
    let filename = source
        .file_name()
        .ok_or_else(|| "媒体文件名无效".to_string())?;
    Ok(directory.join(filename))
}

fn copy_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "媒体归档目标目录无效".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = destination.with_extension(format!(
        "{}.nexora-import-{}.tmp",
        destination
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("file"),
        Uuid::new_v4()
    ));
    fs::copy(source, &temporary).map_err(|error| format!("复制媒体文件失败: {error}"))?;
    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("提交媒体归档文件失败: {error}")
    })
}

fn ensure_tag(
    transaction: &rusqlite::Transaction<'_>,
    name: &str,
    now: i64,
) -> Result<String, String> {
    let normalized = name.trim();
    if normalized.is_empty() {
        return Err("标签名称不能为空".into());
    }
    if normalized.chars().count() > 48 {
        return Err("标签名称不能超过 48 个字符".into());
    }
    let existing = transaction
        .query_row(
            "SELECT id FROM media_tags WHERE name=?1 COLLATE NOCASE",
            params![normalized],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    transaction
        .execute(
            "INSERT INTO media_tags(id,name,created_at) VALUES(?1,?2,?3)",
            params![&id, normalized, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(id)
}

fn link_item_tags(
    transaction: &rusqlite::Transaction<'_>,
    item_id: &str,
    tag_names: &[String],
    now: i64,
) -> Result<(), String> {
    for name in tag_names {
        let tag_id = ensure_tag(transaction, name, now)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO media_item_tags(item_id,tag_id) VALUES(?1,?2)",
                params![item_id, tag_id],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn link_collection(
    transaction: &rusqlite::Transaction<'_>,
    collection_id: Option<&str>,
    item_id: &str,
    now: i64,
) -> Result<(), String> {
    let Some(collection_id) = collection_id else {
        return Ok(());
    };
    let exists = transaction
        .query_row(
            "SELECT 1 FROM media_collections WHERE id=?1",
            params![collection_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !exists {
        return Err("指定的媒体集合不存在".into());
    }
    transaction
        .execute(
            "INSERT OR IGNORE INTO media_collection_members(collection_id,item_id,added_at) VALUES(?1,?2,?3)",
            params![collection_id, item_id, now],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn import_single(
    root: &Path,
    connection: &mut Connection,
    source_path: &str,
    mode: &str,
    collection_id: Option<&str>,
    tag_names: &[String],
) -> MediaImportItemResult {
    let source = match PathBuf::from(source_path).canonicalize() {
        Ok(path) => path,
        Err(error) => {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some(format!("无法读取文件: {error}")),
            }
        }
    };
    let kind = match media_kind(&source) {
        Some(kind) => kind,
        None => {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some("只支持图片、视频、音频和常用参考文件".into()),
            }
        }
    };
    let (digest, size, modified_at) = match hash_file(&source) {
        Ok(value) => value,
        Err(message) => {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some(message),
            }
        }
    };
    let timestamp = now();
    let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate) {
        Ok(transaction) => transaction,
        Err(error) => {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some(format!("锁定媒体资料库失败: {error}")),
            }
        }
    };
    let duplicate_item = transaction
        .query_row(
            "SELECT item_id FROM media_hashes WHERE blake3=?1",
            params![&digest],
            |row| row.get::<_, String>(0),
        )
        .optional();
    let duplicate_item = match duplicate_item {
        Ok(value) => value,
        Err(error) => {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some(error.to_string()),
            }
        }
    };

    if let Some(item_id) = duplicate_item {
        let location_id = Uuid::new_v4().to_string();
        let source_string = source.to_string_lossy().into_owned();
        let insert_result = transaction.execute(
            "INSERT OR IGNORE INTO media_locations(id,item_id,path,original_path,ingest_mode,is_primary,exists_on_disk,size,modified_at,added_at) VALUES(?1,?2,?3,?3,'reference',0,1,?4,?5,?6)",
            params![location_id, &item_id, &source_string, size, modified_at, timestamp],
        );
        if let Err(error) = insert_result
            .and_then(|_| {
                link_collection(&transaction, collection_id, &item_id, timestamp)
                    .map(|_| 0)
                    .map_err(|message| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                            message,
                        )))
                    })
            })
            .and_then(|_| {
                link_item_tags(&transaction, &item_id, tag_names, timestamp)
                    .map(|_| 0)
                    .map_err(|message| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                            message,
                        )))
                    })
            })
        {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some(error.to_string()),
            };
        }
        if let Err(error) = transaction.commit() {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some(error.to_string()),
            };
        }
        return MediaImportItemResult {
            source_path: source_path.into(),
            item_id: Some(item_id),
            outcome: "duplicate-linked".into(),
            message: Some("内容已存在，已关联此位置；为避免误删不会复制或移动原文件".into()),
        };
    }

    let item_id = Uuid::new_v4().to_string();
    let mut stored_path = source.clone();
    let mut original_path = None;
    if mode == "copy" || mode == "move" {
        let destination = match archive_destination(root, &item_id, &source) {
            Ok(path) => path,
            Err(message) => {
                return MediaImportItemResult {
                    source_path: source_path.into(),
                    item_id: None,
                    outcome: "failed".into(),
                    message: Some(message),
                }
            }
        };
        if let Err(message) = copy_file_atomically(&source, &destination) {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: None,
                outcome: "failed".into(),
                message: Some(message),
            };
        }
        stored_path = destination;
        original_path = Some(source.to_string_lossy().into_owned());
    }
    let stored_string = stored_path.to_string_lossy().into_owned();
    let inserted = (|| -> Result<(), String> {
        transaction.execute(
            "INSERT INTO media_items(id,name,media_kind,status,created_at,updated_at) VALUES(?1,?2,?3,'active',?4,?4)",
            params![&item_id, display_name(&source), kind, timestamp],
        ).map_err(|error| error.to_string())?;
        transaction.execute(
            "INSERT INTO media_hashes(item_id,blake3,file_size,modified_at) VALUES(?1,?2,?3,?4)",
            params![&item_id, &digest, size, modified_at],
        ).map_err(|error| error.to_string())?;
        transaction.execute(
            "INSERT INTO media_locations(id,item_id,path,original_path,ingest_mode,is_primary,exists_on_disk,size,modified_at,added_at) VALUES(?1,?2,?3,?4,?5,1,1,?6,?7,?8)",
            params![Uuid::new_v4().to_string(), &item_id, &stored_string, original_path, mode, size, modified_at, timestamp],
        ).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO media_annotations(item_id,updated_at) VALUES(?1,?2)",
                params![&item_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.execute(
            "INSERT INTO media_archive_events(id,item_id,event_kind,source_path,destination_path,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![Uuid::new_v4().to_string(), &item_id, mode, source.to_string_lossy(), &stored_string, timestamp],
        ).map_err(|error| error.to_string())?;
        link_collection(&transaction, collection_id, &item_id, timestamp)?;
        link_item_tags(&transaction, &item_id, tag_names, timestamp)?;
        Ok(())
    })();
    if let Err(message) = inserted {
        // The database transaction has not committed yet. An archive copy at
        // this point has no catalog record, so it is safe to remove it rather
        // than leaving an unreachable duplicate behind. The source is never
        // removed before a successful commit.
        if stored_path != source {
            let _ = fs::remove_file(&stored_path);
            if let Some(parent) = stored_path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
        return MediaImportItemResult {
            source_path: source_path.into(),
            item_id: None,
            outcome: "failed".into(),
            message: Some(message),
        };
    }
    if let Err(error) = transaction.commit() {
        return MediaImportItemResult {
            source_path: source_path.into(),
            item_id: None,
            outcome: "failed".into(),
            message: Some(error.to_string()),
        };
    }
    if mode == "move" {
        if let Err(error) = fs::remove_file(&source) {
            return MediaImportItemResult {
                source_path: source_path.into(),
                item_id: Some(item_id),
                outcome: "imported-with-warning".into(),
                message: Some(format!("已归档副本，但无法删除原文件: {error}")),
            };
        }
    }
    MediaImportItemResult {
        source_path: source_path.into(),
        item_id: Some(item_id),
        outcome: "imported".into(),
        message: None,
    }
}

fn collect_media_sources(paths: &[String]) -> Vec<String> {
    let mut sources = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for path in paths {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            let key = normalize_path_key(&candidate);
            if media_kind(&candidate).is_some() && seen.insert(key) {
                sources.push(candidate.to_string_lossy().into_owned());
            }
            continue;
        }
        if candidate.is_dir() {
            for entry in walkdir::WalkDir::new(candidate)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                if media_kind(entry.path()).is_none() {
                    continue;
                }
                let key = normalize_path_key(entry.path());
                if seen.insert(key) {
                    sources.push(entry.path().to_string_lossy().into_owned());
                }
            }
        }
    }
    sources
}

fn query_items(
    connection: &Connection,
    query: &MediaLibraryQuery,
) -> Result<(Vec<MediaCatalogItem>, i64), String> {
    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(120).clamp(1, 500);
    let search = query.search.as_deref().unwrap_or("").trim().to_string();
    let collection_id = query.collection_id.as_deref().unwrap_or("").to_string();
    let tag_id = query.tag_id.as_deref().unwrap_or("").to_string();
    let filter = "(?1='' OR mi.name LIKE '%' || ?1 || '%' OR EXISTS (SELECT 1 FROM media_item_tags sit JOIN media_tags st ON st.id=sit.tag_id WHERE sit.item_id=mi.id AND st.name LIKE '%' || ?1 || '%')) AND (?2='' OR EXISTS (SELECT 1 FROM media_collection_members scm WHERE scm.item_id=mi.id AND scm.collection_id=?2)) AND (?3='' OR EXISTS (SELECT 1 FROM media_item_tags stm WHERE stm.item_id=mi.id AND stm.tag_id=?3))";
    let count: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM media_items mi WHERE {filter}"),
            params![&search, &collection_id, &tag_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let sql = format!(
        r#"SELECT mi.id,mi.name,mi.media_kind,mi.status,
           COALESCE((SELECT path FROM media_locations ml WHERE ml.item_id=mi.id ORDER BY ml.is_primary DESC,ml.added_at ASC LIMIT 1),''),
           COALESCE((SELECT size FROM media_locations ml WHERE ml.item_id=mi.id ORDER BY ml.is_primary DESC,ml.added_at ASC LIMIT 1),0),
           (SELECT modified_at FROM media_locations ml WHERE ml.item_id=mi.id ORDER BY ml.is_primary DESC,ml.added_at ASC LIMIT 1),
           mi.created_at,mi.updated_at,COALESCE(ma.rating,0),COALESCE(ma.note,''),
           COALESCE((SELECT GROUP_CONCAT(mt.name, char(31)) FROM media_item_tags mit JOIN media_tags mt ON mt.id=mit.tag_id WHERE mit.item_id=mi.id),''),
           (SELECT COUNT(*) FROM media_locations ml WHERE ml.item_id=mi.id),
           MAX(0,(SELECT COUNT(*) - 1 FROM media_locations ml WHERE ml.item_id=mi.id))
           FROM media_items mi LEFT JOIN media_annotations ma ON ma.item_id=mi.id
           WHERE {filter}
           ORDER BY mi.updated_at DESC,mi.id DESC LIMIT ?4 OFFSET ?5"#
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![&search, &collection_id, &tag_id, limit, offset],
            |row| {
                let raw_tags: String = row.get(11)?;
                Ok(MediaCatalogItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    media_kind: row.get(2)?,
                    status: row.get(3)?,
                    primary_path: row.get(4)?,
                    size: row.get(5)?,
                    modified_at: row.get(6)?,
                    imported_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    rating: row.get(9)?,
                    note: row.get(10)?,
                    tags: if raw_tags.is_empty() {
                        Vec::new()
                    } else {
                        raw_tags.split(char::from(31)).map(str::to_string).collect()
                    },
                    location_count: row.get(12)?,
                    duplicate_count: row.get(13)?,
                })
            },
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok((rows, count))
}

fn query_collections(connection: &Connection) -> Result<Vec<MediaCollection>, String> {
    let mut statement = connection
        .prepare("SELECT c.id,c.name,c.color,COUNT(m.item_id),c.created_at FROM media_collections c LEFT JOIN media_collection_members m ON m.collection_id=c.id GROUP BY c.id ORDER BY c.name COLLATE NOCASE")
        .map_err(|error| error.to_string())?;
    let collections = statement
        .query_map([], |row| {
            Ok(MediaCollection {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                item_count: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(collections)
}

fn query_tags(connection: &Connection) -> Result<Vec<MediaTag>, String> {
    let mut statement = connection
        .prepare("SELECT t.id,t.name,t.color,COUNT(it.item_id) FROM media_tags t LEFT JOIN media_item_tags it ON it.tag_id=t.id GROUP BY t.id ORDER BY t.name COLLATE NOCASE")
        .map_err(|error| error.to_string())?;
    let tags = statement
        .query_map([], |row| {
            Ok(MediaTag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                item_count: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(tags)
}

fn snapshot_for_query(query: &MediaLibraryQuery) -> Result<MediaLibrarySnapshot, String> {
    let root = normalize_root(&query.library_path)?;
    let connection = open_catalog(&root)?;
    let (items, total_items) = query_items(&connection, query)?;
    let item_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM media_items", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let duplicate_group_count: i64 = connection.query_row("SELECT COUNT(*) FROM (SELECT item_id FROM media_locations GROUP BY item_id HAVING COUNT(*) > 1)", [], |row| row.get(0)).map_err(|error| error.to_string())?;
    let updated_at = connection
        .query_row("SELECT MAX(updated_at) FROM media_items", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    Ok(MediaLibrarySnapshot {
        library: MediaLibraryInfo {
            root_path: root.to_string_lossy().into_owned(),
            display_name: display_name(&root),
            catalog_path: catalog_path(&root).to_string_lossy().into_owned(),
            archive_path: archive_path(&root).to_string_lossy().into_owned(),
            item_count,
            duplicate_group_count,
            updated_at,
        },
        items,
        collections: query_collections(&connection)?,
        tags: query_tags(&connection)?,
        total_items,
        offset: query.offset.unwrap_or(0).max(0),
        limit: query.limit.unwrap_or(120).clamp(1, 500),
    })
}

fn registry_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let root = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root.join(REGISTRY_FILE))
}

fn load_registry(app_handle: &tauri::AppHandle) -> Result<MediaLibraryRegistry, String> {
    let path = registry_path(app_handle)?;
    if !path.exists() {
        return Ok(MediaLibraryRegistry::default());
    }
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("媒体资料库注册表无效: {error}"))
}

fn save_registry(
    app_handle: &tauri::AppHandle,
    registry: &MediaLibraryRegistry,
) -> Result<(), String> {
    let path = registry_path(app_handle)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(registry).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn remember_library(app_handle: &tauri::AppHandle, root: &Path) -> Result<(), String> {
    let mut registry = load_registry(app_handle)?;
    let root_path = root.to_string_lossy().into_owned();
    registry
        .libraries
        .retain(|item| item.root_path != root_path);
    registry.libraries.insert(
        0,
        MediaLibraryBookmark {
            root_path,
            display_name: display_name(root),
            last_opened_at: now(),
            available: true,
        },
    );
    registry.libraries.truncate(30);
    save_registry(app_handle, &registry)
}

#[tauri::command]
pub async fn list_media_libraries(
    app_handle: tauri::AppHandle,
) -> Result<Vec<MediaLibraryBookmark>, String> {
    wait_until_running().await?;
    let mut registry = load_registry(&app_handle)?;
    let mut changed = false;
    for library in &mut registry.libraries {
        let available = Path::new(&library.root_path).is_dir();
        if library.available != available {
            library.available = available;
            changed = true;
        }
    }
    if changed {
        save_registry(&app_handle, &registry)?;
    }
    Ok(registry.libraries)
}

#[tauri::command]
pub async fn open_media_library(
    app_handle: tauri::AppHandle,
    library_path: String,
) -> Result<MediaLibrarySnapshot, String> {
    wait_until_running().await?;
    let root = normalize_root(&library_path)?;
    let query = MediaLibraryQuery {
        library_path: root.to_string_lossy().into_owned(),
        search: None,
        collection_id: None,
        tag_id: None,
        offset: None,
        limit: None,
    };
    let snapshot = tokio::task::spawn_blocking(move || snapshot_for_query(&query))
        .await
        .map_err(|error| error.to_string())??;
    remember_library(&app_handle, &root)?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn get_media_library_snapshot(
    query: MediaLibraryQuery,
) -> Result<MediaLibrarySnapshot, String> {
    wait_until_running().await?;
    tokio::task::spawn_blocking(move || snapshot_for_query(&query))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn import_media_items(
    request: ImportMediaItemsRequest,
) -> Result<MediaImportResult, String> {
    wait_until_running().await?;
    if !matches!(request.mode.as_str(), "reference" | "copy" | "move") {
        return Err("收集方式必须是 reference、copy 或 move".into());
    }
    if request.paths.is_empty() {
        return Err("请至少选择一个媒体文件".into());
    }
    tokio::task::spawn_blocking(move || {
        let root = normalize_root(&request.library_path)?;
        let mut connection = open_catalog(&root)?;
        let sources = collect_media_sources(&request.paths);
        if sources.is_empty() {
            return Err("所选文件或目录中没有支持的媒体文件".into());
        }
        let items = sources
            .iter()
            .map(|path| {
                import_single(
                    &root,
                    &mut connection,
                    path,
                    &request.mode,
                    request.collection_id.as_deref(),
                    &request.tag_names,
                )
            })
            .collect::<Vec<_>>();
        Ok(MediaImportResult {
            imported: items
                .iter()
                .filter(|item| {
                    item.outcome == "imported" || item.outcome == "imported-with-warning"
                })
                .count(),
            duplicates_linked: items
                .iter()
                .filter(|item| item.outcome == "duplicate-linked")
                .count(),
            failed: items.iter().filter(|item| item.outcome == "failed").count(),
            items,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn create_media_collection(
    request: CreateMediaCollectionRequest,
) -> Result<MediaCollection, String> {
    wait_until_running().await?;
    tokio::task::spawn_blocking(move || {
        let root = normalize_root(&request.library_path)?;
        let connection = open_catalog(&root)?;
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("集合名称必须为 1 到 80 个字符".into());
        }
        let timestamp = now();
        let id = Uuid::new_v4().to_string();
        connection.execute(
            "INSERT INTO media_collections(id,name,color,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",
            params![&id, name, request.color, timestamp],
        ).map_err(|error| {
            if error.to_string().contains("UNIQUE") { "已经有同名媒体集合".to_string() } else { error.to_string() }
        })?;
        Ok(MediaCollection { id, name: name.into(), color: request.color, item_count: 0, created_at: timestamp })
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn update_media_annotation(request: UpdateMediaAnnotationRequest) -> Result<(), String> {
    wait_until_running().await?;
    tokio::task::spawn_blocking(move || {
        let root = normalize_root(&request.library_path)?;
        let connection = open_catalog(&root)?;
        let updated = connection.execute(
            "INSERT INTO media_annotations(item_id,note,rating,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(item_id) DO UPDATE SET note=excluded.note,rating=excluded.rating,updated_at=excluded.updated_at",
            params![&request.item_id, request.note, request.rating.clamp(0, 5), now()],
        ).map_err(|error| error.to_string())?;
        if updated == 0 { return Err("媒体条目不存在".into()); }
        connection.execute("UPDATE media_items SET updated_at=?2 WHERE id=?1", params![request.item_id, now()]).map_err(|error| error.to_string())?;
        Ok(())
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_media_item_tags(
    request: SetMediaItemTagsRequest,
) -> Result<Vec<MediaTag>, String> {
    wait_until_running().await?;
    tokio::task::spawn_blocking(move || {
        let root = normalize_root(&request.library_path)?;
        let mut connection = open_catalog(&root)?;
        let timestamp = now();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let exists = transaction
            .query_row(
                "SELECT 1 FROM media_items WHERE id=?1",
                params![&request.item_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
            return Err("媒体条目不存在".into());
        }
        transaction
            .execute(
                "DELETE FROM media_item_tags WHERE item_id=?1",
                params![&request.item_id],
            )
            .map_err(|error| error.to_string())?;
        link_item_tags(
            &transaction,
            &request.item_id,
            &request.tag_names,
            timestamp,
        )?;
        transaction
            .execute(
                "UPDATE media_items SET updated_at=?2 WHERE id=?1",
                params![&request.item_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        query_tags(&connection)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nexora-media-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn catalog_schema_is_independent_from_project_database() {
        let root = test_root("schema");
        let connection = open_catalog(&root).unwrap();
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'media_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(tables >= 9);
        assert!(catalog_path(&root).is_file());
        assert!(!root.join(".pm_center/data.db").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reference_import_detects_duplicates_and_preserves_source_file() {
        let root = test_root("duplicate");
        let source = root.join("source.png");
        let duplicate_source = root.join("duplicate.png");
        fs::write(
            &source,
            b"not a decoded image but a valid indexed media fixture",
        )
        .unwrap();
        fs::copy(&source, &duplicate_source).unwrap();
        let mut connection = open_catalog(&root).unwrap();
        let first = import_single(
            &root,
            &mut connection,
            &source.to_string_lossy(),
            "reference",
            None,
            &["reference".into()],
        );
        let second = import_single(
            &root,
            &mut connection,
            &duplicate_source.to_string_lossy(),
            "move",
            None,
            &[],
        );
        assert_eq!(first.outcome, "imported");
        assert_eq!(second.outcome, "duplicate-linked");
        assert!(source.exists());
        assert!(duplicate_source.exists());
        let item_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM media_items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(item_count, 1);
        let locations: i64 = connection
            .query_row("SELECT COUNT(*) FROM media_locations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(locations, 2);
        let query = MediaLibraryQuery {
            library_path: root.to_string_lossy().into_owned(),
            search: None,
            collection_id: None,
            tag_id: None,
            offset: None,
            limit: None,
        };
        let (items, _) = query_items(&connection, &query).unwrap();
        assert_eq!(items[0].duplicate_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn copy_and_move_archive_only_after_a_successful_catalog_commit() {
        let root = test_root("copy-move");
        let copy_source = root.join("copy.png");
        let move_source = root.join("move.png");
        fs::write(&copy_source, b"copy-source").unwrap();
        fs::write(&move_source, b"move-source-with-different-content").unwrap();
        let copy_source_canonical = copy_source.canonicalize().unwrap();
        let move_source_canonical = move_source.canonicalize().unwrap();
        let mut connection = open_catalog(&root).unwrap();

        let copied = import_single(
            &root,
            &mut connection,
            &copy_source.to_string_lossy(),
            "copy",
            None,
            &[],
        );
        assert_eq!(copied.outcome, "imported");
        assert!(copy_source.is_file());
        let copied_path: (String, String, Option<String>) = connection
            .query_row(
                "SELECT path,ingest_mode,original_path FROM media_locations WHERE item_id=?1",
                params![copied.item_id.as_deref().unwrap()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(copied_path.1, "copy");
        assert_eq!(
            PathBuf::from(copied_path.2.as_deref().unwrap()),
            copy_source_canonical
        );
        assert!(Path::new(&copied_path.0).is_file());
        assert_eq!(
            fs::read(&copy_source).unwrap(),
            fs::read(&copied_path.0).unwrap()
        );

        let moved = import_single(
            &root,
            &mut connection,
            &move_source.to_string_lossy(),
            "move",
            None,
            &[],
        );
        assert_eq!(moved.outcome, "imported");
        assert!(!move_source.exists());
        let moved_path: (String, String, Option<String>) = connection
            .query_row(
                "SELECT path,ingest_mode,original_path FROM media_locations WHERE item_id=?1",
                params![moved.item_id.as_deref().unwrap()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(moved_path.1, "move");
        assert_eq!(
            PathBuf::from(moved_path.2.as_deref().unwrap()),
            move_source_canonical
        );
        assert!(Path::new(&moved_path.0).is_file());

        let failed_source = root.join("failed.png");
        fs::write(&failed_source, b"must-not-leave-an-orphan").unwrap();
        let before = fs::read_dir(archive_path(&root))
            .map(|entries| entries.count())
            .unwrap_or(0);
        let failed = import_single(
            &root,
            &mut connection,
            &failed_source.to_string_lossy(),
            "move",
            None,
            &["x".repeat(49)],
        );
        assert_eq!(failed.outcome, "failed");
        assert!(failed_source.is_file());
        let after = fs::read_dir(archive_path(&root))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(after, before);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn indexes_support_paginated_ten_thousand_item_catalogues() {
        let root = test_root("ten-thousand");
        let mut connection = open_catalog(&root).unwrap();
        let transaction = connection.transaction().unwrap();
        for index in 0..10_000_i64 {
            let item_id = format!("item-{index:05}");
            transaction.execute(
                "INSERT INTO media_items(id,name,media_kind,status,created_at,updated_at) VALUES(?1,?2,'image','active',?3,?3)",
                params![&item_id, format!("asset-{index:05}.png"), index],
            ).unwrap();
            transaction
                .execute(
                    "INSERT INTO media_hashes(item_id,blake3,file_size) VALUES(?1,?2,1)",
                    params![&item_id, format!("hash-{index:05}")],
                )
                .unwrap();
            transaction.execute(
                "INSERT INTO media_locations(id,item_id,path,ingest_mode,is_primary,exists_on_disk,size,added_at) VALUES(?1,?2,?3,'reference',1,1,1,?4)",
                params![format!("location-{index:05}"), &item_id, format!("C:/media/asset-{index:05}.png"), index],
            ).unwrap();
        }
        transaction.commit().unwrap();
        let query = MediaLibraryQuery {
            library_path: root.to_string_lossy().into_owned(),
            search: Some("asset-099".into()),
            collection_id: None,
            tag_id: None,
            offset: Some(0),
            limit: Some(50),
        };
        let (items, count) = query_items(&connection, &query).unwrap();
        assert_eq!(count, 100);
        assert_eq!(items.len(), 50);
        let _ = fs::remove_dir_all(root);
    }
}
