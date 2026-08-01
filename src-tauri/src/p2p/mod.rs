use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use if_addrs::{get_if_addrs, IfAddr};
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, GenericImageView};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

mod transfer;
pub use transfer::{
    cancel_lan_transfer, create_lan_transfer_staging_path, discard_lan_transfer_staging_file,
    offer_lan_files, respond_lan_transfer, LanTransfer,
};

const DISCOVERY_PORT: u16 = 31523;
const MESSAGE_PORT: u16 = 31524;
const PROTOCOL_VERSION: u16 = 7;
const LOBBY_SYNC_MIN_PROTOCOL_VERSION: u16 = 6;
const BROADCAST_INTERVAL: Duration = Duration::from_secs(4);
const USER_TIMEOUT: Duration = Duration::from_secs(16);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const DELIVERY_ACK_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_CONTROL_BYTES: usize = 8 * 1024 * 1024;
const MAX_AVATAR_SOURCE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_AVATAR_BYTES: usize = 64 * 1024;
const MESSAGE_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const LOBBY_HISTORY_LIMIT: usize = 30;
const LOBBY_HISTORY_SYNC_INTERVAL_MS: u64 = 30 * 1000;
const MIN_DISCOVERY_SUBNET_PREFIX: u8 = 24;
const MAX_DISCOVERY_SUBNET_PREFIX: u8 = 30;

fn notification_preview(content: &str) -> String {
    const MAX_CHARS: usize = 120;
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= MAX_CHARS {
        return normalized;
    }
    format!(
        "{}...",
        normalized.chars().take(MAX_CHARS - 3).collect::<String>()
    )
}

fn open_lan_notification_target(
    app_handle: &tauri::AppHandle,
    conversation_id: &str,
    message_id: Option<&str>,
    transfer_id: Option<&str>,
) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let _ = app_handle.emit(
        "pm-center:open-lan-conversation",
        serde_json::json!({
            "conversationId": conversation_id,
            "messageId": message_id,
            "transferId": transfer_id,
        }),
    );
}

fn show_lan_notification(
    app_handle: &tauri::AppHandle,
    title: String,
    body: String,
    conversation_id: String,
    message_id: Option<String>,
    transfer_id: Option<String>,
) {
    #[cfg(target_os = "windows")]
    {
        let app_handle = app_handle.clone();
        let app_identifier = app_handle.config().identifier.clone();
        std::thread::spawn(move || {
            let mut notification = notify_rust::Notification::new();
            notification
                .summary(&title)
                .body(&body)
                .sound_name("IM")
                .app_id(&app_identifier);
            match notification.show() {
                Ok(handle) => {
                    let result = handle.wait_for_response(
                        move |response: &notify_rust::NotificationResponse| {
                            if matches!(
                                response,
                                notify_rust::NotificationResponse::Default
                                    | notify_rust::NotificationResponse::Action(_)
                            ) {
                                open_lan_notification_target(
                                    &app_handle,
                                    &conversation_id,
                                    message_id.as_deref(),
                                    transfer_id.as_deref(),
                                );
                            }
                        },
                    );
                    if let Err(error) = result {
                        eprintln!("[P2P] 监听系统消息通知点击失败：{error}");
                    }
                }
                Err(error) => eprintln!("[P2P] 显示系统消息通知失败：{error}"),
            }
        });
    }

    #[cfg(not(target_os = "windows"))]
    if let Err(error) = app_handle
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        eprintln!("[P2P] 显示系统消息通知失败：{error}");
    }
}

fn notify_incoming_message(app_handle: &tauri::AppHandle, message: &LanMessage) {
    let title = if message.to_id.is_some() {
        format!("{} · 局域网私聊", message.from_name)
    } else {
        format!("局域网大厅 · {}", message.from_name)
    };
    show_lan_notification(
        app_handle,
        title,
        notification_preview(&message.content),
        message.conversation_id.clone(),
        Some(message.id.clone()),
        None,
    );
}

