use super::{
    connect_to_peer, emit_contacts_changed, now_millis, open_database, set_online_legacy_peer,
    state_paths, upsert_legacy_contact, OnlinePeer, P2P_GLOBAL, PROTOCOL_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, SeekFrom};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::watch;
use walkdir::WalkDir;

const TRANSFER_PROTOCOL_VERSION: u16 = 1;
const LOBBY_TRANSFER_PROTOCOL_VERSION: u16 = 3;
const TRANSFER_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const TRANSFER_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const TRANSFER_IO_TIMEOUT: Duration = Duration::from_secs(60);
const TRANSFER_CHUNK_SIZE: usize = 1024 * 1024;
const MAX_TRANSFER_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const MAX_AUTO_RECEIVE_IMAGE_BYTES: u64 = 100 * 1024 * 1024;
const FILE_TRANSFER_MIN_PROTOCOL_VERSION: u16 = 3;
const DIRECTORY_TRANSFER_MIN_PROTOCOL_VERSION: u16 = 4;
const LOBBY_IMAGE_MIN_PROTOCOL_VERSION: u16 = 6;
const TRANSFER_CANCEL_MIN_PROTOCOL_VERSION: u16 = 7;
const PARALLEL_TRANSFER_MIN_PROTOCOL_VERSION: u16 = 7;
const ENABLE_PARALLEL_FILE_TRANSFER: bool = false;
const TWO_STREAM_MIN_BYTES: u64 = 1024 * 1024;
const FOUR_STREAM_MIN_BYTES: u64 = 10 * 1024 * 1024;
const EIGHT_STREAM_MIN_BYTES: u64 = 50 * 1024 * 1024;
const MAX_DIRECTORY_ITEMS: usize = 20_000;
const MAX_DIRECTORY_DEPTH: usize = 64;
const MAX_RELATIVE_PATH_CHARS: usize = 2_048;
static RECEIVE_PATH_LOCK: Mutex<()> = Mutex::new(());
const TRANSFER_CANCELLED_ERROR: &str = "传输已中断";

lazy_static::lazy_static! {
    static ref ACTIVE_TRANSFER_CANCELLATIONS: Mutex<HashMap<String, ActiveTransferEntry>> =
        Mutex::new(HashMap::new());
}

struct ActiveTransferEntry {
    cancellation: watch::Sender<bool>,
    registrations: usize,
    transferred_bytes: Arc<AtomicU64>,
    started_at: Instant,
}

struct ActiveTransferGuard {
    transfer_id: String,
}

impl Drop for ActiveTransferGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_TRANSFER_CANCELLATIONS.lock() {
            let remove = active
                .get_mut(&self.transfer_id)
                .map(|entry| {
                    entry.registrations = entry.registrations.saturating_sub(1);
                    entry.registrations == 0
                })
                .unwrap_or(false);
            if remove {
                active.remove(&self.transfer_id);
            }
        }
    }
}

fn register_active_transfer(
    transfer_id: &str,
) -> Result<
    (
        ActiveTransferGuard,
        watch::Receiver<bool>,
        Arc<AtomicU64>,
        Instant,
    ),
    String,
> {
    let mut active = ACTIVE_TRANSFER_CANCELLATIONS
        .lock()
        .map_err(|error| error.to_string())?;
    let (receiver, transferred_bytes, started_at) = if let Some(entry) = active.get_mut(transfer_id)
    {
        entry.registrations += 1;
        (
            entry.cancellation.subscribe(),
            entry.transferred_bytes.clone(),
            entry.started_at,
        )
    } else {
        let (sender, receiver) = watch::channel(false);
        let transferred_bytes = Arc::new(AtomicU64::new(0));
        let started_at = Instant::now();
        active.insert(
            transfer_id.to_string(),
            ActiveTransferEntry {
                cancellation: sender,
                registrations: 1,
                transferred_bytes: transferred_bytes.clone(),
                started_at,
            },
        );
        (receiver, transferred_bytes, started_at)
    };
    Ok((
        ActiveTransferGuard {
            transfer_id: transfer_id.to_string(),
        },
        receiver,
        transferred_bytes,
        started_at,
    ))
}

fn signal_active_transfer_cancellation(transfer_id: &str) -> bool {
    ACTIVE_TRANSFER_CANCELLATIONS
        .lock()
        .ok()
        .and_then(|active| {
            active
                .get(transfer_id)
                .map(|entry| entry.cancellation.clone())
        })
        .is_some_and(|sender| sender.send(true).is_ok())
}

async fn wait_for_transfer_cancellation(cancellation: &mut watch::Receiver<bool>) {
    if *cancellation.borrow() {
        return;
    }
    while cancellation.changed().await.is_ok() {
        if *cancellation.borrow() {
            return;
        }
    }
    std::future::pending::<()>().await;
}

