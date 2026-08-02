use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, Request, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, put},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use pmc_protocol::{
    ClientMessage, DeviceProfile, FileOffer, RelayMessage, ServerInfo, ServerMessage,
    HISTORY_LIMIT, MAX_AVATAR_BYTES, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    convert::Infallible,
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncWriteExt, DuplexStream},
    sync::{mpsc, RwLock},
};
use tokio_util::io::ReaderStream;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};

#[derive(Clone)]
struct Config {
    bind: String,
    name: String,
    password: String,
    max_transfer_bytes: u64,
}

#[derive(Clone)]
struct OnlineDevice {
    sender: mpsc::UnboundedSender<ServerMessage>,
    connection_id: String,
}

struct TransferSession {
    offer: FileOffer,
    token: String,
    writer: tokio::sync::Mutex<Option<DuplexStream>>,
    cancelled: AtomicBool,
}

#[derive(Clone)]
struct AppState {
    config: Config,
    info: ServerInfo,
    db: Arc<Mutex<Connection>>,
    online: Arc<RwLock<HashMap<String, OnlineDevice>>>,
    transfers: Arc<RwLock<HashMap<String, Arc<TransferSession>>>>,
    auth_failures: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
    speed_tests: Arc<Mutex<HashMap<String, (String, Instant)>>>,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn initialize_database(data_dir: &PathBuf) -> Result<(Connection, String)> {
    std::fs::create_dir_all(data_dir)?;
    let conn = Connection::open(data_dir.join("pmc-server.db"))?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS server_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS devices(
           device_id TEXT PRIMARY KEY,
           credential_hash TEXT NOT NULL,
           display_name TEXT NOT NULL,
           department TEXT NOT NULL DEFAULT '',
           avatar_base64 TEXT,
           avatar_hash TEXT,
           profile_revision INTEGER NOT NULL DEFAULT 1,
           last_seen INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS messages(
           id TEXT PRIMARY KEY,
           conversation_id TEXT NOT NULL,
           from_id TEXT NOT NULL,
           from_name TEXT NOT NULL,
           to_id TEXT,
           content TEXT NOT NULL,
           timestamp INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_messages_conversation_time
           ON messages(conversation_id,timestamp);",
    )?;
    let server_id = conn
        .query_row(
            "SELECT value FROM server_settings WHERE key='server_id'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    conn.execute(
        "INSERT OR REPLACE INTO server_settings(key,value) VALUES('server_id',?1)",
        params![server_id],
    )?;
    Ok((conn, server_id))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = Config {
        bind: env::var("PMC_SERVER_BIND").unwrap_or_else(|_| "0.0.0.0:7412".to_string()),
        name: env::var("PMC_SERVER_NAME").unwrap_or_else(|_| "PMC Server".to_string()),
        password: env::var("PMC_SERVER_PASSWORD").unwrap_or_default(),
        max_transfer_bytes: env_u64("PMC_MAX_TRANSFER_BYTES", 1024 * 1024 * 1024 * 1024),
    };
    let data_dir = PathBuf::from(env::var("PMC_DATA_DIR").unwrap_or_else(|_| "./data".to_string()));
    let (db, server_id) = initialize_database(&data_dir)?;
    let info = ServerInfo {
        server_id,
        name: config.name.clone(),
        protocol_version: PROTOCOL_VERSION,
        requires_password: !config.password.is_empty(),
        max_transfer_bytes: config.max_transfer_bytes,
    };
    let state = AppState {
        config: config.clone(),
        info,
        db: Arc::new(Mutex::new(db)),
        online: Arc::new(RwLock::new(HashMap::new())),
        transfers: Arc::new(RwLock::new(HashMap::new())),
        auth_failures: Arc::new(Mutex::new(HashMap::new())),
        speed_tests: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .with_context(|| format!("failed to bind {}", config.bind))?;
    info!(bind = %config.bind, "PMC Server started");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/info", get(server_info))
        .route("/api/v1/ws", get(ws_handler))
        .route(
            "/api/v1/transfers/{transfer_id}/download",
            get(download_transfer),
        )
        .route(
            "/api/v1/transfers/{transfer_id}/upload",
            put(upload_transfer),
        )
        .route("/api/v1/speed/download", get(speed_download))
        .route("/api/v1/speed/upload", put(speed_upload))
        .route("/api/v1/speed", delete(speed_finish))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn server_info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json(state.info)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> Response {
    if auth_rate_limited(&state, peer.ip()) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "too many authentication failures",
        )
            .into_response();
    }
    ws.max_message_size(128 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state, peer.ip()))
}

fn auth_rate_limited(state: &AppState, ip: IpAddr) -> bool {
    let Ok(mut failures) = state.auth_failures.lock() else {
        return true;
    };
    let entries = failures.entry(ip).or_default();
    while entries
        .front()
        .is_some_and(|at| at.elapsed() > Duration::from_secs(60))
    {
        entries.pop_front();
    }
    entries.len() >= 5
}

fn record_auth_failure(state: &AppState, ip: IpAddr) {
    if let Ok(mut failures) = state.auth_failures.lock() {
        failures.entry(ip).or_default().push_back(Instant::now());
    }
}

fn credential_hash(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex().to_string()
}

fn authenticate_registration(
    state: &AppState,
    ip: IpAddr,
    device_id: &str,
    credential: Option<&str>,
    password: Option<&str>,
) -> Result<String, String> {
    if state.config.password != password.unwrap_or_default() {
        record_auth_failure(state, ip);
        return Err("共享密码错误".to_string());
    }
    let conn = state.db.lock().map_err(|error| error.to_string())?;
    let existing = conn
        .query_row(
            "SELECT credential_hash FROM devices WHERE device_id=?1",
            params![device_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match existing {
        Some(expected) => {
            let provided = credential
                .ok_or_else(|| "设备凭据缺失，请在服务器管理端删除该设备后重新登记".to_string())?;
            if credential_hash(provided) != expected {
                record_auth_failure(state, ip);
                return Err("设备凭据无效，拒绝冒用 deviceId".to_string());
            }
            Ok(provided.to_string())
        }
        None => Ok(uuid::Uuid::new_v4().to_string()),
    }
}

fn validate_avatar(value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        let bytes = BASE64
            .decode(value)
            .map_err(|_| "头像编码无效".to_string())?;
        if bytes.len() > MAX_AVATAR_BYTES {
            return Err("头像不能超过 64 KiB".to_string());
        }
    }
    Ok(())
}

fn save_device(state: &AppState, profile: &DeviceProfile, credential: &str) -> Result<(), String> {
    let conn = state.db.lock().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO devices(device_id,credential_hash,display_name,department,avatar_base64,avatar_hash,profile_revision,last_seen)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(device_id) DO UPDATE SET display_name=excluded.display_name,department=excluded.department,
           avatar_base64=excluded.avatar_base64,avatar_hash=excluded.avatar_hash,
           profile_revision=excluded.profile_revision,last_seen=excluded.last_seen",
        params![profile.device_id, credential_hash(credential), profile.display_name, profile.department,
            profile.avatar_base64, profile.avatar_hash, profile.profile_revision as i64, profile.last_seen as i64],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

fn load_history(state: &AppState, device_id: &str) -> Result<Vec<RelayMessage>, String> {
    let conn = state.db.lock().map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT id,conversation_id,from_id,from_name,to_id,content,timestamp FROM messages
         WHERE to_id IS NULL OR from_id=?1 OR to_id=?1 ORDER BY timestamp DESC LIMIT 2000",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![device_id], |row| {
            Ok(RelayMessage {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                from_id: row.get(2)?,
                from_name: row.get(3)?,
                to_id: row.get(4)?,
                content: row.get(5)?,
                timestamp: row.get::<_, i64>(6)? as u64,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut counts = HashMap::<String, usize>::new();
    let mut messages = Vec::new();
    for row in rows {
        let message = row.map_err(|error| error.to_string())?;
        let count = counts.entry(message.conversation_id.clone()).or_default();
        if *count < HISTORY_LIMIT {
            *count += 1;
            messages.push(message);
        }
    }
    messages.sort_by_key(|message| message.timestamp);
    Ok(messages)
}

async fn device_profiles(state: &AppState) -> Vec<DeviceProfile> {
    let online_ids: HashSet<String> = state.online.read().await.keys().cloned().collect();
    let Ok(conn) = state.db.lock() else {
        return Vec::new();
    };
    let Ok(mut statement)=conn.prepare("SELECT device_id,display_name,department,avatar_base64,avatar_hash,profile_revision,last_seen FROM devices ORDER BY display_name COLLATE NOCASE") else { return Vec::new(); };
    let Ok(rows) = statement.query_map([], |row| {
        let device_id: String = row.get(0)?;
        Ok(DeviceProfile {
            online: online_ids.contains(&device_id),
            device_id,
            display_name: row.get(1)?,
            department: row.get(2)?,
            avatar_base64: row.get(3)?,
            avatar_hash: row.get(4)?,
            profile_revision: row.get::<_, i64>(5)? as u64,
            last_seen: row.get::<_, i64>(6)? as u64,
        })
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok).collect()
}

async fn broadcast(state: &AppState, message: ServerMessage, except: Option<&str>) {
    for (id, device) in state.online.read().await.iter() {
        if except == Some(id.as_str()) {
            continue;
        }
        let _ = device.sender.send(message.clone());
    }
}

async fn broadcast_presence(state: &AppState) {
    let devices = device_profiles(state).await;
    broadcast(state, ServerMessage::Presence { devices }, None).await;
}

async fn handle_socket(socket: WebSocket, state: AppState, ip: IpAddr) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let registration = tokio::time::timeout(Duration::from_secs(10), ws_receiver.next()).await;
    let Some(Ok(Message::Text(text))) = registration.ok().flatten() else {
        return;
    };
    let Ok(ClientMessage::Register {
        device_id,
        display_name,
        department,
        avatar_base64,
        avatar_hash,
        profile_revision,
        credential,
        shared_password,
    }) = serde_json::from_str::<ClientMessage>(&text)
    else {
        let _ = ws_sender
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Error {
                    code: "registrationRequired".into(),
                    message: "首条消息必须是设备注册".into(),
                })
                .unwrap()
                .into(),
            ))
            .await;
        return;
    };
    if device_id.trim().is_empty()
        || display_name.trim().is_empty()
        || validate_avatar(avatar_base64.as_deref()).is_err()
    {
        record_auth_failure(&state, ip);
        return;
    }
    let credential = match authenticate_registration(
        &state,
        ip,
        &device_id,
        credential.as_deref(),
        shared_password.as_deref(),
    ) {
        Ok(value) => value,
        Err(message) => {
            let _ = ws_sender
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Error {
                        code: "authenticationFailed".into(),
                        message,
                    })
                    .unwrap()
                    .into(),
                ))
                .await;
            return;
        }
    };
    if let Ok(mut failures) = state.auth_failures.lock() {
        failures.remove(&ip);
    }
    let profile = DeviceProfile {
        device_id: device_id.clone(),
        display_name,
        department,
        avatar_base64,
        avatar_hash,
        profile_revision,
        online: true,
        last_seen: now_millis(),
    };
    if let Err(error) = save_device(&state, &profile, &credential) {
        warn!(%error, "failed to persist device");
        return;
    }
    let connection_id = uuid::Uuid::new_v4().to_string();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    state.online.write().await.insert(
        device_id.clone(),
        OnlineDevice {
            sender: event_tx.clone(),
            connection_id: connection_id.clone(),
        },
    );
    let registered = ServerMessage::Registered {
        info: state.info.clone(),
        credential: credential.clone(),
        devices: device_profiles(&state).await,
        history: load_history(&state, &device_id).unwrap_or_default(),
    };
    let _ = event_tx.send(registered);
    broadcast_presence(&state).await;
    let writer = tokio::spawn(async move {
        while let Some(message) = event_rx.recv().await {
            let Ok(text) = serde_json::to_string(&message) else {
                continue;
            };
            if ws_sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = ws_receiver.next().await {
        if !state
            .online
            .read()
            .await
            .get(&device_id)
            .is_some_and(|device| device.connection_id == connection_id)
        {
            break;
        }
        match message {
            Message::Text(text) => {
                if let Ok(message) = serde_json::from_str::<ClientMessage>(&text) {
                    handle_client_message(&state, &device_id, message).await;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    writer.abort();
    let should_remove = state
        .online
        .read()
        .await
        .get(&device_id)
        .is_some_and(|device| device.connection_id == connection_id);
    if should_remove {
        state.online.write().await.remove(&device_id);
        if let Ok(conn) = state.db.lock() {
            let _ = conn.execute(
                "UPDATE devices SET last_seen=?2 WHERE device_id=?1",
                params![device_id, now_millis() as i64],
            );
        }
        cancel_device_transfers(&state, &device_id, "设备连接已断开").await;
        broadcast_presence(&state).await;
    }
}

async fn handle_client_message(state: &AppState, device_id: &str, message: ClientMessage) {
    match message {
        ClientMessage::Ping { nonce, sent_at } => {
            send_to(
                state,
                device_id,
                ServerMessage::Pong {
                    nonce,
                    sent_at,
                    server_at: now_millis(),
                },
            )
            .await;
        }
        ClientMessage::SendMessage { mut message } => {
            if message.from_id != device_id
                || message.content.as_bytes().len() > MAX_MESSAGE_BYTES
                || message.content.trim().is_empty()
            {
                return;
            }
            message.conversation_id = match message.to_id.as_deref() {
                None => format!("server:{}:lobby", state.info.server_id),
                Some(other) => direct_conversation_id(device_id, other),
            };
            if store_message(state, &message).is_err() {
                return;
            }
            if let Some(to_id) = message.to_id.as_deref() {
                send_to(
                    state,
                    to_id,
                    ServerMessage::Message {
                        message: message.clone(),
                    },
                )
                .await;
            } else {
                broadcast(
                    state,
                    ServerMessage::Message {
                        message: message.clone(),
                    },
                    Some(device_id),
                )
                .await;
            }
            send_to(
                state,
                device_id,
                ServerMessage::MessageAck {
                    message_id: message.id,
                },
            )
            .await;
        }
        ClientMessage::FileOffer { offer } => {
            if offer.from_id != device_id || offer.total_bytes > state.config.max_transfer_bytes {
                return;
            }
            if !state.online.read().await.contains_key(&offer.to_id) {
                send_to(
                    state,
                    device_id,
                    ServerMessage::FileResponse {
                        transfer_id: offer.transfer_id,
                        accepted: false,
                        reason: Some("接收设备不在线".into()),
                    },
                )
                .await;
                return;
            }
            let session = Arc::new(TransferSession {
                offer: offer.clone(),
                token: uuid::Uuid::new_v4().to_string(),
                writer: tokio::sync::Mutex::new(None),
                cancelled: AtomicBool::new(false),
            });
            state
                .transfers
                .write()
                .await
                .insert(offer.transfer_id.clone(), session);
            let recipient_id = offer.to_id.clone();
            send_to(state, &recipient_id, ServerMessage::FileOffer { offer }).await;
        }
        ClientMessage::FileResponse {
            transfer_id,
            accepted,
        } => {
            let session = state.transfers.read().await.get(&transfer_id).cloned();
            let Some(session) = session else {
                return;
            };
            if session.offer.to_id != device_id {
                return;
            }
            if !accepted {
                state.transfers.write().await.remove(&transfer_id);
                session.cancelled.store(true, Ordering::SeqCst);
                send_to(
                    state,
                    &session.offer.from_id,
                    ServerMessage::FileResponse {
                        transfer_id,
                        accepted: false,
                        reason: Some("接收方已拒绝".into()),
                    },
                )
                .await;
                return;
            }
            send_to(
                state,
                &session.offer.from_id,
                ServerMessage::FileResponse {
                    transfer_id: transfer_id.clone(),
                    accepted: true,
                    reason: None,
                },
            )
            .await;
            send_to(
                state,
                device_id,
                ServerMessage::FileDownload {
                    transfer_id: transfer_id.clone(),
                    url: format!("/api/v1/transfers/{transfer_id}/download"),
                    token: session.token.clone(),
                },
            )
            .await;
        }
        ClientMessage::CancelTransfer {
            transfer_id,
            reason,
        } => {
            let allowed = state
                .transfers
                .read()
                .await
                .get(&transfer_id)
                .is_some_and(|session| {
                    session.offer.from_id == device_id || session.offer.to_id == device_id
                });
            if allowed {
                cancel_transfer(
                    state,
                    &transfer_id,
                    reason.as_deref().unwrap_or("传输已取消"),
                )
                .await;
            }
        }
        ClientMessage::Register { .. } => {}
    }
}

fn direct_conversation_id(left: &str, right: &str) -> String {
    if left <= right {
        format!("direct:{left}:{right}")
    } else {
        format!("direct:{right}:{left}")
    }
}

fn store_message(state: &AppState, message: &RelayMessage) -> Result<(), String> {
    let conn = state.db.lock().map_err(|error| error.to_string())?;
    conn.execute("INSERT OR IGNORE INTO messages(id,conversation_id,from_id,from_name,to_id,content,timestamp) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![message.id,message.conversation_id,message.from_id,message.from_name,message.to_id,message.content,message.timestamp as i64])
        .map_err(|error| error.to_string())?;
    conn.execute("DELETE FROM messages WHERE conversation_id=?1 AND id NOT IN (SELECT id FROM messages WHERE conversation_id=?1 ORDER BY timestamp DESC LIMIT ?2)",
        params![message.conversation_id, HISTORY_LIMIT as i64]).map_err(|error| error.to_string())?;
    Ok(())
}

async fn send_to(state: &AppState, device_id: &str, message: ServerMessage) {
    if let Some(device) = state.online.read().await.get(device_id) {
        let _ = device.sender.send(message);
    }
}

async fn cancel_transfer(state: &AppState, transfer_id: &str, reason: &str) {
    let session = state.transfers.write().await.remove(transfer_id);
    if let Some(session) = session {
        session.cancelled.store(true, Ordering::SeqCst);
        session.writer.lock().await.take();
        let event = ServerMessage::FileCancelled {
            transfer_id: transfer_id.to_string(),
            reason: reason.to_string(),
        };
        send_to(state, &session.offer.from_id, event.clone()).await;
        send_to(state, &session.offer.to_id, event).await;
    }
}

async fn cancel_device_transfers(state: &AppState, device_id: &str, reason: &str) {
    let ids: Vec<String> = state
        .transfers
        .read()
        .await
        .iter()
        .filter(|(_, session)| {
            session.offer.from_id == device_id || session.offer.to_id == device_id
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        cancel_transfer(state, &id, reason).await;
    }
}

fn query_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-pmc-transfer-token")
        .and_then(|value| value.to_str().ok())
}

async fn download_transfer(
    State(state): State<AppState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let session = state.transfers.read().await.get(&transfer_id).cloned();
    let Some(session) = session else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if query_token(&headers) != Some(session.token.as_str()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let (reader, writer) = tokio::io::duplex(1024 * 1024);
    let mut slot = session.writer.lock().await;
    if slot.is_some() {
        return (StatusCode::CONFLICT, "download already attached").into_response();
    }
    *slot = Some(writer);
    drop(slot);
    send_to(
        &state,
        &session.offer.from_id,
        ServerMessage::FileUpload {
            transfer_id: transfer_id.clone(),
            url: format!("/api/v1/transfers/{transfer_id}/upload"),
            token: session.token.clone(),
        },
    )
    .await;
    Body::from_stream(ReaderStream::new(reader)).into_response()
}

async fn upload_transfer(
    State(state): State<AppState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let session = state.transfers.read().await.get(&transfer_id).cloned();
    let Some(session) = session else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if query_token(&headers) != Some(session.token.as_str()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(mut writer) = session.writer.lock().await.take() else {
        return (StatusCode::CONFLICT, "receiver is not ready").into_response();
    };
    let mut stream = request.into_body().into_data_stream();
    let mut forwarded = 0u64;
    while let Some(chunk) = stream.next().await {
        if session.cancelled.load(Ordering::SeqCst) {
            return StatusCode::GONE.into_response();
        }
        let chunk = match chunk {
            Ok(value) => value,
            Err(_) => {
                cancel_transfer(&state, &transfer_id, "上传流读取失败").await;
                return StatusCode::BAD_REQUEST.into_response();
            }
        };
        forwarded = forwarded.saturating_add(chunk.len() as u64);
        if forwarded > session.offer.total_bytes || writer.write_all(&chunk).await.is_err() {
            cancel_transfer(&state, &transfer_id, "接收流已断开").await;
            return StatusCode::BAD_GATEWAY.into_response();
        }
        let event = ServerMessage::FileProgress {
            transfer_id: transfer_id.clone(),
            forwarded_bytes: forwarded,
            total_bytes: session.offer.total_bytes,
        };
        send_to(&state, &session.offer.from_id, event.clone()).await;
        send_to(&state, &session.offer.to_id, event).await;
    }
    let _ = writer.shutdown().await;
    if forwarded != session.offer.total_bytes {
        cancel_transfer(&state, &transfer_id, "上传字节数与报价不一致").await;
        return StatusCode::BAD_REQUEST.into_response();
    }
    state.transfers.write().await.remove(&transfer_id);
    let event = ServerMessage::FileComplete {
        transfer_id: transfer_id.clone(),
    };
    send_to(&state, &session.offer.from_id, event.clone()).await;
    send_to(&state, &session.offer.to_id, event).await;
    StatusCode::NO_CONTENT.into_response()
}

fn authenticate_http(state: &AppState, headers: &HeaderMap) -> Result<String, StatusCode> {
    let device_id = headers
        .get("x-pmc-device-id")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let credential = headers
        .get("x-pmc-device-credential")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let conn = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let expected = conn
        .query_row(
            "SELECT credential_hash FROM devices WHERE device_id=?1",
            params![device_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if credential_hash(credential) != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(device_id.to_string())
}

fn acquire_speed_test(state: &AppState, device_id: &str, test_id: &str) -> Result<(), StatusCode> {
    let mut tests = state
        .speed_tests
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tests.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(90));
    if tests
        .get(device_id)
        .is_some_and(|(active, _)| active != test_id)
    {
        return Err(StatusCode::CONFLICT);
    }
    tests.insert(device_id.to_string(), (test_id.to_string(), Instant::now()));
    Ok(())
}

async fn speed_download(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let device_id = match authenticate_http(&state, &headers) {
        Ok(value) => value,
        Err(code) => return code.into_response(),
    };
    let test_id = headers
        .get("x-pmc-speed-test-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if test_id.is_empty() || acquire_speed_test(&state, &device_id, test_id).is_err() {
        return StatusCode::CONFLICT.into_response();
    }
    let chunk = Bytes::from(vec![0x5a; 64 * 1024]);
    let stream = futures_util::stream::repeat_with(move || Ok::<Bytes, Infallible>(chunk.clone()));
    Body::from_stream(stream).into_response()
}

async fn speed_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let device_id = match authenticate_http(&state, &headers) {
        Ok(value) => value,
        Err(code) => return code.into_response(),
    };
    let test_id = headers
        .get("x-pmc-speed-test-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if test_id.is_empty() || acquire_speed_test(&state, &device_id, test_id).is_err() {
        return StatusCode::CONFLICT.into_response();
    }
    let mut stream = request.into_body().into_data_stream();
    let mut received = 0u64;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(value) => received += value.len() as u64,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    }
    if let Ok(mut tests) = state.speed_tests.lock() {
        tests.remove(&device_id);
    }
    Json(serde_json::json!({ "receivedBytes": received })).into_response()
}

async fn speed_finish(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let device_id = match authenticate_http(&state, &headers) {
        Ok(value) => value,
        Err(code) => return code.into_response(),
    };
    let test_id = headers
        .get("x-pmc-speed-test-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if test_id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if let Ok(mut tests) = state.speed_tests.lock() {
        if tests
            .get(&device_id)
            .is_some_and(|(active, _)| active == test_id)
        {
            tests.remove(&device_id);
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::{
        connect_async, tungstenite::Message as TungsteniteMessage, MaybeTlsStream, WebSocketStream,
    };

    fn test_state(password: &str) -> (AppState, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("pmc-server-test-{}", uuid::Uuid::new_v4()));
        let (db, server_id) = initialize_database(&directory).unwrap();
        let config = Config {
            bind: "127.0.0.1:0".into(),
            name: "Test".into(),
            password: password.into(),
            max_transfer_bytes: 1024 * 1024,
        };
        let info = ServerInfo {
            server_id,
            name: config.name.clone(),
            protocol_version: PROTOCOL_VERSION,
            requires_password: !password.is_empty(),
            max_transfer_bytes: config.max_transfer_bytes,
        };
        (
            AppState {
                config,
                info,
                db: Arc::new(Mutex::new(db)),
                online: Arc::new(RwLock::new(HashMap::new())),
                transfers: Arc::new(RwLock::new(HashMap::new())),
                auth_failures: Arc::new(Mutex::new(HashMap::new())),
                speed_tests: Arc::new(Mutex::new(HashMap::new())),
            },
            directory,
        )
    }

    #[test]
    fn device_credential_is_issued_once_and_blocks_impersonation() {
        let (state, directory) = test_state("shared");
        let ip = "127.0.0.1".parse().unwrap();
        let credential =
            authenticate_registration(&state, ip, "device-a", None, Some("shared")).unwrap();
        let profile = DeviceProfile {
            device_id: "device-a".into(),
            display_name: "A".into(),
            department: String::new(),
            avatar_base64: None,
            avatar_hash: None,
            profile_revision: 1,
            online: true,
            last_seen: now_millis(),
        };
        save_device(&state, &profile, &credential).unwrap();
        assert_eq!(
            authenticate_registration(&state, ip, "device-a", Some(&credential), Some("shared"))
                .unwrap(),
            credential
        );
        assert!(
            authenticate_registration(&state, ip, "device-a", Some("wrong"), Some("shared"))
                .is_err()
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn messages_are_trimmed_to_thirty_per_conversation() {
        let (state, directory) = test_state("");
        for index in 0..35u64 {
            store_message(
                &state,
                &RelayMessage {
                    id: format!("m-{index}"),
                    conversation_id: "server:test:lobby".into(),
                    from_id: "a".into(),
                    from_name: "A".into(),
                    to_id: None,
                    content: format!("{index}"),
                    timestamp: index,
                },
            )
            .unwrap();
        }
        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id='server:test:lobby'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 30);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn direct_messages_share_one_pair_history_limit() {
        let (state, directory) = test_state("");
        for index in 0..40u64 {
            let (from_id, to_id) = if index % 2 == 0 {
                ("a", "b")
            } else {
                ("b", "a")
            };
            store_message(
                &state,
                &RelayMessage {
                    id: format!("direct-{index}"),
                    conversation_id: direct_conversation_id(from_id, to_id),
                    from_id: from_id.into(),
                    from_name: from_id.into(),
                    to_id: Some(to_id.into()),
                    content: format!("{index}"),
                    timestamp: index,
                },
            )
            .unwrap();
        }
        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id=?1",
                params![direct_conversation_id("a", "b")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 30);
        let _ = std::fs::remove_dir_all(directory);
    }

    async fn receive_event(
        stream: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    ) -> ServerMessage {
        loop {
            if let Some(Ok(TungsteniteMessage::Text(text))) = stream.next().await {
                if let Ok(event) = serde_json::from_str(&text) {
                    return event;
                }
            }
        }
    }

    async fn register(
        stream: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
        id: &str,
    ) {
        let message = ClientMessage::Register {
            device_id: id.into(),
            display_name: id.into(),
            department: String::new(),
            avatar_base64: None,
            avatar_hash: None,
            profile_revision: 1,
            credential: None,
            shared_password: None,
        };
        stream
            .send(TungsteniteMessage::Text(
                serde_json::to_string(&message).unwrap().into(),
            ))
            .await
            .unwrap();
        loop {
            if matches!(
                receive_event(stream).await,
                ServerMessage::Registered { .. }
            ) {
                break;
            }
        }
    }

    #[tokio::test]
    async fn two_devices_can_relay_an_online_file_without_server_disk_storage() {
        let (state, directory) = test_state("");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let (mut sender, _) = connect_async(format!("ws://{address}/api/v1/ws"))
            .await
            .unwrap();
        register(&mut sender, "sender").await;
        let (mut receiver, _) = connect_async(format!("ws://{address}/api/v1/ws"))
            .await
            .unwrap();
        register(&mut receiver, "receiver").await;
        let offer = FileOffer {
            transfer_id: "transfer".into(),
            from_id: "sender".into(),
            from_name: "Sender".into(),
            to_id: "receiver".into(),
            display_name: "hello.txt".into(),
            kind: "file".into(),
            item_count: 1,
            total_bytes: 5,
            mime_type: Some("text/plain".into()),
            content_hash: "a".repeat(64),
            payload_format: "raw".into(),
            manifest: serde_json::json!({"version":1}),
            created_at: now_millis(),
        };
        sender
            .send(TungsteniteMessage::Text(
                serde_json::to_string(&ClientMessage::FileOffer { offer })
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
        loop {
            if matches!(
                receive_event(&mut receiver).await,
                ServerMessage::FileOffer { .. }
            ) {
                break;
            }
        }
        receiver
            .send(TungsteniteMessage::Text(
                serde_json::to_string(&ClientMessage::FileResponse {
                    transfer_id: "transfer".into(),
                    accepted: true,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
        let (download_url, token) = loop {
            if let ServerMessage::FileDownload { url, token, .. } =
                receive_event(&mut receiver).await
            {
                break (url, token);
            }
        };
        let download = tokio::spawn(async move {
            reqwest::Client::new()
                .get(format!("http://{address}{download_url}"))
                .header("x-pmc-transfer-token", token)
                .send()
                .await
                .unwrap()
                .bytes()
                .await
                .unwrap()
        });
        let (upload_url, token) = loop {
            if let ServerMessage::FileUpload { url, token, .. } = receive_event(&mut sender).await {
                break (url, token);
            }
        };
        let response = reqwest::Client::new()
            .put(format!("http://{address}{upload_url}"))
            .header("x-pmc-transfer-token", token)
            .body("hello")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(&download.await.unwrap()[..], b"hello");
        assert!(!directory.join("hello.txt").exists());
        server.abort();
        let _ = std::fs::remove_dir_all(directory);
    }
}
