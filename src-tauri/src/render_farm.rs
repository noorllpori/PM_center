//! R13 trusted-render foundation. Network transport is intentionally absent:
//! only paired identities may report capabilities or claim a controller lease.
//! Uploaded output must be staged under this module's private root, then the
//! controller atomically commits it after its token and hash are checked.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use keyring::Entry;
use rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use tauri::Manager;
use uuid::Uuid;
use x25519_dalek::{PublicKey as TransportPublicKey, StaticSecret as TransportSecret};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub const RENDER_FARM_MODULE_ID: &str = "builtin.render-farm";
pub const RENDER_FARM_DISABLED: &str = "RENDER_FARM_MODULE_DISABLED";
pub const RENDER_FARM_STARTING: &str = "RENDER_FARM_MODULE_STARTING";
pub const RENDER_FARM_STOPPING: &str = "RENDER_FARM_MODULE_STOPPING";

const FARM_DIR: &str = "render-farm";
const KEYRING_SERVICE: &str = "com.nexora.render-farm";
const KEYRING_ACCOUNT: &str = "device-signing-key-v1";
const TRANSPORT_KEYRING_ACCOUNT: &str = "device-transport-key-v1";
const PAIRING_TTL_MS: i64 = 300_000;
const TRANSPORT_TTL_MS: i64 = 300_000;
const MAX_TRANSPORT_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_PACK_FILES: usize = 4096;
const MAX_PACK_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const PHASE_DISABLED: u8 = 0;
const PHASE_STARTING: u8 = 1;
const PHASE_RUNNING: u8 = 2;
const PHASE_STOPPING: u8 = 3;
static PHASE: AtomicU8 = AtomicU8::new(PHASE_DISABLED);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFarmIdentity {
    pub device_id: String,
    pub display_name: String,
    pub public_key: String,
    #[serde(default)]
    pub transport_public_key: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFarmPeer {
    pub device_id: String,
    pub display_name: String,
    pub public_key: String,
    pub transport_public_key: String,
    pub fingerprint: String,
    pub trust_state: String,
    pub paired_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub last_capability_report_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFarmPairingOffer {
    pub schema_version: u16,
    pub offer_id: String,
    pub issuer: RenderFarmIdentity,
    pub nonce: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFarmPairingAcceptance {
    pub schema_version: u16,
    pub offer_id: String,
    pub controller_device_id: String,
    pub controller_public_key: String,
    #[serde(default)]
    pub controller_transport_public_key: String,
    pub nonce: String,
    pub accepter: RenderFarmIdentity,
    pub accepted_at: i64,
    pub signature: String,
}

/// Every encrypted farm message must have one of these narrow, audited uses.
/// This is deliberately not an "arbitrary command" channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RenderFarmTransportPurpose {
    CapabilityReport,
    RenderPackManifest,
    ResourceChunk,
    FrameLease,
    FrameResult,
}

impl RenderFarmTransportPurpose {
    fn max_ttl_ms(&self) -> i64 {
        match self {
            Self::ResourceChunk => 30 * 60 * 1_000,
            _ => TRANSPORT_TTL_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFarmTransportEnvelope {
    pub schema_version: u16,
    pub message_id: String,
    pub sender_device_id: String,
    pub recipient_device_id: String,
    pub purpose: RenderFarmTransportPurpose,
    pub created_at: i64,
    pub expires_at: i64,
    pub nonce: String,
    pub ciphertext: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SealRenderFarmTransportRequest {
    pub recipient_device_id: String,
    pub purpose: RenderFarmTransportPurpose,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRenderFarmTransportRequest {
    pub sender_device_id: String,
    pub envelope: RenderFarmTransportEnvelope,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedRenderFarmTransportMessage {
    pub message_id: String,
    pub sender_device_id: String,
    pub recipient_device_id: String,
    pub purpose: RenderFarmTransportPurpose,
    pub created_at: i64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRenderFarmPairingOfferRequest {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptRenderFarmPairingOfferRequest {
    pub offer: RenderFarmPairingOffer,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteRenderFarmPairingRequest {
    pub acceptance: RenderFarmPairingAcceptance,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeRenderFarmDeviceRequest {
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderNodeGpuCapability {
    pub name: String,
    pub vendor: String,
    pub vram_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderNodeBlenderCapability {
    pub version: String,
    pub build_hash: Option<String>,
    pub supported_engines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderNodeCapabilityReport {
    pub device_id: String,
    pub reported_at: i64,
    pub host_name: String,
    pub logical_cpu_count: u32,
    pub physical_memory_bytes: u64,
    pub free_disk_bytes: u64,
    #[serde(default)]
    pub gpus: Vec<RenderNodeGpuCapability>,
    #[serde(default)]
    pub blender: Vec<RenderNodeBlenderCapability>,
    #[serde(default)]
    pub component_versions: BTreeMap<String, String>,
    #[serde(default)]
    pub protocol_versions: Vec<u16>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderNodePreflightRequest {
    pub device_id: String,
    #[serde(default)]
    pub required_blender_version: Option<String>,
    #[serde(default)]
    pub required_component_ids: Vec<String>,
    #[serde(default)]
    pub minimum_free_disk_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderNodePreflightResult {
    pub device_id: String,
    pub eligible: bool,
    pub issues: Vec<String>,
    pub report: Option<RenderNodeCapabilityReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPackSettings {
    #[serde(default)]
    pub scene_name: Option<String>,
    pub frame_start: i64,
    pub frame_end: i64,
    pub output_format: String,
    #[serde(default)]
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPackResourceInput {
    pub source_path: String,
    #[serde(default)]
    pub archive_path: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRenderPackRequest {
    pub blend_path: String,
    pub destination_path: String,
    pub settings: RenderPackSettings,
    #[serde(default)]
    pub resources: Vec<RenderPackResourceInput>,
    #[serde(default)]
    pub required_components: BTreeMap<String, String>,
    #[serde(default)]
    pub blender_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPackFile {
    pub archive_path: String,
    pub role: String,
    pub blake3: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPackManifest {
    pub schema_version: u16,
    pub pack_id: String,
    pub created_at: i64,
    pub blender_version: Option<String>,
    pub required_components: BTreeMap<String, String>,
    pub settings: RenderPackSettings,
    pub files: Vec<RenderPackFile>,
    pub content_digest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPackSummary {
    pub pack_id: String,
    pub archive_path: String,
    pub archive_digest: String,
    pub manifest: RenderPackManifest,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRemoteRenderFrameRequest {
    pub pack_id: String,
    pub frame: i64,
    pub node_id: String,
    #[serde(default = "default_lease_seconds")]
    pub lease_seconds: u64,
}

fn default_lease_seconds() -> u64 {
    600
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRenderFrameLease {
    pub pack_id: String,
    pub frame: i64,
    pub node_id: String,
    pub lease_epoch: String,
    pub claim_token: String,
    pub status: String,
    pub leased_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRemoteRenderFrameResultRequest {
    pub pack_id: String,
    pub frame: i64,
    pub node_id: String,
    pub lease_epoch: String,
    pub claim_token: String,
    pub staged_path: String,
    pub blake3: String,
    #[serde(default)]
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRenderFrameCommit {
    pub pack_id: String,
    pub frame: i64,
    pub result_path: String,
    pub blake3: String,
    pub completed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFarmSnapshot {
    pub identity: RenderFarmIdentity,
    pub peers: Vec<RenderFarmPeer>,
    pub render_pack_count: i64,
    pub active_lease_count: i64,
    pub database_path: String,
    pub staging_root: String,
    pub results_root: String,
}

struct LocalIdentity {
    public: RenderFarmIdentity,
    key: SigningKey,
    transport_key: TransportSecret,
}

pub fn initialize_lifecycle_control() {
    PHASE.store(PHASE_DISABLED, Ordering::SeqCst);
}
pub fn set_initial_desired_enabled(enabled: bool) {
    PHASE.store(
        if enabled {
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
    PHASE.store(PHASE_DISABLED, Ordering::SeqCst);
    Ok(())
}
pub fn is_running() -> bool {
    PHASE.load(Ordering::SeqCst) == PHASE_RUNNING
}
pub fn ensure_running() -> Result<(), String> {
    match PHASE.load(Ordering::SeqCst) {
        PHASE_RUNNING => Ok(()),
        PHASE_STARTING => Err(format!("{RENDER_FARM_STARTING}: 渲染农场模块正在启动")),
        PHASE_STOPPING => Err(format!("{RENDER_FARM_STOPPING}: 渲染农场模块正在停止")),
        _ => Err(format!("{RENDER_FARM_DISABLED}: 渲染农场模块已停用")),
    }
}

fn now() -> i64 {
    Utc::now().timestamp_millis()
}
fn root(app_data: &Path) -> PathBuf {
    app_data.join(FARM_DIR)
}
fn db_path(root: &Path) -> PathBuf {
    root.join("render_farm.db")
}
fn staging(root: &Path) -> PathBuf {
    root.join("staging")
}
fn results(root: &Path) -> PathBuf {
    root.join("results")
}
fn app_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| root(&path))
        .map_err(|error| format!("获取应用数据目录失败: {error}"))
}

fn db(root: &Path) -> Result<Connection, String> {
    fs::create_dir_all(staging(root))
        .map_err(|error| format!("创建渲染农场暂存目录失败: {error}"))?;
    fs::create_dir_all(results(root))
        .map_err(|error| format!("创建渲染农场结果目录失败: {error}"))?;
    let conn = Connection::open(db_path(root))
        .map_err(|error| format!("打开渲染农场数据库失败: {error}"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
      CREATE TABLE IF NOT EXISTS farm_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS trusted_devices(device_id TEXT PRIMARY KEY,display_name TEXT NOT NULL,public_key TEXT NOT NULL UNIQUE,transport_public_key TEXT NOT NULL DEFAULT '',fingerprint TEXT NOT NULL UNIQUE,trust_state TEXT NOT NULL,paired_at INTEGER NOT NULL,last_seen_at INTEGER,revoked_at INTEGER);
      CREATE TABLE IF NOT EXISTS pending_pairings(offer_id TEXT PRIMARY KEY,nonce TEXT NOT NULL UNIQUE,expires_at INTEGER NOT NULL,issuer_device_id TEXT NOT NULL,issuer_public_key TEXT NOT NULL,issuer_transport_public_key TEXT NOT NULL DEFAULT '');
      CREATE TABLE IF NOT EXISTS consumed_transport_messages(sender_device_id TEXT NOT NULL,message_id TEXT NOT NULL,purpose TEXT NOT NULL,content_digest TEXT NOT NULL,received_at INTEGER NOT NULL,expires_at INTEGER NOT NULL,PRIMARY KEY(sender_device_id,message_id));
      CREATE INDEX IF NOT EXISTS consumed_transport_messages_expiry ON consumed_transport_messages(expires_at);
      CREATE TABLE IF NOT EXISTS capability_reports(id TEXT PRIMARY KEY,device_id TEXT NOT NULL,reported_at INTEGER NOT NULL,report_json TEXT NOT NULL,content_digest TEXT NOT NULL,FOREIGN KEY(device_id) REFERENCES trusted_devices(device_id) ON DELETE CASCADE);
      CREATE INDEX IF NOT EXISTS capability_reports_latest ON capability_reports(device_id,reported_at DESC);
      CREATE TABLE IF NOT EXISTS render_packs(pack_id TEXT PRIMARY KEY,archive_path TEXT NOT NULL UNIQUE,archive_digest TEXT NOT NULL,manifest_json TEXT NOT NULL,created_at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS render_pack_files(pack_id TEXT NOT NULL,archive_path TEXT NOT NULL,role TEXT NOT NULL,blake3 TEXT NOT NULL,size INTEGER NOT NULL,PRIMARY KEY(pack_id,archive_path),FOREIGN KEY(pack_id) REFERENCES render_packs(pack_id) ON DELETE CASCADE);
      CREATE TABLE IF NOT EXISTS frame_leases(pack_id TEXT NOT NULL,frame INTEGER NOT NULL,node_id TEXT NOT NULL,lease_epoch TEXT NOT NULL,claim_token TEXT NOT NULL,status TEXT NOT NULL,leased_at INTEGER NOT NULL,expires_at INTEGER NOT NULL,completed_at INTEGER,result_path TEXT,result_hash TEXT,last_error TEXT,PRIMARY KEY(pack_id,frame),FOREIGN KEY(pack_id) REFERENCES render_packs(pack_id) ON DELETE CASCADE,FOREIGN KEY(node_id) REFERENCES trusted_devices(device_id));
      CREATE INDEX IF NOT EXISTS frame_leases_active ON frame_leases(status,expires_at);")
      .map_err(|error| format!("迁移渲染农场数据库失败: {error}"))?;
    ensure_column(
        &conn,
        "trusted_devices",
        "transport_public_key",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &conn,
        "pending_pairings",
        "issuer_transport_public_key",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    Ok(conn)
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let exists = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("读取渲染农场迁移信息失败: {error}"))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("读取渲染农场迁移信息失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取渲染农场迁移信息失败: {error}"))?
        .iter()
        .any(|value| value == column);
    if exists {
        return Ok(());
    }
    conn.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition}"
    ))
    .map_err(|error| format!("迁移渲染农场数据库字段失败: {error}"))
}

fn valid_id(value: &str, label: &str) -> Result<(), String> {
    if !(2..=128).contains(&value.len())
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        Err(format!("{label}无效"))
    } else {
        Ok(())
    }
}
fn display_name(value: Option<&str>, fallback: &str) -> String {
    let value = value.unwrap_or(fallback).trim();
    let value = if value.is_empty() { fallback } else { value };
    value.chars().take(64).collect()
}
fn decode<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    BASE64
        .decode(value)
        .map_err(|_| format!("{label}不是有效 Base64"))?
        .try_into()
        .map_err(|_| format!("{label}长度无效"))
}
fn fingerprint(public_key: &str) -> String {
    blake3::hash(public_key.as_bytes()).to_hex().to_string()
}
fn public_key(key: &SigningKey) -> String {
    BASE64.encode(key.verifying_key().to_bytes())
}
fn transport_public_key(key: &TransportSecret) -> String {
    BASE64.encode(TransportPublicKey::from(key).as_bytes())
}
fn transport_public_key_bytes(value: &str, label: &str) -> Result<TransportPublicKey, String> {
    if value.trim().is_empty() {
        return Err(format!("{label}缺少加密公钥；请撤销并重新配对该设备"));
    }
    Ok(TransportPublicKey::from(decode::<32>(value, label)?))
}
fn signature(key: &SigningKey, text: &str) -> String {
    BASE64.encode(key.sign(text.as_bytes()).to_bytes())
}
fn verify(public_key: &str, signature: &str, text: &str) -> Result<(), String> {
    let public_key = VerifyingKey::from_bytes(&decode::<32>(public_key, "设备公钥")?)
        .map_err(|_| "设备公钥无效".to_string())?;
    let signature = Signature::from_bytes(&decode::<64>(signature, "签名")?);
    public_key
        .verify(text.as_bytes(), &signature)
        .map_err(|_| "签名校验失败".into())
}

fn identity(root: &Path, requested_name: Option<&str>) -> Result<LocalIdentity, String> {
    let conn = db(root)?;
    let device_id = match conn
        .query_row(
            "SELECT value FROM farm_meta WHERE key='device_id'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    {
        Some(value) => value,
        None => {
            let value = format!("device-{}", Uuid::new_v4());
            conn.execute(
                "INSERT INTO farm_meta(key,value) VALUES('device_id',?1)",
                params![value],
            )
            .map_err(|error| error.to_string())?;
            value
        }
    };
    let old_name: Option<String> = conn
        .query_row(
            "SELECT value FROM farm_meta WHERE key='display_name'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let name = display_name(
        requested_name,
        old_name.as_deref().unwrap_or("Nexora 渲染节点"),
    );
    conn.execute("INSERT INTO farm_meta(key,value) VALUES('display_name',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![name]).map_err(|error| error.to_string())?;
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| format!("打开 Windows 凭据保护失败: {error}"))?;
    let seed = match entry.get_password() {
        Ok(value) => decode::<32>(&value, "设备私钥")?,
        Err(keyring::Error::NoEntry) => {
            let key = SigningKey::generate(&mut OsRng);
            entry
                .set_password(&BASE64.encode(key.to_bytes()))
                .map_err(|error| format!("保存设备私钥失败: {error}"))?;
            key.to_bytes()
        }
        Err(error) => return Err(format!("读取 Windows 凭据保护失败: {error}")),
    };
    let transport_entry = Entry::new(KEYRING_SERVICE, TRANSPORT_KEYRING_ACCOUNT)
        .map_err(|error| format!("打开 Windows 凭据保护失败: {error}"))?;
    let transport_seed = match transport_entry.get_password() {
        Ok(value) => decode::<32>(&value, "设备传输私钥")?,
        Err(keyring::Error::NoEntry) => {
            let mut value = [0_u8; 32];
            OsRng.fill_bytes(&mut value);
            transport_entry
                .set_password(&BASE64.encode(value))
                .map_err(|error| format!("保存设备传输私钥失败: {error}"))?;
            value
        }
        Err(error) => return Err(format!("读取 Windows 凭据保护失败: {error}")),
    };
    let key = SigningKey::from_bytes(&seed);
    let public_key = public_key(&key);
    let transport_key = TransportSecret::from(transport_seed);
    Ok(LocalIdentity {
        public: RenderFarmIdentity {
            device_id,
            display_name: name,
            fingerprint: fingerprint(&public_key),
            public_key,
            transport_public_key: transport_public_key(&transport_key),
        },
        key,
        transport_key,
    })
}

fn offer_text(offer: &RenderFarmPairingOffer) -> String {
    format!(
        "nexora.render-farm.offer.v2\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        offer.schema_version,
        offer.offer_id,
        offer.issuer.device_id,
        offer.issuer.public_key,
        offer.issuer.transport_public_key,
        offer.nonce,
        offer.created_at,
        offer.expires_at
    )
}
fn acceptance_text(value: &RenderFarmPairingAcceptance) -> String {
    format!(
        "nexora.render-farm.acceptance.v2\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        value.schema_version,
        value.offer_id,
        value.controller_device_id,
        value.controller_public_key,
        value.controller_transport_public_key,
        value.nonce,
        value.accepter.device_id,
        value.accepter.public_key,
        value.accepter.transport_public_key,
        value.accepted_at
    )
}
fn validate_offer(offer: &RenderFarmPairingOffer, timestamp: i64) -> Result<(), String> {
    valid_id(&offer.offer_id, "配对请求 ID")?;
    valid_id(&offer.issuer.device_id, "控制端设备 ID")?;
    if offer.schema_version != 2
        || offer.nonce.len() < 16
        || offer.expires_at <= timestamp
        || offer.expires_at - offer.created_at > PAIRING_TTL_MS + 5_000
    {
        return Err("配对请求已过期或格式无效".into());
    }
    if fingerprint(&offer.issuer.public_key) != offer.issuer.fingerprint {
        return Err("控制端设备指纹不匹配".into());
    }
    transport_public_key_bytes(&offer.issuer.transport_public_key, "控制端设备")?;
    verify(
        &offer.issuer.public_key,
        &offer.signature,
        &offer_text(offer),
    )
}
fn make_offer(identity: &LocalIdentity, timestamp: i64) -> RenderFarmPairingOffer {
    let mut offer = RenderFarmPairingOffer {
        schema_version: 2,
        offer_id: format!("pair-{}", Uuid::new_v4()),
        issuer: identity.public.clone(),
        nonce: Uuid::new_v4().to_string(),
        created_at: timestamp,
        expires_at: timestamp + PAIRING_TTL_MS,
        signature: String::new(),
    };
    offer.signature = signature(&identity.key, &offer_text(&offer));
    offer
}
fn make_acceptance(
    identity: &LocalIdentity,
    offer: &RenderFarmPairingOffer,
    timestamp: i64,
) -> RenderFarmPairingAcceptance {
    let mut acceptance = RenderFarmPairingAcceptance {
        schema_version: 2,
        offer_id: offer.offer_id.clone(),
        controller_device_id: offer.issuer.device_id.clone(),
        controller_public_key: offer.issuer.public_key.clone(),
        controller_transport_public_key: offer.issuer.transport_public_key.clone(),
        nonce: offer.nonce.clone(),
        accepter: identity.public.clone(),
        accepted_at: timestamp,
        signature: String::new(),
    };
    acceptance.signature = signature(&identity.key, &acceptance_text(&acceptance));
    acceptance
}

fn peer(conn: &Connection, device_id: &str) -> Result<Option<RenderFarmPeer>, String> {
    conn.query_row("SELECT d.device_id,d.display_name,d.public_key,d.transport_public_key,d.fingerprint,d.trust_state,d.paired_at,d.last_seen_at,d.revoked_at,(SELECT MAX(reported_at) FROM capability_reports c WHERE c.device_id=d.device_id) FROM trusted_devices d WHERE d.device_id=?1", params![device_id], |row| Ok(RenderFarmPeer { device_id: row.get(0)?, display_name: row.get(1)?, public_key: row.get(2)?, transport_public_key: row.get(3)?, fingerprint: row.get(4)?, trust_state: row.get(5)?, paired_at: row.get(6)?, last_seen_at: row.get(7)?, revoked_at: row.get(8)?, last_capability_report_at: row.get(9)? })).optional().map_err(|error| error.to_string())
}
fn trust(
    conn: &Connection,
    identity: &RenderFarmIdentity,
    timestamp: i64,
) -> Result<RenderFarmPeer, String> {
    valid_id(&identity.device_id, "设备 ID")?;
    if fingerprint(&identity.public_key) != identity.fingerprint {
        return Err("设备指纹与公钥不匹配".into());
    }
    transport_public_key_bytes(&identity.transport_public_key, "设备")?;
    conn.execute("INSERT INTO trusted_devices(device_id,display_name,public_key,transport_public_key,fingerprint,trust_state,paired_at,last_seen_at,revoked_at) VALUES(?1,?2,?3,?4,?5,'trusted',?6,?6,NULL) ON CONFLICT(device_id) DO UPDATE SET display_name=excluded.display_name,public_key=excluded.public_key,transport_public_key=excluded.transport_public_key,fingerprint=excluded.fingerprint,trust_state='trusted',paired_at=excluded.paired_at,last_seen_at=excluded.last_seen_at,revoked_at=NULL", params![identity.device_id, display_name(Some(&identity.display_name), "Nexora 渲染节点"), identity.public_key, identity.transport_public_key, identity.fingerprint, timestamp]).map_err(|error| error.to_string())?;
    peer(conn, &identity.device_id)?.ok_or_else(|| "保存可信设备失败".into())
}
fn trusted(conn: &Connection, device_id: &str) -> Result<RenderFarmPeer, String> {
    valid_id(device_id, "设备 ID")?;
    let peer = peer(conn, device_id)?.ok_or_else(|| "设备未配对，不能访问渲染农场".to_string())?;
    if peer.trust_state != "trusted" || peer.revoked_at.is_some() {
        Err("设备已被撤销信任，不能访问渲染农场".into())
    } else {
        Ok(peer)
    }
}

fn transport_aad(value: &RenderFarmTransportEnvelope) -> Vec<u8> {
    format!(
        "nexora.render-farm.transport.aad.v1\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        value.schema_version,
        value.message_id,
        value.sender_device_id,
        value.recipient_device_id,
        serde_json::to_string(&value.purpose).unwrap_or_default(),
        value.created_at,
        value.expires_at,
    )
    .into_bytes()
}

fn transport_signature_text(value: &RenderFarmTransportEnvelope) -> String {
    format!(
        "nexora.render-farm.transport.signature.v1\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        value.schema_version,
        value.message_id,
        value.sender_device_id,
        value.recipient_device_id,
        serde_json::to_string(&value.purpose).unwrap_or_default(),
        value.created_at,
        value.expires_at,
        value.nonce,
        value.ciphertext,
    )
}

fn transport_key(
    private_key: &TransportSecret,
    peer_public_key: &str,
    local_device_id: &str,
    peer_device_id: &str,
) -> Result<[u8; 32], String> {
    let peer_key = transport_public_key_bytes(peer_public_key, "对端设备")?;
    let shared = private_key.diffie_hellman(&peer_key);
    let (first, second) = if local_device_id <= peer_device_id {
        (local_device_id, peer_device_id)
    } else {
        (peer_device_id, local_device_id)
    };
    let mut material = Vec::with_capacity(32 + first.len() + second.len() + 2);
    material.extend_from_slice(shared.as_bytes());
    material.extend_from_slice(first.as_bytes());
    material.push(0);
    material.extend_from_slice(second.as_bytes());
    Ok(blake3::derive_key(
        "nexora.render-farm.transport-key.v1",
        &material,
    ))
}

fn validate_envelope(value: &RenderFarmTransportEnvelope, timestamp: i64) -> Result<(), String> {
    valid_id(&value.message_id, "传输消息 ID")?;
    valid_id(&value.sender_device_id, "发送设备 ID")?;
    valid_id(&value.recipient_device_id, "接收设备 ID")?;
    if value.schema_version != 1
        || value.expires_at <= timestamp
        || value.created_at > timestamp + 60_000
        || value.expires_at - value.created_at > value.purpose.max_ttl_ms() + 5_000
    {
        return Err("加密传输消息已过期或格式无效".into());
    }
    let nonce = BASE64
        .decode(&value.nonce)
        .map_err(|_| "加密传输随机数不是有效 Base64".to_string())?;
    if nonce.len() != 24 {
        return Err("加密传输随机数长度无效".into());
    }
    let ciphertext = BASE64
        .decode(&value.ciphertext)
        .map_err(|_| "加密传输内容不是有效 Base64".to_string())?;
    if ciphertext.len() < 16 || ciphertext.len() > MAX_TRANSPORT_PAYLOAD_BYTES + 16 {
        return Err("加密传输内容大小无效".into());
    }
    Ok(())
}

fn seal_transport(
    identity: &LocalIdentity,
    peer: &RenderFarmPeer,
    purpose: RenderFarmTransportPurpose,
    payload: &serde_json::Value,
    timestamp: i64,
) -> Result<RenderFarmTransportEnvelope, String> {
    valid_id(&peer.device_id, "接收设备 ID")?;
    let plaintext =
        serde_json::to_vec(payload).map_err(|error| format!("序列化传输内容失败: {error}"))?;
    if plaintext.len() > MAX_TRANSPORT_PAYLOAD_BYTES {
        return Err("加密传输内容超过 1 MB 安全上限".into());
    }
    let mut nonce = [0_u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let expires_at = timestamp + purpose.max_ttl_ms();
    let mut envelope = RenderFarmTransportEnvelope {
        schema_version: 1,
        message_id: format!("farm-msg-{}", Uuid::new_v4()),
        sender_device_id: identity.public.device_id.clone(),
        recipient_device_id: peer.device_id.clone(),
        purpose,
        created_at: timestamp,
        expires_at,
        nonce: BASE64.encode(nonce),
        ciphertext: String::new(),
        signature: String::new(),
    };
    let key = transport_key(
        &identity.transport_key,
        &peer.transport_public_key,
        &identity.public.device_id,
        &peer.device_id,
    )?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    envelope.ciphertext = BASE64.encode(
        cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: &transport_aad(&envelope),
                },
            )
            .map_err(|_| "加密渲染农场传输内容失败".to_string())?,
    );
    envelope.signature = signature(&identity.key, &transport_signature_text(&envelope));
    Ok(envelope)
}

fn open_transport(
    identity: &LocalIdentity,
    peer: &RenderFarmPeer,
    envelope: &RenderFarmTransportEnvelope,
    timestamp: i64,
) -> Result<OpenedRenderFarmTransportMessage, String> {
    validate_envelope(envelope, timestamp)?;
    if envelope.sender_device_id != peer.device_id
        || envelope.recipient_device_id != identity.public.device_id
    {
        return Err("加密传输消息的设备绑定不匹配".into());
    }
    verify(
        &peer.public_key,
        &envelope.signature,
        &transport_signature_text(envelope),
    )?;
    let nonce = decode::<24>(&envelope.nonce, "加密传输随机数")?;
    let ciphertext = BASE64
        .decode(&envelope.ciphertext)
        .map_err(|_| "加密传输内容不是有效 Base64".to_string())?;
    let key = transport_key(
        &identity.transport_key,
        &peer.transport_public_key,
        &identity.public.device_id,
        &peer.device_id,
    )?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &transport_aad(envelope),
            },
        )
        .map_err(|_| "加密传输内容校验失败或已被篡改".to_string())?;
    let payload = serde_json::from_slice(&plaintext)
        .map_err(|error| format!("加密传输内容不是有效 JSON: {error}"))?;
    Ok(OpenedRenderFarmTransportMessage {
        message_id: envelope.message_id.clone(),
        sender_device_id: envelope.sender_device_id.clone(),
        recipient_device_id: envelope.recipient_device_id.clone(),
        purpose: envelope.purpose.clone(),
        created_at: envelope.created_at,
        payload,
    })
}

fn consume_transport_message(
    root: &Path,
    envelope: &RenderFarmTransportEnvelope,
) -> Result<(), String> {
    let mut conn = db(root)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM consumed_transport_messages WHERE expires_at<=?1",
        params![now()],
    )
    .map_err(|error| error.to_string())?;
    let content_digest = blake3::hash(envelope.ciphertext.as_bytes())
        .to_hex()
        .to_string();
    match tx.execute(
        "INSERT INTO consumed_transport_messages(sender_device_id,message_id,purpose,content_digest,received_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            envelope.sender_device_id,
            envelope.message_id,
            serde_json::to_string(&envelope.purpose).map_err(|error| error.to_string())?,
            content_digest,
            now(),
            envelope.expires_at,
        ],
    ) {
        Ok(_) => tx.commit().map_err(|error| error.to_string()),
        Err(rusqlite::Error::SqliteFailure(
            error,
            Some(message),
        )) if error.code == rusqlite::ErrorCode::ConstraintViolation
            || message.contains("UNIQUE constraint failed") =>
        {
            Err("加密传输消息已被接收，拒绝重复投递".into())
        }
        Err(error) => Err(format!("记录加密传输消息失败: {error}")),
    }
}

fn validate_report(report: &RenderNodeCapabilityReport) -> Result<(), String> {
    valid_id(&report.device_id, "设备 ID")?;
    if report.host_name.trim().is_empty()
        || report.host_name.len() > 128
        || report.logical_cpu_count == 0
        || report.logical_cpu_count > 4096
        || report.gpus.len() > 32
        || report.blender.len() > 32
        || report.component_versions.len() > 512
    {
        return Err("节点能力报告格式无效".into());
    }
    if serde_json::to_vec(report)
        .map_err(|error| error.to_string())?
        .len()
        > 256 * 1024
    {
        return Err("节点能力报告过大".into());
    }
    for (id, version) in &report.component_versions {
        valid_id(id, "组件 ID")?;
        if version.is_empty() || version.len() > 128 {
            return Err("组件版本无效".into());
        }
    }
    Ok(())
}
fn latest_report(
    conn: &Connection,
    device_id: &str,
) -> Result<Option<RenderNodeCapabilityReport>, String> {
    conn.query_row("SELECT report_json FROM capability_reports WHERE device_id=?1 ORDER BY reported_at DESC,id DESC LIMIT 1", params![device_id], |row| row.get::<_, String>(0)).optional().map_err(|error| error.to_string())?.map(|json| serde_json::from_str(&json).map_err(|error| format!("节点能力报告损坏: {error}"))).transpose()
}

fn record_capability_report(
    root: &Path,
    report: &RenderNodeCapabilityReport,
) -> Result<(), String> {
    validate_report(report)?;
    let conn = db(root)?;
    trusted(&conn, &report.device_id)?;
    let json = serde_json::to_string(report).map_err(|error| error.to_string())?;
    conn.execute("INSERT INTO capability_reports(id,device_id,reported_at,report_json,content_digest) VALUES(?1,?2,?3,?4,?5)",params![Uuid::new_v4().to_string(),report.device_id,report.reported_at,json,blake3::hash(json.as_bytes()).to_hex().to_string()]).map_err(|error|error.to_string())?;
    conn.execute(
        "UPDATE trusted_devices SET last_seen_at=?2 WHERE device_id=?1",
        params![report.device_id, now()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn preflight_node(
    conn: &Connection,
    request: &RenderNodePreflightRequest,
) -> Result<RenderNodePreflightResult, String> {
    let mut issues = Vec::new();
    if let Err(error) = trusted(conn, &request.device_id) {
        issues.push(error);
    }
    let report = latest_report(conn, &request.device_id)?;
    let Some(report) = report else {
        issues.push("节点尚未提交能力报告".into());
        return Ok(RenderNodePreflightResult {
            device_id: request.device_id.clone(),
            eligible: false,
            issues,
            report: None,
        });
    };
    if report.free_disk_bytes < request.minimum_free_disk_bytes {
        issues.push("节点可用磁盘空间不足".into());
    }
    if let Some(version) = request.required_blender_version.as_deref() {
        if !report.blender.iter().any(|item| item.version == version) {
            issues.push(format!("节点缺少 Blender {version}"));
        }
    }
    for component in &request.required_component_ids {
        valid_id(component, "组件 ID")?;
        if !report.component_versions.contains_key(component) {
            issues.push(format!("节点缺少组件：{component}"));
        }
    }
    Ok(RenderNodePreflightResult {
        device_id: request.device_id.clone(),
        eligible: issues.is_empty(),
        issues,
        report: Some(report),
    })
}

fn source(path: &str, label: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("{label}必须是存在的普通文件"));
    }
    path.canonicalize()
        .map_err(|error| format!("无法解析{label}: {error}"))
}
fn relative(value: &str) -> Result<PathBuf, String> {
    let value = Path::new(value);
    if value.as_os_str().is_empty() || value.is_absolute() {
        return Err("渲染包路径必须是非空相对路径".into());
    }
    let mut output = PathBuf::new();
    for component in value.components() {
        match component {
            Component::Normal(part) if !part.is_empty() => output.push(part),
            Component::CurDir => {}
            Component::Normal(_)
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => return Err("渲染包路径不能越界".into()),
        }
    }
    if output.as_os_str().is_empty() {
        Err("渲染包路径无效".into())
    } else {
        Ok(output)
    }
}
fn digest(path: &Path) -> Result<(String, u64), String> {
    let mut file = fs::File::open(path).map_err(|error| format!("读取文件失败: {error}"))?;
    let mut hasher = blake3::Hasher::new();
    let mut size = 0;
    let mut buffer = [0; 131_072];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("读取文件失败: {error}"))?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > MAX_PACK_BYTES {
            return Err("渲染包资源总大小超过 32 GB 安全上限".into());
        }
        hasher.update(&buffer[..count]);
    }
    Ok((hasher.finalize().to_hex().to_string(), size))
}
fn pack_manifest(
    request: &CreateRenderPackRequest,
) -> Result<(RenderPackManifest, Vec<(PathBuf, String)>), String> {
    let blend = source(&request.blend_path, "Blender 文件")?;
    if !blend
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("blend"))
    {
        return Err("渲染包主文件必须是 .blend".into());
    }
    if request.settings.frame_end < request.settings.frame_start
        || request.settings.output_format.trim().is_empty()
        || request.settings.output_format.len() > 32
        || request.resources.len() + 1 > MAX_PACK_FILES
    {
        return Err("渲染包参数无效".into());
    }
    let name = blend
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Blender 文件名无效".to_string())?;
    let mut files = Vec::new();
    let mut payloads = Vec::new();
    let mut targets = BTreeSet::new();
    let target = format!("scene/{name}");
    let (hash, size) = digest(&blend)?;
    targets.insert(target.to_ascii_lowercase());
    files.push(RenderPackFile {
        archive_path: target.clone(),
        role: "blend".into(),
        blake3: hash,
        size,
    });
    payloads.push((blend, target));
    for resource in &request.resources {
        let source = source(&resource.source_path, "渲染包资源")?;
        let fallback = source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "资源文件名无效".to_string())?;
        let target = format!(
            "resources/{}",
            relative(resource.archive_path.as_deref().unwrap_or(fallback))?
                .to_string_lossy()
                .replace('\\', "/")
        );
        if !targets.insert(target.to_ascii_lowercase()) {
            return Err(format!("渲染包内存在重复资源路径: {target}"));
        }
        let (hash, size) = digest(&source)?;
        files.push(RenderPackFile {
            archive_path: target.clone(),
            role: resource.kind.clone().unwrap_or_else(|| "resource".into()),
            blake3: hash,
            size,
        });
        payloads.push((source, target));
    }
    files.sort_by(|a, b| a.archive_path.cmp(&b.archive_path));
    payloads.sort_by(|a, b| a.1.cmp(&b.1));
    let created_at = now();
    let canonical = serde_json::json!({"schemaVersion":1,"createdAt":created_at,"blenderVersion":request.blender_version,"requiredComponents":request.required_components,"settings":request.settings,"files":files});
    let content_digest =
        blake3::hash(&serde_json::to_vec(&canonical).map_err(|error| error.to_string())?)
            .to_hex()
            .to_string();
    Ok((
        RenderPackManifest {
            schema_version: 1,
            pack_id: format!("renderpack-{}", Uuid::new_v4()),
            created_at,
            blender_version: request.blender_version.clone(),
            required_components: request.required_components.clone(),
            settings: request.settings.clone(),
            files,
            content_digest,
        },
        payloads,
    ))
}
fn create_pack(
    root: &Path,
    request: &CreateRenderPackRequest,
) -> Result<RenderPackSummary, String> {
    let (manifest, payloads) = pack_manifest(request)?;
    let mut destination = PathBuf::from(&request.destination_path);
    if destination.extension().is_none() {
        destination.set_extension("pmc-renderpack");
    }
    if !destination
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("pmc-renderpack"))
    {
        return Err("渲染包必须使用 .pmc-renderpack 扩展名".into());
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "渲染包输出路径无效".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建渲染包输出目录失败: {error}"))?;
    if destination.exists() {
        return Err("渲染包输出已存在，为避免覆盖已拒绝创建".into());
    }
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let write = (|| -> Result<(), String> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("创建渲染包临时文件失败: {error}"))?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        zip.start_file("manifest.json", options)
            .map_err(|error| error.to_string())?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        for (source, path) in &payloads {
            zip.start_file(format!("payload/{path}"), options)
                .map_err(|error| error.to_string())?;
            std::io::copy(
                &mut fs::File::open(source).map_err(|error| error.to_string())?,
                &mut zip,
            )
            .map_err(|error| error.to_string())?;
        }
        zip.finish().map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = write {
        let _ = fs::remove_file(&temporary);
        return Err(format!("写入渲染包失败: {error}"));
    }
    fs::rename(&temporary, &destination).map_err(|error| format!("提交渲染包失败: {error}"))?;
    let (archive_digest, _) = digest(&destination)?;
    let conn = db(root)?;
    let record = (|| -> Result<(), String> {
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        tx.execute("INSERT INTO render_packs(pack_id,archive_path,archive_digest,manifest_json,created_at) VALUES(?1,?2,?3,?4,?5)", params![manifest.pack_id,destination.to_string_lossy(),archive_digest,serde_json::to_string(&manifest).map_err(|error| error.to_string())?,manifest.created_at]).map_err(|error| error.to_string())?;
        for file in &manifest.files {
            tx.execute("INSERT INTO render_pack_files(pack_id,archive_path,role,blake3,size) VALUES(?1,?2,?3,?4,?5)", params![manifest.pack_id,file.archive_path,file.role,file.blake3,file.size as i64]).map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())
    })();
    if let Err(error) = record {
        let _ = fs::remove_file(&destination);
        return Err(format!("记录渲染包失败，已回滚输出文件: {error}"));
    }
    Ok(RenderPackSummary {
        pack_id: manifest.pack_id.clone(),
        archive_path: destination.to_string_lossy().into_owned(),
        archive_digest,
        manifest,
    })
}

fn pack_exists(conn: &Connection, pack_id: &str) -> Result<(), String> {
    valid_id(pack_id, "渲染包 ID")?;
    if conn
        .query_row(
            "SELECT 1 FROM render_packs WHERE pack_id=?1",
            params![pack_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        Ok(())
    } else {
        Err("渲染包不存在".into())
    }
}
fn read_lease(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemoteRenderFrameLease> {
    Ok(RemoteRenderFrameLease {
        pack_id: row.get(0)?,
        frame: row.get(1)?,
        node_id: row.get(2)?,
        lease_epoch: row.get(3)?,
        claim_token: row.get(4)?,
        status: row.get(5)?,
        leased_at: row.get(6)?,
        expires_at: row.get(7)?,
    })
}
fn claim(
    root: &Path,
    request: &ClaimRemoteRenderFrameRequest,
) -> Result<RemoteRenderFrameLease, String> {
    valid_id(&request.pack_id, "渲染包 ID")?;
    if request.frame < 0 || request.lease_seconds == 0 || request.lease_seconds > 86_400 {
        return Err("远程帧租约参数无效".into());
    }
    let mut conn = db(root)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    trusted(&tx, &request.node_id)?;
    pack_exists(&tx, &request.pack_id)?;
    let timestamp = now();
    if let Some(existing) = tx.query_row("SELECT pack_id,frame,node_id,lease_epoch,claim_token,status,leased_at,expires_at FROM frame_leases WHERE pack_id=?1 AND frame=?2", params![request.pack_id,request.frame],read_lease).optional().map_err(|error| error.to_string())? { if existing.status == "completed" { return Err("该帧已由控制端提交，不能再次领取".into()); } if matches!(existing.status.as_str(), "leased" | "committing") && existing.expires_at > timestamp { return Err("该帧正在由可信节点领取或提交".into()); } }
    let lease = RemoteRenderFrameLease {
        pack_id: request.pack_id.clone(),
        frame: request.frame,
        node_id: request.node_id.clone(),
        lease_epoch: Uuid::new_v4().to_string(),
        claim_token: Uuid::new_v4().to_string(),
        status: "leased".into(),
        leased_at: timestamp,
        expires_at: timestamp + request.lease_seconds as i64 * 1000,
    };
    tx.execute("INSERT INTO frame_leases(pack_id,frame,node_id,lease_epoch,claim_token,status,leased_at,expires_at,completed_at,result_path,result_hash,last_error) VALUES(?1,?2,?3,?4,?5,'leased',?6,?7,NULL,NULL,NULL,NULL) ON CONFLICT(pack_id,frame) DO UPDATE SET node_id=excluded.node_id,lease_epoch=excluded.lease_epoch,claim_token=excluded.claim_token,status='leased',leased_at=excluded.leased_at,expires_at=excluded.expires_at,completed_at=NULL,result_path=NULL,result_hash=NULL,last_error=NULL",params![lease.pack_id,lease.frame,lease.node_id,lease.lease_epoch,lease.claim_token,lease.leased_at,lease.expires_at]).map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(lease)
}
#[cfg(test)]
fn staged_path(root: &Path, lease: &RemoteRenderFrameLease) -> PathBuf {
    staging(root)
        .join("incoming")
        .join(&lease.pack_id)
        .join(&lease.lease_epoch)
        .join(format!("frame-{:06}.upload", lease.frame))
}
fn under(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("无法解析受控暂存目录: {error}"))?;
    let path = path
        .canonicalize()
        .map_err(|error| format!("无法解析上传暂存文件: {error}"))?;
    if path.starts_with(&root) {
        Ok(path)
    } else {
        Err("远程结果必须先写入控制端私有暂存目录".into())
    }
}
fn extension(value: Option<&str>) -> Result<&str, String> {
    let value = value.unwrap_or("bin");
    if !(1..=12).contains(&value.len()) || !value.chars().all(|c| c.is_ascii_alphanumeric()) {
        Err("远程结果扩展名无效".into())
    } else {
        Ok(value)
    }
}
fn commit(
    root: &Path,
    request: &CommitRemoteRenderFrameResultRequest,
) -> Result<RemoteRenderFrameCommit, String> {
    valid_id(&request.pack_id, "渲染包 ID")?;
    valid_id(&request.node_id, "设备 ID")?;
    let extension = extension(request.extension.as_deref())?;
    let incoming = staging(root).join("incoming");
    fs::create_dir_all(&incoming).map_err(|error| format!("创建结果暂存目录失败: {error}"))?;
    let staged = under(&incoming, Path::new(&request.staged_path))?;
    if !staged.is_file() {
        return Err("远程结果暂存文件不存在".into());
    }
    let (hash, size) = digest(&staged)?;
    if size == 0 || hash != request.blake3 {
        return Err("远程结果校验失败，内容哈希不匹配或文件为空".into());
    }
    let mut conn = db(root)?;
    let timestamp = now();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    trusted(&tx, &request.node_id)?;
    let lease=tx.query_row("SELECT pack_id,frame,node_id,lease_epoch,claim_token,status,leased_at,expires_at FROM frame_leases WHERE pack_id=?1 AND frame=?2",params![request.pack_id,request.frame],read_lease).optional().map_err(|error| error.to_string())?.ok_or_else(||"远程帧没有有效租约".to_string())?;
    if lease.status != "leased"
        || lease.expires_at <= timestamp
        || lease.node_id != request.node_id
        || lease.lease_epoch != request.lease_epoch
        || lease.claim_token != request.claim_token
    {
        return Err("远程结果的租约令牌已过期或不匹配，已拒绝提交".into());
    }
    let target_dir = results(root).join(&lease.pack_id);
    fs::create_dir_all(&target_dir).map_err(|error| format!("创建控制端结果目录失败: {error}"))?;
    let target = target_dir.join(format!("frame-{:06}.{extension}", lease.frame));
    if target.exists() {
        return Err("控制端结果目标已存在，拒绝覆盖".into());
    }
    if tx.execute("UPDATE frame_leases SET status='committing',result_path=?6,result_hash=?7,last_error=NULL WHERE pack_id=?1 AND frame=?2 AND node_id=?3 AND lease_epoch=?4 AND claim_token=?5 AND status='leased'",params![lease.pack_id,lease.frame,lease.node_id,lease.lease_epoch,lease.claim_token,target.to_string_lossy(),hash]).map_err(|error|error.to_string())? != 1 {return Err("远程结果提交竞争失败".into());}
    tx.commit().map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&staged, &target) {
        conn.execute("UPDATE frame_leases SET status='leased',result_path=NULL,result_hash=NULL,last_error=?6 WHERE pack_id=?1 AND frame=?2 AND node_id=?3 AND lease_epoch=?4 AND claim_token=?5 AND status='committing'",params![lease.pack_id,lease.frame,lease.node_id,lease.lease_epoch,lease.claim_token,format!("控制端提交失败: {error}")]).map_err(|rollback|format!("提交失败且恢复租约失败: {error}; {rollback}"))?;
        return Err(format!("提交远程结果失败: {error}"));
    }
    if conn.execute("UPDATE frame_leases SET status='completed',completed_at=?6,last_error=NULL WHERE pack_id=?1 AND frame=?2 AND node_id=?3 AND lease_epoch=?4 AND claim_token=?5 AND status='committing'",params![lease.pack_id,lease.frame,lease.node_id,lease.lease_epoch,lease.claim_token,timestamp]).map_err(|error|format!("提交状态失败；下次恢复会校验: {error}"))? != 1 {return Err("远程结果提交状态发生竞争；下次恢复会校验".into());}
    Ok(RemoteRenderFrameCommit {
        pack_id: request.pack_id.clone(),
        frame: request.frame,
        result_path: target.to_string_lossy().into_owned(),
        blake3: hash,
        completed_at: timestamp,
    })
}
fn recover(root: &Path) -> Result<usize, String> {
    let mut conn = db(root)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let rows = {
        let mut statement=tx.prepare("SELECT pack_id,frame,node_id,lease_epoch,claim_token,result_path,result_hash FROM frame_leases WHERE status='committing'").map_err(|error|error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    let mut count = 0;
    for (pack, frame, node, epoch, token, path, expected) in rows {
        let valid = path
            .as_deref()
            .filter(|path| Path::new(path).is_file())
            .and_then(|path| digest(Path::new(path)).ok())
            .is_some_and(|(hash, size)| size > 0 && Some(hash) == expected);
        let (status, error) = if valid {
            ("completed", None)
        } else {
            ("expired", Some("控制端恢复未找到有效结果；需要重新领取"))
        };
        count+=tx.execute("UPDATE frame_leases SET status=?6,completed_at=CASE WHEN ?6='completed' THEN ?7 ELSE NULL END,last_error=?8 WHERE pack_id=?1 AND frame=?2 AND node_id=?3 AND lease_epoch=?4 AND claim_token=?5 AND status='committing'",params![pack,frame,node,epoch,token,status,now(),error]).map_err(|error|error.to_string())?;
    }
    count+=tx.execute("UPDATE frame_leases SET status='expired',last_error='租约已过期；需要重新领取' WHERE status='leased' AND expires_at<=?1",params![now()]).map_err(|error|error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(count)
}

#[tauri::command]
pub fn get_render_farm_snapshot(
    app_handle: tauri::AppHandle,
) -> Result<RenderFarmSnapshot, String> {
    ensure_running()?;
    let root = app_root(&app_handle)?;
    let identity = identity(&root, None)?.public;
    let conn = db(&root)?;
    let mut statement=conn.prepare("SELECT d.device_id,d.display_name,d.public_key,d.transport_public_key,d.fingerprint,d.trust_state,d.paired_at,d.last_seen_at,d.revoked_at,(SELECT MAX(reported_at) FROM capability_reports c WHERE c.device_id=d.device_id) FROM trusted_devices d ORDER BY d.trust_state,d.display_name COLLATE NOCASE,d.device_id").map_err(|error|error.to_string())?;
    let peers = statement
        .query_map([], |row| {
            Ok(RenderFarmPeer {
                device_id: row.get(0)?,
                display_name: row.get(1)?,
                public_key: row.get(2)?,
                transport_public_key: row.get(3)?,
                fingerprint: row.get(4)?,
                trust_state: row.get(5)?,
                paired_at: row.get(6)?,
                last_seen_at: row.get(7)?,
                revoked_at: row.get(8)?,
                last_capability_report_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let packs = conn
        .query_row("SELECT COUNT(*) FROM render_packs", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let leases=conn.query_row("SELECT COUNT(*) FROM frame_leases WHERE status IN ('leased','committing') AND expires_at>?1",params![now()],|row|row.get(0)).map_err(|error|error.to_string())?;
    Ok(RenderFarmSnapshot {
        identity,
        peers,
        render_pack_count: packs,
        active_lease_count: leases,
        database_path: db_path(&root).to_string_lossy().into_owned(),
        staging_root: staging(&root).to_string_lossy().into_owned(),
        results_root: results(&root).to_string_lossy().into_owned(),
    })
}
#[tauri::command]
pub fn create_render_farm_pairing_offer(
    app_handle: tauri::AppHandle,
    request: CreateRenderFarmPairingOfferRequest,
) -> Result<RenderFarmPairingOffer, String> {
    ensure_running()?;
    let root = app_root(&app_handle)?;
    let identity = identity(&root, request.display_name.as_deref())?;
    let offer = make_offer(&identity, now());
    db(&root)?.execute("INSERT INTO pending_pairings(offer_id,nonce,expires_at,issuer_device_id,issuer_public_key,issuer_transport_public_key) VALUES(?1,?2,?3,?4,?5,?6)",params![offer.offer_id,offer.nonce,offer.expires_at,offer.issuer.device_id,offer.issuer.public_key,offer.issuer.transport_public_key]).map_err(|error|format!("保存配对请求失败: {error}"))?;
    Ok(offer)
}
#[tauri::command]
pub fn accept_render_farm_pairing_offer(
    app_handle: tauri::AppHandle,
    request: AcceptRenderFarmPairingOfferRequest,
) -> Result<RenderFarmPairingAcceptance, String> {
    ensure_running()?;
    let timestamp = now();
    validate_offer(&request.offer, timestamp)?;
    let root = app_root(&app_handle)?;
    let identity = identity(&root, request.display_name.as_deref())?;
    trust(&db(&root)?, &request.offer.issuer, timestamp)?;
    Ok(make_acceptance(&identity, &request.offer, timestamp))
}
#[tauri::command]
pub fn complete_render_farm_pairing(
    app_handle: tauri::AppHandle,
    request: CompleteRenderFarmPairingRequest,
) -> Result<RenderFarmPeer, String> {
    ensure_running()?;
    let root = app_root(&app_handle)?;
    let conn = db(&root)?;
    let pending=conn.query_row("SELECT nonce,expires_at,issuer_device_id,issuer_public_key,issuer_transport_public_key FROM pending_pairings WHERE offer_id=?1",params![request.acceptance.offer_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?))).optional().map_err(|error|error.to_string())?.ok_or_else(||"配对请求不存在、已使用或已过期".to_string())?;
    if request.acceptance.schema_version != 2
        || pending.1 <= now()
        || request.acceptance.nonce != pending.0
        || request.acceptance.controller_device_id != pending.2
        || request.acceptance.controller_public_key != pending.3
        || request.acceptance.controller_transport_public_key != pending.4
        || fingerprint(&request.acceptance.accepter.public_key)
            != request.acceptance.accepter.fingerprint
    {
        return Err("配对确认与本机请求不匹配或已过期".into());
    }
    verify(
        &request.acceptance.accepter.public_key,
        &request.acceptance.signature,
        &acceptance_text(&request.acceptance),
    )?;
    transport_public_key_bytes(
        &request.acceptance.accepter.transport_public_key,
        "接收设备",
    )?;
    let peer = trust(&conn, &request.acceptance.accepter, now())?;
    conn.execute(
        "DELETE FROM pending_pairings WHERE offer_id=?1",
        params![request.acceptance.offer_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(peer)
}
#[tauri::command]
pub fn revoke_render_farm_device(
    app_handle: tauri::AppHandle,
    request: RevokeRenderFarmDeviceRequest,
) -> Result<(), String> {
    ensure_running()?;
    valid_id(&request.device_id, "设备 ID")?;
    let root = app_root(&app_handle)?;
    let mut conn = db(&root)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    if tx.execute("UPDATE trusted_devices SET trust_state='revoked',revoked_at=?2 WHERE device_id=?1 AND trust_state='trusted'",params![request.device_id,now()]).map_err(|error|error.to_string())?!=1{return Err("可信设备不存在或已经撤销".into());}
    tx.execute("UPDATE frame_leases SET status='revoked',last_error='节点信任已撤销' WHERE node_id=?1 AND status IN ('leased','committing')",params![request.device_id]).map_err(|error|error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}
#[tauri::command]
pub fn record_render_node_capability_report(
    app_handle: tauri::AppHandle,
    report: RenderNodeCapabilityReport,
) -> Result<(), String> {
    ensure_running()?;
    let root = app_root(&app_handle)?;
    record_capability_report(&root, &report)
}
#[tauri::command]
pub fn preflight_render_node(
    app_handle: tauri::AppHandle,
    request: RenderNodePreflightRequest,
) -> Result<RenderNodePreflightResult, String> {
    ensure_running()?;
    let conn = db(&app_root(&app_handle)?)?;
    preflight_node(&conn, &request)
}
#[tauri::command]
pub fn create_render_pack(
    app_handle: tauri::AppHandle,
    request: CreateRenderPackRequest,
) -> Result<RenderPackSummary, String> {
    ensure_running()?;
    create_pack(&app_root(&app_handle)?, &request)
}
#[tauri::command]
pub fn claim_remote_render_frame(
    app_handle: tauri::AppHandle,
    request: ClaimRemoteRenderFrameRequest,
) -> Result<RemoteRenderFrameLease, String> {
    ensure_running()?;
    claim(&app_root(&app_handle)?, &request)
}
#[tauri::command]
pub fn commit_remote_render_frame_result(
    app_handle: tauri::AppHandle,
    request: CommitRemoteRenderFrameResultRequest,
) -> Result<RemoteRenderFrameCommit, String> {
    ensure_running()?;
    commit(&app_root(&app_handle)?, &request)
}
#[tauri::command]
pub fn recover_expired_render_farm_leases(app_handle: tauri::AppHandle) -> Result<usize, String> {
    ensure_running()?;
    recover(&app_root(&app_handle)?)
}

#[tauri::command]
pub fn seal_render_farm_transport(
    app_handle: tauri::AppHandle,
    request: SealRenderFarmTransportRequest,
) -> Result<RenderFarmTransportEnvelope, String> {
    ensure_running()?;
    let root = app_root(&app_handle)?;
    let identity = identity(&root, None)?;
    let peer = trusted(&db(&root)?, &request.recipient_device_id)?;
    seal_transport(&identity, &peer, request.purpose, &request.payload, now())
}

#[tauri::command]
pub fn open_render_farm_transport(
    app_handle: tauri::AppHandle,
    request: OpenRenderFarmTransportRequest,
) -> Result<OpenedRenderFarmTransportMessage, String> {
    ensure_running()?;
    let root = app_root(&app_handle)?;
    let identity = identity(&root, None)?;
    let peer = trusted(&db(&root)?, &request.sender_device_id)?;
    let message = open_transport(&identity, &peer, &request.envelope, now())?;
    consume_transport_message(&root, &request.envelope)?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use zip::ZipArchive;
    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nexora-farm-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }
    fn identity(id: &str, seed: u8) -> LocalIdentity {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let public_key = public_key(&key);
        let transport_key = TransportSecret::from([seed.wrapping_add(64); 32]);
        LocalIdentity {
            public: RenderFarmIdentity {
                device_id: id.into(),
                display_name: id.into(),
                fingerprint: fingerprint(&public_key),
                public_key,
                transport_public_key: transport_public_key(&transport_key),
            },
            key,
            transport_key,
        }
    }
    fn pack(root: &Path) -> RenderPackSummary {
        fs::write(root.join("scene.blend"), b"blend").unwrap();
        fs::write(root.join("tex.png"), b"texture").unwrap();
        create_pack(
            root,
            &CreateRenderPackRequest {
                blend_path: root.join("scene.blend").to_string_lossy().into_owned(),
                destination_path: root
                    .join("scene.pmc-renderpack")
                    .to_string_lossy()
                    .into_owned(),
                settings: RenderPackSettings {
                    scene_name: Some("Scene".into()),
                    frame_start: 1,
                    frame_end: 2,
                    output_format: "PNG".into(),
                    engine: None,
                },
                resources: vec![RenderPackResourceInput {
                    source_path: root.join("tex.png").to_string_lossy().into_owned(),
                    archive_path: Some("textures/tex.png".into()),
                    kind: None,
                }],
                required_components: BTreeMap::new(),
                blender_version: Some("4.5".into()),
            },
        )
        .unwrap()
    }

    fn capability_report(device_id: &str) -> RenderNodeCapabilityReport {
        RenderNodeCapabilityReport {
            device_id: device_id.into(),
            reported_at: now(),
            host_name: "test-node".into(),
            logical_cpu_count: 12,
            physical_memory_bytes: 32 * 1024 * 1024 * 1024,
            free_disk_bytes: 1_024,
            gpus: vec![RenderNodeGpuCapability {
                name: "Test GPU".into(),
                vendor: "Nexora".into(),
                vram_bytes: 8 * 1024 * 1024 * 1024,
            }],
            blender: vec![RenderNodeBlenderCapability {
                version: "4.5".into(),
                build_hash: None,
                supported_engines: vec!["CYCLES".into()],
            }],
            component_versions: BTreeMap::from([("pmc.blendio".into(), "1.0.0".into())]),
            protocol_versions: vec![1],
        }
    }
    #[test]
    fn pairing_is_signed_and_nonce_bound() {
        let a = identity("device-controller", 1);
        let b = identity("device-worker", 2);
        let offer = make_offer(&a, now());
        validate_offer(&offer, now()).unwrap();
        let accepted = make_acceptance(&b, &offer, now());
        verify(
            &accepted.accepter.public_key,
            &accepted.signature,
            &acceptance_text(&accepted),
        )
        .unwrap();
        let mut altered = accepted.clone();
        altered.nonce = "other".into();
        assert!(verify(
            &altered.accepter.public_key,
            &altered.signature,
            &acceptance_text(&altered)
        )
        .is_err());

        let mut changed_transport_key = offer.clone();
        changed_transport_key.issuer.transport_public_key = b.public.transport_public_key.clone();
        assert!(validate_offer(&changed_transport_key, now()).is_err());
    }

    #[test]
    fn legacy_database_receives_transport_columns_once() {
        let root = root("legacy-schema");
        let path = db_path(&root);
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE trusted_devices(device_id TEXT PRIMARY KEY,display_name TEXT NOT NULL,public_key TEXT NOT NULL UNIQUE,fingerprint TEXT NOT NULL UNIQUE,trust_state TEXT NOT NULL,paired_at INTEGER NOT NULL,last_seen_at INTEGER,revoked_at INTEGER);
                 CREATE TABLE pending_pairings(offer_id TEXT PRIMARY KEY,nonce TEXT NOT NULL UNIQUE,expires_at INTEGER NOT NULL,issuer_device_id TEXT NOT NULL,issuer_public_key TEXT NOT NULL);",
            )
            .unwrap();
        drop(legacy);

        db(&root).unwrap();
        db(&root).unwrap();
        let verify = Connection::open(&path).unwrap();
        for (table, column) in [
            ("trusted_devices", "transport_public_key"),
            ("pending_pairings", "issuer_transport_public_key"),
        ] {
            let fields = verify
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(fields.iter().any(|field| field == column));
        }
        drop(verify);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preflight_rejects_untrusted_or_insufficient_nodes_and_accepts_matching_report() {
        let root = root("preflight");
        let worker = identity("device-worker", 3);
        trust(&db(&root).unwrap(), &worker.public, now()).unwrap();
        let request = RenderNodePreflightRequest {
            device_id: worker.public.device_id.clone(),
            required_blender_version: Some("4.5".into()),
            required_component_ids: vec!["pmc.blendio".into()],
            minimum_free_disk_bytes: 1_024,
        };

        let missing_report = preflight_node(&db(&root).unwrap(), &request).unwrap();
        assert!(!missing_report.eligible);
        assert!(missing_report
            .issues
            .iter()
            .any(|issue| issue.contains("尚未提交能力报告")));

        let report = capability_report(&worker.public.device_id);
        record_capability_report(&root, &report).unwrap();
        let eligible = preflight_node(&db(&root).unwrap(), &request).unwrap();
        assert!(eligible.eligible, "{:?}", eligible.issues);

        let insufficient = preflight_node(
            &db(&root).unwrap(),
            &RenderNodePreflightRequest {
                device_id: worker.public.device_id.clone(),
                required_blender_version: Some("4.6".into()),
                required_component_ids: vec!["pmc.missing".into()],
                minimum_free_disk_bytes: 2_048,
            },
        )
        .unwrap();
        assert!(!insufficient.eligible);
        assert!(insufficient
            .issues
            .iter()
            .any(|issue| issue.contains("磁盘空间不足")));
        assert!(insufficient
            .issues
            .iter()
            .any(|issue| issue.contains("Blender 4.6")));
        assert!(insufficient
            .issues
            .iter()
            .any(|issue| issue.contains("pmc.missing")));

        let untrusted_report = capability_report("device-untrusted");
        assert!(record_capability_report(&root, &untrusted_report).is_err());
        let untrusted = preflight_node(
            &db(&root).unwrap(),
            &RenderNodePreflightRequest {
                device_id: "device-untrusted".into(),
                required_blender_version: None,
                required_component_ids: Vec::new(),
                minimum_free_disk_bytes: 0,
            },
        )
        .unwrap();
        assert!(!untrusted.eligible);
        assert!(untrusted
            .issues
            .iter()
            .any(|issue| issue.contains("未配对")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn encrypted_transport_is_peer_bound_purpose_bound_and_tamper_evident() {
        let controller = identity("device-controller", 11);
        let worker = identity("device-worker", 22);
        let root = root("transport");
        let conn = db(&root).unwrap();
        let controller_peer = trust(&conn, &controller.public, now()).unwrap();
        let worker_peer = trust(&conn, &worker.public, now()).unwrap();

        let payload = serde_json::json!({"cpu": 12, "blender": "4.5"});
        let envelope = seal_transport(
            &controller,
            &worker_peer,
            RenderFarmTransportPurpose::CapabilityReport,
            &payload,
            now(),
        )
        .unwrap();
        assert_ne!(
            envelope.ciphertext,
            BASE64.encode(serde_json::to_vec(&payload).unwrap())
        );
        let opened = open_transport(&worker, &controller_peer, &envelope, now()).unwrap();
        assert_eq!(opened.payload, payload);
        assert_eq!(opened.purpose, RenderFarmTransportPurpose::CapabilityReport);

        let mut altered = envelope.clone();
        altered.ciphertext = BASE64.encode(b"different");
        assert!(open_transport(&worker, &controller_peer, &altered, now()).is_err());

        let mut wrong_recipient = envelope.clone();
        wrong_recipient.recipient_device_id = "device-other".into();
        assert!(open_transport(&worker, &controller_peer, &wrong_recipient, now()).is_err());

        let mut expired = envelope;
        expired.expires_at = now() - 1;
        assert!(open_transport(&worker, &controller_peer, &expired, now()).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transport_replay_guard_accepts_a_message_once() {
        let root = root("transport-replay");
        let controller = identity("device-controller", 31);
        let worker = identity("device-worker", 32);
        let worker_peer = trust(&db(&root).unwrap(), &worker.public, now()).unwrap();
        let envelope = seal_transport(
            &controller,
            &worker_peer,
            RenderFarmTransportPurpose::FrameLease,
            &serde_json::json!({"frame": 12}),
            now(),
        )
        .unwrap();
        consume_transport_message(&root, &envelope).unwrap();
        assert!(consume_transport_message(&root, &envelope).is_err());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn package_rejects_traversal_and_keeps_payload_hashes() {
        let root = root("pack");
        let summary = pack(&root);
        let file = fs::File::open(&summary.archive_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert!(archive.by_name("manifest.json").is_ok());
        assert!(archive.by_name("payload/scene/scene.blend").is_ok());
        assert!(archive
            .by_name("payload/resources/textures/tex.png")
            .is_ok());
        let request = CreateRenderPackRequest {
            blend_path: root.join("scene.blend").to_string_lossy().into_owned(),
            destination_path: root
                .join("bad.pmc-renderpack")
                .to_string_lossy()
                .into_owned(),
            settings: summary.manifest.settings.clone(),
            resources: vec![RenderPackResourceInput {
                source_path: root.join("tex.png").to_string_lossy().into_owned(),
                archive_path: Some("../bad".into()),
                kind: None,
            }],
            required_components: BTreeMap::new(),
            blender_version: None,
        };
        assert!(create_pack(&root, &request).is_err());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn revoked_or_stale_nodes_cannot_commit_or_overwrite() {
        let root = root("lease");
        let worker = identity("device-worker", 2);
        trust(&db(&root).unwrap(), &worker.public, now()).unwrap();
        let pack = pack(&root);
        let first = claim(
            &root,
            &ClaimRemoteRenderFrameRequest {
                pack_id: pack.pack_id.clone(),
                frame: 1,
                node_id: worker.public.device_id.clone(),
                lease_seconds: 1,
            },
        )
        .unwrap();
        assert!(claim(
            &root,
            &ClaimRemoteRenderFrameRequest {
                pack_id: pack.pack_id.clone(),
                frame: 1,
                node_id: worker.public.device_id.clone(),
                lease_seconds: 60
            }
        )
        .is_err());
        std::thread::sleep(Duration::from_millis(1050));
        recover(&root).unwrap();
        let second = claim(
            &root,
            &ClaimRemoteRenderFrameRequest {
                pack_id: pack.pack_id.clone(),
                frame: 1,
                node_id: worker.public.device_id.clone(),
                lease_seconds: 60,
            },
        )
        .unwrap();
        let staged = staged_path(&root, &second);
        fs::create_dir_all(staged.parent().unwrap()).unwrap();
        fs::write(&staged, b"result").unwrap();
        let hash = blake3::hash(b"result").to_hex().to_string();
        assert!(commit(
            &root,
            &CommitRemoteRenderFrameResultRequest {
                pack_id: pack.pack_id.clone(),
                frame: 1,
                node_id: worker.public.device_id.clone(),
                lease_epoch: first.lease_epoch,
                claim_token: first.claim_token,
                staged_path: staged.to_string_lossy().into_owned(),
                blake3: hash.clone(),
                extension: Some("png".into())
            }
        )
        .is_err());
        let done = commit(
            &root,
            &CommitRemoteRenderFrameResultRequest {
                pack_id: pack.pack_id.clone(),
                frame: 1,
                node_id: worker.public.device_id.clone(),
                lease_epoch: second.lease_epoch,
                claim_token: second.claim_token,
                staged_path: staged.to_string_lossy().into_owned(),
                blake3: hash,
                extension: Some("png".into()),
            },
        )
        .unwrap();
        assert!(Path::new(&done.result_path).is_file());
        db(&root)
            .unwrap()
            .execute(
                "UPDATE trusted_devices SET trust_state='revoked',revoked_at=?2 WHERE device_id=?1",
                params![worker.public.device_id, now()],
            )
            .unwrap();
        assert!(claim(
            &root,
            &ClaimRemoteRenderFrameRequest {
                pack_id: pack.pack_id,
                frame: 2,
                node_id: worker.public.device_id,
                lease_seconds: 60,
            },
        )
        .is_err());
        let _ = fs::remove_dir_all(root);
    }
}