fn ensure_transfer_not_cancelled(cancellation: &watch::Receiver<bool>) -> Result<(), String> {
    if *cancellation.borrow() {
        Err(TRANSFER_CANCELLED_ERROR.to_string())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanTransfer {
    pub id: String,
    pub lobby_item_id: Option<String>,
    pub conversation_id: String,
    pub kind: String,
    pub from_id: String,
    pub from_name: String,
    pub provider_id: String,
    pub provider_name: String,
    pub to_id: String,
    pub display_name: String,
    pub item_count: u64,
    pub total_bytes: u64,
    pub mime_type: Option<String>,
    pub content_hash: String,
    pub payload_format: String,
    pub manifest: serde_json::Value,
    pub status: String,
    pub direction: String,
    pub source_path: Option<String>,
    pub received_path: Option<String>,
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanTransferProgress {
    pub transfer_id: String,
    pub status: String,
    pub direction: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_second: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferLanFilesRequest {
    #[serde(default)]
    pub to_id: Option<String>,
    #[serde(default)]
    pub to_ids: Vec<String>,
    pub paths: Vec<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanTransferFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanFileOfferResult {
    pub transfers: Vec<LanTransfer>,
    pub failures: Vec<LanTransferFailure>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondLanTransferRequest {
    pub transfer_id: String,
    pub action: String,
    pub destination_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelLanTransferRequest {
    pub transfer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WireTransferOffer {
    pub(super) id: String,
    #[serde(default)]
    pub(super) lobby_item_id: Option<String>,
    pub(super) kind: String,
    pub(super) from_id: String,
    pub(super) from_name: String,
    pub(super) provider_id: String,
    pub(super) provider_name: String,
    pub(super) to_id: String,
    #[serde(default)]
    pub(super) conversation_id: Option<String>,
    pub(super) display_name: String,
    pub(super) item_count: u64,
    pub(super) total_bytes: u64,
    pub(super) mime_type: Option<String>,
    pub(super) content_hash: String,
    pub(super) payload_format: String,
    pub(super) manifest: serde_json::Value,
    pub(super) created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum TransferControlEnvelope {
    TransferOffer {
        protocol_version: u16,
        transfer_protocol_version: u16,
        offer: WireTransferOffer,
    },
    TransferOfferAck {
        transfer_id: String,
    },
    TransferDecision {
        transfer_id: String,
        receiver_id: String,
        accepted: bool,
    },
    TransferDecisionAck {
        transfer_id: String,
    },
    TransferRequest {
        transfer_id: String,
        receiver_id: String,
    },
    TransferReady {
        transfer_id: String,
        total_bytes: u64,
        content_hash: String,
    },
    TransferRangeRequest {
        transfer_id: String,
        receiver_id: String,
        offset: u64,
        length: u64,
    },
    TransferRangeReady {
        transfer_id: String,
        offset: u64,
        length: u64,
    },
    TransferParallelReceipt {
        transfer_id: String,
        receiver_id: String,
        status: String,
        error: Option<String>,
    },
    TransferParallelReceiptAck {
        transfer_id: String,
    },
    TransferReceipt {
        transfer_id: String,
        status: String,
        error: Option<String>,
    },
    TransferError {
        transfer_id: String,
        error: String,
    },
    TransferCancel {
        transfer_id: String,
        requester_id: String,
    },
    TransferCancelAck {
        transfer_id: String,
    },
}

pub(super) fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS lan_transfers (
           id TEXT PRIMARY KEY,
           conversation_id TEXT NOT NULL,
           kind TEXT NOT NULL,
           from_id TEXT NOT NULL,
           from_name TEXT NOT NULL,
           to_id TEXT NOT NULL,
           display_name TEXT NOT NULL,
           item_count INTEGER NOT NULL,
           total_bytes INTEGER NOT NULL,
           mime_type TEXT,
           content_hash TEXT NOT NULL,
           payload_format TEXT NOT NULL,
           manifest_json TEXT NOT NULL,
           status TEXT NOT NULL,
           direction TEXT NOT NULL,
           source_path TEXT,
           received_path TEXT,
           error TEXT,
           created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_lan_transfers_conversation_time
           ON lan_transfers(conversation_id,created_at);
         CREATE INDEX IF NOT EXISTS idx_lan_transfers_status
           ON lan_transfers(status);",
    )
    .map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare("PRAGMA table_info(lan_transfers)")
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    if !columns.contains("lobby_item_id") {
        conn.execute(
            "ALTER TABLE lan_transfers ADD COLUMN lobby_item_id TEXT",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    if !columns.contains("provider_id") {
        conn.execute(
            "ALTER TABLE lan_transfers ADD COLUMN provider_id TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    if !columns.contains("provider_name") {
        conn.execute(
            "ALTER TABLE lan_transfers ADD COLUMN provider_name TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    conn.execute_batch(
        "UPDATE lan_transfers SET provider_id=from_id WHERE provider_id='';
         UPDATE lan_transfers SET provider_name=from_name WHERE provider_name='';
         CREATE INDEX IF NOT EXISTS idx_lan_transfers_lobby_item
           ON lan_transfers(lobby_item_id);",
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn recover_interrupted(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE lan_transfers
         SET status='failed',error='应用退出导致传输中断，可重新接收',updated_at=?1
         WHERE status IN ('preparing','transferring')",
        params![now_millis() as i64],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn prune_expired(conn: &Connection) -> Result<(), String> {
    let cutoff = now_millis().saturating_sub(TRANSFER_RETENTION_MS);
    conn.execute(
        "DELETE FROM lan_transfers WHERE created_at < ?1",
        params![cutoff as i64],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn prune_staging_files(staging_dir: &Path) {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_millis(TRANSFER_RETENTION_MS))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let Ok(entries) = fs::read_dir(staging_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let should_remove = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .is_some_and(|modified| modified < cutoff);
        if should_remove {
            if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn parse_transfer_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LanTransfer> {
    let manifest_json: String = row.get(15)?;
    Ok(LanTransfer {
        id: row.get(0)?,
        lobby_item_id: row.get(1)?,
        conversation_id: row.get(2)?,
        kind: row.get(3)?,
        from_id: row.get(4)?,
        from_name: row.get(5)?,
        provider_id: row.get(6)?,
        provider_name: row.get(7)?,
        to_id: row.get(8)?,
        display_name: row.get(9)?,
        item_count: row.get::<_, i64>(10)? as u64,
        total_bytes: row.get::<_, i64>(11)? as u64,
        mime_type: row.get(12)?,
        content_hash: row.get(13)?,
        payload_format: row.get(14)?,
        manifest: serde_json::from_str(&manifest_json).unwrap_or_else(|_| json!({})),
        status: row.get(16)?,
        direction: row.get(17)?,
        source_path: row.get(18)?,
        received_path: row.get(19)?,
        error: row.get(20)?,
        created_at: row.get::<_, i64>(21)? as u64,
        updated_at: row.get::<_, i64>(22)? as u64,
    })
}

const TRANSFER_COLUMNS: &str =
    "id,lobby_item_id,conversation_id,kind,from_id,from_name,provider_id,provider_name,to_id,display_name,item_count,total_bytes,mime_type,content_hash,payload_format,manifest_json,status,direction,source_path,received_path,error,created_at,updated_at";

pub(super) fn load_transfers(conn: &Connection) -> Result<Vec<LanTransfer>, String> {
    let query =
        format!("SELECT {TRANSFER_COLUMNS} FROM lan_transfers ORDER BY created_at ASC LIMIT 5000");
    let mut statement = conn.prepare(&query).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], parse_transfer_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_transfer(conn: &Connection, transfer_id: &str) -> Result<LanTransfer, String> {
    let query = format!("SELECT {TRANSFER_COLUMNS} FROM lan_transfers WHERE id=?1");
    conn.query_row(&query, params![transfer_id], parse_transfer_row)
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "文件传输记录不存在".to_string())
}

fn insert_transfer(conn: &Connection, transfer: &LanTransfer) -> Result<bool, String> {
    let manifest_json =
        serde_json::to_string(&transfer.manifest).map_err(|error| error.to_string())?;
    let sql = if transfer.direction == "incoming" && transfer.lobby_item_id.is_some() {
        "INSERT OR IGNORE INTO lan_transfers(
           id,lobby_item_id,conversation_id,kind,from_id,from_name,provider_id,provider_name,to_id,display_name,item_count,total_bytes,mime_type,content_hash,payload_format,manifest_json,status,direction,source_path,received_path,error,created_at,updated_at
         ) SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23
         WHERE NOT EXISTS(SELECT 1 FROM lan_transfers WHERE lobby_item_id=?2)"
    } else {
        "INSERT OR IGNORE INTO lan_transfers(
           id,lobby_item_id,conversation_id,kind,from_id,from_name,provider_id,provider_name,to_id,display_name,item_count,total_bytes,mime_type,content_hash,payload_format,manifest_json,status,direction,source_path,received_path,error,created_at,updated_at
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)"
    };
    let inserted = conn
        .execute(
            sql,
            params![
                transfer.id,
                transfer.lobby_item_id,
                transfer.conversation_id,
                transfer.kind,
                transfer.from_id,
                transfer.from_name,
                transfer.provider_id,
                transfer.provider_name,
                transfer.to_id,
                transfer.display_name,
                transfer.item_count as i64,
                transfer.total_bytes as i64,
                transfer.mime_type,
                transfer.content_hash,
                transfer.payload_format,
                manifest_json,
                transfer.status,
                transfer.direction,
                transfer.source_path,
                transfer.received_path,
                transfer.error,
                transfer.created_at as i64,
                transfer.updated_at as i64,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(inserted > 0)
}

fn update_transfer(
    conn: &Connection,
    transfer_id: &str,
    status: &str,
    error: Option<&str>,
    received_path: Option<&str>,
) -> Result<LanTransfer, String> {
    conn.execute(
        "UPDATE lan_transfers
         SET status=?2,error=?3,received_path=COALESCE(?4,received_path),updated_at=?5
         WHERE id=?1",
        params![
            transfer_id,
            status,
            error,
            received_path,
            now_millis() as i64
        ],
    )
    .map_err(|error| error.to_string())?;
    load_transfer(conn, transfer_id)
}

fn mark_transfer_cancelled(conn: &Connection, transfer_id: &str) -> Result<LanTransfer, String> {
    let changed = conn
        .execute(
            "UPDATE lan_transfers
             SET status='cancelled',error=NULL,updated_at=?2
             WHERE id=?1 AND status IN ('transferring','failed')",
            params![transfer_id, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        let transfer = load_transfer(conn, transfer_id)?;
        if transfer.status == "cancelled" {
            return Ok(transfer);
        }
        return Err("当前文件没有正在进行的传输".to_string());
    }
    load_transfer(conn, transfer_id)
}

fn claim_incoming_transfer(
    conn: &Connection,
    transfer_id: &str,
    destination_path: &Path,
) -> Result<LanTransfer, String> {
    let claimed = conn
        .execute(
            "UPDATE lan_transfers
             SET status='transferring',error=NULL,received_path=?2,updated_at=?3
             WHERE id=?1 AND direction='incoming' AND status IN ('pending','failed','cancelled')",
            params![
                transfer_id,
                destination_path.to_string_lossy(),
                now_millis() as i64
            ],
        )
        .map_err(|error| error.to_string())?;
    if claimed == 0 {
        return Err("当前传输已被其他窗口处理或状态已变化".to_string());
    }
    load_transfer(conn, transfer_id)
}

fn sanitize_sender_directory(display_name: &str, sender_id: &str) -> String {
    let mut sanitized = display_name
        .trim()
        .chars()
        .map(|value| {
            if value.is_control()
                || matches!(value, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                value
            }
        })
        .take(80)
        .collect::<String>();
    sanitized = sanitized.trim_matches([' ', '.']).to_string();
    let reserved_stem = sanitized
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let windows_reserved = matches!(
        reserved_stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if windows_reserved {
        sanitized.insert(0, '_');
    }
    if sanitized.is_empty() {
        let suffix = sender_id.chars().take(8).collect::<String>();
        format!("联系人-{suffix}")
    } else {
        sanitized
    }
}

fn destination_is_reserved(
    conn: &Connection,
    transfer_id: &str,
    destination: &Path,
) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM lan_transfers
             WHERE id<>?1 AND received_path=?2 AND status<>'rejected'",
            params![transfer_id, destination.to_string_lossy()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(count > 0)
}

fn allocate_default_destination(
    conn: &Connection,
    transfer: &LanTransfer,
) -> Result<PathBuf, String> {
    let root = super::configured_receive_directory()?;
    let sender_directory = root.join(sanitize_sender_directory(
        &transfer.from_name,
        &transfer.from_id,
    ));
    fs::create_dir_all(&sender_directory)
        .map_err(|error| format!("创建联系人接收目录失败：{error}"))?;
    let file_name = validate_file_name(&transfer.display_name)?;
    let (stem, extension) = if transfer.kind == "directory" {
        (file_name.as_str(), None)
    } else {
        let original = Path::new(&file_name);
        (
            original
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("文件"),
            original.extension().and_then(|value| value.to_str()),
        )
    };
    for index in 0..10_000u32 {
        let candidate_name = if index == 0 {
            file_name.clone()
        } else if let Some(extension) = extension {
            format!("{stem} ({index}).{extension}")
        } else {
            format!("{stem} ({index})")
        };
        let candidate = sender_directory.join(candidate_name);
        if !candidate.exists() && !destination_is_reserved(conn, &transfer.id, &candidate)? {
            return Ok(candidate);
        }
    }
    Err("接收位置中同名内容过多，请整理后重试".to_string())
}

fn is_image_transfer(transfer: &LanTransfer) -> bool {
    transfer
        .mime_type
        .as_deref()
        .is_some_and(|value| value.starts_with("image/"))
}

fn should_auto_receive_image(transfer: &LanTransfer) -> bool {
    is_image_transfer(transfer) && transfer.total_bytes <= MAX_AUTO_RECEIVE_IMAGE_BYTES
}

async fn validate_received_image(path: &Path) -> Result<(), String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let reader = image::io::Reader::open(&path)
            .map_err(|error| format!("无法读取接收图片：{error}"))?
            .with_guessed_format()
            .map_err(|error| format!("无法识别接收图片：{error}"))?;
        if reader.format().is_none() {
            return Err("接收内容不是可识别的图片".to_string());
        }
        reader
            .into_dimensions()
            .map_err(|error| format!("接收图片校验失败：{error}"))?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn reject_incoming_transfer(conn: &Connection, transfer_id: &str) -> Result<LanTransfer, String> {
    let rejected = conn
        .execute(
            "UPDATE lan_transfers
             SET status='rejected',error=NULL,updated_at=?2
             WHERE id=?1 AND direction='incoming' AND status IN ('pending','failed','cancelled')",
            params![transfer_id, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    if rejected == 0 {
        return Err("当前传输已被其他窗口处理或状态已变化".to_string());
    }
    load_transfer(conn, transfer_id)
}

pub(super) fn unread_counts(conn: &Connection) -> Result<HashMap<String, u64>, String> {
    let mut statement = conn
        .prepare(
            "SELECT t.conversation_id,COUNT(*)
             FROM lan_transfers t
             LEFT JOIN lan_conversation_reads r ON r.conversation_id=t.conversation_id
             WHERE t.direction='incoming' AND t.created_at>COALESCE(r.last_read_at,0)
             GROUP BY t.conversation_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn activity_by_conversation(transfers: &[LanTransfer]) -> HashMap<String, u64> {
    let mut activity = HashMap::<String, u64>::new();
    for transfer in transfers {
        activity
            .entry(transfer.conversation_id.clone())
            .and_modify(|value| *value = (*value).max(transfer.created_at))
            .or_insert(transfer.created_at);
    }
    activity
}

fn emit_changed(app_handle: &tauri::AppHandle, transfer: &LanTransfer) {
    let _ = app_handle.emit("pm-center:lan-transfer-changed", transfer);
}

fn emit_progress(
    app_handle: &tauri::AppHandle,
    transfer_id: &str,
    status: &str,
    direction: &str,
    transferred_bytes: u64,
    total_bytes: u64,
    started_at: Instant,
) {
    let elapsed = started_at.elapsed().as_secs_f64();
    let bytes_per_second = if elapsed > 0.0 {
        (transferred_bytes as f64 / elapsed) as u64
    } else {
        0
    };
    let _ = app_handle.emit(
        "pm-center:lan-transfer-progress",
        LanTransferProgress {
            transfer_id: transfer_id.to_string(),
            status: status.to_string(),
            direction: direction.to_string(),
            transferred_bytes,
            total_bytes,
            bytes_per_second,
        },
    );
}

fn validate_file_name(file_name: &str) -> Result<String, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err("文件名无效".to_string());
    }
    if trimmed.chars().count() > 240
        || trimmed.chars().any(char::is_control)
        || trimmed.contains(['/', '\\'])
    {
        return Err("文件名包含无效字符或过长".to_string());
    }
    Ok(trimmed.to_string())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| format!("读取文件失败：{error}"))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; TRANSFER_CHUNK_SIZE];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("读取文件失败：{error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferManifestItem {
    path: String,
    #[serde(default = "default_manifest_item_kind")]
    kind: String,
    size_bytes: u64,
    mime_type: Option<String>,
    content_hash: Option<String>,
    modified_at: u64,
}

fn default_manifest_item_kind() -> String {
    "file".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferManifest {
    version: u16,
    kind: String,
    payload_format: String,
    items: Vec<TransferManifestItem>,
}

struct SourceInspection {
    path: PathBuf,
    display_name: String,
    kind: String,
    item_count: u64,
    total_bytes: u64,
    mime_type: Option<String>,
    content_hash: String,
    payload_format: String,
    manifest: serde_json::Value,
}

fn system_time_millis(value: SystemTime) -> u64 {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn validate_relative_path(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(value) = component else {
            return Err("目录清单包含无效路径".to_string());
        };
        let part = value
            .to_str()
            .ok_or_else(|| "目录内存在无法转换为 UTF-8 的名称".to_string())?;
        parts.push(validate_file_name(part)?);
    }
    if parts.is_empty() || parts.len() > MAX_DIRECTORY_DEPTH {
        return Err("目录层级为空或过深".to_string());
    }
    let relative = parts.join("/");
    if relative.chars().count() > MAX_RELATIVE_PATH_CHARS {
        return Err("目录内路径过长".to_string());
    }
    Ok(relative)
}

fn manifest_hash(manifest: &TransferManifest) -> Result<String, String> {
    let bytes = serde_json::to_vec(manifest).map_err(|error| error.to_string())?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn parse_and_validate_manifest(
    kind: &str,
    payload_format: &str,
    item_count: u64,
    total_bytes: u64,
    content_hash: &str,
    value: &serde_json::Value,
) -> Result<TransferManifest, String> {
    if serde_json::to_vec(value)
        .map_err(|error| error.to_string())?
        .len()
        > super::MAX_CONTROL_BYTES
    {
        return Err("目录清单超过允许大小".to_string());
    }
    let manifest: TransferManifest =
        serde_json::from_value(value.clone()).map_err(|_| "传输清单格式无效".to_string())?;
    if manifest.version != 1
        || manifest.kind != kind
        || manifest.payload_format != payload_format
        || manifest.items.len() as u64 != item_count
    {
        return Err("传输清单与请求不一致".to_string());
    }
    if kind == "file" {
        let item = manifest
            .items
            .first()
            .ok_or_else(|| "文件传输清单为空".to_string())?;
        if payload_format != "raw"
            || item_count != 1
            || item.kind != "file"
            || item.size_bytes != total_bytes
            || item.content_hash.as_deref() != Some(content_hash)
        {
            return Err("文件传输清单无效".to_string());
        }
        validate_relative_path(Path::new(&item.path))?;
        return Ok(manifest);
    }
    if kind != "directory" || payload_format != "tree-v1" {
        return Err("不支持的传输内容类型".to_string());
    }
    if manifest.items.len() > MAX_DIRECTORY_ITEMS {
        return Err("目录项目数量超过限制".to_string());
    }
    let mut seen = HashSet::new();
    let mut kinds = HashMap::new();
    let mut calculated_bytes = 0u64;
    for item in &manifest.items {
        let normalized = validate_relative_path(Path::new(&item.path))?;
        if normalized != item.path.replace('\\', "/") {
            return Err("目录清单路径格式无效".to_string());
        }
        let key = normalized.to_lowercase();
        if !seen.insert(key.clone()) {
            return Err("目录清单包含重复路径".to_string());
        }
        if item.kind == "file" {
            let hash = item
                .content_hash
                .as_deref()
                .ok_or_else(|| "目录文件缺少完整性哈希".to_string())?;
            if hash.len() != 64 || !hash.bytes().all(|value| value.is_ascii_hexdigit()) {
                return Err("目录文件哈希无效".to_string());
            }
            calculated_bytes = calculated_bytes
                .checked_add(item.size_bytes)
                .ok_or_else(|| "目录总大小超出限制".to_string())?;
        } else if item.kind == "directory" {
            if item.size_bytes != 0 || item.content_hash.is_some() {
                return Err("目录项目清单无效".to_string());
            }
        } else {
            return Err("目录清单包含不支持的项目".to_string());
        }
        kinds.insert(key, item.kind.as_str());
    }
    for path in kinds.keys() {
        let parts = path.split('/').collect::<Vec<_>>();
        for index in 1..parts.len() {
            let parent = parts[..index].join("/");
            if kinds.get(&parent).is_some_and(|kind| *kind == "file") {
                return Err("目录清单中存在文件与子路径冲突".to_string());
            }
        }
    }
    if calculated_bytes != total_bytes || total_bytes > MAX_TRANSFER_BYTES {
        return Err("目录总大小与清单不一致".to_string());
    }
    if manifest_hash(&manifest)? != content_hash {
        return Err("目录清单完整性校验失败".to_string());
    }
    Ok(manifest)
}

fn inspect_file_source(
    canonical: PathBuf,
    metadata: fs::Metadata,
) -> Result<SourceInspection, String> {
    if metadata.len() > MAX_TRANSFER_BYTES {
        return Err("单个传输内容不能超过 1 TB".to_string());
    }
    let display_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "文件名无法转换为 UTF-8".to_string())?;
    let display_name = validate_file_name(display_name)?;
    let mime_type = mime_guess::from_path(&canonical)
        .first_raw()
        .map(str::to_string);
    let content_hash = hash_file(&canonical)?;
    let modified_at = metadata
        .modified()
        .map(system_time_millis)
        .unwrap_or_default();
    let manifest = TransferManifest {
        version: 1,
        kind: "file".to_string(),
        payload_format: "raw".to_string(),
        items: vec![TransferManifestItem {
            path: display_name.clone(),
            kind: "file".to_string(),
            size_bytes: metadata.len(),
            mime_type: mime_type.clone(),
            content_hash: Some(content_hash.clone()),
            modified_at,
        }],
    };
    Ok(SourceInspection {
        path: canonical,
        display_name,
        kind: "file".to_string(),
        item_count: 1,
        total_bytes: metadata.len(),
        mime_type,
        content_hash,
        payload_format: "raw".to_string(),
        manifest: serde_json::to_value(manifest).map_err(|error| error.to_string())?,
    })
}

fn inspect_directory_source(canonical: PathBuf) -> Result<SourceInspection, String> {
    let display_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "目录名无法转换为 UTF-8".to_string())?;
    let display_name = validate_file_name(display_name)?;
    let mut items = Vec::new();
    let mut total_bytes = 0u64;
    for entry in WalkDir::new(&canonical)
        .min_depth(1)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| format!("扫描目录失败：{error}"))?;
        if items.len() >= MAX_DIRECTORY_ITEMS {
            return Err(format!("单个目录最多包含 {MAX_DIRECTORY_ITEMS} 个项目"));
        }
        if entry.path_is_symlink() {
            return Err(format!(
                "目录中包含不支持的符号链接：{}",
                entry.path().display()
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(&canonical)
            .map_err(|error| error.to_string())?;
        let relative = validate_relative_path(relative)?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        let modified_at = metadata
            .modified()
            .map(system_time_millis)
            .unwrap_or_default();
        if metadata.is_dir() {
            items.push(TransferManifestItem {
                path: relative,
                kind: "directory".to_string(),
                size_bytes: 0,
                mime_type: None,
                content_hash: None,
                modified_at,
            });
        } else if metadata.is_file() {
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| "目录总大小超出限制".to_string())?;
            if total_bytes > MAX_TRANSFER_BYTES {
                return Err("单个传输内容不能超过 1 TB".to_string());
            }
            items.push(TransferManifestItem {
                path: relative,
                kind: "file".to_string(),
                size_bytes: metadata.len(),
                mime_type: mime_guess::from_path(entry.path())
                    .first_raw()
                    .map(str::to_string),
                content_hash: Some(hash_file(entry.path())?),
                modified_at,
            });
        } else {
            return Err(format!(
                "目录中包含不支持的项目：{}",
                entry.path().display()
            ));
        }
    }
    let manifest = TransferManifest {
        version: 1,
        kind: "directory".to_string(),
        payload_format: "tree-v1".to_string(),
        items,
    };
    let content_hash = manifest_hash(&manifest)?;
    let item_count = manifest.items.len() as u64;
    Ok(SourceInspection {
        path: canonical,
        display_name,
        kind: "directory".to_string(),
        item_count,
        total_bytes,
        mime_type: None,
        content_hash,
        payload_format: "tree-v1".to_string(),
        manifest: serde_json::to_value(manifest).map_err(|error| error.to_string())?,
    })
}

async fn inspect_source(path: PathBuf) -> Result<SourceInspection, String> {
    tokio::task::spawn_blocking(move || {
        let source_metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("无法读取待发送内容：{error}"))?;
        if source_metadata.file_type().is_symlink() {
            return Err("不支持发送符号链接".to_string());
        }
        let canonical =
            fs::canonicalize(&path).map_err(|error| format!("无法读取待发送内容：{error}"))?;
        let metadata = fs::symlink_metadata(&canonical).map_err(|error| error.to_string())?;
        if metadata.is_file() {
            inspect_file_source(canonical, metadata)
        } else if metadata.is_dir() {
            inspect_directory_source(canonical)
        } else {
            Err("只能发送普通文件或目录".to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn write_envelope(
    writer: &mut OwnedWriteHalf,
    envelope: &TransferControlEnvelope,
) -> Result<(), String> {
    let mut data = serde_json::to_vec(envelope).map_err(|error| error.to_string())?;
    data.push(b'\n');
    tokio::time::timeout(TRANSFER_HANDSHAKE_TIMEOUT, writer.write_all(&data))
        .await
        .map_err(|_| "发送传输控制消息超时".to_string())?
        .map_err(|error| error.to_string())
}

async fn read_envelope(
    reader: &mut BufReader<OwnedReadHalf>,
    timeout: Duration,
) -> Result<TransferControlEnvelope, String> {
    let mut line = String::new();
    tokio::time::timeout(timeout, reader.read_line(&mut line))
        .await
        .map_err(|_| "等待传输响应超时".to_string())?
        .map_err(|error| error.to_string())?;
    if line.is_empty() || line.len() > super::MAX_CONTROL_BYTES {
        return Err("收到无效传输响应".to_string());
    }
    serde_json::from_str(line.trim()).map_err(|error| format!("无法解析传输响应：{error}"))
}

fn transfer_target(contact_id: &str, min_protocol_version: u16) -> Result<OnlinePeer, String> {
    let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
    let peer = state
        .online_users
        .get(contact_id)
        .cloned()
        .ok_or_else(|| "联系人当前离线，无法传输文件".to_string())?;
    if peer.protocol_version < min_protocol_version {
        let feature = if min_protocol_version >= DIRECTORY_TRANSFER_MIN_PROTOCOL_VERSION {
            "目录传输"
        } else {
            "确认式文件传输"
        };
        return Err(format!("对方客户端版本不支持{feature}，请先升级到兼容版本"));
    }
    Ok(peer)
}

fn transfer_min_protocol_version(kind: &str, conversation_id: Option<&str>) -> u16 {
    if conversation_id == Some("lobby") {
        LOBBY_IMAGE_MIN_PROTOCOL_VERSION
    } else if kind == "directory" {
        DIRECTORY_TRANSFER_MIN_PROTOCOL_VERSION
    } else {
        FILE_TRANSFER_MIN_PROTOCOL_VERSION
    }
}

fn resolve_offer_conversation(
    offer: &WireTransferOffer,
    protocol_version: u16,
) -> Result<String, String> {
    match offer.conversation_id.as_deref() {
        None => Ok(format!("direct:{}", offer.from_id)),
        Some("lobby")
            if protocol_version >= LOBBY_IMAGE_MIN_PROTOCOL_VERSION
                && offer.lobby_item_id.is_some()
                && offer.kind == "file"
                && offer
                    .mime_type
                    .as_deref()
                    .is_some_and(|value| value.starts_with("image/")) =>
        {
            Ok("lobby".to_string())
        }
        Some("lobby") => Err("大厅仅支持由新版客户端发送图片".to_string()),
        Some(_) => Err("收到无效的传输会话标识".to_string()),
    }
}

fn validate_peer_ip(contact_id: &str, ip: &str) -> Result<(), String> {
    let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
    let peer = state
        .online_users
        .get(contact_id)
        .ok_or_else(|| "传输联系人当前不在线".to_string())?;
    if peer.ip != ip {
        return Err("传输请求来源与联系人地址不一致".to_string());
    }
    Ok(())
}

async fn send_offer(ip: &str, offer: WireTransferOffer) -> Result<(), String> {
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_envelope(
        &mut writer,
        &TransferControlEnvelope::TransferOffer {
            protocol_version: PROTOCOL_VERSION,
            transfer_protocol_version: if offer.conversation_id.as_deref() == Some("lobby") {
                LOBBY_TRANSFER_PROTOCOL_VERSION
            } else {
                TRANSFER_PROTOCOL_VERSION
            },
            offer: offer.clone(),
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    match read_envelope(&mut reader, TRANSFER_HANDSHAKE_TIMEOUT).await? {
        TransferControlEnvelope::TransferOfferAck { transfer_id } if transfer_id == offer.id => {
            Ok(())
        }
        TransferControlEnvelope::TransferError { error, .. } => Err(error),
        _ => Err("对方未确认文件传输请求".to_string()),
    }
}

async fn retain_lobby_image(
    app_handle: &tauri::AppHandle,
    lobby_item_id: &str,
    source: &SourceInspection,
) -> Result<PathBuf, String> {
    let root = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("lan_collaboration")
        .join("lobby");
    let item_directory = root.join(lobby_item_id);
    tokio::fs::create_dir_all(&item_directory)
        .await
        .map_err(|error| format!("创建大厅图片副本目录失败：{error}"))?;
    let target = item_directory.join(&source.display_name);
    if let Err(error) = tokio::fs::copy(&source.path, &target).await {
        let _ = tokio::fs::remove_dir_all(&item_directory).await;
        return Err(format!("保存大厅图片副本失败：{error}"));
    }
    Ok(target)
}

#[tauri::command]
pub async fn offer_lan_files(
    app_handle: tauri::AppHandle,
    request: OfferLanFilesRequest,
) -> Result<LanFileOfferResult, String> {
    let OfferLanFilesRequest {
        to_id,
        mut to_ids,
        paths,
        conversation_id: requested_conversation,
    } = request;
    if paths.is_empty() {
        return Err("请选择至少一个文件或目录".to_string());
    }
    if paths.len() > 100 {
        return Err("一次最多发送 100 个文件或目录".to_string());
    }
    if let Some(to_id) = to_id {
        to_ids.push(to_id);
    }
    let mut seen_recipients = HashSet::new();
    to_ids.retain(|recipient_id| {
        !recipient_id.trim().is_empty() && seen_recipients.insert(recipient_id.clone())
    });
    let is_lobby = requested_conversation.as_deref() == Some("lobby");
    if to_ids.is_empty() && !is_lobby {
        return Err("没有可接收传输的在线联系人".to_string());
    }
    let conversation_id = match requested_conversation.as_deref() {
        None if to_ids.len() == 1 => format!("direct:{}", to_ids[0]),
        None => return Err("私聊传输只能选择一个联系人".to_string()),
        Some("lobby") => "lobby".to_string(),
        Some(_) => return Err("不支持的传输会话".to_string()),
    };
    let profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let db_path = state_paths()?.0;
    let mut transfers = Vec::new();
    let mut failures = Vec::new();

    for source_path in paths {
        let mut inspection = match inspect_source(PathBuf::from(&source_path)).await {
            Ok(value) => value,
            Err(error) => {
                failures.push(LanTransferFailure {
                    path: source_path,
                    error,
                });
                continue;
            }
        };
        if is_lobby
            && (inspection.kind != "file"
                || !inspection
                    .mime_type
                    .as_deref()
                    .is_some_and(|value| value.starts_with("image/")))
        {
            failures.push(LanTransferFailure {
                path: source_path,
                error: "局域网大厅只能发送图片，其他内容请在联系人私聊中发送".to_string(),
            });
            continue;
        }
        let lobby_item_id = is_lobby.then(|| uuid::Uuid::new_v4().to_string());
        if let Some(item_id) = lobby_item_id.as_deref() {
            let retained_path = match retain_lobby_image(&app_handle, item_id, &inspection).await {
                Ok(path) => path,
                Err(error) => {
                    failures.push(LanTransferFailure {
                        path: source_path,
                        error,
                    });
                    continue;
                }
            };
            inspection = match inspect_source(retained_path).await {
                Ok(value) => value,
                Err(error) => {
                    failures.push(LanTransferFailure {
                        path: source_path,
                        error,
                    });
                    continue;
                }
            };
        }
        let created_at = now_millis();
        if let Some(item_id) = lobby_item_id.as_ref() {
            let canonical = LanTransfer {
                id: item_id.clone(),
                lobby_item_id: Some(item_id.clone()),
                conversation_id: "lobby".to_string(),
                kind: inspection.kind.clone(),
                from_id: profile.id.clone(),
                from_name: profile.display_name.clone(),
                provider_id: profile.id.clone(),
                provider_name: profile.display_name.clone(),
                to_id: String::new(),
                display_name: inspection.display_name.clone(),
                item_count: inspection.item_count,
                total_bytes: inspection.total_bytes,
                mime_type: inspection.mime_type.clone(),
                content_hash: inspection.content_hash.clone(),
                payload_format: inspection.payload_format.clone(),
                manifest: inspection.manifest.clone(),
                status: "completed".to_string(),
                direction: "outgoing".to_string(),
                source_path: Some(inspection.path.to_string_lossy().to_string()),
                received_path: None,
                error: None,
                created_at,
                updated_at: created_at,
            };
            let conn = open_database(&db_path)?;
            insert_transfer(&conn, &canonical)?;
            emit_changed(&app_handle, &canonical);
            transfers.push(canonical);
        }
        for recipient_id in &to_ids {
            let peer = match transfer_target(
                recipient_id,
                transfer_min_protocol_version(&inspection.kind, is_lobby.then_some("lobby")),
            ) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(LanTransferFailure {
                        path: source_path.clone(),
                        error: format!("联系人 {recipient_id}：{error}"),
                    });
                    continue;
                }
            };
            let transfer_id = uuid::Uuid::new_v4().to_string();
            let manifest = inspection.manifest.clone();
            let offer = WireTransferOffer {
                id: transfer_id.clone(),
                lobby_item_id: lobby_item_id.clone(),
                kind: inspection.kind.clone(),
                from_id: profile.id.clone(),
                from_name: profile.display_name.clone(),
                provider_id: profile.id.clone(),
                provider_name: profile.display_name.clone(),
                to_id: recipient_id.clone(),
                conversation_id: requested_conversation.clone(),
                display_name: inspection.display_name.clone(),
                item_count: inspection.item_count,
                total_bytes: inspection.total_bytes,
                mime_type: inspection.mime_type.clone(),
                content_hash: inspection.content_hash.clone(),
                payload_format: inspection.payload_format.clone(),
                manifest: manifest.clone(),
                created_at,
            };
            let transfer = LanTransfer {
                id: transfer_id.clone(),
                lobby_item_id: lobby_item_id.clone(),
                conversation_id: conversation_id.clone(),
                kind: inspection.kind.clone(),
                from_id: profile.id.clone(),
                from_name: profile.display_name.clone(),
                provider_id: profile.id.clone(),
                provider_name: profile.display_name.clone(),
                to_id: recipient_id.clone(),
                display_name: inspection.display_name.clone(),
                item_count: inspection.item_count,
                total_bytes: inspection.total_bytes,
                mime_type: inspection.mime_type.clone(),
                content_hash: inspection.content_hash.clone(),
                payload_format: inspection.payload_format.clone(),
                manifest,
                status: "waiting".to_string(),
                direction: "outgoing".to_string(),
                source_path: Some(inspection.path.to_string_lossy().to_string()),
                received_path: None,
                error: None,
                created_at,
                updated_at: created_at,
            };
            let conn = open_database(&db_path)?;
            insert_transfer(&conn, &transfer)?;
            if let Err(error) = send_offer(&peer.ip, offer).await {
                conn.execute(
                    "DELETE FROM lan_transfers WHERE id=?1",
                    params![transfer.id],
                )
                .map_err(|database_error| database_error.to_string())?;
                failures.push(LanTransferFailure {
                    path: source_path.clone(),
                    error: format!("{}：{error}", peer.display_name),
                });
                continue;
            }
            emit_changed(&app_handle, &transfer);
            transfers.push(transfer);
        }
    }

    Ok(LanFileOfferResult {
        transfers,
        failures,
    })
}

async fn send_rejection(ip: &str, transfer_id: &str, receiver_id: &str) -> Result<(), String> {
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_envelope(
        &mut writer,
        &TransferControlEnvelope::TransferDecision {
            transfer_id: transfer_id.to_string(),
            receiver_id: receiver_id.to_string(),
            accepted: false,
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    match read_envelope(&mut reader, TRANSFER_HANDSHAKE_TIMEOUT).await? {
        TransferControlEnvelope::TransferDecisionAck {
            transfer_id: ack_id,
        } if ack_id == transfer_id => Ok(()),
        _ => Err("发送方未确认拒绝操作".to_string()),
    }
}

async fn send_transfer_cancellation(
    ip: &str,
    transfer_id: &str,
    requester_id: &str,
) -> Result<(), String> {
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_envelope(
        &mut writer,
        &TransferControlEnvelope::TransferCancel {
            transfer_id: transfer_id.to_string(),
            requester_id: requester_id.to_string(),
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    match read_envelope(&mut reader, TRANSFER_HANDSHAKE_TIMEOUT).await? {
        TransferControlEnvelope::TransferCancelAck {
            transfer_id: ack_id,
        } if ack_id == transfer_id => Ok(()),
        _ => Err("对方未确认中断操作".to_string()),
    }
}

fn atomic_replace_file(temp_path: &Path, target_path: &Path) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<u16> = temp_path.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = target_path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|error| format!("保存接收文件失败：{error}"))?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if target_path.exists() {
            fs::remove_file(target_path).map_err(|error| error.to_string())?;
        }
        fs::rename(temp_path, target_path).map_err(|error| error.to_string())
    }
}

struct TemporaryTransferFile(PathBuf);

impl Drop for TemporaryTransferFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct TemporaryTransferDirectory(PathBuf);

impl Drop for TemporaryTransferDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn atomic_commit_directory(temp_path: &Path, target_path: &Path) -> Result<(), String> {
    if target_path.exists() {
        return Err("接收位置已存在同名目录，请重新接收".to_string());
    }
    let parent = target_path
        .parent()
        .ok_or_else(|| "目录保存位置没有有效的父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::rename(temp_path, target_path).map_err(|error| format!("保存接收目录失败：{error}"))
}

async fn verify_source_unchanged(
    transfer: &LanTransfer,
) -> Result<(PathBuf, TransferManifest), String> {
    let source_path = transfer
        .source_path
        .as_deref()
        .ok_or_else(|| "发送源路径丢失".to_string())?;
    let expected_manifest = parse_and_validate_manifest(
        &transfer.kind,
        &transfer.payload_format,
        transfer.item_count,
        transfer.total_bytes,
        &transfer.content_hash,
        &transfer.manifest,
    )?;
    let source_path = PathBuf::from(source_path);
    let expected_kind = transfer.kind.clone();
    let expected_item_count = transfer.item_count;
    let expected_total_bytes = transfer.total_bytes;
    let expected_content_hash = transfer.content_hash.clone();
    let expected_payload_format = transfer.payload_format.clone();
    let expected_manifest_for_check = expected_manifest.clone();
    tokio::task::spawn_blocking(move || {
        let canonical = fs::canonicalize(source_path)
            .map_err(|error| format!("无法读取发送源内容：{error}"))?;
        let metadata = fs::symlink_metadata(&canonical).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("发送源已变为符号链接，请重新发送".to_string());
        }
        let current = if expected_kind == "directory" && metadata.is_dir() {
            inspect_directory_source(canonical.clone())?
        } else if expected_kind == "file" && metadata.is_file() {
            inspect_file_source(canonical.clone(), metadata)?
        } else {
            return Err("发送源类型已发生变化，请重新发送".to_string());
        };
        let current_manifest: TransferManifest =
            serde_json::from_value(current.manifest.clone())
                .map_err(|_| "无法校验当前发送源清单".to_string())?;
        if current.item_count != expected_item_count
            || current.total_bytes != expected_total_bytes
            || current.content_hash != expected_content_hash
            || current.payload_format != expected_payload_format
            || current_manifest != expected_manifest_for_check
        {
            return Err("发送源内容已发生变化，请重新发送".to_string());
        }
        Ok(canonical)
    })
    .await
    .map_err(|error| error.to_string())?
    .map(|path| (path, expected_manifest))
}

#[allow(clippy::too_many_arguments)]
async fn receive_manifest_file(
    app_handle: &tauri::AppHandle,
    transfer: &LanTransfer,
    reader: &mut BufReader<OwnedReadHalf>,
    item: &TransferManifestItem,
    target_path: &Path,
    buffer: &mut [u8],
    received_total: &mut u64,
    started_at: Instant,
    last_progress: &mut Instant,
    cancellation: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut file = tokio::fs::File::create(target_path)
        .await
        .map_err(|error| format!("创建临时接收文件失败：{error}"))?;
    let mut file_received = 0u64;
    let mut hasher = blake3::Hasher::new();
    while file_received < item.size_bytes {
        let remaining = (item.size_bytes - file_received).min(buffer.len() as u64) as usize;
        let count = tokio::select! {
            _ = wait_for_transfer_cancellation(cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = tokio::time::timeout(TRANSFER_IO_TIMEOUT, reader.read(&mut buffer[..remaining])) => {
                result
                    .map_err(|_| "接收文件数据超时".to_string())?
                    .map_err(|error| error.to_string())?
            }
        };
        if count == 0 {
            return Err("发送方提前结束了文件传输".to_string());
        }
        tokio::select! {
            _ = wait_for_transfer_cancellation(cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = file.write_all(&buffer[..count]) => {
                result.map_err(|error| error.to_string())?;
            }
        }
        hasher.update(&buffer[..count]);
        file_received += count as u64;
        *received_total += count as u64;
        if last_progress.elapsed() >= Duration::from_millis(150)
            || *received_total == transfer.total_bytes
        {
            emit_progress(
                app_handle,
                &transfer.id,
                "transferring",
                "incoming",
                *received_total,
                transfer.total_bytes,
                started_at,
            );
            *last_progress = Instant::now();
        }
    }
    file.flush().await.map_err(|error| error.to_string())?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    drop(file);
    let expected_hash = item
        .content_hash
        .as_deref()
        .ok_or_else(|| "接收清单缺少文件完整性哈希".to_string())?;
    if hasher.finalize().to_hex().as_str() != expected_hash {
        return Err(format!("文件完整性校验失败：{}", item.path));
    }
    Ok(())
}

fn parallel_transfer_stream_count(total_bytes: u64) -> usize {
    if total_bytes > EIGHT_STREAM_MIN_BYTES {
        8
    } else if total_bytes > FOUR_STREAM_MIN_BYTES {
        4
    } else if total_bytes > TWO_STREAM_MIN_BYTES {
        2
    } else {
        1
    }
}

fn parallel_transfer_ranges(total_bytes: u64) -> Vec<(u64, u64)> {
    if total_bytes == 0 {
        return Vec::new();
    }
    let stream_count = parallel_transfer_stream_count(total_bytes);
    let base_length = total_bytes / stream_count as u64;
    let remainder = total_bytes % stream_count as u64;
    let mut offset = 0u64;
    (0..stream_count)
        .map(|index| {
            let length = base_length + u64::from((index as u64) < remainder);
            let range = (offset, length);
            offset += length;
            range
        })
        .collect()
}

async fn send_parallel_receipt(
    ip: &str,
    transfer_id: &str,
    receiver_id: &str,
    status: &str,
    error: Option<String>,
) -> Result<(), String> {
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_envelope(
        &mut writer,
        &TransferControlEnvelope::TransferParallelReceipt {
            transfer_id: transfer_id.to_string(),
            receiver_id: receiver_id.to_string(),
            status: status.to_string(),
            error,
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    match read_envelope(&mut reader, TRANSFER_HANDSHAKE_TIMEOUT).await? {
        TransferControlEnvelope::TransferParallelReceiptAck {
            transfer_id: ack_id,
        } if ack_id == transfer_id => Ok(()),
        _ => Err("发送方未确认并行传输结果".to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn receive_file_range(
    app_handle: tauri::AppHandle,
    transfer: LanTransfer,
    peer_ip: String,
    temp_path: PathBuf,
    offset: u64,
    length: u64,
    mut cancellation: watch::Receiver<bool>,
    received_total: Arc<AtomicU64>,
    started_at: Instant,
    last_progress_ms: Arc<AtomicU64>,
) -> Result<(), String> {
    ensure_transfer_not_cancelled(&cancellation)?;
    let stream = connect_to_peer(&peer_ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_envelope(
        &mut writer,
        &TransferControlEnvelope::TransferRangeRequest {
            transfer_id: transfer.id.clone(),
            receiver_id: transfer.to_id.clone(),
            offset,
            length,
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    match read_envelope(&mut reader, TRANSFER_IO_TIMEOUT).await? {
        TransferControlEnvelope::TransferRangeReady {
            transfer_id,
            offset: ready_offset,
            length: ready_length,
        } if transfer_id == transfer.id && ready_offset == offset && ready_length == length => {}
        TransferControlEnvelope::TransferError { error, .. } => return Err(error),
        _ => return Err("发送方返回了无效的分段传输响应".to_string()),
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&temp_path)
        .await
        .map_err(|error| format!("打开临时接收文件失败：{error}"))?;
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; TRANSFER_CHUNK_SIZE];
    let mut range_received = 0u64;
    while range_received < length {
        let remaining = (length - range_received).min(buffer.len() as u64) as usize;
        let count = tokio::select! {
            _ = wait_for_transfer_cancellation(&mut cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = tokio::time::timeout(TRANSFER_IO_TIMEOUT, reader.read(&mut buffer[..remaining])) => {
                result
                    .map_err(|_| "接收文件分段超时".to_string())?
                    .map_err(|error| error.to_string())?
            }
        };
        if count == 0 {
            return Err("发送方提前结束了文件分段传输".to_string());
        }
        tokio::select! {
            _ = wait_for_transfer_cancellation(&mut cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = file.write_all(&buffer[..count]) => {
                result.map_err(|error| error.to_string())?;
            }
        }
        range_received += count as u64;
        let received = received_total.fetch_add(count as u64, Ordering::Relaxed) + count as u64;
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        let last_ms = last_progress_ms.load(Ordering::Relaxed);
        if received == transfer.total_bytes
            || (elapsed_ms.saturating_sub(last_ms) >= 150
                && last_progress_ms
                    .compare_exchange(last_ms, elapsed_ms, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok())
        {
            emit_progress(
                &app_handle,
                &transfer.id,
                "transferring",
                "incoming",
                received,
                transfer.total_bytes,
                started_at,
            );
        }
    }
    file.flush().await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn download_file_parallel(
    app_handle: &tauri::AppHandle,
    transfer: &LanTransfer,
    peer: &OnlinePeer,
    item: &TransferManifestItem,
    temp_path: &Path,
    destination_path: &Path,
    cancellation: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    let _temp_cleanup = TemporaryTransferFile(temp_path.to_path_buf());
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temp_path)
        .await
        .map_err(|error| format!("创建临时接收文件失败：{error}"))?;
    file.set_len(transfer.total_bytes)
        .await
        .map_err(|error| format!("预分配临时接收文件失败：{error}"))?;
    drop(file);

    let started_at = Instant::now();
    let received_total = Arc::new(AtomicU64::new(0));
    let last_progress_ms = Arc::new(AtomicU64::new(0));
    let mut tasks = tokio::task::JoinSet::new();
    for (offset, length) in parallel_transfer_ranges(transfer.total_bytes) {
        tasks.spawn(receive_file_range(
            app_handle.clone(),
            transfer.clone(),
            peer.ip.clone(),
            temp_path.to_path_buf(),
            offset,
            length,
            cancellation.clone(),
            received_total.clone(),
            started_at,
            last_progress_ms.clone(),
        ));
    }

    let mut transfer_error = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                transfer_error = Some(error);
                tasks.abort_all();
                break;
            }
            Err(error) => {
                transfer_error = Some(format!("文件分段任务异常结束：{error}"));
                tasks.abort_all();
                break;
            }
        }
    }
    while tasks.join_next().await.is_some() {}
    if let Some(error) = transfer_error {
        if error != TRANSFER_CANCELLED_ERROR {
            let _ = send_parallel_receipt(
                &peer.ip,
                &transfer.id,
                &transfer.to_id,
                "failed",
                Some(error.clone()),
            )
            .await;
        }
        return Err(error);
    }
    let finalize_result = async {
        ensure_transfer_not_cancelled(cancellation)?;
        if received_total.load(Ordering::Relaxed) != transfer.total_bytes {
            return Err("并行接收字节数与文件清单不一致".to_string());
        }

        let sync_file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(temp_path)
            .await
            .map_err(|error| error.to_string())?;
        sync_file
            .sync_all()
            .await
            .map_err(|error| error.to_string())?;
        drop(sync_file);
        let hash_path = temp_path.to_path_buf();
        let received_hash = tokio::task::spawn_blocking(move || hash_file(&hash_path))
            .await
            .map_err(|error| error.to_string())??;
        let expected_hash = item
            .content_hash
            .as_deref()
            .ok_or_else(|| "接收清单缺少文件完整性哈希".to_string())?;
        if received_hash != expected_hash {
            return Err("并行传输后的文件完整性校验失败".to_string());
        }
        if is_image_transfer(transfer) {
            validate_received_image(temp_path).await?;
        }
        ensure_transfer_not_cancelled(cancellation)?;
        atomic_replace_file(temp_path, destination_path)
    }
    .await;
    match finalize_result {
        Ok(()) => {
            let _ =
                send_parallel_receipt(&peer.ip, &transfer.id, &transfer.to_id, "completed", None)
                    .await;
            Ok(())
        }
        Err(error) => {
            if error != TRANSFER_CANCELLED_ERROR {
                let _ = send_parallel_receipt(
                    &peer.ip,
                    &transfer.id,
                    &transfer.to_id,
                    "failed",
                    Some(error.clone()),
                )
                .await;
            }
            Err(error)
        }
    }
}

async fn download_transfer(
    app_handle: &tauri::AppHandle,
    transfer: &LanTransfer,
    peer: &OnlinePeer,
    destination_path: &Path,
    cancellation: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    ensure_transfer_not_cancelled(cancellation)?;
    if destination_path.file_name().is_none() {
        return Err("请选择有效的保存位置".to_string());
    }
    let manifest = parse_and_validate_manifest(
        &transfer.kind,
        &transfer.payload_format,
        transfer.item_count,
        transfer.total_bytes,
        &transfer.content_hash,
        &transfer.manifest,
    )?;
    let parent = destination_path
        .parent()
        .ok_or_else(|| "保存位置没有有效的父目录".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| error.to_string())?;
    let temp_name = format!(
        ".{}.{}.pm-transfer.part",
        destination_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("received"),
        transfer.id
    );
    let temp_path = parent.join(temp_name);
    if transfer.kind == "directory" {
        let _ = tokio::fs::remove_dir_all(&temp_path).await;
    } else {
        let _ = tokio::fs::remove_file(&temp_path).await;
    }

    if transfer.kind == "file"
        && ENABLE_PARALLEL_FILE_TRANSFER
        && parallel_transfer_stream_count(transfer.total_bytes) > 1
        && peer.protocol_version >= PARALLEL_TRANSFER_MIN_PROTOCOL_VERSION
    {
        let item = manifest
            .items
            .first()
            .ok_or_else(|| "文件传输清单为空".to_string())?;
        return download_file_parallel(
            app_handle,
            transfer,
            peer,
            item,
            &temp_path,
            destination_path,
            cancellation,
        )
        .await;
    }

    let stream = connect_to_peer(&peer.ip).await?;
    ensure_transfer_not_cancelled(cancellation)?;
    let (reader, mut writer) = stream.into_split();
    write_envelope(
        &mut writer,
        &TransferControlEnvelope::TransferRequest {
            transfer_id: transfer.id.clone(),
            receiver_id: transfer.to_id.clone(),
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    let (total_bytes, content_hash) = match read_envelope(&mut reader, TRANSFER_IO_TIMEOUT).await? {
        TransferControlEnvelope::TransferReady {
            transfer_id,
            total_bytes,
            content_hash,
        } if transfer_id == transfer.id => (total_bytes, content_hash),
        TransferControlEnvelope::TransferError { error, .. } => return Err(error),
        _ => return Err("发送方返回了无效传输响应".to_string()),
    };
    if total_bytes != transfer.total_bytes || content_hash != transfer.content_hash {
        return Err("发送方内容已发生变化，请重新发送".to_string());
    }

    let mut buffer = vec![0u8; TRANSFER_CHUNK_SIZE];
    let mut received = 0u64;
    let started_at = Instant::now();
    let mut last_progress = Instant::now();
    if transfer.kind == "directory" {
        let _temp_cleanup = TemporaryTransferDirectory(temp_path.clone());
        tokio::fs::create_dir_all(&temp_path)
            .await
            .map_err(|error| format!("创建临时接收目录失败：{error}"))?;
        for item in &manifest.items {
            let target = temp_path.join(Path::new(&item.path));
            if item.kind == "directory" {
                tokio::fs::create_dir_all(&target)
                    .await
                    .map_err(|error| error.to_string())?;
            } else {
                receive_manifest_file(
                    app_handle,
                    transfer,
                    &mut reader,
                    item,
                    &target,
                    &mut buffer,
                    &mut received,
                    started_at,
                    &mut last_progress,
                    cancellation,
                )
                .await?;
            }
        }
        if received != total_bytes {
            return Err("目录接收字节数与清单不一致".to_string());
        }
        if total_bytes == 0 {
            emit_progress(
                app_handle,
                &transfer.id,
                "transferring",
                "incoming",
                0,
                0,
                started_at,
            );
        }
        ensure_transfer_not_cancelled(cancellation)?;
        atomic_commit_directory(&temp_path, destination_path)?;
    } else {
        let _temp_cleanup = TemporaryTransferFile(temp_path.clone());
        let item = manifest
            .items
            .first()
            .ok_or_else(|| "文件传输清单为空".to_string())?;
        receive_manifest_file(
            app_handle,
            transfer,
            &mut reader,
            item,
            &temp_path,
            &mut buffer,
            &mut received,
            started_at,
            &mut last_progress,
            cancellation,
        )
        .await?;
        if is_image_transfer(transfer) {
            validate_received_image(&temp_path).await?;
        }
        ensure_transfer_not_cancelled(cancellation)?;
        atomic_replace_file(&temp_path, destination_path)?;
    }
    let _ = write_envelope(
        &mut writer,
        &TransferControlEnvelope::TransferReceipt {
            transfer_id: transfer.id.clone(),
            status: "completed".to_string(),
            error: None,
        },
    )
    .await;
    Ok(())
}

#[tauri::command]
pub async fn respond_lan_transfer(
    app_handle: tauri::AppHandle,
    request: RespondLanTransferRequest,
) -> Result<LanTransfer, String> {
    let db_path = state_paths()?.0;
    let conn = open_database(&db_path)?;
    let transfer = load_transfer(&conn, &request.transfer_id)?;
    if transfer.direction != "incoming" {
        return Err("只能接收或拒绝收到的文件".to_string());
    }
    let profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    if transfer.to_id != profile.id {
        return Err("该文件传输不属于当前用户".to_string());
    }

    match request.action.as_str() {
        "reject" => {
            if transfer.status != "pending" && transfer.status != "failed" {
                return Err("当前传输状态不能拒绝".to_string());
            }
            let rejected = reject_incoming_transfer(&conn, &transfer.id)?;
            emit_changed(&app_handle, &rejected);
            let notification_error = match transfer_target(
                &transfer.provider_id,
                transfer_min_protocol_version(
                    &transfer.kind,
                    Some(transfer.conversation_id.as_str()),
                ),
            ) {
                Ok(peer) => send_rejection(&peer.ip, &transfer.id, &profile.id)
                    .await
                    .err(),
                Err(error) => Some(format!("已在本机拒绝；未能通知发送方：{error}")),
            };
            let updated = if let Some(error) = notification_error.as_deref() {
                update_transfer(&conn, &transfer.id, "rejected", Some(error), None)?
            } else {
                rejected
            };
            if notification_error.is_some() {
                emit_changed(&app_handle, &updated);
            }
            Ok(updated)
        }
        "accept" => {
            if transfer.status != "pending" && transfer.status != "failed" {
                return Err("当前传输状态不能开始接收".to_string());
            }
            let peer = transfer_target(
                &transfer.provider_id,
                transfer_min_protocol_version(
                    &transfer.kind,
                    Some(transfer.conversation_id.as_str()),
                ),
            )?;
            let (_active_guard, mut cancellation, _, _) = register_active_transfer(&transfer.id)?;
            let path_guard = RECEIVE_PATH_LOCK
                .lock()
                .map_err(|error| error.to_string())?;
            let destination = if let Some(value) = request
                .destination_path
                .filter(|value| !value.trim().is_empty())
            {
                PathBuf::from(value)
            } else {
                allocate_default_destination(&conn, &transfer)?
            };
            let transferring = claim_incoming_transfer(&conn, &transfer.id, &destination)?;
            drop(path_guard);
            emit_changed(&app_handle, &transferring);
            match download_transfer(
                &app_handle,
                &transfer,
                &peer,
                &destination,
                &mut cancellation,
            )
            .await
            {
                Ok(()) => {
                    let completed = update_transfer(
                        &open_database(&db_path)?,
                        &transfer.id,
                        "completed",
                        None,
                        Some(&destination.to_string_lossy()),
                    )?;
                    emit_changed(&app_handle, &completed);
                    Ok(completed)
                }
                Err(error) => {
                    if error == TRANSFER_CANCELLED_ERROR
                        || load_transfer(&open_database(&db_path)?, &transfer.id)?.status
                            == "cancelled"
                    {
                        let cancelled =
                            mark_transfer_cancelled(&open_database(&db_path)?, &transfer.id)?;
                        emit_changed(&app_handle, &cancelled);
                        return Err(TRANSFER_CANCELLED_ERROR.to_string());
                    }
                    let failed = update_transfer(
                        &open_database(&db_path)?,
                        &transfer.id,
                        "failed",
                        Some(&error),
                        None,
                    )?;
                    emit_changed(&app_handle, &failed);
                    Err(error)
                }
            }
        }
        _ => Err("不支持的文件传输操作".to_string()),
    }
}

#[tauri::command]
pub async fn cancel_lan_transfer(
    app_handle: tauri::AppHandle,
    request: CancelLanTransferRequest,
) -> Result<LanTransfer, String> {
    let (db_path, _) = state_paths()?;
    let conn = open_database(&db_path)?;
    let transfer = load_transfer(&conn, &request.transfer_id)?;
    if transfer.status != "transferring" {
        return Err("当前文件没有正在进行的传输".to_string());
    }
    let profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let peer_id = if transfer.direction == "outgoing" && transfer.provider_id == profile.id {
        transfer.to_id.clone()
    } else if transfer.direction == "incoming" && transfer.to_id == profile.id {
        transfer.provider_id.clone()
    } else {
        return Err("该文件传输不属于当前用户".to_string());
    };
    let cancelled = mark_transfer_cancelled(&conn, &transfer.id)?;
    signal_active_transfer_cancellation(&transfer.id);
    emit_changed(&app_handle, &cancelled);
    if let Ok(peer) = transfer_target(
        &peer_id,
        transfer_min_protocol_version(&transfer.kind, Some(&transfer.conversation_id)),
    ) {
        if peer.protocol_version >= TRANSFER_CANCEL_MIN_PROTOCOL_VERSION {
            let transfer_id = transfer.id.clone();
            let requester_id = profile.id;
            tokio::spawn(async move {
                let _ = send_transfer_cancellation(&peer.ip, &transfer_id, &requester_id).await;
            });
        }
    }
    Ok(cancelled)
}

fn staging_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_data_dir()
        .map(|path| path.join("lan_collaboration").join("staging"))
        .map_err(|error| error.to_string())
}

pub(super) fn clear_lobby_cache(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let directory = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("lan_collaboration")
        .join("lobby");
    if directory.exists() {
        fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&directory).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_lan_transfer_staging_path(
    app_handle: tauri::AppHandle,
    file_name: String,
) -> Result<String, String> {
    let file_name = validate_file_name(&file_name)?;
    let directory = staging_dir(&app_handle)?;
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    let item_directory = directory.join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&item_directory)
        .await
        .map_err(|error| error.to_string())?;
    let target = item_directory.join(file_name);
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn discard_lan_transfer_staging_file(
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<(), String> {
    let staging = staging_dir(&app_handle)?;
    let target = PathBuf::from(path);
    let item_directory = target
        .parent()
        .ok_or_else(|| "无效的传输暂存路径".to_string())?;
    if item_directory.parent() != Some(staging.as_path()) {
        return Err("拒绝删除传输暂存目录之外的文件".to_string());
    }
    if let Err(error) = tokio::fs::remove_file(&target).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error.to_string());
        }
    }
    let _ = tokio::fs::remove_dir(item_directory).await;
    Ok(())
}

fn transfer_from_offer(offer: &WireTransferOffer, conversation_id: String) -> LanTransfer {
    let received_at = now_millis();
    LanTransfer {
        id: offer.id.clone(),
        lobby_item_id: offer.lobby_item_id.clone(),
        conversation_id,
        kind: offer.kind.clone(),
        from_id: offer.from_id.clone(),
        from_name: offer.from_name.clone(),
        provider_id: offer.provider_id.clone(),
        provider_name: offer.provider_name.clone(),
        to_id: offer.to_id.clone(),
        display_name: offer.display_name.clone(),
        item_count: offer.item_count,
        total_bytes: offer.total_bytes,
        mime_type: offer.mime_type.clone(),
        content_hash: offer.content_hash.clone(),
        payload_format: offer.payload_format.clone(),
        manifest: offer.manifest.clone(),
        status: "pending".to_string(),
        direction: "incoming".to_string(),
        source_path: None,
        received_path: None,
        error: None,
        created_at: offer.created_at,
        updated_at: received_at,
    }
}

pub(super) fn known_lobby_image_ids(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT DISTINCT lobby_item_id FROM lan_transfers
             WHERE conversation_id='lobby' AND lobby_item_id IS NOT NULL
               AND status NOT IN ('failed','rejected')",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<HashSet<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn relayable_lobby_images(conn: &Connection) -> Result<Vec<LanTransfer>, String> {
    let mut transfers = load_transfers(conn)?;
    transfers.reverse();
    let mut seen = HashSet::new();
    transfers.retain(|transfer| {
        let Some(item_id) = transfer.lobby_item_id.as_ref() else {
            return false;
        };
        if transfer.conversation_id != "lobby" || !is_image_transfer(transfer) {
            return false;
        }
        let has_copy = transfer
            .received_path
            .as_deref()
            .filter(|path| Path::new(path).is_file())
            .or_else(|| {
                transfer
                    .source_path
                    .as_deref()
                    .filter(|path| Path::new(path).is_file())
            })
            .is_some();
        has_copy && seen.insert(item_id.clone())
    });
    Ok(transfers)
}

pub(super) async fn prepare_lobby_history_offer(
    db_path: &Path,
    source: &LanTransfer,
    requester_id: &str,
    provider: &super::LanProfile,
) -> Result<WireTransferOffer, String> {
    let item_id = source
        .lobby_item_id
        .clone()
        .ok_or_else(|| "大厅图片缺少公共条目标识".to_string())?;
    let local_path = source
        .received_path
        .as_deref()
        .filter(|path| Path::new(path).is_file())
        .or_else(|| {
            source
                .source_path
                .as_deref()
                .filter(|path| Path::new(path).is_file())
        })
        .ok_or_else(|| "大厅图片本地副本已不存在".to_string())?;
    let inspection = inspect_source(PathBuf::from(local_path)).await?;
    if inspection.kind != "file"
        || !inspection
            .mime_type
            .as_deref()
            .is_some_and(|value| value.starts_with("image/"))
        || inspection.content_hash != source.content_hash
    {
        return Err("大厅历史图片副本已不存在或内容已变化".to_string());
    }
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let transfer = LanTransfer {
        id: transfer_id.clone(),
        lobby_item_id: Some(item_id.clone()),
        conversation_id: "lobby".to_string(),
        kind: inspection.kind.clone(),
        from_id: source.from_id.clone(),
        from_name: source.from_name.clone(),
        provider_id: provider.id.clone(),
        provider_name: provider.display_name.clone(),
        to_id: requester_id.to_string(),
        display_name: inspection.display_name.clone(),
        item_count: inspection.item_count,
        total_bytes: inspection.total_bytes,
        mime_type: inspection.mime_type.clone(),
        content_hash: inspection.content_hash.clone(),
        payload_format: inspection.payload_format.clone(),
        manifest: inspection.manifest.clone(),
        status: "waiting".to_string(),
        direction: "outgoing".to_string(),
        source_path: Some(inspection.path.to_string_lossy().to_string()),
        received_path: None,
        error: None,
        created_at: source.created_at,
        updated_at: now_millis(),
    };
    insert_transfer(&open_database(db_path)?, &transfer)?;
    Ok(WireTransferOffer {
        id: transfer_id,
        lobby_item_id: Some(item_id),
        kind: transfer.kind,
        from_id: transfer.from_id,
        from_name: transfer.from_name,
        provider_id: transfer.provider_id,
        provider_name: transfer.provider_name,
        to_id: transfer.to_id,
        conversation_id: Some("lobby".to_string()),
        display_name: transfer.display_name,
        item_count: transfer.item_count,
        total_bytes: transfer.total_bytes,
        mime_type: transfer.mime_type,
        content_hash: transfer.content_hash,
        payload_format: transfer.payload_format,
        manifest: transfer.manifest,
        created_at: transfer.created_at,
    })
}

pub(super) async fn accept_lobby_history_offers(
    offers: Vec<WireTransferOffer>,
    remote_ip: &str,
    app_handle: &tauri::AppHandle,
    db_path: &Path,
) -> Result<usize, String> {
    let local_profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let conn = open_database(db_path)?;
    let mut inserted_count = 0usize;
    for offer in offers {
        if offer.to_id != local_profile.id
            || offer.provider_id.is_empty()
            || offer.lobby_item_id.is_none()
            || resolve_offer_conversation(&offer, PROTOCOL_VERSION).as_deref() != Ok("lobby")
        {
            continue;
        }
        if validate_peer_ip(&offer.provider_id, remote_ip).is_err()
            || parse_and_validate_manifest(
                &offer.kind,
                &offer.payload_format,
                offer.item_count,
                offer.total_bytes,
                &offer.content_hash,
                &offer.manifest,
            )
            .is_err()
        {
            continue;
        }
        let transfer = transfer_from_offer(&offer, "lobby".to_string());
        if let Some(item_id) = transfer.lobby_item_id.as_deref() {
            conn.execute(
                "DELETE FROM lan_transfers
                 WHERE lobby_item_id=?1 AND direction='incoming'
                   AND status IN ('failed','rejected')",
                params![item_id],
            )
            .map_err(|error| error.to_string())?;
        }
        if !insert_transfer(&conn, &transfer)? {
            let _ = send_rejection(remote_ip, &offer.id, &local_profile.id).await;
            continue;
        }
        inserted_count += 1;
        emit_changed(app_handle, &transfer);
        if should_auto_receive_image(&transfer) {
            let app_handle = app_handle.clone();
            let transfer_id = transfer.id.clone();
            tokio::spawn(async move {
                tokio::task::yield_now().await;
                let _ = respond_lan_transfer(
                    app_handle,
                    RespondLanTransferRequest {
                        transfer_id,
                        action: "accept".to_string(),
                        destination_path: None,
                    },
                )
                .await;
            });
        }
    }
    Ok(inserted_count)
}

#[allow(clippy::too_many_arguments)]
async fn send_manifest_file(
    app_handle: &tauri::AppHandle,
    transfer: &LanTransfer,
    writer: &mut OwnedWriteHalf,
    item: &TransferManifestItem,
    source_path: &Path,
    buffer: &mut [u8],
    sent_total: &mut u64,
    started_at: Instant,
    last_progress: &mut Instant,
    cancellation: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    let mut file = tokio::fs::File::open(source_path)
        .await
        .map_err(|error| format!("读取发送源失败：{error}"))?;
    let mut file_sent = 0u64;
    while file_sent < item.size_bytes {
        let remaining = (item.size_bytes - file_sent).min(buffer.len() as u64) as usize;
        let count = tokio::select! {
            _ = wait_for_transfer_cancellation(cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = file.read(&mut buffer[..remaining]) => {
                result.map_err(|error| error.to_string())?
            }
        };
        if count == 0 {
            return Err(format!("发送源文件在传输前被截断：{}", item.path));
        }
        tokio::select! {
            _ = wait_for_transfer_cancellation(cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = tokio::time::timeout(TRANSFER_IO_TIMEOUT, writer.write_all(&buffer[..count])) => {
                result
                    .map_err(|_| "发送文件数据超时".to_string())?
                    .map_err(|error| error.to_string())?;
            }
        }
        file_sent += count as u64;
        *sent_total += count as u64;
        if last_progress.elapsed() >= Duration::from_millis(150)
            || *sent_total == transfer.total_bytes
        {
            emit_progress(
                app_handle,
                &transfer.id,
                "transferring",
                "outgoing",
                *sent_total,
                transfer.total_bytes,
                started_at,
            );
            *last_progress = Instant::now();
        }
    }
    let mut extra = [0u8; 1];
    if file
        .read(&mut extra)
        .await
        .map_err(|error| error.to_string())?
        != 0
    {
        return Err(format!("发送源文件大小已发生变化：{}", item.path));
    }
    Ok(())
}

async fn serve_transfer_range(
    transfer_id: &str,
    receiver_id: &str,
    remote_ip: &str,
    offset: u64,
    length: u64,
    writer: &mut OwnedWriteHalf,
    app_handle: &tauri::AppHandle,
    db_path: &Path,
) -> Result<(), String> {
    validate_peer_ip(receiver_id, remote_ip)?;
    let conn = open_database(db_path)?;
    let transfer = load_transfer(&conn, transfer_id)?;
    if transfer.direction != "outgoing"
        || transfer.to_id != receiver_id
        || transfer.kind != "file"
        || parallel_transfer_stream_count(transfer.total_bytes) == 1
    {
        return Err("文件分段请求与发送记录不匹配".to_string());
    }
    let range_end = offset
        .checked_add(length)
        .ok_or_else(|| "文件分段范围无效".to_string())?;
    if length == 0 || range_end > transfer.total_bytes {
        return Err("文件分段范围无效".to_string());
    }
    let (_active_guard, mut cancellation, sent_total, started_at) =
        register_active_transfer(transfer_id)?;
    let claimed = conn
        .execute(
            "UPDATE lan_transfers SET status='transferring',error=NULL,updated_at=?2
             WHERE id=?1 AND direction='outgoing' AND status IN ('waiting','failed','cancelled')",
            params![transfer_id, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    if claimed > 0 {
        emit_changed(app_handle, &load_transfer(&conn, transfer_id)?);
    } else if load_transfer(&conn, transfer_id)?.status != "transferring" {
        return Err("该文件当前不能进行分段传输".to_string());
    }

    let manifest = parse_and_validate_manifest(
        &transfer.kind,
        &transfer.payload_format,
        transfer.item_count,
        transfer.total_bytes,
        &transfer.content_hash,
        &transfer.manifest,
    )?;
    let item = manifest
        .items
        .first()
        .ok_or_else(|| "文件传输清单为空".to_string())?;
    if item.kind != "file" || item.size_bytes != transfer.total_bytes {
        return Err("文件传输清单与分段请求不匹配".to_string());
    }
    let source_path = PathBuf::from(
        transfer
            .source_path
            .as_deref()
            .ok_or_else(|| "发送记录缺少源文件路径".to_string())?,
    );
    let metadata = tokio::fs::symlink_metadata(&source_path)
        .await
        .map_err(|error| format!("无法读取发送源内容：{error}"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != transfer.total_bytes
        || metadata
            .modified()
            .map(system_time_millis)
            .unwrap_or_default()
            != item.modified_at
    {
        return Err("发送源内容已发生变化，请重新发送".to_string());
    }
    ensure_transfer_not_cancelled(&cancellation)?;
    let mut file = tokio::fs::File::open(&source_path)
        .await
        .map_err(|error| format!("读取发送源失败：{error}"))?;
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(|error| error.to_string())?;
    write_envelope(
        writer,
        &TransferControlEnvelope::TransferRangeReady {
            transfer_id: transfer.id.clone(),
            offset,
            length,
        },
    )
    .await?;

    let mut buffer = vec![0u8; TRANSFER_CHUNK_SIZE];
    let mut range_sent = 0u64;
    while range_sent < length {
        let remaining = (length - range_sent).min(buffer.len() as u64) as usize;
        let count = tokio::select! {
            _ = wait_for_transfer_cancellation(&mut cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = file.read(&mut buffer[..remaining]) => {
                result.map_err(|error| error.to_string())?
            }
        };
        if count == 0 {
            return Err("发送源文件在分段传输前被截断".to_string());
        }
        tokio::select! {
            _ = wait_for_transfer_cancellation(&mut cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = tokio::time::timeout(TRANSFER_IO_TIMEOUT, writer.write_all(&buffer[..count])) => {
                result
                    .map_err(|_| "发送文件分段超时".to_string())?
                    .map_err(|error| error.to_string())?;
            }
        }
        range_sent += count as u64;
        let sent = sent_total.fetch_add(count as u64, Ordering::Relaxed) + count as u64;
        emit_progress(
            app_handle,
            &transfer.id,
            "transferring",
            "outgoing",
            sent.min(transfer.total_bytes),
            transfer.total_bytes,
            started_at,
        );
    }
    writer.flush().await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn complete_parallel_transfer(
    transfer_id: &str,
    receiver_id: &str,
    remote_ip: &str,
    status: &str,
    error: Option<&str>,
    writer: &mut OwnedWriteHalf,
    app_handle: &tauri::AppHandle,
    db_path: &Path,
) -> Result<(), String> {
    validate_peer_ip(receiver_id, remote_ip)?;
    let conn = open_database(db_path)?;
    let transfer = load_transfer(&conn, transfer_id)?;
    if transfer.direction != "outgoing" || transfer.to_id != receiver_id {
        return Err("并行传输结果与发送记录不匹配".to_string());
    }
    let updated = match status {
        "completed" => update_transfer(&conn, transfer_id, "completed", None, None)?,
        "failed" => {
            signal_active_transfer_cancellation(transfer_id);
            update_transfer(
                &conn,
                transfer_id,
                "failed",
                Some(error.unwrap_or("接收方报告并行文件传输失败")),
                None,
            )?
        }
        _ => return Err("接收方返回了无效的并行传输状态".to_string()),
    };
    emit_changed(app_handle, &updated);
    write_envelope(
        writer,
        &TransferControlEnvelope::TransferParallelReceiptAck {
            transfer_id: transfer_id.to_string(),
        },
    )
    .await
}

async fn serve_transfer(
    transfer_id: &str,
    receiver_id: &str,
    remote_ip: &str,
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    app_handle: &tauri::AppHandle,
    db_path: &Path,
) -> Result<(), String> {
    validate_peer_ip(receiver_id, remote_ip)?;
    let conn = open_database(db_path)?;
    let transfer = load_transfer(&conn, transfer_id)?;
    if transfer.direction != "outgoing" || transfer.to_id != receiver_id {
        return Err("文件传输请求与发送记录不匹配".to_string());
    }
    let (_active_guard, mut cancellation, _, _) = register_active_transfer(transfer_id)?;
    let claimed = conn
        .execute(
            "UPDATE lan_transfers SET status='transferring',error=NULL,updated_at=?2
             WHERE id=?1 AND direction='outgoing' AND status IN ('waiting','failed','cancelled')",
            params![transfer_id, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    if claimed == 0 {
        return Err("该文件当前不能重复传输".to_string());
    }
    let transferring = load_transfer(&conn, transfer_id)?;
    emit_changed(app_handle, &transferring);

    let result = async {
        let (source_path, manifest) = verify_source_unchanged(&transfer).await?;
        ensure_transfer_not_cancelled(&cancellation)?;
        write_envelope(
            writer,
            &TransferControlEnvelope::TransferReady {
                transfer_id: transfer.id.clone(),
                total_bytes: transfer.total_bytes,
                content_hash: transfer.content_hash.clone(),
            },
        )
        .await?;

        let mut buffer = vec![0u8; TRANSFER_CHUNK_SIZE];
        let mut sent = 0u64;
        let started_at = Instant::now();
        let mut last_progress = Instant::now();
        if transfer.kind == "directory" {
            for item in &manifest.items {
                if item.kind != "file" {
                    continue;
                }
                let item_source = source_path.join(Path::new(&item.path));
                send_manifest_file(
                    app_handle,
                    &transfer,
                    writer,
                    item,
                    &item_source,
                    &mut buffer,
                    &mut sent,
                    started_at,
                    &mut last_progress,
                    &mut cancellation,
                )
                .await?;
            }
        } else {
            let item = manifest
                .items
                .first()
                .ok_or_else(|| "文件传输清单为空".to_string())?;
            send_manifest_file(
                app_handle,
                &transfer,
                writer,
                item,
                &source_path,
                &mut buffer,
                &mut sent,
                started_at,
                &mut last_progress,
                &mut cancellation,
            )
            .await?;
        }
        if sent != transfer.total_bytes {
            return Err("发送字节数与传输清单不一致".to_string());
        }
        if transfer.total_bytes == 0 {
            emit_progress(
                app_handle,
                &transfer.id,
                "transferring",
                "outgoing",
                0,
                0,
                started_at,
            );
        }
        ensure_transfer_not_cancelled(&cancellation)?;
        writer.flush().await.map_err(|error| error.to_string())?;
        let receipt = tokio::select! {
            _ = wait_for_transfer_cancellation(&mut cancellation) => {
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            result = read_envelope(reader, TRANSFER_IO_TIMEOUT) => result?,
        };
        match receipt {
            TransferControlEnvelope::TransferReceipt {
                transfer_id: receipt_id,
                status,
                error,
            } if receipt_id == transfer.id => {
                if status == "completed" {
                    Ok(())
                } else {
                    Err(error.unwrap_or_else(|| "接收方报告文件传输失败".to_string()))
                }
            }
            _ => Err("接收方未返回文件完整性确认".to_string()),
        }
    }
    .await;

    match result {
        Ok(()) => {
            let completed = update_transfer(
                &open_database(db_path)?,
                transfer_id,
                "completed",
                None,
                None,
            )?;
            emit_changed(app_handle, &completed);
            Ok(())
        }
        Err(error) => {
            if error == TRANSFER_CANCELLED_ERROR
                || load_transfer(&open_database(db_path)?, transfer_id)?.status == "cancelled"
            {
                let cancelled = mark_transfer_cancelled(&open_database(db_path)?, transfer_id)?;
                emit_changed(app_handle, &cancelled);
                return Err(TRANSFER_CANCELLED_ERROR.to_string());
            }
            let failed = update_transfer(
                &open_database(db_path)?,
                transfer_id,
                "failed",
                Some(&error),
                None,
            )?;
            emit_changed(app_handle, &failed);
            let _ = write_envelope(
                writer,
                &TransferControlEnvelope::TransferError {
                    transfer_id: transfer_id.to_string(),
                    error: error.clone(),
                },
            )
            .await;
            Err(error)
        }
    }
}

pub(super) async fn handle_connection(
    envelope: TransferControlEnvelope,
    remote_ip: String,
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
) -> Result<(), String> {
    match envelope {
        TransferControlEnvelope::TransferOffer {
            protocol_version,
            transfer_protocol_version,
            offer,
        } => {
            let min_protocol_version =
                transfer_min_protocol_version(&offer.kind, offer.conversation_id.as_deref());
            let expected_transfer_protocol = if offer.conversation_id.as_deref() == Some("lobby") {
                LOBBY_TRANSFER_PROTOCOL_VERSION
            } else {
                TRANSFER_PROTOCOL_VERSION
            };
            if protocol_version < min_protocol_version
                || transfer_protocol_version != expected_transfer_protocol
            {
                return Err("不兼容的文件传输协议版本".to_string());
            }
            let local_profile = P2P_GLOBAL
                .lock()
                .map_err(|error| error.to_string())?
                .profile
                .clone()
                .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
            if offer.to_id != local_profile.id
                || offer.provider_id.is_empty()
                || offer.total_bytes > MAX_TRANSFER_BYTES
                || offer.content_hash.len() != 64
                || !offer
                    .content_hash
                    .bytes()
                    .all(|value| value.is_ascii_hexdigit())
            {
                return Err("收到无效的文件传输请求".to_string());
            }
            validate_peer_ip(&offer.provider_id, &remote_ip)?;
            validate_file_name(&offer.display_name)?;
            let conversation_id = resolve_offer_conversation(&offer, protocol_version)?;
            parse_and_validate_manifest(
                &offer.kind,
                &offer.payload_format,
                offer.item_count,
                offer.total_bytes,
                &offer.content_hash,
                &offer.manifest,
            )?;
            let conn = open_database(&db_path)?;
            let mut contact_changed =
                upsert_legacy_contact(&conn, &offer.provider_id, &offer.provider_name, &remote_ip)?
                    | set_online_legacy_peer(&offer.provider_id, &offer.provider_name, &remote_ip)?;
            let updated_protocol = conn
                .execute(
                    "UPDATE lan_contacts SET protocol_version=MAX(protocol_version,?2) WHERE id=?1 AND protocol_version<?2",
                    params![offer.provider_id, protocol_version as i64],
                )
                .map_err(|error| error.to_string())?;
            contact_changed |= updated_protocol > 0;
            if let Ok(mut state) = P2P_GLOBAL.lock() {
                if let Some(peer) = state.online_users.get_mut(&offer.provider_id) {
                    peer.protocol_version = peer.protocol_version.max(protocol_version);
                }
            }
            if contact_changed {
                emit_contacts_changed(&app_handle);
            }
            let transfer = transfer_from_offer(&offer, conversation_id);
            let inserted = insert_transfer(&conn, &transfer)?;
            let auto_receive = inserted
                && transfer.kind == "file"
                && should_auto_receive_image(&transfer)
                && (transfer.conversation_id == "lobby"
                    || super::load_local_settings(&conn)
                        .map(|settings| settings.auto_receive_images)
                        .unwrap_or(true));
            if inserted {
                emit_changed(&app_handle, &transfer);
                super::notify_incoming_transfer(&app_handle, &transfer, auto_receive);
            }
            write_envelope(
                writer,
                &TransferControlEnvelope::TransferOfferAck {
                    transfer_id: offer.id,
                },
            )
            .await?;
            if auto_receive {
                let app_handle = app_handle.clone();
                let transfer_id = transfer.id.clone();
                tokio::spawn(async move {
                    tokio::task::yield_now().await;
                    let _ = respond_lan_transfer(
                        app_handle,
                        RespondLanTransferRequest {
                            transfer_id,
                            action: "accept".to_string(),
                            destination_path: None,
                        },
                    )
                    .await;
                });
            }
            Ok(())
        }
        TransferControlEnvelope::TransferDecision {
            transfer_id,
            receiver_id,
            accepted,
        } => {
            if accepted {
                return Err("接收文件应通过传输请求启动".to_string());
            }
            validate_peer_ip(&receiver_id, &remote_ip)?;
            let conn = open_database(&db_path)?;
            let transfer = load_transfer(&conn, &transfer_id)?;
            if transfer.direction != "outgoing" || transfer.to_id != receiver_id {
                return Err("拒绝请求与文件传输记录不匹配".to_string());
            }
            if transfer.status != "rejected" {
                let rejected = conn
                    .execute(
                        "UPDATE lan_transfers SET status='rejected',error=NULL,updated_at=?2
                         WHERE id=?1 AND direction='outgoing' AND status IN ('waiting','failed','cancelled')",
                        params![transfer_id, now_millis() as i64],
                    )
                    .map_err(|error| error.to_string())?;
                if rejected == 0 {
                    return Err("该文件已经开始传输，不能再拒绝".to_string());
                }
                emit_changed(&app_handle, &load_transfer(&conn, &transfer_id)?);
            }
            write_envelope(
                writer,
                &TransferControlEnvelope::TransferDecisionAck { transfer_id },
            )
            .await
        }
        TransferControlEnvelope::TransferRequest {
            transfer_id,
            receiver_id,
        } => {
            serve_transfer(
                &transfer_id,
                &receiver_id,
                &remote_ip,
                reader,
                writer,
                &app_handle,
                &db_path,
            )
            .await
        }
        TransferControlEnvelope::TransferRangeRequest {
            transfer_id,
            receiver_id,
            offset,
            length,
        } => {
            serve_transfer_range(
                &transfer_id,
                &receiver_id,
                &remote_ip,
                offset,
                length,
                writer,
                &app_handle,
                &db_path,
            )
            .await
        }
        TransferControlEnvelope::TransferParallelReceipt {
            transfer_id,
            receiver_id,
            status,
            error,
        } => {
            complete_parallel_transfer(
                &transfer_id,
                &receiver_id,
                &remote_ip,
                &status,
                error.as_deref(),
                writer,
                &app_handle,
                &db_path,
            )
            .await
        }
        TransferControlEnvelope::TransferCancel {
            transfer_id,
            requester_id,
        } => {
            validate_peer_ip(&requester_id, &remote_ip)?;
            let conn = open_database(&db_path)?;
            let transfer = load_transfer(&conn, &transfer_id)?;
            let requester_matches = if transfer.direction == "outgoing" {
                transfer.to_id == requester_id
            } else {
                transfer.provider_id == requester_id
            };
            if !requester_matches {
                return Err("中断请求与文件传输记录不匹配".to_string());
            }
            let cancelled = mark_transfer_cancelled(&conn, &transfer_id)?;
            signal_active_transfer_cancellation(&transfer_id);
            emit_changed(&app_handle, &cancelled);
            write_envelope(
                writer,
                &TransferControlEnvelope::TransferCancelAck { transfer_id },
            )
            .await
        }
        _ => Err("不支持的文件传输控制消息".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pm-center-transfer-{name}-{}.db",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn parallel_ranges_cover_file_without_gaps_or_overlap() {
        let total_bytes = EIGHT_STREAM_MIN_BYTES + 3;
        let ranges = parallel_transfer_ranges(total_bytes);
        assert_eq!(ranges.len(), 8);
        let mut expected_offset = 0u64;
        for (offset, length) in ranges {
            assert_eq!(offset, expected_offset);
            assert!(length > 0);
            expected_offset += length;
        }
        assert_eq!(expected_offset, total_bytes);
    }

    #[test]
    fn parallel_stream_count_uses_size_tiers() {
        assert_eq!(parallel_transfer_stream_count(TWO_STREAM_MIN_BYTES), 1);
        assert_eq!(parallel_transfer_stream_count(TWO_STREAM_MIN_BYTES + 1), 2);
        assert_eq!(parallel_transfer_stream_count(FOUR_STREAM_MIN_BYTES), 2);
        assert_eq!(parallel_transfer_stream_count(FOUR_STREAM_MIN_BYTES + 1), 4);
        assert_eq!(parallel_transfer_stream_count(EIGHT_STREAM_MIN_BYTES), 4);
        assert_eq!(
            parallel_transfer_stream_count(EIGHT_STREAM_MIN_BYTES + 1),
            8
        );
    }

    #[test]
    fn active_transfer_registrations_share_cancellation_and_progress() {
        let transfer_id = format!("parallel-test-{}", uuid::Uuid::new_v4());
        let (first_guard, first_cancellation, first_progress, first_started_at) =
            register_active_transfer(&transfer_id).unwrap();
        let (second_guard, second_cancellation, second_progress, second_started_at) =
            register_active_transfer(&transfer_id).unwrap();
        assert!(Arc::ptr_eq(&first_progress, &second_progress));
        assert_eq!(first_started_at, second_started_at);
        assert!(!*first_cancellation.borrow());
        assert!(!*second_cancellation.borrow());
        assert!(signal_active_transfer_cancellation(&transfer_id));
        assert!(*first_cancellation.borrow());
        assert!(*second_cancellation.borrow());
        drop(first_guard);
        assert!(ACTIVE_TRANSFER_CANCELLATIONS
            .lock()
            .unwrap()
            .contains_key(&transfer_id));
        drop(second_guard);
        assert!(!ACTIVE_TRANSFER_CANCELLATIONS
            .lock()
            .unwrap()
            .contains_key(&transfer_id));
    }

    #[test]
    fn transfer_schema_and_recovery_are_idempotent() {
        let path = temp_db("schema");
        let conn = Connection::open(&path).unwrap();
        initialize_schema(&conn).unwrap();
        initialize_schema(&conn).unwrap();
        let transfer = LanTransfer {
            id: "transfer-1".to_string(),
            lobby_item_id: None,
            conversation_id: "direct:peer".to_string(),
            kind: "file".to_string(),
            from_id: "self".to_string(),
            from_name: "Self".to_string(),
            provider_id: "self".to_string(),
            provider_name: "Self".to_string(),
            to_id: "peer".to_string(),
            display_name: "test.bin".to_string(),
            item_count: 1,
            total_bytes: 4,
            mime_type: None,
            content_hash: "hash".to_string(),
            payload_format: "raw".to_string(),
            manifest: json!({ "version": 1 }),
            status: "transferring".to_string(),
            direction: "outgoing".to_string(),
            source_path: Some("test.bin".to_string()),
            received_path: None,
            error: None,
            created_at: 1,
            updated_at: 1,
        };
        insert_transfer(&conn, &transfer).unwrap();
        recover_interrupted(&conn).unwrap();
        assert_eq!(load_transfer(&conn, "transfer-1").unwrap().status, "failed");
        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn transfer_file_names_cannot_escape_destination() {
        assert!(validate_file_name("image.png").is_ok());
        assert!(validate_file_name("../image.png").is_err());
        assert!(validate_file_name("folder\\image.png").is_err());
    }

    #[test]
    fn directory_manifest_supports_nested_files_and_empty_directories() {
        let directory =
            std::env::temp_dir().join(format!("pm-center-transfer-tree-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(directory.join("empty")).unwrap();
        fs::create_dir_all(directory.join("nested")).unwrap();
        fs::write(directory.join("nested").join("hello.txt"), b"hello").unwrap();

        let inspection = inspect_directory_source(fs::canonicalize(&directory).unwrap()).unwrap();
        assert_eq!(inspection.kind, "directory");
        assert_eq!(inspection.item_count, 3);
        assert_eq!(inspection.total_bytes, 5);
        let manifest = parse_and_validate_manifest(
            &inspection.kind,
            &inspection.payload_format,
            inspection.item_count,
            inspection.total_bytes,
            &inspection.content_hash,
            &inspection.manifest,
        )
        .unwrap();
        assert!(manifest
            .items
            .iter()
            .any(|item| item.path == "empty" && item.kind == "directory"));
        assert!(manifest
            .items
            .iter()
            .any(|item| item.path == "nested/hello.txt" && item.kind == "file"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn directory_manifest_rejects_unsafe_or_conflicting_paths() {
        fn validate(items: Vec<TransferManifestItem>, total_bytes: u64) -> Result<(), String> {
            let manifest = TransferManifest {
                version: 1,
                kind: "directory".to_string(),
                payload_format: "tree-v1".to_string(),
                items,
            };
            let hash = manifest_hash(&manifest)?;
            let value = serde_json::to_value(&manifest).map_err(|error| error.to_string())?;
            parse_and_validate_manifest(
                "directory",
                "tree-v1",
                manifest.items.len() as u64,
                total_bytes,
                &hash,
                &value,
            )
            .map(|_| ())
        }

        let file = |path: &str| TransferManifestItem {
            path: path.to_string(),
            kind: "file".to_string(),
            size_bytes: 1,
            mime_type: None,
            content_hash: Some("a".repeat(64)),
            modified_at: 1,
        };
        assert!(validate(vec![file("../outside.txt")], 1).is_err());
        assert!(validate(vec![file("Same.txt"), file("same.txt")], 2).is_err());
        assert!(validate(vec![file("folder"), file("folder/nested.txt")], 2).is_err());
    }

    #[test]
    fn legacy_file_manifest_without_item_kind_is_still_valid() {
        let content_hash = "a".repeat(64);
        let manifest = json!({
            "version": 1,
            "kind": "file",
            "payloadFormat": "raw",
            "items": [{
                "path": "legacy.bin",
                "sizeBytes": 4,
                "mimeType": null,
                "contentHash": content_hash,
                "modifiedAt": 1,
            }],
        });
        assert!(
            parse_and_validate_manifest("file", "raw", 1, 4, &"a".repeat(64), &manifest,).is_ok()
        );
        assert_eq!(transfer_min_protocol_version("file", None), 3);
        assert_eq!(transfer_min_protocol_version("directory", None), 4);
        assert_eq!(transfer_min_protocol_version("file", Some("lobby")), 6);
    }

    #[test]
    fn temporary_directory_cleanup_removes_partial_tree() {
        let directory = std::env::temp_dir().join(format!(
            "pm-center-transfer-partial-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(directory.join("nested")).unwrap();
        fs::write(directory.join("nested").join("partial.bin"), b"partial").unwrap();
        {
            let _cleanup = TemporaryTransferDirectory(directory.clone());
        }
        assert!(!directory.exists());
    }

    #[test]
    fn lobby_transfer_only_accepts_images_from_current_protocol() {
        let mut offer = WireTransferOffer {
            id: "lobby-image".to_string(),
            lobby_item_id: Some("lobby-item".to_string()),
            kind: "file".to_string(),
            from_id: "sender".to_string(),
            from_name: "Sender".to_string(),
            provider_id: "sender".to_string(),
            provider_name: "Sender".to_string(),
            to_id: "receiver".to_string(),
            conversation_id: Some("lobby".to_string()),
            display_name: "image.png".to_string(),
            item_count: 1,
            total_bytes: 4,
            mime_type: Some("image/png".to_string()),
            content_hash: "a".repeat(64),
            payload_format: "raw".to_string(),
            manifest: json!({}),
            created_at: 1,
        };
        assert_eq!(resolve_offer_conversation(&offer, 6).unwrap(), "lobby");
        assert!(resolve_offer_conversation(&offer, 5).is_err());
        offer.mime_type = Some("application/octet-stream".to_string());
        assert!(resolve_offer_conversation(&offer, 6).is_err());
        offer.kind = "directory".to_string();
        assert!(resolve_offer_conversation(&offer, 6).is_err());
    }

    #[test]
    fn incoming_transfer_can_only_be_claimed_or_rejected_once() {
        let path = temp_db("claim");
        let conn = Connection::open(&path).unwrap();
        initialize_schema(&conn).unwrap();

        let mut transfer = LanTransfer {
            id: "incoming-claim".to_string(),
            lobby_item_id: None,
            conversation_id: "direct:sender".to_string(),
            kind: "file".to_string(),
            from_id: "sender".to_string(),
            from_name: "Sender".to_string(),
            provider_id: "sender".to_string(),
            provider_name: "Sender".to_string(),
            to_id: "receiver".to_string(),
            display_name: "test.bin".to_string(),
            item_count: 1,
            total_bytes: 4,
            mime_type: None,
            content_hash: "a".repeat(64),
            payload_format: "raw".to_string(),
            manifest: json!({ "version": 1 }),
            status: "pending".to_string(),
            direction: "incoming".to_string(),
            source_path: None,
            received_path: None,
            error: None,
            created_at: 1,
            updated_at: 1,
        };
        insert_transfer(&conn, &transfer).unwrap();
        assert_eq!(
            claim_incoming_transfer(&conn, &transfer.id, Path::new("received.bin"))
                .unwrap()
                .status,
            "transferring"
        );
        assert!(claim_incoming_transfer(&conn, &transfer.id, Path::new("received.bin")).is_err());
        assert!(reject_incoming_transfer(&conn, &transfer.id).is_err());

        transfer.id = "incoming-reject".to_string();
        transfer.status = "pending".to_string();
        insert_transfer(&conn, &transfer).unwrap();
        assert_eq!(
            reject_incoming_transfer(&conn, &transfer.id)
                .unwrap()
                .status,
            "rejected"
        );
        assert!(claim_incoming_transfer(&conn, &transfer.id, Path::new("received.bin")).is_err());
        assert!(reject_incoming_transfer(&conn, &transfer.id).is_err());

        transfer.id = "incoming-cancel".to_string();
        transfer.status = "transferring".to_string();
        insert_transfer(&conn, &transfer).unwrap();
        assert_eq!(
            mark_transfer_cancelled(&conn, &transfer.id).unwrap().status,
            "cancelled"
        );
        assert_eq!(
            claim_incoming_transfer(&conn, &transfer.id, Path::new("received.bin"))
                .unwrap()
                .status,
            "transferring"
        );

        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn active_transfer_cancellation_signal_reaches_the_stream_worker() {
        let transfer_id = format!("cancel-signal-{}", uuid::Uuid::new_v4());
        let (_guard, receiver, _, _) = register_active_transfer(&transfer_id).unwrap();
        assert!(!*receiver.borrow());
        assert!(signal_active_transfer_cancellation(&transfer_id));
        assert!(*receiver.borrow());
    }

    #[test]
    fn lobby_images_are_deduplicated_across_multiple_providers() {
        let path = temp_db("lobby-dedup");
        let conn = Connection::open(&path).unwrap();
        initialize_schema(&conn).unwrap();
        let mut transfer = LanTransfer {
            id: "offer-a".to_string(),
            lobby_item_id: Some("shared-lobby-item".to_string()),
            conversation_id: "lobby".to_string(),
            kind: "file".to_string(),
            from_id: "author".to_string(),
            from_name: "Author".to_string(),
            provider_id: "provider-a".to_string(),
            provider_name: "Provider A".to_string(),
            to_id: "receiver".to_string(),
            display_name: "image.png".to_string(),
            item_count: 1,
            total_bytes: 4,
            mime_type: Some("image/png".to_string()),
            content_hash: "a".repeat(64),
            payload_format: "raw".to_string(),
            manifest: json!({ "version": 1 }),
            status: "pending".to_string(),
            direction: "incoming".to_string(),
            source_path: None,
            received_path: None,
            error: None,
            created_at: 1,
            updated_at: 1,
        };
        assert!(insert_transfer(&conn, &transfer).unwrap());
        transfer.id = "offer-b".to_string();
        transfer.provider_id = "provider-b".to_string();
        transfer.provider_name = "Provider B".to_string();
        assert!(!insert_transfer(&conn, &transfer).unwrap());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM lan_transfers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn sender_names_are_safe_directory_names() {
        assert_eq!(sanitize_sender_directory(" Alice ", "peer"), "Alice");
        assert_eq!(sanitize_sender_directory("A/B:C*", "peer"), "A_B_C_");
        assert_eq!(sanitize_sender_directory("CON", "peer"), "_CON");
        assert_eq!(
            sanitize_sender_directory("...", "peer-123"),
            "联系人-peer-123"
        );
    }

    #[tokio::test]
    async fn automatic_images_must_have_a_decodable_header() {
        let directory =
            std::env::temp_dir().join(format!("pm-center-transfer-image-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let valid_path = directory.join("valid.png");
        image::RgbImage::new(2, 2).save(&valid_path).unwrap();
        validate_received_image(&valid_path).await.unwrap();

        let invalid_path = directory.join("invalid.png");
        fs::write(&invalid_path, b"not an image").unwrap();
        assert!(validate_received_image(&invalid_path).await.is_err());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn transfer_offer_contains_manifest_but_never_local_source_path() {
        let envelope = TransferControlEnvelope::TransferOffer {
            protocol_version: PROTOCOL_VERSION,
            transfer_protocol_version: TRANSFER_PROTOCOL_VERSION,
            offer: WireTransferOffer {
                id: "transfer-1".to_string(),
                lobby_item_id: None,
                kind: "file".to_string(),
                from_id: "sender".to_string(),
                from_name: "Sender".to_string(),
                provider_id: "sender".to_string(),
                provider_name: "Sender".to_string(),
                to_id: "receiver".to_string(),
                conversation_id: None,
                display_name: "image.png".to_string(),
                item_count: 1,
                total_bytes: 4,
                mime_type: Some("image/png".to_string()),
                content_hash: "a".repeat(64),
                payload_format: "raw".to_string(),
                manifest: json!({ "version": 1, "items": [{ "path": "image.png" }] }),
                created_at: 1,
            },
        };
        let serialized = serde_json::to_string(&envelope).unwrap();
        assert!(serialized.contains("\"type\":\"transfer-offer\""));
        assert!(serialized.contains("\"manifest\""));
        assert!(!serialized.contains("sourcePath"));
        assert!(!serialized.contains("C:\\\\"));
    }
}