fn notify_incoming_transfer(
    app_handle: &tauri::AppHandle,
    transfer: &LanTransfer,
    auto_receiving: bool,
) {
    let body = if transfer.conversation_id == "lobby" {
        format!("在大厅发来图片：{}，正在自动接收", transfer.display_name)
    } else if transfer.kind == "directory" {
        format!(
            "发来目录：{}（{} 个项目），等待你接收",
            transfer.display_name, transfer.item_count
        )
    } else if auto_receiving {
        format!("发来图片：{}，正在自动接收", transfer.display_name)
    } else {
        format!("发来文件：{}，等待你接收", transfer.display_name)
    };
    show_lan_notification(
        app_handle,
        format!("{} · 文件传输", transfer.from_name),
        notification_preview(&body),
        transfer.conversation_id.clone(),
        None,
        Some(transfer.id.clone()),
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyLanProfile {
    pub user_id: Option<String>,
    pub user_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanProfile {
    pub id: String,
    pub display_name: String,
    pub department: String,
    pub avatar_hash: Option<String>,
    pub avatar_path: Option<String>,
    pub profile_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanLocalSettings {
    pub receive_directory: String,
    pub auto_receive_images: bool,
    pub discovery_subnet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanContact {
    pub id: String,
    pub display_name: String,
    pub department: String,
    pub avatar_hash: Option<String>,
    pub avatar_path: Option<String>,
    pub ip: String,
    pub online: bool,
    pub first_seen: u64,
    pub last_seen: u64,
    pub protocol_version: u16,
    pub profile_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PUser {
    pub id: String,
    pub name: String,
    pub ip: String,
    #[serde(alias = "lastSeen")]
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PMessage {
    pub id: String,
    #[serde(alias = "fromId")]
    pub from_id: String,
    #[serde(alias = "fromName")]
    pub from_name: String,
    #[serde(alias = "toId")]
    pub to_id: Option<String>,
    pub content: String,
    pub timestamp: u64,
    #[serde(default)]
    pub protocol_version: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanMessage {
    pub id: String,
    pub conversation_id: String,
    pub from_id: String,
    pub from_name: String,
    pub to_id: Option<String>,
    pub content: String,
    pub timestamp: u64,
    pub direction: String,
    pub delivery_status: String,
    pub delivery_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanConversation {
    pub id: String,
    pub kind: String,
    pub contact_id: Option<String>,
    pub title: String,
    pub unread_count: u64,
    pub last_message_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanServiceStatus {
    pub is_running: bool,
    pub udp_bound: bool,
    pub tcp_bound: bool,
    pub local_addresses: Vec<String>,
    pub online_count: usize,
    pub last_discovery_at: Option<u64>,
    pub last_error: Option<String>,
}

impl Default for LanServiceStatus {
    fn default() -> Self {
        Self {
            is_running: false,
            udp_bound: false,
            tcp_bound: false,
            local_addresses: Vec::new(),
            online_count: 0,
            last_discovery_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanSnapshot {
    pub profile: LanProfile,
    pub local_settings: LanLocalSettings,
    pub contacts: Vec<LanContact>,
    pub messages: Vec<LanMessage>,
    pub transfers: Vec<LanTransfer>,
    pub conversations: Vec<LanConversation>,
    pub unread_count: u64,
    pub service: LanServiceStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLanProfileRequest {
    pub display_name: String,
    pub department: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLanReceiveDirectoryRequest {
    pub receive_directory: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLanDiscoverySubnetRequest {
    pub discovery_subnet: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanDiscoverySubnetScanResult {
    pub discovery_subnet: String,
    pub target_count: usize,
    pub sent_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendLanMessageRequest {
    pub to_id: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PDeliveryFailure {
    pub user_id: String,
    pub user_name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PDeliveryResult {
    pub target_count: usize,
    pub delivered_count: usize,
    pub failures: Vec<P2PDeliveryFailure>,
}

pub type LanDeliveryResult = P2PDeliveryResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct P2PDeliveryAck {
    #[serde(rename = "type")]
    kind: String,
    message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryPacketFields {
    user_id: String,
    user_name: String,
    #[serde(default)]
    protocol_version: Option<u16>,
    #[serde(default)]
    department: Option<String>,
    #[serde(default)]
    avatar_hash: Option<String>,
    #[serde(default)]
    profile_revision: Option<u64>,
    #[serde(default)]
    message_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum DiscoveryPacket {
    Announce {
        user_id: String,
        user_name: String,
        #[serde(default)]
        protocol_version: Option<u16>,
        #[serde(default)]
        department: Option<String>,
        #[serde(default)]
        avatar_hash: Option<String>,
        #[serde(default)]
        profile_revision: Option<u64>,
        #[serde(default)]
        message_port: Option<u16>,
    },
    Response {
        user_id: String,
        user_name: String,
        #[serde(default)]
        protocol_version: Option<u16>,
        #[serde(default)]
        department: Option<String>,
        #[serde(default)]
        avatar_hash: Option<String>,
        #[serde(default)]
        profile_revision: Option<u64>,
        #[serde(default)]
        message_port: Option<u16>,
    },
}

impl DiscoveryPacket {
    fn fields(self) -> (DiscoveryPacketFields, bool) {
        match self {
            Self::Announce {
                user_id,
                user_name,
                protocol_version,
                department,
                avatar_hash,
                profile_revision,
                message_port,
            } => (
                DiscoveryPacketFields {
                    user_id,
                    user_name,
                    protocol_version,
                    department,
                    avatar_hash,
                    profile_revision,
                    message_port,
                },
                true,
            ),
            Self::Response {
                user_id,
                user_name,
                protocol_version,
                department,
                avatar_hash,
                profile_revision,
                message_port,
            } => (
                DiscoveryPacketFields {
                    user_id,
                    user_name,
                    protocol_version,
                    department,
                    avatar_hash,
                    profile_revision,
                    message_port,
                },
                false,
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireProfile {
    id: String,
    display_name: String,
    department: String,
    avatar_hash: Option<String>,
    profile_revision: u64,
}

impl From<&LanProfile> for WireProfile {
    fn from(profile: &LanProfile) -> Self {
        Self {
            id: profile.id.clone(),
            display_name: profile.display_name.clone(),
            department: profile.department.clone(),
            avatar_hash: profile.avatar_hash.clone(),
            profile_revision: profile.profile_revision,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum LanControlEnvelope {
    Hello {
        protocol_version: u16,
        profile: WireProfile,
    },
    HelloAck {
        protocol_version: u16,
        profile: WireProfile,
    },
    ProfileRequest {
        requester_id: String,
        known_avatar_hash: Option<String>,
    },
    ProfileResponse {
        profile: WireProfile,
        avatar_base64: Option<String>,
    },
    LobbyHistoryRequest {
        requester_id: String,
        #[serde(default)]
        known_image_ids: Vec<String>,
        limit: u16,
    },
    LobbyHistoryResponse {
        messages: Vec<P2PMessage>,
        image_offers: Vec<transfer::WireTransferOffer>,
    },
}

#[derive(Clone)]
struct OnlinePeer {
    id: String,
    display_name: String,
    department: String,
    avatar_hash: Option<String>,
    ip: String,
    last_seen: u64,
    protocol_version: u16,
    profile_revision: u64,
}

struct P2PStateInner {
    profile: Option<LanProfile>,
    db_path: Option<PathBuf>,
    avatars_dir: Option<PathBuf>,
    online_users: HashMap<String, OnlinePeer>,
    is_running: bool,
    discovery_worker_started: bool,
    tcp_server_started: bool,
    announce_generation: u64,
    last_cleanup_at: u64,
    last_history_sync: HashMap<String, u64>,
    service: LanServiceStatus,
}

type P2PState = Arc<Mutex<P2PStateInner>>;

lazy_static::lazy_static! {
    static ref P2P_GLOBAL: P2PState = Arc::new(Mutex::new(P2PStateInner {
        profile: None,
        db_path: None,
        avatars_dir: None,
        online_users: HashMap::new(),
        is_running: false,
        discovery_worker_started: false,
        tcp_server_started: false,
        announce_generation: 0,
        last_cleanup_at: 0,
        last_history_sync: HashMap::new(),
        service: LanServiceStatus::default(),
    }));
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn open_database(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|error| error.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS lan_profile (
           singleton INTEGER PRIMARY KEY CHECK(singleton=1),
           id TEXT NOT NULL,
           display_name TEXT NOT NULL,
           department TEXT NOT NULL DEFAULT '',
           avatar_hash TEXT,
           avatar_path TEXT,
           profile_revision INTEGER NOT NULL DEFAULT 1,
           updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS lan_contacts (
           id TEXT PRIMARY KEY,
           display_name TEXT NOT NULL,
           department TEXT NOT NULL DEFAULT '',
           avatar_hash TEXT,
           avatar_path TEXT,
           ip TEXT NOT NULL,
           first_seen INTEGER NOT NULL,
           last_seen INTEGER NOT NULL,
           protocol_version INTEGER NOT NULL DEFAULT 1,
           profile_revision INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS lan_messages (
           id TEXT PRIMARY KEY,
           conversation_id TEXT NOT NULL,
           from_id TEXT NOT NULL,
           from_name TEXT NOT NULL,
           to_id TEXT,
           content TEXT NOT NULL,
           timestamp INTEGER NOT NULL,
           direction TEXT NOT NULL,
           delivery_status TEXT NOT NULL,
           delivery_summary TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_lan_messages_conversation_time
           ON lan_messages(conversation_id,timestamp);
         CREATE TABLE IF NOT EXISTS lan_conversation_reads (
           conversation_id TEXT PRIMARY KEY,
           last_read_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS lan_local_settings (
           singleton INTEGER PRIMARY KEY CHECK(singleton=1),
           receive_directory TEXT NOT NULL,
           auto_receive_images INTEGER NOT NULL DEFAULT 1,
           discovery_subnet TEXT NOT NULL DEFAULT '',
           updated_at INTEGER NOT NULL
         );",
    )
    .map_err(|error| error.to_string())?;
    ensure_local_settings_schema(&conn)?;
    transfer::initialize_schema(&conn)?;
    Ok(conn)
}

fn ensure_local_settings_schema(conn: &Connection) -> Result<(), String> {
    let mut statement = conn
        .prepare("PRAGMA table_info(lan_local_settings)")
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| error.to_string())?;
    if !columns.contains("discovery_subnet") {
        conn.execute(
            "ALTER TABLE lan_local_settings ADD COLUMN discovery_subnet TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn load_local_settings(conn: &Connection) -> Result<LanLocalSettings, String> {
    conn.query_row(
        "SELECT receive_directory,auto_receive_images,discovery_subnet FROM lan_local_settings WHERE singleton=1",
        [],
        |row| {
            Ok(LanLocalSettings {
                receive_directory: row.get(0)?,
                auto_receive_images: row.get::<_, i64>(1)? != 0,
                discovery_subnet: row.get(2)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

fn ensure_local_settings(conn: &Connection, default_directory: &Path) -> Result<(), String> {
    fs::create_dir_all(default_directory)
        .map_err(|error| format!("创建默认局域网接收目录失败：{error}"))?;
    conn.execute(
        "INSERT OR IGNORE INTO lan_local_settings(
           singleton,receive_directory,auto_receive_images,discovery_subnet,updated_at
         ) VALUES(1,?1,1,'',?2)",
        params![default_directory.to_string_lossy(), now_millis() as i64],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn configured_receive_directory() -> Result<PathBuf, String> {
    let (db_path, _) = state_paths()?;
    let settings = load_local_settings(&open_database(&db_path)?)?;
    Ok(PathBuf::from(settings.receive_directory))
}

fn validate_profile_text(value: &str, label: &str, max_chars: usize) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.chars().count() > max_chars {
        return Err(format!("{label}不能超过 {max_chars} 个字符"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(format!("{label}包含无效控制字符"));
    }
    Ok(trimmed.to_string())
}

fn random_display_name(user_id: &str) -> String {
    const STEMS: &[&str] = &[
        "青竹", "云杉", "星河", "松风", "远山", "白榆", "海盐", "晨光", "银杏", "清泉", "山岚",
        "雨燕", "月桂", "溪石", "南风", "霜叶",
    ];
    let digest = blake3::hash(user_id.as_bytes());
    let bytes = digest.as_bytes();
    let stem = STEMS[bytes[0] as usize % STEMS.len()];
    let number = u16::from_le_bytes([bytes[1], bytes[2]]) % 10_000;
    format!("{stem}-{number:04}")
}

fn load_or_create_profile(
    conn: &Connection,
    legacy: Option<LegacyLanProfile>,
) -> Result<LanProfile, String> {
    let existing = conn
        .query_row(
            "SELECT id,display_name,department,avatar_hash,avatar_path,profile_revision FROM lan_profile WHERE singleton=1",
            [],
            |row| {
                Ok(LanProfile {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    department: row.get(2)?,
                    avatar_hash: row.get(3)?,
                    avatar_path: row.get(4)?,
                    profile_revision: row.get::<_, i64>(5)? as u64,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(profile) = existing {
        return Ok(profile);
    }

    let legacy_id = legacy.as_ref().and_then(|value| value.user_id.clone());
    let id = legacy_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let legacy_name = legacy.and_then(|value| value.user_name);
    let display_name = legacy_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "匿名用户")
        .unwrap_or_else(|| random_display_name(&id));
    let profile = LanProfile {
        id,
        display_name,
        department: String::new(),
        avatar_hash: None,
        avatar_path: None,
        profile_revision: 1,
    };
    conn.execute(
        "INSERT INTO lan_profile(singleton,id,display_name,department,avatar_hash,avatar_path,profile_revision,updated_at)
         VALUES(1,?1,?2,?3,NULL,NULL,1,?4)",
        params![profile.id, profile.display_name, profile.department, now_millis() as i64],
    )
    .map_err(|error| error.to_string())?;
    Ok(profile)
}

fn prune_old_messages(conn: &Connection) -> Result<(), String> {
    let cutoff = now_millis().saturating_sub(MESSAGE_RETENTION_MS);
    conn.execute(
        "DELETE FROM lan_messages WHERE timestamp < ?1",
        params![cutoff as i64],
    )
    .map_err(|error| error.to_string())?;
    transfer::prune_expired(conn)?;
    Ok(())
}

fn state_paths() -> Result<(PathBuf, PathBuf), String> {
    let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
    let db_path = state
        .db_path
        .clone()
        .ok_or_else(|| "局域网协同尚未初始化".to_string())?;
    let avatars_dir = state
        .avatars_dir
        .clone()
        .ok_or_else(|| "局域网头像目录尚未初始化".to_string())?;
    Ok((db_path, avatars_dir))
}

fn conversation_id(message: &P2PMessage, local_id: &str) -> String {
    match &message.to_id {
        None => "lobby".to_string(),
        Some(to_id) if message.from_id == local_id => format!("direct:{to_id}"),
        Some(_) => format!("direct:{}", message.from_id),
    }
}

fn insert_message(
    conn: &Connection,
    message: &P2PMessage,
    local_id: &str,
    direction: &str,
    delivery_status: &str,
    delivery_summary: Option<&str>,
) -> Result<bool, String> {
    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO lan_messages(
               id,conversation_id,from_id,from_name,to_id,content,timestamp,direction,delivery_status,delivery_summary
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                message.id,
                conversation_id(message, local_id),
                message.from_id,
                message.from_name,
                message.to_id,
                message.content,
                message.timestamp as i64,
                direction,
                delivery_status,
                delivery_summary,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(inserted > 0)
}

fn wire_to_lan_message(
    message: &P2PMessage,
    local_id: &str,
    direction: &str,
    delivery_status: &str,
    delivery_summary: Option<String>,
) -> LanMessage {
    LanMessage {
        id: message.id.clone(),
        conversation_id: conversation_id(message, local_id),
        from_id: message.from_id.clone(),
        from_name: message.from_name.clone(),
        to_id: message.to_id.clone(),
        content: message.content.clone(),
        timestamp: message.timestamp,
        direction: direction.to_string(),
        delivery_status: delivery_status.to_string(),
        delivery_summary,
    }
}

fn upsert_contact(
    conn: &Connection,
    profile: &WireProfile,
    ip: &str,
    protocol_version: u16,
) -> Result<bool, String> {
    let previous: Option<(String, String, Option<String>, Option<String>, String, u64, u16)> = conn
        .query_row(
            "SELECT display_name,department,avatar_hash,avatar_path,ip,profile_revision,protocol_version FROM lan_contacts WHERE id=?1",
            params![profile.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get::<_, i64>(5)? as u64, row.get::<_, i64>(6)? as u16)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let now = now_millis();
    let avatar_path = previous.as_ref().and_then(|value| {
        if value.2 == profile.avatar_hash {
            value.3.clone()
        } else {
            None
        }
    });
    conn.execute(
        "INSERT INTO lan_contacts(
           id,display_name,department,avatar_hash,avatar_path,ip,first_seen,last_seen,protocol_version,profile_revision
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,?8,?9)
         ON CONFLICT(id) DO UPDATE SET
           display_name=excluded.display_name,
           department=excluded.department,
           avatar_hash=excluded.avatar_hash,
           avatar_path=excluded.avatar_path,
           ip=excluded.ip,
           last_seen=excluded.last_seen,
           protocol_version=excluded.protocol_version,
           profile_revision=excluded.profile_revision",
        params![
            profile.id,
            profile.display_name,
            profile.department,
            profile.avatar_hash,
            avatar_path,
            ip,
            now as i64,
            protocol_version as i64,
            profile.profile_revision as i64,
        ],
    )
    .map_err(|error| error.to_string())?;
    if previous
        .as_ref()
        .is_some_and(|value| value.2 != profile.avatar_hash)
    {
        cleanup_orphan_avatar(conn, previous.as_ref().and_then(|value| value.3.clone()));
    }
    Ok(previous
        .map(|value| {
            value.0 != profile.display_name
                || value.1 != profile.department
                || value.2 != profile.avatar_hash
                || value.4 != ip
                || value.5 != profile.profile_revision
                || value.6 != protocol_version
        })
        .unwrap_or(true))
}

fn upsert_legacy_contact(
    conn: &Connection,
    id: &str,
    name: &str,
    ip: &str,
) -> Result<bool, String> {
    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT display_name,ip FROM lan_contacts WHERE id=?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let now = now_millis();
    conn.execute(
        "INSERT INTO lan_contacts(id,display_name,department,ip,first_seen,last_seen,protocol_version,profile_revision)
         VALUES(?1,?2,'',?3,?4,?4,1,0)
         ON CONFLICT(id) DO UPDATE SET display_name=?2,ip=?3,last_seen=?4",
        params![id, name, ip, now as i64],
    )
    .map_err(|error| error.to_string())?;
    Ok(existing
        .map(|value| value.0 != name || value.1 != ip)
        .unwrap_or(true))
}

fn set_online_peer(profile: &WireProfile, ip: &str, protocol_version: u16) -> Result<bool, String> {
    let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
    let now = now_millis();
    let previous = state.online_users.get(&profile.id);
    let changed = previous
        .map(|peer| {
            peer.display_name != profile.display_name
                || peer.department != profile.department
                || peer.avatar_hash != profile.avatar_hash
                || peer.ip != ip
                || peer.protocol_version != protocol_version
                || peer.profile_revision != profile.profile_revision
        })
        .unwrap_or(true);
    state.online_users.insert(
        profile.id.clone(),
        OnlinePeer {
            id: profile.id.clone(),
            display_name: profile.display_name.clone(),
            department: profile.department.clone(),
            avatar_hash: profile.avatar_hash.clone(),
            ip: ip.to_string(),
            last_seen: now,
            protocol_version,
            profile_revision: profile.profile_revision,
        },
    );
    state.service.online_count = state.online_users.len();
    state.service.last_discovery_at = Some(now);
    Ok(changed)
}

fn set_online_legacy_peer(id: &str, name: &str, ip: &str) -> Result<bool, String> {
    let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
    let now = now_millis();
    let changed = if let Some(peer) = state.online_users.get_mut(id) {
        let changed = peer.display_name != name || peer.ip != ip;
        peer.display_name = name.to_string();
        peer.ip = ip.to_string();
        peer.last_seen = now;
        changed
    } else {
        state.online_users.insert(
            id.to_string(),
            OnlinePeer {
                id: id.to_string(),
                display_name: name.to_string(),
                department: String::new(),
                avatar_hash: None,
                ip: ip.to_string(),
                last_seen: now,
                protocol_version: 1,
                profile_revision: 0,
            },
        );
        true
    };
    state.service.online_count = state.online_users.len();
    state.service.last_discovery_at = Some(now);
    Ok(changed)
}

fn emit_contacts_changed(app_handle: &tauri::AppHandle) {
    let _ = app_handle.emit("pm-center:lan-contacts-changed", ());
    if let Ok(state) = P2P_GLOBAL.lock() {
        let users: Vec<P2PUser> = state
            .online_users
            .values()
            .map(|peer| P2PUser {
                id: peer.id.clone(),
                name: peer.display_name.clone(),
                ip: peer.ip.clone(),
                last_seen: peer.last_seen,
            })
            .collect();
        let _ = app_handle.emit("p2p-users", serde_json::json!({ "users": users }));
    }
}

fn emit_service_status(app_handle: &tauri::AppHandle) {
    if let Ok(state) = P2P_GLOBAL.lock() {
        let _ = app_handle.emit("pm-center:lan-service-status", state.service.clone());
    }
}

fn discovery_packet(profile: &LanProfile, response: bool) -> DiscoveryPacket {
    let fields = (
        profile.id.clone(),
        profile.display_name.clone(),
        Some(PROTOCOL_VERSION),
        Some(profile.department.clone()),
        profile.avatar_hash.clone(),
        Some(profile.profile_revision),
        Some(MESSAGE_PORT),
    );
    if response {
        DiscoveryPacket::Response {
            user_id: fields.0,
            user_name: fields.1,
            protocol_version: fields.2,
            department: fields.3,
            avatar_hash: fields.4,
            profile_revision: fields.5,
            message_port: fields.6,
        }
    } else {
        DiscoveryPacket::Announce {
            user_id: fields.0,
            user_name: fields.1,
            protocol_version: fields.2,
            department: fields.3,
            avatar_hash: fields.4,
            profile_revision: fields.5,
            message_port: fields.6,
        }
    }
}

fn discovery_targets() -> (Vec<SocketAddr>, Vec<String>) {
    let mut targets = HashSet::new();
    let mut local_addresses = HashSet::new();
    targets.insert(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::BROADCAST),
        DISCOVERY_PORT,
    ));
    if let Ok(interfaces) = get_if_addrs() {
        for interface in interfaces {
            if let IfAddr::V4(address) = interface.addr {
                if address.ip.is_loopback() || address.ip.is_unspecified() {
                    continue;
                }
                local_addresses.insert(address.ip.to_string());
                let broadcast = address.broadcast.unwrap_or_else(|| {
                    Ipv4Addr::from(u32::from(address.ip) | !u32::from(address.netmask))
                });
                targets.insert(SocketAddr::new(IpAddr::V4(broadcast), DISCOVERY_PORT));
            }
        }
    }
    let mut targets: Vec<_> = targets.into_iter().collect();
    targets.sort_by_key(|value| value.to_string());
    let mut local_addresses: Vec<_> = local_addresses.into_iter().collect();
    local_addresses.sort();
    (targets, local_addresses)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ipv4DiscoverySubnet {
    network: u32,
    broadcast: u32,
    prefix: u8,
}

impl Ipv4DiscoverySubnet {
    fn normalized(self) -> String {
        format!("{}/{}", Ipv4Addr::from(self.network), self.prefix)
    }

    fn targets(self, local_addresses: &HashSet<Ipv4Addr>) -> Vec<Ipv4Addr> {
        ((self.network + 1)..self.broadcast)
            .map(Ipv4Addr::from)
            .filter(|address| !local_addresses.contains(address))
            .collect()
    }
}

fn parse_discovery_subnet(value: &str) -> Result<Ipv4DiscoverySubnet, String> {
    let value = value.trim();
    let (address, prefix) = value
        .split_once('/')
        .ok_or_else(|| "请输入 IPv4 CIDR 网段，例如 10.13.13.0/24".to_string())?;
    let address = address
        .parse::<Ipv4Addr>()
        .map_err(|_| "网段地址不是有效的 IPv4 地址".to_string())?;
    let prefix = prefix
        .parse::<u8>()
        .map_err(|_| "CIDR 前缀必须是 24 到 30 的整数".to_string())?;
    if !(MIN_DISCOVERY_SUBNET_PREFIX..=MAX_DISCOVERY_SUBNET_PREFIX).contains(&prefix) {
        return Err(format!(
            "扫描网段仅支持 /{MIN_DISCOVERY_SUBNET_PREFIX} 到 /{MAX_DISCOVERY_SUBNET_PREFIX}，单次最多探测 254 个地址"
        ));
    }
    let mask = u32::MAX << (32 - prefix);
    let network = u32::from(address) & mask;
    Ok(Ipv4DiscoverySubnet {
        network,
        broadcast: network | !mask,
        prefix,
    })
}

fn local_ipv4_addresses() -> HashSet<Ipv4Addr> {
    get_if_addrs()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter_map(|interface| match interface.addr {
                    IfAddr::V4(address) => Some(address.ip),
                    IfAddr::V6(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn scan_discovery_subnet_value(
    discovery_subnet: &str,
) -> Result<LanDiscoverySubnetScanResult, String> {
    let subnet = parse_discovery_subnet(discovery_subnet)?;
    let (profile, running) = {
        let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        (state.profile.clone(), state.is_running)
    };
    if !running {
        return Err("请先开启局域网发现，再扫描隧道网段".to_string());
    }
    let profile = profile.ok_or_else(|| "局域网协同尚未初始化".to_string())?;
    let targets = subnet.targets(&local_ipv4_addresses());
    let packet = serde_json::to_vec(&discovery_packet(&profile, false))
        .map_err(|error| error.to_string())?;
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|error| format!("创建网段扫描套接字失败：{error}"))?;
    let mut sent_count = 0usize;
    let mut last_error = None;
    for address in &targets {
        match socket
            .send_to(
                &packet,
                SocketAddr::new(IpAddr::V4(*address), DISCOVERY_PORT),
            )
            .await
        {
            Ok(_) => sent_count += 1,
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    if sent_count == 0 && !targets.is_empty() {
        return Err(last_error.unwrap_or_else(|| "未能发送网段探测包".to_string()));
    }
    Ok(LanDiscoverySubnetScanResult {
        discovery_subnet: subnet.normalized(),
        target_count: targets.len(),
        sent_count,
    })
}

async fn broadcast_profile(
    socket: &UdpSocket,
    profile: &LanProfile,
) -> Result<Vec<String>, String> {
    let packet =
        serde_json::to_vec(&discovery_packet(profile, false)).map_err(|error| error.to_string())?;
    let (mut targets, local_addresses) = discovery_targets();
    if let Ok(state) = P2P_GLOBAL.lock() {
        targets.extend(state.online_users.values().filter_map(|peer| {
            peer.ip
                .parse::<Ipv4Addr>()
                .ok()
                .map(|address| SocketAddr::new(IpAddr::V4(address), DISCOVERY_PORT))
        }));
    }
    targets.sort_unstable();
    targets.dedup();
    let mut sent = 0usize;
    let mut last_error = None;
    for target in targets {
        match socket.send_to(&packet, target).await {
            Ok(_) => sent += 1,
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    if sent == 0 {
        return Err(last_error.unwrap_or_else(|| "没有可用的局域网广播地址".to_string()));
    }
    Ok(local_addresses)
}

async fn run_discovery_loop(socket: UdpSocket, app_handle: tauri::AppHandle, db_path: PathBuf) {
    let mut buffer = vec![0u8; 4096];
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let mut last_broadcast = Instant::now() - BROADCAST_INTERVAL;
    let mut seen_generation = 0u64;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let (profile, running, generation, removed) = {
                    let mut state = match P2P_GLOBAL.lock() {
                        Ok(state) => state,
                        Err(_) => continue,
                    };
                    let now = now_millis();
                    let before = state.online_users.len();
                    state.online_users.retain(|_, peer| now.saturating_sub(peer.last_seen) < USER_TIMEOUT.as_millis() as u64);
                    let removed = before != state.online_users.len();
                    if removed {
                        state.service.online_count = state.online_users.len();
                    }
                    (state.profile.clone(), state.is_running, state.announce_generation, removed)
                };
                if removed {
                    emit_contacts_changed(&app_handle);
                    emit_service_status(&app_handle);
                }
                if !running {
                    continue;
                }
                if generation != seen_generation || last_broadcast.elapsed() >= BROADCAST_INTERVAL {
                    if let Some(profile) = profile {
                        match broadcast_profile(&socket, &profile).await {
                            Ok(addresses) => {
                                if let Ok(mut state) = P2P_GLOBAL.lock() {
                                    state.service.local_addresses = addresses;
                                    state.service.last_error = None;
                                }
                            }
                            Err(error) => {
                                if let Ok(mut state) = P2P_GLOBAL.lock() {
                                    state.service.last_error = Some(format!("局域网广播失败：{error}"));
                                }
                            }
                        }
                        emit_service_status(&app_handle);
                    }
                    seen_generation = generation;
                    last_broadcast = Instant::now();
                }
            }
            received = socket.recv_from(&mut buffer) => {
                let Ok((length, source)) = received else { continue; };
                let running = P2P_GLOBAL.lock().map(|state| state.is_running).unwrap_or(false);
                if !running {
                    continue;
                }
                let Ok(packet) = serde_json::from_slice::<DiscoveryPacket>(&buffer[..length]) else { continue; };
                let (fields, needs_response) = packet.fields();
                let local_profile = P2P_GLOBAL.lock().ok().and_then(|state| state.profile.clone());
                if local_profile.as_ref().is_some_and(|profile| profile.id == fields.user_id) {
                    continue;
                }
                let profile = WireProfile {
                    id: fields.user_id,
                    display_name: fields.user_name,
                    department: fields.department.unwrap_or_default(),
                    avatar_hash: fields.avatar_hash,
                    profile_revision: fields.profile_revision.unwrap_or(0),
                };
                let protocol_version = fields.protocol_version.unwrap_or(1);
                let ip = source.ip().to_string();
                let changed = open_database(&db_path)
                    .and_then(|conn| upsert_contact(&conn, &profile, &ip, protocol_version))
                    .unwrap_or(false)
                    | set_online_peer(&profile, &ip, protocol_version).unwrap_or(false);
                if changed {
                    emit_contacts_changed(&app_handle);
                }
                emit_service_status(&app_handle);

                if needs_response {
                    if let Some(local_profile) = local_profile.clone() {
                        if let Ok(data) = serde_json::to_vec(&discovery_packet(&local_profile, true)) {
                            let response_port = if fields.message_port.is_some() { DISCOVERY_PORT } else { source.port() };
                            let _ = socket.send_to(&data, SocketAddr::new(source.ip(), response_port)).await;
                        }
                    }
                }

                if protocol_version >= 2 && changed {
                    let app_handle = app_handle.clone();
                    let db_path = db_path.clone();
                    tokio::spawn(async move {
                        if let Err(error) = send_hello_and_fetch(&ip, app_handle.clone(), db_path).await {
                            if let Ok(mut state) = P2P_GLOBAL.lock() {
                                state.service.last_error = Some(format!("与 {ip} 同步联系人资料失败：{error}"));
                            }
                            emit_service_status(&app_handle);
                        }
                    });
                } else if protocol_version >= LOBBY_SYNC_MIN_PROTOCOL_VERSION {
                    let app_handle = app_handle.clone();
                    let db_path = db_path.clone();
                    let peer_id = profile.id.clone();
                    tokio::spawn(async move {
                        if let Err(error) =
                            sync_lobby_history(&peer_id, &ip, app_handle.clone(), db_path).await
                        {
                            if let Ok(mut state) = P2P_GLOBAL.lock() {
                                state.service.last_error =
                                    Some(format!("与 {ip} 同步大厅记录失败：{error}"));
                            }
                            emit_service_status(&app_handle);
                        }
                    });
                }
            }
        }
    }
}

async fn connect_to_peer(ip: &str) -> Result<TcpStream, String> {
    let address = format!("{ip}:{MESSAGE_PORT}");
    tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&address))
        .await
        .map_err(|_| format!("连接 {address} 超时"))?
        .map_err(|error| format!("连接 {address} 失败：{error}"))
}

async fn write_json_line<T: Serialize>(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    value: &T,
) -> Result<(), String> {
    let mut data = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    data.push(b'\n');
    tokio::time::timeout(DELIVERY_ACK_TIMEOUT, writer.write_all(&data))
        .await
        .map_err(|_| "发送数据超时".to_string())?
        .map_err(|error| error.to_string())
}

async fn read_json_line(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<String, String> {
    read_json_line_with_timeout(reader, DELIVERY_ACK_TIMEOUT).await
}

async fn read_json_line_with_timeout(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    timeout: Duration,
) -> Result<String, String> {
    let mut line = String::new();
    tokio::time::timeout(timeout, reader.read_line(&mut line))
        .await
        .map_err(|_| "等待对方响应超时".to_string())?
        .map_err(|error| error.to_string())?;
    if line.len() > MAX_CONTROL_BYTES {
        return Err("对方响应超过允许大小".to_string());
    }
    Ok(line)
}

async fn send_hello_and_fetch(
    ip: &str,
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
) -> Result<(), String> {
    let profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_json_line(
        &mut writer,
        &LanControlEnvelope::Hello {
            protocol_version: PROTOCOL_VERSION,
            profile: WireProfile::from(&profile),
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    let line = read_json_line_with_timeout(&mut reader, Duration::from_secs(60)).await?;
    let envelope: LanControlEnvelope =
        serde_json::from_str(line.trim()).map_err(|error| error.to_string())?;
    let LanControlEnvelope::HelloAck {
        protocol_version,
        profile: remote_profile,
    } = envelope
    else {
        return Err("对方未返回联系人资料确认".to_string());
    };
    let changed = upsert_contact(
        &open_database(&db_path)?,
        &remote_profile,
        ip,
        protocol_version,
    )? | set_online_peer(&remote_profile, ip, protocol_version)?;
    if changed {
        emit_contacts_changed(&app_handle);
    }
    fetch_remote_avatar(ip, &remote_profile, app_handle.clone(), db_path.clone()).await?;
    if protocol_version >= LOBBY_SYNC_MIN_PROTOCOL_VERSION {
        sync_lobby_history(&remote_profile.id, ip, app_handle, db_path).await?;
    }
    Ok(())
}

fn contact_avatar_is_current(
    conn: &Connection,
    contact_id: &str,
    avatar_hash: &str,
) -> Result<bool, String> {
    let stored: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT avatar_hash,avatar_path FROM lan_contacts WHERE id=?1",
            params![contact_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(stored.is_some_and(|(hash, path)| {
        hash.as_deref() == Some(avatar_hash) && path.is_some_and(|value| Path::new(&value).exists())
    }))
}

async fn fetch_remote_avatar(
    ip: &str,
    profile: &WireProfile,
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
) -> Result<(), String> {
    let Some(expected_hash) = profile.avatar_hash.as_deref() else {
        return Ok(());
    };
    let conn = open_database(&db_path)?;
    if contact_avatar_is_current(&conn, &profile.id, expected_hash)? {
        return Ok(());
    }
    drop(conn);
    let local_profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_json_line(
        &mut writer,
        &LanControlEnvelope::ProfileRequest {
            requester_id: local_profile.id,
            known_avatar_hash: None,
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    let line = read_json_line(&mut reader).await?;
    let response: LanControlEnvelope =
        serde_json::from_str(line.trim()).map_err(|error| error.to_string())?;
    let LanControlEnvelope::ProfileResponse {
        profile: returned_profile,
        avatar_base64,
    } = response
    else {
        return Err("对方返回了无效头像资料".to_string());
    };
    if returned_profile.id != profile.id {
        return Err("对方返回的头像身份不匹配".to_string());
    }
    let Some(encoded) = avatar_base64 else {
        return Ok(());
    };
    let bytes = BASE64.decode(encoded).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_AVATAR_BYTES || image::load_from_memory(&bytes).is_err() {
        return Err("对方头像无效或超过大小限制".to_string());
    }
    let actual_hash = blake3::hash(&bytes).to_hex().to_string();
    if returned_profile.avatar_hash.as_deref() != Some(actual_hash.as_str()) {
        return Err("对方头像哈希校验失败".to_string());
    }
    let (_, avatars_dir) = state_paths()?;
    fs::create_dir_all(&avatars_dir).map_err(|error| error.to_string())?;
    let avatar_path = avatars_dir.join(format!("{actual_hash}.jpg"));
    fs::write(&avatar_path, bytes).map_err(|error| error.to_string())?;
    let conn = open_database(&db_path)?;
    conn.execute(
        "UPDATE lan_contacts SET avatar_hash=?2,avatar_path=?3 WHERE id=?1",
        params![
            returned_profile.id,
            actual_hash,
            avatar_path.to_string_lossy()
        ],
    )
    .map_err(|error| error.to_string())?;
    emit_contacts_changed(&app_handle);
    Ok(())
}

fn recent_lobby_messages(conn: &Connection) -> Result<Vec<P2PMessage>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id,from_id,from_name,content,timestamp
             FROM lan_messages
             WHERE conversation_id='lobby' AND to_id IS NULL
             ORDER BY timestamp DESC LIMIT ?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![LOBBY_HISTORY_LIMIT as i64], |row| {
            Ok(P2PMessage {
                id: row.get(0)?,
                from_id: row.get(1)?,
                from_name: row.get(2)?,
                to_id: None,
                content: row.get(3)?,
                timestamp: row.get::<_, i64>(4)? as u64,
                protocol_version: Some(PROTOCOL_VERSION),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

async fn build_lobby_history(
    db_path: &Path,
    requester_id: &str,
    known_image_ids: &HashSet<String>,
    requested_limit: u16,
) -> Result<(Vec<P2PMessage>, Vec<transfer::WireTransferOffer>), String> {
    enum HistoryEntry {
        Message(P2PMessage),
        Image(LanTransfer),
    }

    impl HistoryEntry {
        fn timestamp(&self) -> u64 {
            match self {
                Self::Message(message) => message.timestamp,
                Self::Image(transfer) => transfer.created_at,
            }
        }
    }

    let conn = open_database(db_path)?;
    let mut entries = recent_lobby_messages(&conn)?
        .into_iter()
        .map(HistoryEntry::Message)
        .chain(
            transfer::relayable_lobby_images(&conn)?
                .into_iter()
                .map(HistoryEntry::Image),
        )
        .collect::<Vec<_>>();
    drop(conn);
    entries.sort_by_key(HistoryEntry::timestamp);
    let limit = usize::from(requested_limit).clamp(1, LOBBY_HISTORY_LIMIT);
    if entries.len() > limit {
        entries.drain(..entries.len() - limit);
    }

    let provider = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let mut messages = Vec::new();
    let mut image_offers = Vec::new();
    for entry in entries {
        match entry {
            HistoryEntry::Message(message) => messages.push(message),
            HistoryEntry::Image(transfer) => {
                if transfer
                    .lobby_item_id
                    .as_ref()
                    .is_some_and(|id| known_image_ids.contains(id))
                {
                    continue;
                }
                match transfer::prepare_lobby_history_offer(
                    db_path,
                    &transfer,
                    requester_id,
                    &provider,
                )
                .await
                {
                    Ok(offer) => image_offers.push(offer),
                    Err(error) => eprintln!(
                        "[P2P] 跳过无法提供的大厅图片 {}：{error}",
                        transfer.display_name
                    ),
                }
            }
        }
    }
    Ok((messages, image_offers))
}

async fn sync_lobby_history(
    peer_id: &str,
    ip: &str,
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
) -> Result<(), String> {
    let local_profile = {
        let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        let now = now_millis();
        if state
            .last_history_sync
            .get(peer_id)
            .is_some_and(|last| now.saturating_sub(*last) < LOBBY_HISTORY_SYNC_INTERVAL_MS)
        {
            return Ok(());
        }
        state.last_history_sync.insert(peer_id.to_string(), now);
        state
            .profile
            .clone()
            .ok_or_else(|| "局域网身份尚未初始化".to_string())?
    };
    let known_image_ids = transfer::known_lobby_image_ids(&open_database(&db_path)?)?;
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_json_line(
        &mut writer,
        &LanControlEnvelope::LobbyHistoryRequest {
            requester_id: local_profile.id.clone(),
            known_image_ids: known_image_ids.into_iter().take(5_000).collect(),
            limit: LOBBY_HISTORY_LIMIT as u16,
        },
    )
    .await?;
    let mut reader = BufReader::new(reader);
    let line = read_json_line_with_timeout(&mut reader, Duration::from_secs(60)).await?;
    let envelope: LanControlEnvelope =
        serde_json::from_str(line.trim()).map_err(|error| error.to_string())?;
    let LanControlEnvelope::LobbyHistoryResponse {
        mut messages,
        image_offers,
    } = envelope
    else {
        return Err("对方未返回大厅历史".to_string());
    };
    if messages.len() + image_offers.len() > LOBBY_HISTORY_LIMIT {
        return Err("对方返回的大厅历史超过限制".to_string());
    }
    messages.sort_by_key(|message| message.timestamp);
    let conn = open_database(&db_path)?;
    for message in messages {
        if message.id.is_empty()
            || message.to_id.is_some()
            || message.content.trim().is_empty()
            || message.content.as_bytes().len() > MAX_MESSAGE_BYTES
        {
            continue;
        }
        let direction = if message.from_id == local_profile.id {
            "outgoing"
        } else {
            "incoming"
        };
        if insert_message(
            &conn,
            &message,
            &local_profile.id,
            direction,
            "synced",
            None,
        )? {
            let lan_message =
                wire_to_lan_message(&message, &local_profile.id, direction, "synced", None);
            let _ = app_handle.emit("pm-center:lan-message", &lan_message);
        }
    }
    drop(conn);
    transfer::accept_lobby_history_offers(image_offers, ip, &app_handle, &db_path).await?;
    Ok(())
}

async fn run_tcp_server(listener: TcpListener, app_handle: tauri::AppHandle, db_path: PathBuf) {
    loop {
        match listener.accept().await {
            Ok((stream, address)) => {
                let app_handle = app_handle.clone();
                let db_path = db_path.clone();
                tokio::spawn(async move {
                    if let Err(error) =
                        handle_tcp_connection(stream, address, app_handle, db_path).await
                    {
                        eprintln!("[P2P] 处理连接失败：{error}");
                    }
                });
            }
            Err(error) => {
                if let Ok(mut state) = P2P_GLOBAL.lock() {
                    state.service.last_error = Some(format!("接受局域网连接失败：{error}"));
                }
                emit_service_status(&app_handle);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn handle_tcp_connection(
    stream: TcpStream,
    address: SocketAddr,
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
) -> Result<(), String> {
    if !P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .is_running
    {
        return Ok(());
    }
    let ip = address.ip().to_string();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let line = read_json_line(&mut reader).await?;
    let trimmed = line.trim();

    if let Ok(message) = serde_json::from_str::<P2PMessage>(trimmed) {
        if message.content.trim().is_empty() || message.content.len() > MAX_MESSAGE_BYTES {
            return Err("收到无效消息".to_string());
        }
        let local_profile = P2P_GLOBAL
            .lock()
            .map_err(|error| error.to_string())?
            .profile
            .clone()
            .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
        if message
            .to_id
            .as_deref()
            .is_some_and(|to_id| to_id != local_profile.id)
        {
            return Ok(());
        }
        let conn = open_database(&db_path)?;
        let contact_changed =
            upsert_legacy_contact(&conn, &message.from_id, &message.from_name, &ip)?
                | set_online_legacy_peer(&message.from_id, &message.from_name, &ip)?;
        let inserted = insert_message(
            &conn,
            &message,
            &local_profile.id,
            "incoming",
            "delivered",
            None,
        )?;
        if contact_changed {
            emit_contacts_changed(&app_handle);
        }
        if inserted {
            let lan_message =
                wire_to_lan_message(&message, &local_profile.id, "incoming", "delivered", None);
            let _ = app_handle.emit("pm-center:lan-message", &lan_message);
            let _ = app_handle.emit("p2p-message", &message);
            notify_incoming_message(&app_handle, &lan_message);
        }
        write_json_line(
            &mut writer,
            &P2PDeliveryAck {
                kind: "ack".to_string(),
                message_id: message.id,
            },
        )
        .await?;
        return Ok(());
    }

    if let Ok(envelope) = serde_json::from_str::<transfer::TransferControlEnvelope>(trimmed) {
        transfer::handle_connection(envelope, ip, &mut reader, &mut writer, app_handle, db_path)
            .await?;
        return Ok(());
    }

    let envelope: LanControlEnvelope =
        serde_json::from_str(trimmed).map_err(|_| "收到无法识别的局域网数据".to_string())?;
    match envelope {
        LanControlEnvelope::Hello {
            protocol_version,
            profile,
        } => {
            let conn = open_database(&db_path)?;
            let changed = upsert_contact(&conn, &profile, &ip, protocol_version)?
                | set_online_peer(&profile, &ip, protocol_version)?;
            if changed {
                emit_contacts_changed(&app_handle);
            }
            let local_profile = P2P_GLOBAL
                .lock()
                .map_err(|error| error.to_string())?
                .profile
                .clone()
                .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
            write_json_line(
                &mut writer,
                &LanControlEnvelope::HelloAck {
                    protocol_version: PROTOCOL_VERSION,
                    profile: WireProfile::from(&local_profile),
                },
            )
            .await?;
            if profile.avatar_hash.is_some() {
                let avatar_app_handle = app_handle.clone();
                let avatar_ip = ip.clone();
                let avatar_profile = profile.clone();
                let avatar_db_path = db_path.clone();
                tokio::spawn(async move {
                    let _ = fetch_remote_avatar(
                        &avatar_ip,
                        &avatar_profile,
                        avatar_app_handle,
                        avatar_db_path,
                    )
                    .await;
                });
            }
            if protocol_version >= LOBBY_SYNC_MIN_PROTOCOL_VERSION {
                let history_app_handle = app_handle.clone();
                let history_ip = ip.clone();
                let history_peer_id = profile.id.clone();
                let history_db_path = db_path.clone();
                tokio::spawn(async move {
                    let _ = sync_lobby_history(
                        &history_peer_id,
                        &history_ip,
                        history_app_handle,
                        history_db_path,
                    )
                    .await;
                });
            }
        }
        LanControlEnvelope::ProfileRequest {
            known_avatar_hash, ..
        } => {
            let profile = P2P_GLOBAL
                .lock()
                .map_err(|error| error.to_string())?
                .profile
                .clone()
                .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
            let avatar_base64 = if profile.avatar_hash != known_avatar_hash {
                profile
                    .avatar_path
                    .as_deref()
                    .and_then(|path| fs::read(path).ok())
                    .filter(|bytes| bytes.len() <= MAX_AVATAR_BYTES)
                    .map(|bytes| BASE64.encode(bytes))
            } else {
                None
            };
            write_json_line(
                &mut writer,
                &LanControlEnvelope::ProfileResponse {
                    profile: WireProfile::from(&profile),
                    avatar_base64,
                },
            )
            .await?;
        }
        LanControlEnvelope::LobbyHistoryRequest {
            requester_id,
            known_image_ids,
            limit,
        } => {
            let requester_is_peer = P2P_GLOBAL
                .lock()
                .map_err(|error| error.to_string())?
                .online_users
                .get(&requester_id)
                .is_some_and(|peer| {
                    peer.ip == ip && peer.protocol_version >= LOBBY_SYNC_MIN_PROTOCOL_VERSION
                });
            if !requester_is_peer {
                return Err("大厅历史请求方身份无效".to_string());
            }
            let known_image_ids = known_image_ids
                .into_iter()
                .take(5_000)
                .collect::<HashSet<_>>();
            let (messages, image_offers) =
                build_lobby_history(&db_path, &requester_id, &known_image_ids, limit).await?;
            write_json_line(
                &mut writer,
                &LanControlEnvelope::LobbyHistoryResponse {
                    messages,
                    image_offers,
                },
            )
            .await?;
        }
        _ => return Err("不支持的局域网控制消息".to_string()),
    }
    Ok(())
}

async fn send_tcp_message(ip: &str, message: &P2PMessage) -> Result<(), String> {
    let stream = connect_to_peer(ip).await?;
    let (reader, mut writer) = stream.into_split();
    write_json_line(&mut writer, message).await?;
    let mut reader = BufReader::new(reader);
    let response = read_json_line(&mut reader).await?;
    let ack: P2PDeliveryAck =
        serde_json::from_str(response.trim()).map_err(|_| "对方返回了无效送达确认".to_string())?;
    if ack.kind != "ack" || ack.message_id != message.id {
        return Err("对方未确认当前消息".to_string());
    }
    Ok(())
}

fn load_contacts(
    conn: &Connection,
    online: &HashMap<String, OnlinePeer>,
) -> Result<Vec<LanContact>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id,display_name,department,avatar_hash,avatar_path,ip,first_seen,last_seen,protocol_version,profile_revision
             FROM lan_contacts ORDER BY display_name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(LanContact {
                online: online.contains_key(&id),
                id,
                display_name: row.get(1)?,
                department: row.get(2)?,
                avatar_hash: row.get(3)?,
                avatar_path: row.get(4)?,
                ip: row.get(5)?,
                first_seen: row.get::<_, i64>(6)? as u64,
                last_seen: row.get::<_, i64>(7)? as u64,
                protocol_version: row.get::<_, i64>(8)? as u16,
                profile_revision: row.get::<_, i64>(9)? as u64,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_messages(conn: &Connection) -> Result<Vec<LanMessage>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id,conversation_id,from_id,from_name,to_id,content,timestamp,direction,delivery_status,delivery_summary
             FROM (SELECT * FROM lan_messages ORDER BY timestamp DESC LIMIT 5000)
             ORDER BY timestamp ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(LanMessage {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                from_id: row.get(2)?,
                from_name: row.get(3)?,
                to_id: row.get(4)?,
                content: row.get(5)?,
                timestamp: row.get::<_, i64>(6)? as u64,
                direction: row.get(7)?,
                delivery_status: row.get(8)?,
                delivery_summary: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn unread_counts(conn: &Connection) -> Result<HashMap<String, u64>, String> {
    let mut statement = conn
        .prepare(
            "SELECT m.conversation_id,COUNT(*)
             FROM lan_messages m
             LEFT JOIN lan_conversation_reads r ON r.conversation_id=m.conversation_id
             WHERE m.direction='incoming' AND m.timestamp>COALESCE(r.last_read_at,0)
             GROUP BY m.conversation_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })
        .map_err(|error| error.to_string())?;
    let mut counts = rows
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|error| error.to_string())?;
    for (conversation_id, count) in transfer::unread_counts(conn)? {
        counts
            .entry(conversation_id)
            .and_modify(|value| *value += count)
            .or_insert(count);
    }
    Ok(counts)
}

fn build_conversations(
    contacts: &[LanContact],
    messages: &[LanMessage],
    transfers: &[LanTransfer],
    unread: &HashMap<String, u64>,
) -> Vec<LanConversation> {
    let mut last_message = HashMap::<String, u64>::new();
    for message in messages {
        last_message
            .entry(message.conversation_id.clone())
            .and_modify(|value| *value = (*value).max(message.timestamp))
            .or_insert(message.timestamp);
    }
    for (conversation_id, timestamp) in transfer::activity_by_conversation(transfers) {
        last_message
            .entry(conversation_id)
            .and_modify(|value| *value = (*value).max(timestamp))
            .or_insert(timestamp);
    }
    let mut conversations = vec![LanConversation {
        id: "lobby".to_string(),
        kind: "lobby".to_string(),
        contact_id: None,
        title: "局域网大厅".to_string(),
        unread_count: unread.get("lobby").copied().unwrap_or(0),
        last_message_at: last_message.get("lobby").copied(),
    }];
    conversations.extend(contacts.iter().map(|contact| {
        let id = format!("direct:{}", contact.id);
        LanConversation {
            id: id.clone(),
            kind: "direct".to_string(),
            contact_id: Some(contact.id.clone()),
            title: contact.display_name.clone(),
            unread_count: unread.get(&id).copied().unwrap_or(0),
            last_message_at: last_message.get(&id).copied(),
        }
    }));
    let known_ids: HashSet<String> = conversations
        .iter()
        .map(|conversation| conversation.id.clone())
        .collect();
    for (id, timestamp) in &last_message {
        if !id.starts_with("direct:") || known_ids.contains(id) {
            continue;
        }
        conversations.push(LanConversation {
            id: id.clone(),
            kind: "direct".to_string(),
            contact_id: Some(id.trim_start_matches("direct:").to_string()),
            title: "已移除联系人".to_string(),
            unread_count: unread.get(id).copied().unwrap_or(0),
            last_message_at: Some(*timestamp),
        });
    }
    conversations.sort_by(|left, right| {
        if left.id == "lobby" {
            return std::cmp::Ordering::Less;
        }
        if right.id == "lobby" {
            return std::cmp::Ordering::Greater;
        }
        right
            .last_message_at
            .unwrap_or(0)
            .cmp(&left.last_message_at.unwrap_or(0))
            .then(left.title.cmp(&right.title))
    });
    conversations
}

fn snapshot() -> Result<LanSnapshot, String> {
    let (profile, db_path, online, service) = {
        let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        (
            state
                .profile
                .clone()
                .ok_or_else(|| "局域网协同尚未初始化".to_string())?,
            state
                .db_path
                .clone()
                .ok_or_else(|| "局域网数据库尚未初始化".to_string())?,
            state.online_users.clone(),
            state.service.clone(),
        )
    };
    let conn = open_database(&db_path)?;
    let local_settings = load_local_settings(&conn)?;
    let contacts = load_contacts(&conn, &online)?;
    let messages = load_messages(&conn)?;
    let transfers = transfer::load_transfers(&conn)?;
    let unread = unread_counts(&conn)?;
    let conversations = build_conversations(&contacts, &messages, &transfers, &unread);
    Ok(LanSnapshot {
        profile,
        local_settings,
        contacts,
        messages,
        transfers,
        conversations,
        unread_count: unread.values().sum(),
        service,
    })
}

#[tauri::command]
pub async fn initialize_lan_collaboration(
    app_handle: tauri::AppHandle,
    legacy_profile: Option<LegacyLanProfile>,
) -> Result<LanSnapshot, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败：{error}"))?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let db_path = app_data_dir.join("lan_collaboration.db");
    let avatars_dir = app_data_dir.join("lan_collaboration").join("avatars");
    let transfer_staging_dir = app_data_dir.join("lan_collaboration").join("staging");
    let lobby_cache_dir = app_data_dir.join("lan_collaboration").join("lobby");
    fs::create_dir_all(&avatars_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(&transfer_staging_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(&lobby_cache_dir).map_err(|error| error.to_string())?;
    let conn = open_database(&db_path)?;
    let default_receive_directory = app_handle
        .path()
        .download_dir()
        .unwrap_or_else(|_| app_data_dir.join("lan_collaboration").join("received"))
        .join("PM Center 接收文件");
    ensure_local_settings(&conn, &default_receive_directory)?;
    transfer::recover_interrupted(&conn)?;
    transfer::prune_staging_files(&transfer_staging_dir);
    transfer::prune_staging_files(&lobby_cache_dir);
    let profile = load_or_create_profile(&conn, legacy_profile)?;
    prune_old_messages(&conn)?;
    {
        let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        state.profile = Some(profile);
        state.db_path = Some(db_path);
        state.avatars_dir = Some(avatars_dir);
        state.last_cleanup_at = now_millis();
    }
    match start_lan_discovery(app_handle.clone()).await {
        Ok(()) => {
            let discovery_subnet = load_local_settings(&conn)?.discovery_subnet;
            if !discovery_subnet.is_empty() {
                if let Err(error) = scan_discovery_subnet_value(&discovery_subnet).await {
                    if let Ok(mut state) = P2P_GLOBAL.lock() {
                        state.service.last_error = Some(format!("启动时扫描隧道网段失败：{error}"));
                    }
                    emit_service_status(&app_handle);
                }
            }
        }
        Err(error) => {
            if let Ok(mut state) = P2P_GLOBAL.lock() {
                state.service.last_error = Some(error);
            }
        }
    }
    snapshot()
}

#[tauri::command]
pub async fn get_lan_collaboration_snapshot() -> Result<LanSnapshot, String> {
    let should_cleanup = {
        let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        now_millis().saturating_sub(state.last_cleanup_at) > 24 * 60 * 60 * 1000
    };
    if should_cleanup {
        let (db_path, _) = state_paths()?;
        prune_old_messages(&open_database(&db_path)?)?;
        if let Ok(mut state) = P2P_GLOBAL.lock() {
            state.last_cleanup_at = now_millis();
        }
    }
    snapshot()
}

#[tauri::command]
pub async fn update_lan_profile(
    app_handle: tauri::AppHandle,
    request: UpdateLanProfileRequest,
) -> Result<LanProfile, String> {
    let display_name = validate_profile_text(&request.display_name, "显示名称", 32)?;
    if display_name.is_empty() {
        return Err("显示名称不能为空".to_string());
    }
    let department = validate_profile_text(&request.department, "部门", 40)?;
    let (db_path, _) = state_paths()?;
    let mut profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    profile.display_name = display_name;
    profile.department = department;
    profile.profile_revision = profile.profile_revision.saturating_add(1);
    open_database(&db_path)?
        .execute(
            "UPDATE lan_profile SET display_name=?1,department=?2,profile_revision=?3,updated_at=?4 WHERE singleton=1",
            params![profile.display_name, profile.department, profile.profile_revision as i64, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    {
        let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        state.profile = Some(profile.clone());
        state.announce_generation = state.announce_generation.wrapping_add(1);
    }
    let _ = app_handle.emit("pm-center:lan-profile-changed", &profile);
    Ok(profile)
}

#[tauri::command]
pub async fn update_lan_receive_directory(
    app_handle: tauri::AppHandle,
    request: UpdateLanReceiveDirectoryRequest,
) -> Result<LanLocalSettings, String> {
    let requested = request.receive_directory.trim();
    if requested.is_empty() {
        return Err("请选择局域网文件接收目录".to_string());
    }
    let path = PathBuf::from(requested);
    if !path.is_absolute() {
        return Err("局域网文件接收目录必须是绝对路径".to_string());
    }
    fs::create_dir_all(&path).map_err(|error| format!("创建局域网接收目录失败：{error}"))?;
    let canonical =
        fs::canonicalize(&path).map_err(|error| format!("无法访问局域网接收目录：{error}"))?;
    if !canonical.is_dir() {
        return Err("选择的位置不是文件夹".to_string());
    }
    let (db_path, _) = state_paths()?;
    let conn = open_database(&db_path)?;
    conn.execute(
        "UPDATE lan_local_settings SET receive_directory=?1,updated_at=?2 WHERE singleton=1",
        params![canonical.to_string_lossy(), now_millis() as i64],
    )
    .map_err(|error| error.to_string())?;
    let settings = load_local_settings(&conn)?;
    let _ = app_handle.emit("pm-center:lan-settings-changed", &settings);
    Ok(settings)
}

#[tauri::command]
pub async fn update_lan_discovery_subnet(
    app_handle: tauri::AppHandle,
    request: UpdateLanDiscoverySubnetRequest,
) -> Result<LanDiscoverySubnetScanResult, String> {
    let requested = request.discovery_subnet.trim();
    let normalized = if requested.is_empty() {
        String::new()
    } else {
        parse_discovery_subnet(requested)?.normalized()
    };
    if !normalized.is_empty()
        && !P2P_GLOBAL
            .lock()
            .map_err(|error| error.to_string())?
            .is_running
    {
        return Err("请先开启局域网发现，再保存并扫描隧道网段".to_string());
    }
    let (db_path, _) = state_paths()?;
    let conn = open_database(&db_path)?;
    conn.execute(
        "UPDATE lan_local_settings SET discovery_subnet=?1,updated_at=?2 WHERE singleton=1",
        params![normalized, now_millis() as i64],
    )
    .map_err(|error| error.to_string())?;
    let settings = load_local_settings(&conn)?;
    let _ = app_handle.emit("pm-center:lan-settings-changed", &settings);
    if settings.discovery_subnet.is_empty() {
        return Ok(LanDiscoverySubnetScanResult {
            discovery_subnet: String::new(),
            target_count: 0,
            sent_count: 0,
        });
    }
    scan_discovery_subnet_value(&settings.discovery_subnet).await
}

#[tauri::command]
pub async fn scan_lan_discovery_subnet() -> Result<LanDiscoverySubnetScanResult, String> {
    let (db_path, _) = state_paths()?;
    let settings = load_local_settings(&open_database(&db_path)?)?;
    if settings.discovery_subnet.is_empty() {
        return Err("请先设置要扫描的隧道网段".to_string());
    }
    scan_discovery_subnet_value(&settings.discovery_subnet).await
}

fn create_avatar(source_path: &Path) -> Result<(Vec<u8>, String), String> {
    let metadata = fs::metadata(source_path).map_err(|error| format!("读取头像失败：{error}"))?;
    if metadata.len() > MAX_AVATAR_SOURCE_BYTES {
        return Err("头像原图不能超过 10 MB".to_string());
    }
    let image = image::open(source_path).map_err(|error| format!("无法解码头像图片：{error}"))?;
    let (width, height) = image.dimensions();
    let side = width.min(height);
    let cropped = image.crop_imm((width - side) / 2, (height - side) / 2, side, side);
    let resized = cropped
        .resize_exact(128, 128, FilterType::Lanczos3)
        .to_rgb8();
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut bytes), 84)
        .encode_image(&resized)
        .map_err(|error| format!("压缩头像失败：{error}"))?;
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err("压缩后的头像超过 64 KB".to_string());
    }
    let hash = blake3::hash(&bytes).to_hex().to_string();
    Ok((bytes, hash))
}

#[tauri::command]
pub async fn set_lan_avatar(
    app_handle: tauri::AppHandle,
    image_path: Option<String>,
) -> Result<LanProfile, String> {
    let (db_path, avatars_dir) = state_paths()?;
    let mut profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    if let Some(image_path) = image_path.filter(|value| !value.trim().is_empty()) {
        let previous_path = profile.avatar_path.clone();
        let (bytes, hash) = create_avatar(Path::new(&image_path))?;
        fs::create_dir_all(&avatars_dir).map_err(|error| error.to_string())?;
        let target = avatars_dir.join(format!("self-{hash}.jpg"));
        fs::write(&target, bytes).map_err(|error| format!("保存头像失败：{error}"))?;
        profile.avatar_hash = Some(hash);
        profile.avatar_path = Some(target.to_string_lossy().to_string());
        if previous_path.as_deref() != profile.avatar_path.as_deref() {
            if let Some(previous_path) = previous_path {
                let _ = fs::remove_file(previous_path);
            }
        }
    } else {
        if let Some(path) = profile.avatar_path.as_deref() {
            let _ = fs::remove_file(path);
        }
        profile.avatar_hash = None;
        profile.avatar_path = None;
    }
    profile.profile_revision = profile.profile_revision.saturating_add(1);
    open_database(&db_path)?
        .execute(
            "UPDATE lan_profile SET avatar_hash=?1,avatar_path=?2,profile_revision=?3,updated_at=?4 WHERE singleton=1",
            params![profile.avatar_hash, profile.avatar_path, profile.profile_revision as i64, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    {
        let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        state.profile = Some(profile.clone());
        state.announce_generation = state.announce_generation.wrapping_add(1);
    }
    let _ = app_handle.emit("pm-center:lan-profile-changed", &profile);
    Ok(profile)
}

#[tauri::command]
pub async fn start_lan_discovery(app_handle: tauri::AppHandle) -> Result<(), String> {
    let (start_udp, start_tcp, db_path) = {
        let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        let db_path = state
            .db_path
            .clone()
            .ok_or_else(|| "局域网协同尚未初始化".to_string())?;
        (
            !state.discovery_worker_started,
            !state.tcp_server_started,
            db_path,
        )
    };
    let udp_socket = if start_udp {
        let socket = UdpSocket::bind(format!("0.0.0.0:{DISCOVERY_PORT}"))
            .await
            .map_err(|error| format!("无法绑定局域网发现端口 {DISCOVERY_PORT}：{error}"))?;
        socket
            .set_broadcast(true)
            .map_err(|error| format!("无法启用局域网广播：{error}"))?;
        Some(socket)
    } else {
        None
    };
    let tcp_listener = if start_tcp {
        Some(
            TcpListener::bind(format!("0.0.0.0:{MESSAGE_PORT}"))
                .await
                .map_err(|error| format!("无法绑定局域网消息端口 {MESSAGE_PORT}：{error}"))?,
        )
    } else {
        None
    };
    {
        let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        state.is_running = true;
        state.service.is_running = true;
        state.service.last_error = None;
        state.announce_generation = state.announce_generation.wrapping_add(1);
        if udp_socket.is_some() {
            state.discovery_worker_started = true;
            state.service.udp_bound = true;
        }
        if tcp_listener.is_some() {
            state.tcp_server_started = true;
            state.service.tcp_bound = true;
        }
    }
    if let Some(socket) = udp_socket {
        tokio::spawn(run_discovery_loop(
            socket,
            app_handle.clone(),
            db_path.clone(),
        ));
    }
    if let Some(listener) = tcp_listener {
        tokio::spawn(run_tcp_server(listener, app_handle.clone(), db_path));
    }
    emit_service_status(&app_handle);
    Ok(())
}

#[tauri::command]
pub async fn stop_lan_discovery(app_handle: tauri::AppHandle) -> Result<(), String> {
    {
        let mut state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        state.is_running = false;
        state.online_users.clear();
        state.service.is_running = false;
        state.service.online_count = 0;
    }
    emit_contacts_changed(&app_handle);
    emit_service_status(&app_handle);
    Ok(())
}

async fn deliver_message(message: &P2PMessage) -> Result<P2PDeliveryResult, String> {
    let targets: Vec<(String, String, String)> = {
        let state = P2P_GLOBAL.lock().map_err(|error| error.to_string())?;
        if let Some(to_id) = &message.to_id {
            state
                .online_users
                .get(to_id)
                .map(|peer| vec![(peer.id.clone(), peer.display_name.clone(), peer.ip.clone())])
                .ok_or_else(|| "联系人当前离线，无法发送私聊".to_string())?
        } else {
            state
                .online_users
                .values()
                .filter(|peer| peer.id != message.from_id)
                .map(|peer| (peer.id.clone(), peer.display_name.clone(), peer.ip.clone()))
                .collect()
        }
    };
    if targets.is_empty() {
        if message.to_id.is_some() {
            return Err("联系人当前离线，无法发送私聊".to_string());
        }
        return Ok(P2PDeliveryResult {
            target_count: 0,
            delivered_count: 0,
            failures: Vec::new(),
        });
    }
    let target_count = targets.len();
    let mut delivered_count = 0usize;
    let mut failures = Vec::new();
    for (user_id, user_name, ip) in targets {
        match send_tcp_message(&ip, message).await {
            Ok(()) => delivered_count += 1,
            Err(error) => failures.push(P2PDeliveryFailure {
                user_id,
                user_name,
                error,
            }),
        }
    }
    Ok(P2PDeliveryResult {
        target_count,
        delivered_count,
        failures,
    })
}

async fn send_and_store_message(
    app_handle: tauri::AppHandle,
    message: P2PMessage,
) -> Result<P2PDeliveryResult, String> {
    if message.content.trim().is_empty() {
        return Err("消息不能为空".to_string());
    }
    if message.content.as_bytes().len() > MAX_MESSAGE_BYTES {
        return Err("消息过长，单条消息不能超过 16 KB".to_string());
    }
    let local_profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let (db_path, _) = state_paths()?;
    let conn = open_database(&db_path)?;
    let is_lobby = message.to_id.is_none();
    if is_lobby
        && insert_message(
            &conn,
            &message,
            &local_profile.id,
            "outgoing",
            "stored",
            None,
        )?
    {
        let stored = wire_to_lan_message(&message, &local_profile.id, "outgoing", "stored", None);
        let _ = app_handle.emit("pm-center:lan-message", &stored);
    }
    let result = deliver_message(&message).await?;
    let status = if result.target_count == 0 {
        "stored"
    } else if result.delivered_count == result.target_count {
        "delivered"
    } else if result.delivered_count > 0 {
        "partial"
    } else {
        "failed"
    };
    let summary = if result.failures.is_empty() {
        None
    } else {
        Some(
            result
                .failures
                .iter()
                .map(|failure| format!("{}：{}", failure.user_name, failure.error))
                .collect::<Vec<_>>()
                .join("；"),
        )
    };
    if is_lobby {
        conn.execute(
            "UPDATE lan_messages SET delivery_status=?2,delivery_summary=?3 WHERE id=?1",
            params![message.id, status, summary.as_deref()],
        )
        .map_err(|error| error.to_string())?;
        let lan_message =
            wire_to_lan_message(&message, &local_profile.id, "outgoing", status, summary);
        let _ = app_handle.emit("pm-center:lan-message", &lan_message);
    } else if insert_message(
        &conn,
        &message,
        &local_profile.id,
        "outgoing",
        status,
        summary.as_deref(),
    )? {
        let lan_message =
            wire_to_lan_message(&message, &local_profile.id, "outgoing", status, summary);
        let _ = app_handle.emit("pm-center:lan-message", &lan_message);
    }
    Ok(result)
}

#[tauri::command]
pub async fn send_lan_message(
    app_handle: tauri::AppHandle,
    request: SendLanMessageRequest,
) -> Result<LanDeliveryResult, String> {
    let profile = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "局域网身份尚未初始化".to_string())?;
    let message = P2PMessage {
        id: uuid::Uuid::new_v4().to_string(),
        from_id: profile.id,
        from_name: profile.display_name,
        to_id: request.to_id,
        content: request.content.trim().to_string(),
        timestamp: now_millis(),
        protocol_version: Some(PROTOCOL_VERSION),
    };
    send_and_store_message(app_handle, message).await
}

#[tauri::command]
pub async fn mark_lan_conversation_read(
    app_handle: tauri::AppHandle,
    conversation_id: String,
) -> Result<(), String> {
    let (db_path, _) = state_paths()?;
    open_database(&db_path)?
        .execute(
            "INSERT INTO lan_conversation_reads(conversation_id,last_read_at) VALUES(?1,?2)
             ON CONFLICT(conversation_id) DO UPDATE SET last_read_at=excluded.last_read_at",
            params![conversation_id, now_millis() as i64],
        )
        .map_err(|error| error.to_string())?;
    let _ = app_handle.emit(
        "pm-center:lan-read-state-changed",
        serde_json::json!({ "conversationId": conversation_id }),
    );
    Ok(())
}

#[tauri::command]
pub async fn clear_lan_conversation(
    app_handle: tauri::AppHandle,
    conversation_id: String,
) -> Result<(), String> {
    let (db_path, _) = state_paths()?;
    let mut conn = open_database(&db_path)?;
    let active_transfers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM lan_transfers
             WHERE conversation_id=?1 AND status IN ('waiting','pending','transferring')",
            params![conversation_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_transfers > 0 {
        return Err("当前会话仍有待处理或正在传输的文件，处理完成后再清理记录".to_string());
    }
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM lan_messages WHERE conversation_id=?1",
            params![conversation_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM lan_transfers WHERE conversation_id=?1",
            params![conversation_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM lan_conversation_reads WHERE conversation_id=?1",
            params![conversation_id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    if conversation_id == "lobby" {
        transfer::clear_lobby_cache(&app_handle)?;
    }
    let _ = app_handle.emit("pm-center:lan-read-state-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn clear_lan_history(app_handle: tauri::AppHandle) -> Result<(), String> {
    let (db_path, _) = state_paths()?;
    let conn = open_database(&db_path)?;
    let active_transfers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM lan_transfers
             WHERE status IN ('waiting','pending','transferring')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_transfers > 0 {
        return Err("仍有待处理或正在传输的文件，处理完成后再清理全部记录".to_string());
    }
    conn.execute_batch(
        "DELETE FROM lan_messages;
             DELETE FROM lan_transfers;
             DELETE FROM lan_conversation_reads;",
    )
    .map_err(|error| error.to_string())?;
    transfer::clear_lobby_cache(&app_handle)?;
    let _ = app_handle.emit("pm-center:lan-read-state-changed", ());
    Ok(())
}

fn cleanup_orphan_avatar(conn: &Connection, path: Option<String>) {
    let Some(path) = path else {
        return;
    };
    let references: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM lan_contacts WHERE avatar_path=?1",
            params![path],
            |row| row.get(0),
        )
        .unwrap_or(1);
    if references == 0 {
        let _ = fs::remove_file(path);
    }
}

#[tauri::command]
pub async fn remove_lan_contact(
    app_handle: tauri::AppHandle,
    contact_id: String,
) -> Result<(), String> {
    let is_online = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .online_users
        .contains_key(&contact_id);
    if is_online {
        return Err("在线联系人不能删除；停止发现或等待对方离线后再试".to_string());
    }
    let (db_path, _) = state_paths()?;
    let conn = open_database(&db_path)?;
    let avatar_path: Option<String> = conn
        .query_row(
            "SELECT avatar_path FROM lan_contacts WHERE id=?1",
            params![contact_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    conn.execute("DELETE FROM lan_contacts WHERE id=?1", params![contact_id])
        .map_err(|error| error.to_string())?;
    cleanup_orphan_avatar(&conn, avatar_path);
    emit_contacts_changed(&app_handle);
    Ok(())
}

// Legacy Tauri command wrappers retained for the existing UI and older callers.
#[tauri::command]
pub async fn init_p2p(
    app_handle: tauri::AppHandle,
    user_id: String,
    user_name: String,
) -> Result<(), String> {
    initialize_lan_collaboration(
        app_handle,
        Some(LegacyLanProfile {
            user_id: Some(user_id),
            user_name: Some(user_name),
        }),
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub async fn update_p2p_user(
    app_handle: tauri::AppHandle,
    user_id: String,
    user_name: String,
) -> Result<(), String> {
    let _ = user_id;
    let department = P2P_GLOBAL
        .lock()
        .map_err(|error| error.to_string())?
        .profile
        .as_ref()
        .map(|profile| profile.department.clone())
        .unwrap_or_default();
    update_lan_profile(
        app_handle,
        UpdateLanProfileRequest {
            display_name: user_name,
            department,
        },
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub async fn start_p2p_discovery(app_handle: tauri::AppHandle) -> Result<(), String> {
    start_lan_discovery(app_handle).await
}

#[tauri::command]
pub async fn stop_p2p_discovery(app_handle: tauri::AppHandle) -> Result<(), String> {
    stop_lan_discovery(app_handle).await
}

#[tauri::command]
pub async fn send_p2p_message(
    app_handle: tauri::AppHandle,
    message: P2PMessage,
) -> Result<P2PDeliveryResult, String> {
    send_and_store_message(app_handle, message).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pm-center-lan-{name}-{}.db", uuid::Uuid::new_v4()))
    }

    #[test]
    fn random_name_is_stable_and_not_anonymous() {
        let first = random_display_name("fixed-id");
        assert_eq!(first, random_display_name("fixed-id"));
        assert_ne!(first, "匿名用户");
    }

    #[test]
    fn legacy_discovery_packet_is_accepted() {
        let packet: DiscoveryPacket =
            serde_json::from_str(r#"{"Announce":{"user_id":"peer","user_name":"旧设备"}}"#)
                .unwrap();
        let (fields, needs_response) = packet.fields();
        assert!(needs_response);
        assert_eq!(fields.protocol_version, None);
        assert_eq!(fields.user_name, "旧设备");
    }

    #[test]
    fn messages_are_deduplicated_and_pruned() {
        let path = temp_db("messages");
        let conn = open_database(&path).unwrap();
        let message = P2PMessage {
            id: "message-1".to_string(),
            from_id: "peer".to_string(),
            from_name: "Peer".to_string(),
            to_id: None,
            content: "hello".to_string(),
            timestamp: now_millis().saturating_sub(MESSAGE_RETENTION_MS + 1),
            protocol_version: None,
        };
        assert!(insert_message(&conn, &message, "self", "incoming", "delivered", None).unwrap());
        assert!(!insert_message(&conn, &message, "self", "incoming", "delivered", None).unwrap());
        prune_old_messages(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM lan_messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn lobby_history_returns_only_the_latest_thirty_messages() {
        let path = temp_db("lobby-history");
        let conn = open_database(&path).unwrap();
        for index in 0..35u64 {
            let message = P2PMessage {
                id: format!("message-{index:02}"),
                from_id: "peer".to_string(),
                from_name: "Peer".to_string(),
                to_id: None,
                content: format!("message {index}"),
                timestamp: index + 1,
                protocol_version: Some(PROTOCOL_VERSION),
            };
            insert_message(&conn, &message, "self", "incoming", "delivered", None).unwrap();
        }
        let messages = recent_lobby_messages(&conn).unwrap();
        assert_eq!(messages.len(), LOBBY_HISTORY_LIMIT);
        assert_eq!(messages.first().unwrap().id, "message-34");
        assert_eq!(messages.last().unwrap().id, "message-05");
        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn local_receive_settings_are_initialized_once() {
        let path = temp_db("local-settings");
        let receive_directory = path.with_extension("received");
        let conn = open_database(&path).unwrap();
        ensure_local_settings(&conn, &receive_directory).unwrap();
        ensure_local_settings(&conn, &path.with_extension("other")).unwrap();
        let settings = load_local_settings(&conn).unwrap();
        assert_eq!(PathBuf::from(settings.receive_directory), receive_directory);
        assert!(settings.auto_receive_images);
        assert!(settings.discovery_subnet.is_empty());
        drop(conn);
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(receive_directory);
    }

    #[test]
    fn discovery_subnet_is_normalized_and_excludes_reserved_addresses() {
        let subnet = parse_discovery_subnet("10.13.13.8/24").unwrap();
        assert_eq!(subnet.normalized(), "10.13.13.0/24");
        let local = HashSet::from([Ipv4Addr::new(10, 13, 13, 8)]);
        let targets = subnet.targets(&local);
        assert_eq!(targets.len(), 253);
        assert_eq!(targets.first(), Some(&Ipv4Addr::new(10, 13, 13, 1)));
        assert_eq!(targets.last(), Some(&Ipv4Addr::new(10, 13, 13, 254)));
        assert!(!targets.contains(&Ipv4Addr::new(10, 13, 13, 8)));
        assert!(!targets.contains(&Ipv4Addr::new(10, 13, 13, 255)));
    }

    #[test]
    fn discovery_subnet_rejects_sweeps_larger_than_a_class_c() {
        assert!(parse_discovery_subnet("10.13.0.0/16").is_err());
        assert!(parse_discovery_subnet("10.13.13.0/31").is_err());
    }

    #[test]
    fn existing_local_settings_table_is_migrated() {
        let path = temp_db("local-settings-migration");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE lan_local_settings (
               singleton INTEGER PRIMARY KEY CHECK(singleton=1),
               receive_directory TEXT NOT NULL,
               auto_receive_images INTEGER NOT NULL DEFAULT 1,
               updated_at INTEGER NOT NULL
             );
             INSERT INTO lan_local_settings(singleton,receive_directory,auto_receive_images,updated_at)
             VALUES(1,'C:\\Temp',1,0);",
        )
        .unwrap();
        drop(conn);
        let conn = open_database(&path).unwrap();
        let settings = load_local_settings(&conn).unwrap();
        assert!(settings.discovery_subnet.is_empty());
        drop(conn);
        let _ = fs::remove_file(path);
    }
}
