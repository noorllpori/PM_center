use super::{
    connect_to_peer, emit_contacts_changed, now_millis, open_database, set_online_legacy_peer,
    state_paths, upsert_legacy_contact, OnlinePeer, P2P_GLOBAL, PROTOCOL_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

const TRANSFER_PROTOCOL_VERSION: u16 = 1;
const TRANSFER_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const TRANSFER_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const TRANSFER_IO_TIMEOUT: Duration = Duration::from_secs(60);
const TRANSFER_CHUNK_SIZE: usize = 1024 * 1024;
const MAX_TRANSFER_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanTransfer {
    pub id: String,
    pub conversation_id: String,
    pub kind: String,
    pub from_id: String,
    pub from_name: String,
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
    pub to_id: String,
    pub paths: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WireTransferOffer {
    id: String,
    kind: String,
    from_id: String,
    from_name: String,
    to_id: String,
    display_name: String,
    item_count: u64,
    total_bytes: u64,
    mime_type: Option<String>,
    content_hash: String,
    payload_format: String,
    manifest: serde_json::Value,
    created_at: u64,
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
    TransferReceipt {
        transfer_id: String,
        status: String,
        error: Option<String>,
    },
    TransferError {
        transfer_id: String,
        error: String,
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
    let manifest_json: String = row.get(12)?;
    Ok(LanTransfer {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        kind: row.get(2)?,
        from_id: row.get(3)?,
        from_name: row.get(4)?,
        to_id: row.get(5)?,
        display_name: row.get(6)?,
        item_count: row.get::<_, i64>(7)? as u64,
        total_bytes: row.get::<_, i64>(8)? as u64,
        mime_type: row.get(9)?,
        content_hash: row.get(10)?,
        payload_format: row.get(11)?,
        manifest: serde_json::from_str(&manifest_json).unwrap_or_else(|_| json!({})),
        status: row.get(13)?,
        direction: row.get(14)?,
        source_path: row.get(15)?,
        received_path: row.get(16)?,
        error: row.get(17)?,
        created_at: row.get::<_, i64>(18)? as u64,
        updated_at: row.get::<_, i64>(19)? as u64,
    })
}

const TRANSFER_COLUMNS: &str =
    "id,conversation_id,kind,from_id,from_name,to_id,display_name,item_count,total_bytes,mime_type,content_hash,payload_format,manifest_json,status,direction,source_path,received_path,error,created_at,updated_at";

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
    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO lan_transfers(
               id,conversation_id,kind,from_id,from_name,to_id,display_name,item_count,total_bytes,mime_type,content_hash,payload_format,manifest_json,status,direction,source_path,received_path,error,created_at,updated_at
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
            params![
                transfer.id,
                transfer.conversation_id,
                transfer.kind,
                transfer.from_id,
                transfer.from_name,
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

fn claim_incoming_transfer(conn: &Connection, transfer_id: &str) -> Result<LanTransfer, String> {
    let claimed = conn
        .execute(
            "UPDATE lan_transfers
             SET status='transferring',error=NULL,updated_at=?2
             WHERE id=?1 AND direction='incoming' AND status IN ('pending','failed')",
            params![transfer_id, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    if claimed == 0 {
        return Err("当前传输已被其他窗口处理或状态已变化".to_string());
    }
    load_transfer(conn, transfer_id)
}

fn reject_incoming_transfer(conn: &Connection, transfer_id: &str) -> Result<LanTransfer, String> {
    let rejected = conn
        .execute(
            "UPDATE lan_transfers
             SET status='rejected',error=NULL,updated_at=?2
             WHERE id=?1 AND direction='incoming' AND status IN ('pending','failed')",
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

struct SourceInspection {
    path: PathBuf,
    file_name: String,
    size_bytes: u64,
    mime_type: Option<String>,
    content_hash: String,
    modified_at: u64,
}

fn system_time_millis(value: SystemTime) -> u64 {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn inspect_source(path: PathBuf) -> Result<SourceInspection, String> {
    tokio::task::spawn_blocking(move || {
        let canonical =
            fs::canonicalize(&path).map_err(|error| format!("无法读取待发送文件：{error}"))?;
        let metadata = fs::metadata(&canonical).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("当前只支持发送文件，目录同步将在后续功能中提供".to_string());
        }
        if metadata.len() > MAX_TRANSFER_BYTES {
            return Err("单个传输内容不能超过 1 TB".to_string());
        }
        let file_name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "文件名无法转换为 UTF-8".to_string())?;
        let file_name = validate_file_name(file_name)?;
        let mime_type = mime_guess::from_path(&canonical)
            .first_raw()
            .map(str::to_string);
        let content_hash = hash_file(&canonical)?;
        let modified_at = metadata
            .modified()
            .map(system_time_millis)
            .unwrap_or_default();
        Ok(SourceInspection {
            path: canonical,
            file_name,
            size_bytes: metadata.len(),
            mime_type,
            content_hash,
            modified_at,
        })
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
    if line.is_empty() || line.len() > 160 * 1024 {
        return Err("收到无效传输响应".to_string());
    }
    serde_json::from_str(line.trim()).map_err(|error| format!("无法解析传输响应：{error}"))
}

fn transfer_target(contact_id: &str) -> Result<OnlinePeer, String> {
    let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
    let peer = state
        .online_users
        .get(contact_id)
        .cloned()
        .ok_or_else(|| "联系人当前离线，无法传输文件".to_string())?;
    if peer.protocol_version < PROTOCOL_VERSION {
        return Err("对方客户端版本不支持确认式文件传输，请先升级到 2.7.0 或更高版本".to_string());
    }
    Ok(peer)
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
            transfer_protocol_version: TRANSFER_PROTOCOL_VERSION,
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

#[tauri::command]
pub async fn offer_lan_files(
    app_handle: tauri::AppHandle,
    request: OfferLanFilesRequest,
) -> Result<LanFileOfferResult, String> {
    if request.paths.is_empty() {
        return Err("请选择至少一个文件".to_string());
    }
    if request.paths.len() > 100 {
        return Err("一次最多发送 100 个文件".to_string());
    }
    let peer = transfer_target(&request.to_id)?;
    let profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let db_path = state_paths()?.0;
    let mut transfers = Vec::new();
    let mut failures = Vec::new();

    for source_path in request.paths {
        let inspection = match inspect_source(PathBuf::from(&source_path)).await {
            Ok(value) => value,
            Err(error) => {
                failures.push(LanTransferFailure {
                    path: source_path,
                    error,
                });
                continue;
            }
        };
        let transfer_id = uuid::Uuid::new_v4().to_string();
        let created_at = now_millis();
        let manifest = json!({
            "version": 1,
            "kind": "file",
            "payloadFormat": "raw",
            "items": [{
                "path": inspection.file_name,
                "sizeBytes": inspection.size_bytes,
                "mimeType": inspection.mime_type,
                "contentHash": inspection.content_hash,
                "modifiedAt": inspection.modified_at,
            }],
        });
        let offer = WireTransferOffer {
            id: transfer_id.clone(),
            kind: "file".to_string(),
            from_id: profile.id.clone(),
            from_name: profile.display_name.clone(),
            to_id: request.to_id.clone(),
            display_name: inspection.file_name.clone(),
            item_count: 1,
            total_bytes: inspection.size_bytes,
            mime_type: inspection.mime_type.clone(),
            content_hash: inspection.content_hash.clone(),
            payload_format: "raw".to_string(),
            manifest: manifest.clone(),
            created_at,
        };
        let transfer = LanTransfer {
            id: transfer_id.clone(),
            conversation_id: format!("direct:{}", request.to_id),
            kind: "file".to_string(),
            from_id: profile.id.clone(),
            from_name: profile.display_name.clone(),
            to_id: request.to_id.clone(),
            display_name: inspection.file_name,
            item_count: 1,
            total_bytes: inspection.size_bytes,
            mime_type: inspection.mime_type,
            content_hash: inspection.content_hash,
            payload_format: "raw".to_string(),
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
                path: source_path,
                error,
            });
            continue;
        }
        emit_changed(&app_handle, &transfer);
        transfers.push(transfer);
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

async fn verify_source_unchanged(transfer: &LanTransfer) -> Result<PathBuf, String> {
    let source_path = transfer
        .source_path
        .as_deref()
        .ok_or_else(|| "发送源文件路径丢失".to_string())?;
    let expected_modified_at = transfer
        .manifest
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("modifiedAt"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default();
    let source_path = PathBuf::from(source_path);
    let expected_size = transfer.total_bytes;
    tokio::task::spawn_blocking(move || {
        let canonical = fs::canonicalize(source_path)
            .map_err(|error| format!("无法读取发送源文件：{error}"))?;
        let metadata = fs::metadata(&canonical).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() != expected_size {
            return Err("发送源文件大小已发生变化，请重新发送".to_string());
        }
        let modified_at = metadata
            .modified()
            .map(system_time_millis)
            .unwrap_or_default();
        if expected_modified_at > 0 && modified_at != expected_modified_at {
            return Err("发送源文件已发生变化，请重新发送".to_string());
        }
        Ok(canonical)
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn download_transfer(
    app_handle: &tauri::AppHandle,
    transfer: &LanTransfer,
    peer: &OnlinePeer,
    destination_path: &Path,
) -> Result<(), String> {
    if destination_path.file_name().is_none() {
        return Err("请选择有效的文件保存位置".to_string());
    }
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
    let _ = tokio::fs::remove_file(&temp_path).await;
    let _temp_cleanup = TemporaryTransferFile(temp_path.clone());

    let stream = connect_to_peer(&peer.ip).await?;
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
        _ => return Err("发送方返回了无效文件传输响应".to_string()),
    };
    if total_bytes != transfer.total_bytes || content_hash != transfer.content_hash {
        return Err("发送方文件已发生变化，请重新发送".to_string());
    }

    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|error| format!("创建临时接收文件失败：{error}"))?;
    let mut buffer = vec![0u8; TRANSFER_CHUNK_SIZE];
    let mut received = 0u64;
    let mut hasher = blake3::Hasher::new();
    let started_at = Instant::now();
    let mut last_progress = Instant::now();
    while received < total_bytes {
        let remaining = (total_bytes - received).min(buffer.len() as u64) as usize;
        let count =
            tokio::time::timeout(TRANSFER_IO_TIMEOUT, reader.read(&mut buffer[..remaining]))
                .await
                .map_err(|_| "接收文件数据超时".to_string())?
                .map_err(|error| error.to_string())?;
        if count == 0 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err("发送方提前结束了文件传输".to_string());
        }
        file.write_all(&buffer[..count])
            .await
            .map_err(|error| error.to_string())?;
        hasher.update(&buffer[..count]);
        received += count as u64;
        if last_progress.elapsed() >= Duration::from_millis(150) || received == total_bytes {
            emit_progress(
                app_handle,
                &transfer.id,
                "transferring",
                "incoming",
                received,
                total_bytes,
                started_at,
            );
            last_progress = Instant::now();
        }
    }
    file.flush().await.map_err(|error| error.to_string())?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    drop(file);
    let actual_hash = hasher.finalize().to_hex().to_string();
    if actual_hash != content_hash {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err("文件完整性校验失败，临时文件已删除".to_string());
    }
    atomic_replace_file(&temp_path, destination_path)?;
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
            let notification_error = match transfer_target(&transfer.from_id) {
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
            let destination = request
                .destination_path
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "请选择文件保存位置".to_string())?;
            let peer = transfer_target(&transfer.from_id)?;
            let transferring = claim_incoming_transfer(&conn, &transfer.id)?;
            emit_changed(&app_handle, &transferring);
            match download_transfer(&app_handle, &transfer, &peer, Path::new(&destination)).await {
                Ok(()) => {
                    let completed = update_transfer(
                        &open_database(&db_path)?,
                        &transfer.id,
                        "completed",
                        None,
                        Some(&destination),
                    )?;
                    emit_changed(&app_handle, &completed);
                    Ok(completed)
                }
                Err(error) => {
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

fn staging_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_data_dir()
        .map(|path| path.join("lan_collaboration").join("staging"))
        .map_err(|error| error.to_string())
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

fn transfer_from_offer(offer: &WireTransferOffer) -> LanTransfer {
    let received_at = now_millis();
    LanTransfer {
        id: offer.id.clone(),
        conversation_id: format!("direct:{}", offer.from_id),
        kind: offer.kind.clone(),
        from_id: offer.from_id.clone(),
        from_name: offer.from_name.clone(),
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
        created_at: received_at,
        updated_at: received_at,
    }
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
    let claimed = conn
        .execute(
            "UPDATE lan_transfers SET status='transferring',error=NULL,updated_at=?2
             WHERE id=?1 AND direction='outgoing' AND status IN ('waiting','failed')",
            params![transfer_id, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    if claimed == 0 {
        return Err("该文件当前不能重复传输".to_string());
    }
    let transferring = load_transfer(&conn, transfer_id)?;
    emit_changed(app_handle, &transferring);

    let result = async {
        let source_path = verify_source_unchanged(&transfer).await?;
        write_envelope(
            writer,
            &TransferControlEnvelope::TransferReady {
                transfer_id: transfer.id.clone(),
                total_bytes: transfer.total_bytes,
                content_hash: transfer.content_hash.clone(),
            },
        )
        .await?;

        let mut file = tokio::fs::File::open(&source_path)
            .await
            .map_err(|error| error.to_string())?;
        let mut buffer = vec![0u8; TRANSFER_CHUNK_SIZE];
        let mut sent = 0u64;
        let started_at = Instant::now();
        let mut last_progress = Instant::now();
        loop {
            let count = file
                .read(&mut buffer)
                .await
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            tokio::time::timeout(TRANSFER_IO_TIMEOUT, writer.write_all(&buffer[..count]))
                .await
                .map_err(|_| "发送文件数据超时".to_string())?
                .map_err(|error| error.to_string())?;
            sent += count as u64;
            if last_progress.elapsed() >= Duration::from_millis(150) || sent == transfer.total_bytes
            {
                emit_progress(
                    app_handle,
                    &transfer.id,
                    "transferring",
                    "outgoing",
                    sent,
                    transfer.total_bytes,
                    started_at,
                );
                last_progress = Instant::now();
            }
        }
        writer.flush().await.map_err(|error| error.to_string())?;
        match read_envelope(reader, TRANSFER_IO_TIMEOUT).await? {
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
            if protocol_version < PROTOCOL_VERSION
                || transfer_protocol_version != TRANSFER_PROTOCOL_VERSION
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
                || offer.kind != "file"
                || offer.payload_format != "raw"
                || offer.item_count != 1
                || offer.total_bytes > MAX_TRANSFER_BYTES
                || offer.content_hash.len() != 64
                || !offer
                    .content_hash
                    .bytes()
                    .all(|value| value.is_ascii_hexdigit())
            {
                return Err("收到无效的文件传输请求".to_string());
            }
            validate_file_name(&offer.display_name)?;
            let conn = open_database(&db_path)?;
            let mut contact_changed =
                upsert_legacy_contact(&conn, &offer.from_id, &offer.from_name, &remote_ip)?
                    | set_online_legacy_peer(&offer.from_id, &offer.from_name, &remote_ip)?;
            let updated_protocol = conn
                .execute(
                    "UPDATE lan_contacts SET protocol_version=MAX(protocol_version,?2) WHERE id=?1 AND protocol_version<?2",
                    params![offer.from_id, protocol_version as i64],
                )
                .map_err(|error| error.to_string())?;
            contact_changed |= updated_protocol > 0;
            if let Ok(mut state) = P2P_GLOBAL.lock() {
                if let Some(peer) = state.online_users.get_mut(&offer.from_id) {
                    peer.protocol_version = peer.protocol_version.max(protocol_version);
                }
            }
            if contact_changed {
                emit_contacts_changed(&app_handle);
            }
            let transfer = transfer_from_offer(&offer);
            if insert_transfer(&conn, &transfer)? {
                emit_changed(&app_handle, &transfer);
            }
            write_envelope(
                writer,
                &TransferControlEnvelope::TransferOfferAck {
                    transfer_id: offer.id,
                },
            )
            .await
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
                         WHERE id=?1 AND direction='outgoing' AND status IN ('waiting','failed')",
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
    fn transfer_schema_and_recovery_are_idempotent() {
        let path = temp_db("schema");
        let conn = Connection::open(&path).unwrap();
        initialize_schema(&conn).unwrap();
        initialize_schema(&conn).unwrap();
        let transfer = LanTransfer {
            id: "transfer-1".to_string(),
            conversation_id: "direct:peer".to_string(),
            kind: "file".to_string(),
            from_id: "self".to_string(),
            from_name: "Self".to_string(),
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
    fn incoming_transfer_can_only_be_claimed_or_rejected_once() {
        let path = temp_db("claim");
        let conn = Connection::open(&path).unwrap();
        initialize_schema(&conn).unwrap();

        let mut transfer = LanTransfer {
            id: "incoming-claim".to_string(),
            conversation_id: "direct:sender".to_string(),
            kind: "file".to_string(),
            from_id: "sender".to_string(),
            from_name: "Sender".to_string(),
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
            claim_incoming_transfer(&conn, &transfer.id).unwrap().status,
            "transferring"
        );
        assert!(claim_incoming_transfer(&conn, &transfer.id).is_err());
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
        assert!(claim_incoming_transfer(&conn, &transfer.id).is_err());
        assert!(reject_incoming_transfer(&conn, &transfer.id).is_err());

        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn transfer_offer_contains_manifest_but_never_local_source_path() {
        let envelope = TransferControlEnvelope::TransferOffer {
            protocol_version: PROTOCOL_VERSION,
            transfer_protocol_version: TRANSFER_PROTOCOL_VERSION,
            offer: WireTransferOffer {
                id: "transfer-1".to_string(),
                kind: "file".to_string(),
                from_id: "sender".to_string(),
                from_name: "Sender".to_string(),
                to_id: "receiver".to_string(),
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
