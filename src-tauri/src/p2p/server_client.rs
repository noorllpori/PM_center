use super::transfer::{
    self, CancelLanTransferRequest, LanFileOfferResult, LanTransfer, LanTransferFailure,
    LanTransferProgress, OfferLanFilesRequest, RespondLanTransferRequest,
};
use super::{
    now_millis, open_database, state_paths, LanContact, LanConversation, LanDeliveryResult,
    LanMessage, P2PDeliveryFailure, P2P_GLOBAL,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use keyring::Entry;
use pmc_protocol::{
    ClientMessage, DeviceProfile, RelayMessage, ServerInfo, ServerMessage, SpeedTestResult,
    PROTOCOL_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::Emitter;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::io::StreamReader;
use url::Url;

const CREDENTIAL_SERVICE: &str = "com.httplink.pmcenter.server";
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmcServerSettings {
    pub address: String,
    pub enabled: bool,
    pub server_id: Option<String>,
    pub password_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmcServerStatus {
    pub connected: bool,
    pub connecting: bool,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub protocol_version: Option<u16>,
    pub requires_password: bool,
    pub insecure_http: bool,
    pub online_count: usize,
    pub latency_ms: Option<f64>,
    pub last_error: Option<String>,
}

impl Default for PmcServerStatus {
    fn default() -> Self {
        Self {
            connected: false,
            connecting: false,
            server_id: None,
            server_name: None,
            protocol_version: None,
            requires_password: false,
            insecure_http: false,
            online_count: 0,
            latency_ms: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePmcServerSettingsRequest {
    pub address: String,
    pub enabled: bool,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendServerMessageRequest {
    pub to_id: Option<String>,
    pub content: String,
}

#[derive(Default)]
struct RuntimeState {
    status: PmcServerStatus,
    outbound: Option<mpsc::UnboundedSender<ClientMessage>>,
    cancel: Option<watch::Sender<bool>>,
    online_ids: HashSet<String>,
    speed_test: Option<SpeedTestResult>,
    transfer_cancellations: HashMap<String, watch::Sender<bool>>,
    transfer_started: HashMap<String, Instant>,
}

lazy_static::lazy_static! {
    static ref SERVER_RUNTIME: Mutex<RuntimeState> = Mutex::new(RuntimeState::default());
}

fn emit_changed(app_handle: &tauri::AppHandle) {
    let _ = app_handle.emit("pm-center:server-collaboration-changed", ());
}

fn credential_key(address: &str, suffix: &str) -> String {
    format!("{}:{suffix}", blake3::hash(address.as_bytes()).to_hex())
}

fn read_secret(address: &str, suffix: &str) -> Option<String> {
    Entry::new(CREDENTIAL_SERVICE, &credential_key(address, suffix))
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

fn write_secret(address: &str, suffix: &str, value: &str) -> Result<(), String> {
    let entry = Entry::new(CREDENTIAL_SERVICE, &credential_key(address, suffix))
        .map_err(|error| error.to_string())?;
    if value.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    } else {
        entry.set_password(value).map_err(|error| error.to_string())
    }
}

fn device_credential_scope(server_id: &str) -> String {
    format!("server:{server_id}")
}

fn read_device_credential(address: &str, server_id: &str, device_id: &str) -> Option<String> {
    let suffix = format!("credential:{device_id}");
    read_secret(&device_credential_scope(server_id), &suffix)
        .or_else(|| read_secret(address, &suffix))
}

fn write_device_credential(
    server_id: &str,
    device_id: &str,
    credential: &str,
) -> Result<(), String> {
    write_secret(
        &device_credential_scope(server_id),
        &format!("credential:{device_id}"),
        credential,
    )
}

struct SpeedTestCleanup {
    client: reqwest::Client,
    url: String,
    device_id: String,
    credential: String,
    test_id: String,
}

impl Drop for SpeedTestCleanup {
    fn drop(&mut self) {
        let client = self.client.clone();
        let url = self.url.clone();
        let device_id = self.device_id.clone();
        let credential = self.credential.clone();
        let test_id = self.test_id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = client
                .delete(url)
                .header("x-pmc-device-id", device_id)
                .header("x-pmc-device-credential", credential)
                .header("x-pmc-speed-test-id", test_id)
                .send()
                .await;
        });
    }
}

fn normalize_address(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let candidate = if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    };
    let mut url = Url::parse(&candidate).map_err(|error| format!("服务器地址无效：{error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("服务器地址只支持 HTTP 或 HTTPS".to_string());
    }
    if url.host_str().is_none() {
        return Err("服务器地址缺少主机名或 IP".to_string());
    }
    if url.port().is_none() {
        url.set_port(Some(if url.scheme() == "https" { 443 } else { 7412 }))
            .map_err(|_| "无法设置服务器端口".to_string())?;
    }
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string().trim_end_matches('/').to_string())
}

pub(super) fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pmc_server_settings(
           singleton INTEGER PRIMARY KEY CHECK(singleton=1),
           address TEXT NOT NULL DEFAULT '',
           enabled INTEGER NOT NULL DEFAULT 0,
           server_id TEXT,
           updated_at INTEGER NOT NULL
         );
         INSERT OR IGNORE INTO pmc_server_settings(singleton,address,enabled,updated_at) VALUES(1,'',0,0);
         CREATE TABLE IF NOT EXISTS pmc_server_contacts(
           device_id TEXT PRIMARY KEY,
           display_name TEXT NOT NULL,
           department TEXT NOT NULL DEFAULT '',
           avatar_hash TEXT,
           avatar_path TEXT,
           profile_revision INTEGER NOT NULL DEFAULT 1,
           last_seen INTEGER NOT NULL,
           server_id TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS pmc_server_messages(
           id TEXT PRIMARY KEY,
           conversation_id TEXT NOT NULL,
           from_id TEXT NOT NULL,
           from_name TEXT NOT NULL,
           to_id TEXT,
           content TEXT NOT NULL,
           timestamp INTEGER NOT NULL,
           direction TEXT NOT NULL,
           delivery_status TEXT NOT NULL,
           delivery_summary TEXT,
           server_id TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_pmc_server_messages_conversation_time
           ON pmc_server_messages(conversation_id,timestamp);
         CREATE TABLE IF NOT EXISTS pmc_server_transfers(
           id TEXT PRIMARY KEY,
           conversation_id TEXT NOT NULL,
           kind TEXT NOT NULL,
           from_id TEXT NOT NULL,
           from_name TEXT NOT NULL,
           provider_id TEXT NOT NULL,
           provider_name TEXT NOT NULL,
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
           updated_at INTEGER NOT NULL,
           server_id TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_pmc_server_transfers_conversation_time
           ON pmc_server_transfers(conversation_id,created_at);",
    )
    .map_err(|error| error.to_string())
}

pub(super) fn recover_interrupted(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE pmc_server_transfers SET status='failed',error='应用退出导致传输中断，请重新发起',updated_at=?1 WHERE status IN ('waiting','pending','transferring')",
        params![now_millis() as i64],
    ).map_err(|error|error.to_string())?;
    Ok(())
}

pub(super) fn settings(conn: &Connection) -> Result<PmcServerSettings, String> {
    let (address, enabled, server_id) = conn
        .query_row(
            "SELECT address,enabled,server_id FROM pmc_server_settings WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? != 0,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(PmcServerSettings {
        password_configured: !address.is_empty() && read_secret(&address, "password").is_some(),
        address,
        enabled,
        server_id,
    })
}

pub(super) fn status() -> PmcServerStatus {
    SERVER_RUNTIME
        .lock()
        .map(|state| state.status.clone())
        .unwrap_or_default()
}

pub(super) fn speed_test_result() -> Option<SpeedTestResult> {
    SERVER_RUNTIME
        .lock()
        .ok()
        .and_then(|state| state.speed_test.clone())
}

pub(super) fn is_contact_online(contact_id: &str) -> bool {
    SERVER_RUNTIME
        .lock()
        .map(|runtime| runtime.online_ids.contains(contact_id))
        .unwrap_or(false)
}

pub(super) fn initialize(app_handle: tauri::AppHandle, conn: &Connection) -> Result<(), String> {
    let current = settings(conn)?;
    if current.enabled && !current.address.is_empty() {
        restart_connection(app_handle, current.address)?;
    }
    Ok(())
}

pub(super) fn refresh_profile(app_handle: tauri::AppHandle) {
    let current = state_paths()
        .ok()
        .and_then(|(db_path, _)| open_database(&db_path).ok())
        .and_then(|conn| settings(&conn).ok());
    if let Some(current) =
        current.filter(|settings| settings.enabled && !settings.address.is_empty())
    {
        let _ = restart_connection(app_handle, current.address);
    }
}

fn restart_connection(app_handle: tauri::AppHandle, address: String) -> Result<(), String> {
    let cancel_rx = {
        let mut runtime = SERVER_RUNTIME.lock().map_err(|error| error.to_string())?;
        if let Some(cancel) = runtime.cancel.take() {
            let _ = cancel.send(true);
        }
        runtime.outbound = None;
        runtime.online_ids.clear();
        runtime.status.connected = false;
        runtime.status.connecting = true;
        runtime.status.last_error = None;
        let (cancel, cancel_rx) = watch::channel(false);
        runtime.cancel = Some(cancel);
        cancel_rx
    };
    emit_changed(&app_handle);
    tauri::async_runtime::spawn(connection_loop(app_handle, address, cancel_rx));
    Ok(())
}

async fn connection_loop(
    app_handle: tauri::AppHandle,
    address: String,
    mut cancel: watch::Receiver<bool>,
) {
    loop {
        if *cancel.borrow() {
            return;
        }
        let result = connect_once(app_handle.clone(), &address, &mut cancel).await;
        if *cancel.borrow() {
            return;
        }
        let error = result.err();
        let pause_reconnect = error
            .as_deref()
            .is_some_and(should_pause_automatic_reconnect);
        if let Ok(mut runtime) = SERVER_RUNTIME.lock() {
            for (_, cancellation) in runtime.transfer_cancellations.drain() {
                let _ = cancellation.send(true);
            }
            runtime.transfer_started.clear();
            runtime.status.connected = false;
            runtime.status.connecting = false;
            runtime.status.online_count = 0;
            runtime.status.last_error = error;
            runtime.outbound = None;
            runtime.online_ids.clear();
        }
        emit_changed(&app_handle);
        if pause_reconnect {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(RECONNECT_DELAY) => {
                if let Ok(mut runtime) = SERVER_RUNTIME.lock() { runtime.status.connecting = true; }
                emit_changed(&app_handle);
            }
            _ = cancel.changed() => return,
        }
    }
}

fn should_pause_automatic_reconnect(error: &str) -> bool {
    error.contains("共享密码错误")
        || error.contains("设备凭据")
        || error.contains("拒绝冒用")
        || error.contains("429 Too Many Requests")
}

async fn connect_once(
    app_handle: tauri::AppHandle,
    address: &str,
    cancel: &mut watch::Receiver<bool>,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let info_started = Instant::now();
    let info_response = client
        .get(format!("{address}/api/v1/info"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !info_response.status().is_success() {
        return Err(format!(
            "服务器信息请求失败：HTTP {}",
            info_response.status()
        ));
    }
    let info: ServerInfo =
        serde_json::from_str(&info_response.text().await.map_err(|e| e.to_string())?)
            .map_err(|error| format!("服务器信息格式无效：{error}"))?;
    if info.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "协议版本不兼容：客户端 {PROTOCOL_VERSION}，服务器 {}",
            info.protocol_version
        ));
    }
    let profile = P2P_GLOBAL
        .lock()
        .map_err(|e| e.to_string())?
        .profile
        .clone()
        .ok_or_else(|| "设备身份尚未初始化".to_string())?;
    let avatar_base64 = profile
        .avatar_path
        .as_deref()
        .and_then(|path| fs::read(path).ok())
        .map(|bytes| BASE64.encode(bytes));
    let credential = read_device_credential(address, &info.server_id, &profile.id);
    let password = read_secret(address, "password");
    let mut ws_url = Url::parse(address).map_err(|e| e.to_string())?;
    ws_url
        .set_scheme(if ws_url.scheme() == "https" {
            "wss"
        } else {
            "ws"
        })
        .map_err(|_| "无法生成 WebSocket 地址".to_string())?;
    ws_url.set_path("/api/v1/ws");
    let (socket, _) = tokio_tungstenite::connect_async(ws_url.as_str())
        .await
        .map_err(|e| e.to_string())?;
    let (mut writer, mut reader) = socket.split();
    let registration = ClientMessage::Register {
        device_id: profile.id.clone(),
        display_name: profile.display_name.clone(),
        department: profile.department.clone(),
        avatar_base64,
        avatar_hash: profile.avatar_hash.clone(),
        profile_revision: profile.profile_revision,
        credential,
        shared_password: password,
    };
    writer
        .send(Message::Text(
            serde_json::to_string(&registration)
                .map_err(|e| e.to_string())?
                .into(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    let first = tokio::time::timeout(Duration::from_secs(10), reader.next())
        .await
        .map_err(|_| "服务器注册超时".to_string())?
        .ok_or_else(|| "服务器在注册前断开".to_string())?
        .map_err(|e| e.to_string())?;
    let Message::Text(first) = first else {
        return Err("服务器注册响应无效".to_string());
    };
    let first: ServerMessage = serde_json::from_str(&first).map_err(|e| e.to_string())?;
    let (credential, devices, history) = match first {
        ServerMessage::Registered {
            credential,
            devices,
            history,
            ..
        } => (credential, devices, history),
        ServerMessage::Error { message, .. } => return Err(message),
        _ => return Err("服务器没有返回注册确认".to_string()),
    };
    write_device_credential(&info.server_id, &profile.id, &credential)?;
    persist_server_identity(&info)?;
    persist_devices(&devices, &info.server_id)?;
    persist_history(&history, &profile.id, &info.server_id)?;
    let (outbound, mut outbound_rx) = mpsc::unbounded_channel();
    {
        let mut runtime = SERVER_RUNTIME.lock().map_err(|e| e.to_string())?;
        runtime.outbound = Some(outbound);
        runtime.online_ids = devices
            .iter()
            .filter(|device| device.online)
            .map(|device| device.device_id.clone())
            .collect();
        runtime.status = PmcServerStatus {
            connected: true,
            connecting: false,
            server_id: Some(info.server_id.clone()),
            server_name: Some(info.name.clone()),
            protocol_version: Some(info.protocol_version),
            requires_password: info.requires_password,
            insecure_http: address.starts_with("http://"),
            online_count: devices
                .iter()
                .filter(|device| device.online && device.device_id != profile.id)
                .count(),
            latency_ms: Some(info_started.elapsed().as_secs_f64() * 1000.0),
            last_error: None,
        };
    }
    emit_changed(&app_handle);
    loop {
        tokio::select! {
            _ = cancel.changed() => return Ok(()),
            outgoing = outbound_rx.recv() => {
                let Some(outgoing) = outgoing else { return Ok(()); };
                writer.send(Message::Text(serde_json::to_string(&outgoing).map_err(|e| e.to_string())?.into())).await.map_err(|e| e.to_string())?;
            }
            incoming = reader.next() => {
                let Some(incoming) = incoming else { return Err("服务器连接已关闭".to_string()); };
                match incoming.map_err(|e| e.to_string())? {
                    Message::Text(text) => {
                        if let Ok(event) = serde_json::from_str::<ServerMessage>(&text) {
                            handle_server_event(&app_handle, address, &profile.id, &info.server_id, event).await?;
                        }
                    }
                    Message::Close(_) => return Err("服务器连接已关闭".to_string()),
                    _ => {}
                }
            }
        }
    }
}

fn persist_server_identity(info: &ServerInfo) -> Result<(), String> {
    let db_path = state_paths()?.0;
    open_database(&db_path)?
        .execute(
            "UPDATE pmc_server_settings SET server_id=?1,updated_at=?2 WHERE singleton=1",
            params![info.server_id, now_millis() as i64],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn save_avatar(device: &DeviceProfile) -> Option<String> {
    let encoded = device.avatar_base64.as_deref()?;
    let bytes = BASE64.decode(encoded).ok()?;
    let (_, avatars_dir) = state_paths().ok()?;
    fs::create_dir_all(&avatars_dir).ok()?;
    let hash = device
        .avatar_hash
        .clone()
        .unwrap_or_else(|| blake3::hash(&bytes).to_hex().to_string());
    let path = avatars_dir.join(format!("server-{}-{hash}.jpg", device.device_id));
    fs::write(&path, bytes).ok()?;
    Some(path.to_string_lossy().to_string())
}

fn persist_devices(devices: &[DeviceProfile], server_id: &str) -> Result<(), String> {
    let db_path = state_paths()?.0;
    let conn = open_database(&db_path)?;
    for device in devices {
        let previous_path = conn
            .query_row(
                "SELECT avatar_path FROM pmc_server_contacts WHERE device_id=?1",
                params![device.device_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();
        let avatar_path = if device.avatar_base64.is_some() {
            save_avatar(device)
        } else {
            previous_path
        };
        conn.execute(
            "INSERT INTO pmc_server_contacts(device_id,display_name,department,avatar_hash,avatar_path,profile_revision,last_seen,server_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(device_id) DO UPDATE SET display_name=excluded.display_name,department=excluded.department,
               avatar_hash=excluded.avatar_hash,avatar_path=excluded.avatar_path,profile_revision=excluded.profile_revision,
               last_seen=excluded.last_seen,server_id=excluded.server_id",
            params![device.device_id,device.display_name,device.department,device.avatar_hash,avatar_path,
                device.profile_revision as i64,device.last_seen as i64,server_id],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn normalized_conversation(message: &RelayMessage, local_id: &str, server_id: &str) -> String {
    match message.to_id.as_deref() {
        None => format!("server:{server_id}:lobby"),
        Some(to_id) if message.from_id == local_id => format!("direct:{to_id}"),
        Some(_) => format!("direct:{}", message.from_id),
    }
}

fn persist_message(
    message: &RelayMessage,
    local_id: &str,
    server_id: &str,
    status: &str,
) -> Result<(), String> {
    let db_path = state_paths()?.0;
    open_database(&db_path)?.execute(
        "INSERT OR REPLACE INTO pmc_server_messages(id,conversation_id,from_id,from_name,to_id,content,timestamp,direction,delivery_status,delivery_summary,server_id)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,NULL,?10)",
        params![message.id,normalized_conversation(message,local_id,server_id),message.from_id,message.from_name,
            message.to_id,message.content,message.timestamp as i64,if message.from_id == local_id {"outgoing"} else {"incoming"},status,server_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

fn persist_history(
    messages: &[RelayMessage],
    local_id: &str,
    server_id: &str,
) -> Result<(), String> {
    for message in messages {
        persist_message(message, local_id, server_id, "delivered")?;
    }
    Ok(())
}

fn insert_server_transfer(conn: &Connection, transfer: &LanTransfer) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO pmc_server_transfers(
           id,conversation_id,kind,from_id,from_name,provider_id,provider_name,to_id,display_name,item_count,total_bytes,
           mime_type,content_hash,payload_format,manifest_json,status,direction,source_path,received_path,error,created_at,updated_at,server_id
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
        params![transfer.id,transfer.conversation_id,transfer.kind,transfer.from_id,transfer.from_name,
            transfer.provider_id,transfer.provider_name,transfer.to_id,transfer.display_name,transfer.item_count as i64,
            transfer.total_bytes as i64,transfer.mime_type,transfer.content_hash,transfer.payload_format,
            serde_json::to_string(&transfer.manifest).map_err(|e|e.to_string())?,transfer.status,transfer.direction,
            transfer.source_path,transfer.received_path,transfer.error,transfer.created_at as i64,transfer.updated_at as i64,
            transfer.server_id],
    ).map_err(|e|e.to_string())?;
    Ok(())
}

fn parse_server_transfer(row: &rusqlite::Row<'_>) -> rusqlite::Result<LanTransfer> {
    let manifest_json: String = row.get(14)?;
    Ok(LanTransfer {
        id: row.get(0)?,
        lobby_item_id: None,
        conversation_id: row.get(1)?,
        kind: row.get(2)?,
        from_id: row.get(3)?,
        from_name: row.get(4)?,
        provider_id: row.get(5)?,
        provider_name: row.get(6)?,
        to_id: row.get(7)?,
        display_name: row.get(8)?,
        item_count: row.get::<_, i64>(9)? as u64,
        total_bytes: row.get::<_, i64>(10)? as u64,
        mime_type: row.get(11)?,
        content_hash: row.get(12)?,
        payload_format: row.get(13)?,
        manifest: serde_json::from_str(&manifest_json).unwrap_or_default(),
        status: row.get(15)?,
        direction: row.get(16)?,
        source_path: row.get(17)?,
        received_path: row.get(18)?,
        error: row.get(19)?,
        created_at: row.get::<_, i64>(20)? as u64,
        updated_at: row.get::<_, i64>(21)? as u64,
        transport: "server".to_string(),
        server_id: Some(row.get(22)?),
    })
}

const SERVER_TRANSFER_COLUMNS: &str = "id,conversation_id,kind,from_id,from_name,provider_id,provider_name,to_id,display_name,item_count,total_bytes,mime_type,content_hash,payload_format,manifest_json,status,direction,source_path,received_path,error,created_at,updated_at,server_id";

fn load_server_transfer(conn: &Connection, transfer_id: &str) -> Result<LanTransfer, String> {
    conn.query_row(
        &format!("SELECT {SERVER_TRANSFER_COLUMNS} FROM pmc_server_transfers WHERE id=?1"),
        params![transfer_id],
        parse_server_transfer,
    )
    .map_err(|e| e.to_string())
}

fn update_server_transfer(
    transfer_id: &str,
    status: &str,
    error: Option<&str>,
    received_path: Option<&str>,
) -> Result<LanTransfer, String> {
    let db_path = state_paths()?.0;
    let conn = open_database(&db_path)?;
    conn.execute("UPDATE pmc_server_transfers SET status=?2,error=?3,received_path=COALESCE(?4,received_path),updated_at=?5 WHERE id=?1",
        params![transfer_id,status,error,received_path,now_millis() as i64]).map_err(|e|e.to_string())?;
    load_server_transfer(&conn, transfer_id)
}

fn offer_to_transfer(offer: pmc_protocol::FileOffer, server_id: &str) -> LanTransfer {
    LanTransfer {
        id: offer.transfer_id,
        lobby_item_id: None,
        conversation_id: format!("direct:{}", offer.from_id),
        kind: offer.kind,
        from_id: offer.from_id.clone(),
        from_name: offer.from_name.clone(),
        provider_id: offer.from_id,
        provider_name: offer.from_name,
        to_id: offer.to_id,
        display_name: offer.display_name,
        item_count: offer.item_count,
        total_bytes: offer.total_bytes,
        mime_type: offer.mime_type,
        content_hash: offer.content_hash,
        payload_format: offer.payload_format,
        manifest: offer.manifest,
        status: "pending".to_string(),
        direction: "incoming".to_string(),
        source_path: None,
        received_path: None,
        error: None,
        created_at: offer.created_at,
        updated_at: now_millis(),
        transport: "server".to_string(),
        server_id: Some(server_id.to_string()),
    }
}

fn transfer_offer(transfer: &LanTransfer) -> pmc_protocol::FileOffer {
    pmc_protocol::FileOffer {
        transfer_id: transfer.id.clone(),
        from_id: transfer.from_id.clone(),
        from_name: transfer.from_name.clone(),
        to_id: transfer.to_id.clone(),
        display_name: transfer.display_name.clone(),
        kind: transfer.kind.clone(),
        item_count: transfer.item_count,
        total_bytes: transfer.total_bytes,
        mime_type: transfer.mime_type.clone(),
        content_hash: transfer.content_hash.clone(),
        payload_format: transfer.payload_format.clone(),
        manifest: transfer.manifest.clone(),
        created_at: transfer.created_at,
    }
}

fn transfer_cancellation(transfer_id: &str) -> Result<watch::Receiver<bool>, String> {
    let mut runtime = SERVER_RUNTIME.lock().map_err(|e| e.to_string())?;
    if let Some(sender) = runtime.transfer_cancellations.get(transfer_id) {
        return Ok(sender.subscribe());
    }
    let (sender, receiver) = watch::channel(false);
    runtime
        .transfer_cancellations
        .insert(transfer_id.to_string(), sender);
    runtime
        .transfer_started
        .insert(transfer_id.to_string(), Instant::now());
    Ok(receiver)
}

fn signal_transfer_cancel(transfer_id: &str) {
    if let Ok(mut runtime) = SERVER_RUNTIME.lock() {
        if let Some(sender) = runtime.transfer_cancellations.remove(transfer_id) {
            let _ = sender.send(true);
        }
        runtime.transfer_started.remove(transfer_id);
    }
}

async fn handle_server_event(
    app_handle: &tauri::AppHandle,
    address: &str,
    local_id: &str,
    server_id: &str,
    event: ServerMessage,
) -> Result<(), String> {
    match event {
        ServerMessage::Presence { devices } => {
            persist_devices(&devices, server_id)?;
            if let Ok(mut runtime) = SERVER_RUNTIME.lock() {
                runtime.online_ids = devices
                    .iter()
                    .filter(|device| device.online)
                    .map(|device| device.device_id.clone())
                    .collect();
                runtime.status.online_count = devices
                    .iter()
                    .filter(|device| device.online && device.device_id != local_id)
                    .count();
            }
        }
        ServerMessage::Message { message } => {
            persist_message(&message, local_id, server_id, "delivered")?
        }
        ServerMessage::MessageAck { message_id } => {
            let db_path = state_paths()?.0;
            open_database(&db_path)?
                .execute(
                    "UPDATE pmc_server_messages SET delivery_status='delivered' WHERE id=?1",
                    params![message_id],
                )
                .map_err(|e| e.to_string())?;
        }
        ServerMessage::Pong { sent_at, .. } => {
            if let Ok(mut runtime) = SERVER_RUNTIME.lock() {
                runtime.status.latency_ms = Some(now_millis().saturating_sub(sent_at) as f64);
            }
        }
        ServerMessage::Error { message, .. } => {
            if let Ok(mut runtime) = SERVER_RUNTIME.lock() {
                runtime.status.last_error = Some(message);
            }
        }
        ServerMessage::FileOffer { offer } => {
            let transfer = offer_to_transfer(offer, server_id);
            let conn = open_database(&state_paths()?.0)?;
            insert_server_transfer(&conn, &transfer)?;
            if transfer
                .mime_type
                .as_deref()
                .is_some_and(|value| value.starts_with("image/"))
                && transfer.total_bytes <= 100 * 1024 * 1024
            {
                let app_handle = app_handle.clone();
                let transfer_id = transfer.id.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = respond_server_transfer(
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
        ServerMessage::FileResponse {
            transfer_id,
            accepted,
            reason,
        } => {
            if !accepted {
                let _ = update_server_transfer(&transfer_id, "rejected", reason.as_deref(), None);
            }
        }
        ServerMessage::FileDownload {
            transfer_id,
            url,
            token,
        } => {
            let app_handle = app_handle.clone();
            let address = address.to_string();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = download_server_transfer(
                    app_handle.clone(),
                    address,
                    transfer_id.clone(),
                    url,
                    token,
                )
                .await
                {
                    let _ = update_server_transfer(
                        &transfer_id,
                        if error == "传输已中断" {
                            "cancelled"
                        } else {
                            "failed"
                        },
                        Some(&error),
                        None,
                    );
                    emit_changed(&app_handle);
                }
            });
        }
        ServerMessage::FileUpload {
            transfer_id,
            url,
            token,
        } => {
            let app_handle = app_handle.clone();
            let address = address.to_string();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = upload_server_transfer(
                    app_handle.clone(),
                    address,
                    transfer_id.clone(),
                    url,
                    token,
                )
                .await
                {
                    let _ = update_server_transfer(
                        &transfer_id,
                        if error == "传输已中断" {
                            "cancelled"
                        } else {
                            "failed"
                        },
                        Some(&error),
                        None,
                    );
                    emit_changed(&app_handle);
                }
            });
        }
        ServerMessage::FileProgress {
            transfer_id,
            forwarded_bytes,
            total_bytes,
        } => {
            let started = SERVER_RUNTIME
                .lock()
                .ok()
                .and_then(|runtime| runtime.transfer_started.get(&transfer_id).copied())
                .unwrap_or_else(Instant::now);
            let speed =
                (forwarded_bytes as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
            let direction = open_database(&state_paths()?.0)
                .ok()
                .and_then(|conn| load_server_transfer(&conn, &transfer_id).ok())
                .map(|transfer| transfer.direction)
                .unwrap_or_else(|| "outgoing".to_string());
            let _ = app_handle.emit(
                "pm-center:lan-transfer-progress",
                LanTransferProgress {
                    transfer_id,
                    status: "transferring".to_string(),
                    direction,
                    transferred_bytes: forwarded_bytes,
                    total_bytes,
                    bytes_per_second: speed,
                },
            );
        }
        ServerMessage::FileComplete { transfer_id } => {
            if let Ok(conn) = open_database(&state_paths()?.0) {
                if let Ok(transfer) = load_server_transfer(&conn, &transfer_id) {
                    if transfer.direction == "outgoing" {
                        let _ = update_server_transfer(&transfer_id, "completed", None, None);
                        signal_transfer_cancel(&transfer_id);
                    }
                }
            }
        }
        ServerMessage::FileCancelled {
            transfer_id,
            reason,
        } => {
            signal_transfer_cancel(&transfer_id);
            let _ = update_server_transfer(&transfer_id, "cancelled", Some(&reason), None);
        }
        _ => {}
    }
    emit_changed(app_handle);
    Ok(())
}

fn absolute_endpoint(address: &str, endpoint: &str) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!(
            "{}{}",
            address.trim_end_matches('/'),
            if endpoint.starts_with('/') {
                endpoint.to_string()
            } else {
                format!("/{endpoint}")
            }
        )
    }
}

async fn download_server_transfer(
    app_handle: tauri::AppHandle,
    address: String,
    transfer_id: String,
    url: String,
    token: String,
) -> Result<(), String> {
    let conn = open_database(&state_paths()?.0)?;
    let transfer = load_server_transfer(&conn, &transfer_id)?;
    let destination = transfer
        .received_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "接收位置尚未准备".to_string())?;
    let mut cancellation = transfer_cancellation(&transfer_id)?;
    let response = reqwest::Client::new()
        .get(absolute_endpoint(&address, &url))
        .header("x-pmc-transfer-token", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("下载中转流失败：HTTP {}", response.status()));
    }
    let stream = response
        .bytes_stream()
        .map(|result| result.map_err(io::Error::other));
    let mut reader = StreamReader::new(stream);
    transfer::receive_server_payload(
        &app_handle,
        &transfer,
        &destination,
        &mut reader,
        &mut cancellation,
    )
    .await?;
    update_server_transfer(
        &transfer_id,
        "completed",
        None,
        Some(&destination.to_string_lossy()),
    )?;
    signal_transfer_cancel(&transfer_id);
    emit_changed(&app_handle);
    Ok(())
}

async fn upload_server_transfer(
    app_handle: tauri::AppHandle,
    address: String,
    transfer_id: String,
    url: String,
    token: String,
) -> Result<(), String> {
    let conn = open_database(&state_paths()?.0)?;
    let transfer = load_server_transfer(&conn, &transfer_id)?;
    update_server_transfer(&transfer_id, "transferring", None, None)?;
    let cancellation = transfer_cancellation(&transfer_id)?;
    let (sender, receiver) = mpsc::channel(4);
    let stream_task = tauri::async_runtime::spawn(transfer::stream_server_payload(
        transfer.clone(),
        sender,
        cancellation,
    ));
    let body_stream = ReceiverStream::new(receiver).map(Ok::<_, io::Error>);
    let response = reqwest::Client::new()
        .put(absolute_endpoint(&address, &url))
        .header("x-pmc-transfer-token", token)
        .header(reqwest::header::CONTENT_LENGTH, transfer.total_bytes)
        .body(reqwest::Body::wrap_stream(body_stream))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let stream_result = stream_task.await.map_err(|e| e.to_string())?;
    stream_result?;
    if !response.status().is_success() {
        return Err(format!("上传中转流失败：HTTP {}", response.status()));
    }
    emit_changed(&app_handle);
    Ok(())
}

pub(super) fn augment_snapshot(
    conn: &Connection,
    contacts: &mut Vec<LanContact>,
    messages: &mut Vec<LanMessage>,
    transfers: &mut Vec<LanTransfer>,
    conversations: &mut Vec<LanConversation>,
    unread_count: &mut u64,
) -> Result<(), String> {
    let (online_ids, active_server_id) = SERVER_RUNTIME
        .lock()
        .map(|runtime| (runtime.online_ids.clone(), runtime.status.server_id.clone()))
        .unwrap_or_default();
    let local_id = P2P_GLOBAL
        .lock()
        .ok()
        .and_then(|state| state.profile.as_ref().map(|profile| profile.id.clone()))
        .unwrap_or_default();
    let mut statement = conn.prepare("SELECT device_id,display_name,department,avatar_hash,avatar_path,profile_revision,last_seen,server_id FROM pmc_server_contacts ORDER BY display_name COLLATE NOCASE").map_err(|e| e.to_string())?;
    let server_contacts = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)? as u64,
                row.get::<_, i64>(6)? as u64,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, name, department, avatar_hash, avatar_path, revision, last_seen, server_id) in
        server_contacts
    {
        if id == local_id {
            continue;
        }
        let is_online =
            active_server_id.as_deref() == Some(server_id.as_str()) && online_ids.contains(&id);
        if let Some(existing) = contacts.iter_mut().find(|contact| contact.id == id) {
            existing.server_online = is_online;
            existing.online = existing.lan_online || is_online;
            existing.server_id = Some(server_id);
            if !existing.lan_online && revision >= existing.profile_revision {
                existing.display_name = name;
                existing.department = department;
                existing.avatar_hash = avatar_hash;
                existing.avatar_path = avatar_path;
                existing.profile_revision = revision;
                existing.last_seen = existing.last_seen.max(last_seen);
            }
        } else {
            contacts.push(LanContact {
                id,
                display_name: name,
                department,
                avatar_hash,
                avatar_path,
                ip: String::new(),
                online: is_online,
                lan_online: false,
                server_online: is_online,
                server_id: Some(server_id),
                first_seen: last_seen,
                last_seen,
                protocol_version: super::PROTOCOL_VERSION,
                profile_revision: revision,
            });
        }
    }
    let mut statement = conn.prepare("SELECT id,conversation_id,from_id,from_name,to_id,content,timestamp,direction,delivery_status,delivery_summary,server_id FROM (SELECT * FROM pmc_server_messages ORDER BY timestamp DESC LIMIT 5000) ORDER BY timestamp ASC").map_err(|e| e.to_string())?;
    let server_messages = statement
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
                transport: "server".to_string(),
                server_id: Some(row.get(10)?),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    messages.extend(server_messages);
    messages.sort_by_key(|message| message.timestamp);
    let mut transfer_statement=conn.prepare(&format!("SELECT {SERVER_TRANSFER_COLUMNS} FROM pmc_server_transfers ORDER BY created_at ASC LIMIT 5000")).map_err(|e|e.to_string())?;
    transfers.extend(
        transfer_statement
            .query_map([], parse_server_transfer)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
    );
    let mut unread_statement = conn.prepare(
        "SELECT m.conversation_id,COUNT(*) FROM pmc_server_messages m LEFT JOIN lan_conversation_reads r ON r.conversation_id=m.conversation_id
         WHERE m.direction='incoming' AND m.timestamp>COALESCE(r.last_read_at,0) GROUP BY m.conversation_id"
    ).map_err(|e| e.to_string())?;
    let server_unread = unread_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|e| e.to_string())?;
    *unread_count += server_unread.values().sum::<u64>();
    if let Some(server_id) = settings(conn)?.server_id {
        let lobby_id = format!("server:{server_id}:lobby");
        let last_message_at = messages
            .iter()
            .filter(|message| message.conversation_id == lobby_id)
            .map(|message| message.timestamp)
            .max();
        conversations.insert(
            1.min(conversations.len()),
            LanConversation {
                id: lobby_id.clone(),
                kind: "serverLobby".to_string(),
                contact_id: None,
                title: "服务器大厅".to_string(),
                unread_count: server_unread.get(&lobby_id).copied().unwrap_or(0),
                last_message_at,
            },
        );
    }
    for conversation in conversations
        .iter_mut()
        .filter(|conversation| conversation.id.starts_with("direct:"))
    {
        conversation.unread_count += server_unread.get(&conversation.id).copied().unwrap_or(0);
        if let Some(last) = messages
            .iter()
            .filter(|message| message.conversation_id == conversation.id)
            .map(|message| message.timestamp)
            .max()
        {
            conversation.last_message_at =
                Some(conversation.last_message_at.unwrap_or(0).max(last));
        }
    }
    for contact in contacts.iter() {
        let id = format!("direct:{}", contact.id);
        if conversations
            .iter()
            .any(|conversation| conversation.id == id)
        {
            continue;
        }
        conversations.push(LanConversation {
            id: id.clone(),
            kind: "direct".to_string(),
            contact_id: Some(contact.id.clone()),
            title: contact.display_name.clone(),
            unread_count: server_unread.get(&id).copied().unwrap_or(0),
            last_message_at: messages
                .iter()
                .filter(|message| message.conversation_id == id)
                .map(|message| message.timestamp)
                .max(),
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn update_pmc_server_settings(
    app_handle: tauri::AppHandle,
    request: UpdatePmcServerSettingsRequest,
) -> Result<PmcServerSettings, String> {
    let address = normalize_address(&request.address)?;
    if request.enabled && address.is_empty() {
        return Err("启用服务器连接前请填写服务器地址".to_string());
    }
    if let Some(password) = request.password.as_deref() {
        write_secret(&address, "password", password)?;
    }
    let db_path = state_paths()?.0;
    let conn = open_database(&db_path)?;
    conn.execute("UPDATE pmc_server_settings SET address=?1,enabled=?2,server_id=CASE WHEN address=?1 THEN server_id ELSE NULL END,updated_at=?3 WHERE singleton=1",
        params![address,request.enabled as i64,now_millis() as i64]).map_err(|e| e.to_string())?;
    if request.enabled {
        restart_connection(app_handle, address)?;
    } else {
        disconnect_runtime(&app_handle)?;
    }
    settings(&conn)
}

fn disconnect_runtime(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let mut runtime = SERVER_RUNTIME.lock().map_err(|e| e.to_string())?;
    if let Some(cancel) = runtime.cancel.take() {
        let _ = cancel.send(true);
    }
    runtime.outbound = None;
    runtime.online_ids.clear();
    runtime.status = PmcServerStatus::default();
    drop(runtime);
    emit_changed(app_handle);
    Ok(())
}

#[tauri::command]
pub async fn connect_pmc_server(app_handle: tauri::AppHandle) -> Result<(), String> {
    let db_path = state_paths()?.0;
    let current = settings(&open_database(&db_path)?)?;
    if current.address.is_empty() {
        return Err("请先填写服务器地址".to_string());
    }
    restart_connection(app_handle, current.address)
}

#[tauri::command]
pub async fn disconnect_pmc_server(app_handle: tauri::AppHandle) -> Result<(), String> {
    disconnect_runtime(&app_handle)
}

#[tauri::command]
pub async fn send_server_message(
    app_handle: tauri::AppHandle,
    request: SendServerMessageRequest,
) -> Result<LanDeliveryResult, String> {
    let content = request.content.trim().to_string();
    if content.is_empty() {
        return Err("消息不能为空".to_string());
    }
    if content.as_bytes().len() > pmc_protocol::MAX_MESSAGE_BYTES {
        return Err("单条消息不能超过 16 KiB".to_string());
    }
    let (profile, server_id, outbound) = {
        let profile = P2P_GLOBAL
            .lock()
            .map_err(|e| e.to_string())?
            .profile
            .clone()
            .ok_or_else(|| "设备身份尚未初始化".to_string())?;
        let runtime = SERVER_RUNTIME.lock().map_err(|e| e.to_string())?;
        (
            profile,
            runtime
                .status
                .server_id
                .clone()
                .ok_or_else(|| "服务器尚未连接".to_string())?,
            runtime
                .outbound
                .clone()
                .ok_or_else(|| "服务器尚未连接".to_string())?,
        )
    };
    let message = RelayMessage {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: String::new(),
        from_id: profile.id.clone(),
        from_name: profile.display_name,
        to_id: request.to_id,
        content,
        timestamp: now_millis(),
    };
    persist_message(&message, &profile.id, &server_id, "sending")?;
    outbound
        .send(ClientMessage::SendMessage { message })
        .map_err(|_| "服务器连接已关闭".to_string())?;
    emit_changed(&app_handle);
    Ok(LanDeliveryResult {
        target_count: 1,
        delivered_count: 1,
        failures: Vec::<P2PDeliveryFailure>::new(),
    })
}

#[tauri::command]
pub async fn offer_server_files(
    app_handle: tauri::AppHandle,
    request: OfferLanFilesRequest,
) -> Result<LanFileOfferResult, String> {
    let mut recipients = request.to_ids;
    if let Some(to_id) = request.to_id {
        recipients.push(to_id);
    }
    recipients.sort();
    recipients.dedup();
    if recipients.len() != 1 || request.conversation_id.as_deref() == Some("lobby") {
        return Err("服务器文件传输只能在单个联系人私聊中发起".to_string());
    }
    let recipient_id = recipients.remove(0);
    let (server_id, outbound, online) = {
        let runtime = SERVER_RUNTIME.lock().map_err(|e| e.to_string())?;
        (
            runtime
                .status
                .server_id
                .clone()
                .ok_or_else(|| "服务器尚未连接".to_string())?,
            runtime
                .outbound
                .clone()
                .ok_or_else(|| "服务器尚未连接".to_string())?,
            runtime.online_ids.contains(&recipient_id),
        )
    };
    if !online {
        return Err("接收设备当前未通过服务器在线".to_string());
    }
    let mut transfers = Vec::new();
    let mut failures = Vec::new();
    let conn = open_database(&state_paths()?.0)?;
    for path in request.paths {
        match transfer::prepare_server_transfer(
            path.clone(),
            recipient_id.clone(),
            server_id.clone(),
        )
        .await
        {
            Ok(transfer) => {
                insert_server_transfer(&conn, &transfer)?;
                if outbound
                    .send(ClientMessage::FileOffer {
                        offer: transfer_offer(&transfer),
                    })
                    .is_err()
                {
                    failures.push(LanTransferFailure {
                        path,
                        error: "服务器连接已关闭".to_string(),
                    });
                } else {
                    transfers.push(transfer);
                }
            }
            Err(error) => failures.push(LanTransferFailure { path, error }),
        }
    }
    emit_changed(&app_handle);
    Ok(LanFileOfferResult {
        transfers,
        failures,
    })
}

#[tauri::command]
pub async fn respond_server_transfer(
    app_handle: tauri::AppHandle,
    request: RespondLanTransferRequest,
) -> Result<LanTransfer, String> {
    let conn = open_database(&state_paths()?.0)?;
    let transfer = load_server_transfer(&conn, &request.transfer_id)?;
    if transfer.direction != "incoming" {
        return Err("只能处理收到的服务器文件".to_string());
    }
    let outbound = SERVER_RUNTIME
        .lock()
        .map_err(|e| e.to_string())?
        .outbound
        .clone()
        .ok_or_else(|| "服务器尚未连接".to_string())?;
    match request.action.as_str() {
        "reject" => {
            outbound
                .send(ClientMessage::FileResponse {
                    transfer_id: transfer.id.clone(),
                    accepted: false,
                })
                .map_err(|_| "服务器连接已关闭".to_string())?;
            let updated = update_server_transfer(&transfer.id, "rejected", None, None)?;
            emit_changed(&app_handle);
            Ok(updated)
        }
        "accept" => {
            let destination = if let Some(value) = request
                .destination_path
                .filter(|value| !value.trim().is_empty())
            {
                PathBuf::from(value)
            } else {
                transfer::allocate_server_destination(&conn, &transfer)?
            };
            conn.execute("UPDATE pmc_server_transfers SET status='transferring',received_path=?2,error=NULL,updated_at=?3 WHERE id=?1",params![transfer.id,destination.to_string_lossy(),now_millis() as i64]).map_err(|e|e.to_string())?;
            let _ = transfer_cancellation(&transfer.id)?;
            outbound
                .send(ClientMessage::FileResponse {
                    transfer_id: transfer.id.clone(),
                    accepted: true,
                })
                .map_err(|_| "服务器连接已关闭".to_string())?;
            let updated = load_server_transfer(&conn, &transfer.id)?;
            emit_changed(&app_handle);
            Ok(updated)
        }
        _ => Err("不支持的文件响应".to_string()),
    }
}

#[tauri::command]
pub async fn cancel_server_transfer(
    app_handle: tauri::AppHandle,
    request: CancelLanTransferRequest,
) -> Result<LanTransfer, String> {
    let outbound = SERVER_RUNTIME
        .lock()
        .map_err(|e| e.to_string())?
        .outbound
        .clone();
    signal_transfer_cancel(&request.transfer_id);
    if let Some(outbound) = outbound {
        let _ = outbound.send(ClientMessage::CancelTransfer {
            transfer_id: request.transfer_id.clone(),
            reason: Some("用户中断传输".to_string()),
        });
    }
    let updated =
        update_server_transfer(&request.transfer_id, "cancelled", Some("传输已中断"), None)?;
    emit_changed(&app_handle);
    Ok(updated)
}

#[tauri::command]
pub async fn test_pmc_server_speed(
    app_handle: tauri::AppHandle,
) -> Result<SpeedTestResult, String> {
    let result: Result<SpeedTestResult, String> = async {
        let (address, device_id, credential) = {
            let db_path = state_paths()?.0;
            let current = settings(&open_database(&db_path)?)?;
            let current_status = status();
            if !current_status.connected {
                return Err("服务器尚未连接".to_string());
            }
            let server_id = current_status
                .server_id
                .ok_or_else(|| "服务器身份尚未确认".to_string())?;
            let device_id = P2P_GLOBAL
                .lock()
                .map_err(|e| e.to_string())?
                .profile
                .as_ref()
                .map(|p| p.id.clone())
                .ok_or_else(|| "设备身份尚未初始化".to_string())?;
            let credential = read_device_credential(&current.address, &server_id, &device_id)
                .ok_or_else(|| "设备凭据缺失".to_string())?;
            (current.address, device_id, credential)
        };
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| e.to_string())?;
        let mut latencies = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            let response = client
                .get(format!("{address}/api/v1/info"))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("延迟测试失败：HTTP {}", response.status()));
            }
            let _ = response.bytes().await.map_err(|e| e.to_string())?;
            latencies.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        latencies.sort_by(|a, b| a.total_cmp(b));
        let test_id = uuid::Uuid::new_v4().to_string();
        let _cleanup = SpeedTestCleanup {
            client: client.clone(),
            url: format!("{address}/api/v1/speed"),
            device_id: device_id.clone(),
            credential: credential.clone(),
            test_id: test_id.clone(),
        };
        let headers = |builder: reqwest::RequestBuilder| {
            builder
                .header("x-pmc-device-id", &device_id)
                .header("x-pmc-device-credential", &credential)
                .header("x-pmc-speed-test-id", &test_id)
        };
        let response = headers(client.get(format!("{address}/api/v1/speed/download")))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("下载测速失败：HTTP {}", response.status()));
        }
        let mut stream = response.bytes_stream();
        let download_started = Instant::now();
        let mut downloaded = 0u64;
        while download_started.elapsed() < Duration::from_secs(5) {
            match tokio::time::timeout(Duration::from_millis(250), stream.next()).await {
                Ok(Some(Ok(chunk))) => downloaded += chunk.len() as u64,
                Ok(Some(Err(e))) => return Err(e.to_string()),
                Ok(None) => break,
                Err(_) => {}
            }
        }
        drop(stream);
        let upload_started = Instant::now();
        let upload_chunk = bytes::Bytes::from(vec![0x3c; 64 * 1024]);
        let upload_stream = futures_util::stream::unfold(
            (upload_started, upload_chunk),
            |(started, chunk)| async move {
                if started.elapsed() >= Duration::from_secs(5) {
                    None
                } else {
                    Some((Ok::<_, std::io::Error>(chunk.clone()), (started, chunk)))
                }
            },
        );
        let response = headers(client.put(format!("{address}/api/v1/speed/upload")))
            .body(reqwest::Body::wrap_stream(upload_stream))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("上传测速失败：HTTP {}", response.status()));
        }
        let value: serde_json::Value =
            serde_json::from_str(&response.text().await.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let uploaded = value
            .get("receivedBytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        Ok(SpeedTestResult {
            latency_ms: latencies[2],
            download_bytes_per_second: (downloaded as f64
                / download_started.elapsed().as_secs_f64())
                as u64,
            upload_bytes_per_second: (uploaded as f64 / upload_started.elapsed().as_secs_f64())
                as u64,
            tested_at: now_millis(),
            error: None,
        })
    }
    .await;
    let snapshot = match &result {
        Ok(value) => value.clone(),
        Err(error) => SpeedTestResult {
            latency_ms: 0.0,
            download_bytes_per_second: 0,
            upload_bytes_per_second: 0,
            tested_at: now_millis(),
            error: Some(error.clone()),
        },
    };
    if let Ok(mut runtime) = SERVER_RUNTIME.lock() {
        runtime.speed_test = Some(snapshot);
    }
    emit_changed(&app_handle);
    result
}

#[cfg(test)]
mod tests {
    use super::should_pause_automatic_reconnect;

    #[test]
    fn authentication_failures_pause_automatic_reconnect() {
        assert!(should_pause_automatic_reconnect("共享密码错误"));
        assert!(should_pause_automatic_reconnect(
            "HTTP error: 429 Too Many Requests"
        ));
        assert!(should_pause_automatic_reconnect("设备凭据无效"));
        assert!(!should_pause_automatic_reconnect("连接服务器超时"));
    }
}
