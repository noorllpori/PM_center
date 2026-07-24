use crate::process_utils::{std_command, tokio_command};
use crate::tools::resolve_ffmpeg_path;
use image::GenericImageView;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

const EVENT_PREFIX: &str = "PM_RENDER_EVENT ";
const INSPECT_START: &str = "PM_RENDER_INSPECT_START";
const INSPECT_END: &str = "PM_RENDER_INSPECT_END";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSourceRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSourceInfo {
    pub path: String,
    pub scenes: Vec<RenderSceneInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSceneInfo {
    pub name: String,
    pub frame_start: i64,
    pub frame_end: i64,
    pub resolution_x: i64,
    pub resolution_y: i64,
    pub fps: f64,
    pub engine: String,
    pub output_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRenderBatchRequest {
    pub name: String,
    pub blender_path: String,
    pub output_root: Option<String>,
    #[serde(default)]
    pub pre_hook: Option<String>,
    #[serde(default)]
    pub post_hook: Option<String>,
    #[serde(default)]
    pub force_overwrite: bool,
    #[serde(default = "default_retry_count")]
    pub max_retries: i64,
    pub jobs: Vec<CreateRenderJobRequest>,
}

fn default_retry_count() -> i64 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRenderJobRequest {
    pub blend_path: String,
    pub scene_name: String,
    pub frame_start: i64,
    pub frame_end: i64,
    #[serde(default = "default_frame_step")]
    pub frame_step: i64,
    #[serde(default = "default_parallelism")]
    pub parallelism: i64,
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    #[serde(default = "default_frame_order_mode")]
    pub frame_order_mode: String,
    pub resolution_x: Option<i64>,
    pub resolution_y: Option<i64>,
    pub resolution_percentage: Option<i64>,
    pub engine: Option<String>,
    pub output_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRenderJobRequest {
    pub scene_name: String,
    pub frame_start: i64,
    pub frame_end: i64,
    #[serde(default = "default_frame_step")]
    pub frame_step: i64,
    #[serde(default = "default_parallelism")]
    pub parallelism: i64,
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    #[serde(default = "default_frame_order_mode")]
    pub frame_order_mode: String,
    pub resolution_percentage: i64,
    pub engine: Option<String>,
    pub output_format: String,
}

fn default_frame_step() -> i64 {
    1
}

fn default_parallelism() -> i64 {
    1
}

fn default_execution_mode() -> String {
    "persistent".into()
}

fn default_frame_order_mode() -> String {
    "dynamic".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderBatchResult {
    pub batch_id: String,
    pub job_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderBatchPackageRequest {
    pub fps: f64,
    pub format: String,
    pub ffmpeg_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderBatchPackageOutput {
    pub job_id: String,
    pub job_name: String,
    pub output_path: String,
    /// Frames that had no readable image on disk and were replaced with black frames.
    pub missing_frames: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderBatchPackageResult {
    pub output_dir: String,
    pub outputs: Vec<RenderBatchPackageOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderJob {
    pub id: String,
    pub batch_id: String,
    pub project_path: String,
    pub name: String,
    pub blend_path: String,
    pub scene_name: String,
    pub status: String,
    pub frame_start: i64,
    pub frame_end: i64,
    pub frame_step: i64,
    pub parallelism: i64,
    pub effective_parallelism: i64,
    pub ready_workers: i64,
    pub execution_mode: String,
    pub frame_order_mode: String,
    pub total_frames: i64,
    pub completed_frames: i64,
    pub failed_frames: i64,
    pub skipped_frames: i64,
    pub current_frame: Option<i64>,
    pub progress: f64,
    pub output_dir: String,
    pub blender_path: String,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub archived: bool,
    pub cpu_usage: f64,
    pub memory_bytes: i64,
    pub peak_cpu_usage: f64,
    pub peak_memory_bytes: i64,
    pub performance_updated_at: Option<i64>,
    pub position: i64,
    pub batch_name: String,
    pub batch_status: String,
    pub batch_position: i64,
    pub attention_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFrame {
    pub job_id: String,
    pub frame: i64,
    pub status: String,
    pub attempts: i64,
    pub output_path: String,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub render_duration_ms: Option<i64>,
    pub worker_id: Option<String>,
    pub claim_token: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderWorkerState {
    pub worker_id: String,
    pub ordinal: i64,
    pub pid: Option<u32>,
    pub state: String,
    pub current_frame: Option<i64>,
    pub startup_ms: Option<i64>,
    pub error: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RenderStartupStats {
    pub requested_workers: i64,
    pub ready_workers: i64,
    pub average_startup_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderEta {
    pub status: String,
    pub estimated_finish_at: Option<i64>,
    pub remaining_ms: Option<i64>,
    pub sample_count: usize,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPerformanceSample {
    pub sampled_at: i64,
    pub cpu_usage: f64,
    pub memory_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderJobDetail {
    pub job: RenderJob,
    pub settings: RenderJobSettings,
    pub frames: Vec<RenderFrame>,
    pub log_tail: Vec<String>,
    pub performance_samples: Vec<RenderPerformanceSample>,
    pub eta: RenderEta,
    pub workers: Vec<RenderWorkerState>,
    pub startup: RenderStartupStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderJobSettings {
    pub scene_name: String,
    pub frame_start: i64,
    pub frame_end: i64,
    pub frame_step: i64,
    pub parallelism: i64,
    pub execution_mode: String,
    pub frame_order_mode: String,
    pub resolution_percentage: i64,
    pub engine: Option<String>,
    pub output_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPreset {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub settings: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerSettings {
    pub concurrency: i64,
    #[serde(default = "default_max_blender_processes")]
    pub max_blender_processes: i64,
}

fn default_max_blender_processes() -> i64 {
    std::thread::available_parallelism()
        .map(|value| value.get() as i64)
        .unwrap_or(1)
        .clamp(1, 4)
}

const PROGRESSIVE_WORKER_WARMUP_FRAMES: i64 = 3;

#[derive(Default)]
struct JobControl {
    cancel: bool,
    pause: bool,
    attention: bool,
    degraded_worker_limit: Option<i64>,
    completed_frames: i64,
    no_more_work: bool,
    pids: HashSet<u32>,
    metrics: HashMap<u32, RenderProcessMetrics>,
    workers: HashMap<String, RenderWorkerState>,
    verified_source: Option<SourceFingerprint>,
}

fn worker_exceeds_runtime_limit(control: &Arc<Mutex<JobControl>>, ordinal: i64) -> bool {
    control
        .lock()
        .unwrap()
        .degraded_worker_limit
        .is_some_and(|limit| ordinal >= limit)
}

fn activate_single_worker_fallback(control: &Arc<Mutex<JobControl>>) -> bool {
    let mut value = control.lock().unwrap();
    if value.degraded_worker_limit.is_some() {
        return false;
    }
    value.degraded_worker_limit = Some(1);
    true
}

fn progressive_worker_admitted(control: &JobControl, ordinal: i64) -> bool {
    if ordinal == 0 {
        return true;
    }
    let stable_workers = control
        .workers
        .values()
        .filter(|worker| matches!(worker.state.as_str(), "ready" | "rendering"))
        .count() as i64;
    control.completed_frames >= ordinal * PROGRESSIVE_WORKER_WARMUP_FRAMES
        && stable_workers >= ordinal
}

async fn wait_for_progressive_worker_admission(
    control: &Arc<Mutex<JobControl>>,
    ordinal: i64,
) -> bool {
    if ordinal == 0 {
        return true;
    }
    loop {
        let admitted = {
            let value = control.lock().unwrap();
            if value.cancel
                || value.pause
                || value.attention
                || value.no_more_work
                || value
                    .degraded_worker_limit
                    .is_some_and(|limit| ordinal >= limit)
            {
                return false;
            }
            progressive_worker_admitted(&value, ordinal)
        };
        if admitted {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn record_worker_frame_completion(control: &Arc<Mutex<JobControl>>) {
    control.lock().unwrap().completed_frames += 1;
}

#[derive(Default)]
struct RuntimeState {
    running: HashMap<String, Arc<Mutex<JobControl>>>,
    projects: HashSet<String>,
    project_order: Vec<String>,
    project_cursor: usize,
    active_worker_slots: usize,
}

lazy_static::lazy_static! {
    static ref RUNTIME: Mutex<RuntimeState> = Mutex::new(RuntimeState::default());
    static ref RECOVERED_PROJECTS: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    static ref SCHEDULER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
    static ref PROCESS_BUDGET_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::new();
    static ref SOURCE_VERIFY_LOCK: Mutex<()> = Mutex::new(());
    static ref BATCH_PACKAGE_RUNNING: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

struct ProcessSlotPermit {
    released: bool,
}

impl ProcessSlotPermit {
    fn retire_if_over_limit(&mut self, app: &tauri::AppHandle) -> bool {
        if self.released {
            return true;
        }
        let limit = load_scheduler_settings(app)
            .max_blender_processes
            .clamp(1, 16) as usize;
        let mut runtime = RUNTIME.lock().unwrap();
        if runtime.active_worker_slots <= limit {
            return false;
        }
        runtime.active_worker_slots = runtime.active_worker_slots.saturating_sub(1);
        self.released = true;
        drop(runtime);
        PROCESS_BUDGET_NOTIFY.notify_waiters();
        true
    }
}

impl Drop for ProcessSlotPermit {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let mut runtime = RUNTIME.lock().unwrap();
        runtime.active_worker_slots = runtime.active_worker_slots.saturating_sub(1);
        drop(runtime);
        PROCESS_BUDGET_NOTIFY.notify_waiters();
    }
}

async fn acquire_process_slot(
    app: &tauri::AppHandle,
    control: &Arc<Mutex<JobControl>>,
) -> Option<ProcessSlotPermit> {
    loop {
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            return None;
        }
        let limit = load_scheduler_settings(app)
            .max_blender_processes
            .clamp(1, 16) as usize;
        {
            let mut runtime = RUNTIME.lock().unwrap();
            if runtime.active_worker_slots < limit {
                runtime.active_worker_slots += 1;
                return Some(ProcessSlotPermit { released: false });
            }
        }
        tokio::select! {
            _ = PROCESS_BUDGET_NOTIFY.notified() => {},
            _ = tokio::time::sleep(Duration::from_millis(150)) => {},
        }
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceFingerprint {
    hash: String,
    size: i64,
    modified_at: i64,
}

fn source_fingerprint(path: &Path) -> Result<SourceFingerprint, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("读取源文件信息失败 {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("源文件不存在: {}", path.display()));
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0);
    let mut file = fs::File::open(path)
        .map_err(|error| format!("打开源文件失败 {}: {error}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("校验源文件失败 {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(SourceFingerprint {
        hash: hasher.finalize().to_hex().to_string(),
        size: metadata.len().min(i64::MAX as u64) as i64,
        modified_at,
    })
}

fn normalize_execution_mode(value: &str) -> Result<&'static str, String> {
    match value {
        "persistent" => Ok("persistent"),
        "isolated" => Ok("isolated"),
        _ => Err("执行模式必须是 persistent 或 isolated".into()),
    }
}

fn normalize_frame_order_mode(value: &str) -> Result<&'static str, String> {
    match value {
        "dynamic" => Ok("dynamic"),
        "strict" => Ok("strict"),
        _ => Err("帧顺序必须是 dynamic 或 strict".into()),
    }
}

fn open_db(project_path: &str) -> Result<Connection, String> {
    let root = PathBuf::from(project_path).join(".pm_center");
    fs::create_dir_all(root.join("render_jobs"))
        .map_err(|error| format!("创建渲染任务目录失败: {error}"))?;
    let conn = Connection::open(root.join("data.db"))
        .map_err(|error| format!("打开项目数据库失败: {error}"))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|error| format!("设置项目数据库等待时间失败: {error}"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|error| format!("初始化项目数据库失败: {error}"))?;
    init_schema(&conn).map_err(|error| format!("初始化渲染数据表失败: {error}"))?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS render_batches (
            id TEXT PRIMARY KEY,
            project_path TEXT NOT NULL,
            name TEXT NOT NULL,
            status TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS render_jobs (
            id TEXT PRIMARY KEY,
            batch_id TEXT NOT NULL,
            project_path TEXT NOT NULL,
            name TEXT NOT NULL,
            blend_path TEXT NOT NULL,
            scene_name TEXT NOT NULL,
            status TEXT NOT NULL,
            frame_start INTEGER NOT NULL,
            frame_end INTEGER NOT NULL,
            frame_step INTEGER NOT NULL,
            parallelism INTEGER NOT NULL DEFAULT 1,
            execution_mode TEXT NOT NULL DEFAULT 'persistent',
            frame_order_mode TEXT NOT NULL DEFAULT 'dynamic',
            output_dir TEXT NOT NULL,
            blender_path TEXT NOT NULL,
            python_path TEXT,
            pre_hook TEXT,
            post_hook TEXT,
            force_overwrite INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 2,
            spec_json TEXT NOT NULL,
            current_frame INTEGER,
            error TEXT,
            attention_code TEXT,
            source_hash TEXT,
            source_size INTEGER,
            source_modified_at INTEGER,
            archived INTEGER NOT NULL DEFAULT 0,
            position INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            started_at INTEGER,
            finished_at INTEGER,
            FOREIGN KEY(batch_id) REFERENCES render_batches(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS render_frames (
            job_id TEXT NOT NULL,
            frame INTEGER NOT NULL,
            status TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            output_path TEXT NOT NULL,
            error TEXT,
            duration_ms INTEGER,
            render_duration_ms INTEGER,
            force_render INTEGER NOT NULL DEFAULT 0,
            worker_id TEXT,
            claim_token TEXT,
            claimed_at INTEGER,
            temp_output_path TEXT,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY(job_id, frame),
            FOREIGN KEY(job_id) REFERENCES render_jobs(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS render_attempts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id TEXT NOT NULL,
            frame INTEGER NOT NULL,
            attempt INTEGER NOT NULL,
            status TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            finished_at INTEGER,
            exit_code INTEGER,
            error TEXT,
            worker_id TEXT,
            claim_token TEXT,
            temp_output_path TEXT,
            render_duration_ms INTEGER
        );
        CREATE TABLE IF NOT EXISTS render_artifacts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id TEXT NOT NULL,
            frame INTEGER,
            path TEXT NOT NULL,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS render_performance_samples (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id TEXT NOT NULL,
            sampled_at INTEGER NOT NULL,
            cpu_usage REAL NOT NULL,
            memory_bytes INTEGER NOT NULL,
            FOREIGN KEY(job_id) REFERENCES render_jobs(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS render_workers (
            worker_id TEXT PRIMARY KEY,
            job_id TEXT NOT NULL,
            ordinal INTEGER NOT NULL,
            pid INTEGER,
            state TEXT NOT NULL,
            current_frame INTEGER,
            startup_ms INTEGER,
            error TEXT,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(job_id) REFERENCES render_jobs(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS render_presets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            scope TEXT NOT NULL,
            settings_json TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS render_scheduler_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            concurrency INTEGER NOT NULL DEFAULT 1
        );
        INSERT OR IGNORE INTO render_scheduler_settings(id, concurrency) VALUES(1, 1);
        CREATE INDEX IF NOT EXISTS idx_render_jobs_queue ON render_jobs(status, archived, position, created_at);
        CREATE INDEX IF NOT EXISTS idx_render_frames_job_status ON render_frames(job_id, status, frame);
        CREATE INDEX IF NOT EXISTS idx_render_workers_job_state ON render_workers(job_id, state);
        CREATE INDEX IF NOT EXISTS idx_render_performance_samples_job_time ON render_performance_samples(job_id, sampled_at DESC);
        "#,
    )?;
    ensure_column(conn, "render_jobs", "cpu_usage", "REAL NOT NULL DEFAULT 0")?;
    ensure_column(
        conn,
        "render_jobs",
        "memory_bytes",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "render_jobs",
        "peak_cpu_usage",
        "REAL NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "render_jobs",
        "peak_memory_bytes",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(conn, "render_jobs", "performance_updated_at", "INTEGER")?;
    ensure_column(
        conn,
        "render_jobs",
        "parallelism",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(
        conn,
        "render_jobs",
        "execution_mode",
        "TEXT NOT NULL DEFAULT 'persistent'",
    )?;
    ensure_column(
        conn,
        "render_jobs",
        "frame_order_mode",
        "TEXT NOT NULL DEFAULT 'dynamic'",
    )?;
    ensure_column(conn, "render_jobs", "attention_code", "TEXT")?;
    ensure_column(conn, "render_jobs", "source_hash", "TEXT")?;
    ensure_column(conn, "render_jobs", "source_size", "INTEGER")?;
    ensure_column(conn, "render_jobs", "source_modified_at", "INTEGER")?;
    ensure_column(
        conn,
        "render_frames",
        "force_render",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(conn, "render_frames", "render_duration_ms", "INTEGER")?;
    ensure_column(conn, "render_frames", "worker_id", "TEXT")?;
    ensure_column(conn, "render_frames", "claim_token", "TEXT")?;
    ensure_column(conn, "render_frames", "claimed_at", "INTEGER")?;
    ensure_column(conn, "render_frames", "temp_output_path", "TEXT")?;
    ensure_column(conn, "render_attempts", "worker_id", "TEXT")?;
    ensure_column(conn, "render_attempts", "claim_token", "TEXT")?;
    ensure_column(conn, "render_attempts", "temp_output_path", "TEXT")?;
    ensure_column(conn, "render_attempts", "render_duration_ms", "INTEGER")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_render_frames_claim ON render_frames(job_id, claim_token);",
    )?;
    ensure_column(
        conn,
        "render_batches",
        "position",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    // Projects created before batch ordering have every batch at position 0. Give
    // them a stable initial order while retaining their original queue sequence.
    conn.execute(
        "WITH legacy_projects AS (SELECT project_path FROM render_batches GROUP BY project_path HAVING MIN(position)=0 AND MAX(position)=0) UPDATE render_batches AS target SET position=(SELECT COUNT(*) FROM render_batches AS preceding WHERE preceding.project_path=target.project_path AND (preceding.created_at < target.created_at OR (preceding.created_at=target.created_at AND preceding.id < target.id))) WHERE target.project_path IN (SELECT project_path FROM legacy_projects)",
        [],
    )?;
    // Batches created before queue sequencing used `queued` while their jobs were
    // already runnable. Preserve that behavior for existing projects only.
    conn.execute(
        "UPDATE render_batches SET status='running' WHERE status='queued' AND EXISTS (SELECT 1 FROM render_jobs WHERE render_jobs.batch_id=render_batches.id AND render_jobs.archived=0 AND render_jobs.status IN ('pending','starting','running','pausing'))",
        [],
    )?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|value| value == column) {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

pub fn init_project_storage(project_path: &str) -> Result<(), String> {
    let conn = open_db(project_path)?;
    let project_key = project_path
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if !RECOVERED_PROJECTS.lock().unwrap().insert(project_key) {
        return Ok(());
    }
    recover_interrupted_frames(&conn)?;
    conn.execute(
        "UPDATE render_jobs SET status = 'paused', error = '应用上次退出时任务仍在运行，请手动继续', cpu_usage=0, memory_bytes=0 WHERE status IN ('starting', 'running', 'pausing', 'cancelling')",
        [],
    ).map_err(|error| error.to_string())?;
    conn.execute("DELETE FROM render_workers", [])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn recover_interrupted_frames(conn: &Connection) -> Result<(), String> {
    let interrupted = {
        let mut statement = conn
            .prepare("SELECT job_id,frame,status,output_path,temp_output_path,claim_token FROM render_frames WHERE status IN ('running','committing')")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for (job_id, frame, status, output_path, temp_output_path, claim_token) in interrupted {
        let recovered = if status == "committing" {
            if let Some(temp) = temp_output_path
                .as_deref()
                .filter(|path| valid_output(Path::new(path)))
            {
                atomic_replace_output(Path::new(temp), Path::new(&output_path)).is_ok()
            } else {
                valid_output(Path::new(&output_path))
            }
        } else {
            false
        };
        if recovered {
            conn.execute(
                "UPDATE render_frames SET status='completed',attempts=MAX(attempts,COALESCE((SELECT MAX(attempt) FROM render_attempts WHERE job_id=?1 AND frame=?2),attempts)),error=NULL,force_render=0,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2",
                params![job_id, frame, now()],
            ).map_err(|error| error.to_string())?;
            if let Some(token) = claim_token.as_deref() {
                conn.execute(
                    "UPDATE render_attempts SET status='completed',finished_at=?4,exit_code=0 WHERE job_id=?1 AND frame=?2 AND claim_token=?3",
                    params![job_id, frame, token, now()],
                ).map_err(|error| error.to_string())?;
            }
        } else {
            if let Some(temp) = temp_output_path.as_deref() {
                let _ = fs::remove_file(temp);
            }
            if let Some(token) = claim_token.as_deref() {
                conn.execute(
                    "DELETE FROM render_attempts WHERE job_id=?1 AND frame=?2 AND claim_token=?3 AND status='running'",
                    params![job_id, frame, token],
                ).map_err(|error| error.to_string())?;
            }
            conn.execute(
                "UPDATE render_frames SET status='pending',error=NULL,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2",
                params![job_id, frame, now()],
            ).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn inspect_script() -> &'static str {
    r#"import bpy, json
result = []
for scene in bpy.data.scenes:
    result.append({
        'name': scene.name,
        'frameStart': scene.frame_start,
        'frameEnd': scene.frame_end,
        'resolutionX': scene.render.resolution_x,
        'resolutionY': scene.render.resolution_y,
        'fps': scene.render.fps / max(scene.render.fps_base, 0.0001),
        'engine': scene.render.engine,
        'outputFormat': scene.render.image_settings.file_format,
    })
print('PM_RENDER_INSPECT_START')
print(json.dumps(result, ensure_ascii=False))
print('PM_RENDER_INSPECT_END')
"#
}

fn bootstrap_script() -> &'static str {
    r#"import bpy, json, os, sys, time, traceback

def emit(kind, **payload):
    payload['type'] = kind
    print('PM_RENDER_EVENT ' + json.dumps(payload, ensure_ascii=False), flush=True)

try:
    separator = sys.argv.index('--')
    spec_path = sys.argv[separator + 2] if sys.argv[separator + 1] == '--spec' else sys.argv[separator + 1]
    with open(spec_path, 'r', encoding='utf-8') as stream:
        spec = json.load(stream)
    scene = bpy.data.scenes.get(spec['sceneName'])
    if scene is None:
        raise RuntimeError('Scene not found: ' + spec['sceneName'])
    scene.frame_set(int(spec['frame']))
    if spec.get('engine'):
        scene.render.engine = spec['engine']
    if spec.get('resolutionX'):
        scene.render.resolution_x = int(spec['resolutionX'])
    if spec.get('resolutionY'):
        scene.render.resolution_y = int(spec['resolutionY'])
    if spec.get('resolutionPercentage'):
        scene.render.resolution_percentage = int(spec['resolutionPercentage'])
    scene.render.image_settings.file_format = spec.get('outputFormat') or 'PNG'
    scene.render.filepath = spec['outputPath']
    scene.render.use_file_extension = False
    os.makedirs(os.path.dirname(spec['outputPath']), exist_ok=True)
    emit('frame-started', frame=spec['frame'], outputPath=spec['outputPath'])
    started = time.time()
    bpy.ops.render.render(write_still=True, scene=scene.name)
    emit('frame-completed', frame=spec['frame'], outputPath=spec['outputPath'], durationMs=int((time.time()-started)*1000))
except Exception as exc:
    emit('frame-failed', error=str(exc), traceback=traceback.format_exc())
    raise
"#
}

fn persistent_worker_script() -> &'static str {
    r#"import bpy, json, os, sys, time, traceback

PREFIX = 'PM_RENDER_EVENT '

def emit(kind, **payload):
    payload['type'] = kind
    print(PREFIX + json.dumps(payload, ensure_ascii=False), flush=True)

separator = sys.argv.index('--')
worker_id = sys.argv[separator + 2] if sys.argv[separator + 1] == '--worker-id' else 'worker'
emit('worker-ready', workerId=worker_id)

for raw_line in sys.stdin:
    raw_line = raw_line.strip()
    if not raw_line:
        continue
    try:
        command = json.loads(raw_line)
    except Exception as exc:
        emit('protocol-error', workerId=worker_id, error=str(exc))
        continue
    command_type = command.get('type')
    if command_type == 'shutdown':
        emit('worker-stopped', workerId=worker_id)
        break
    if command_type != 'render':
        emit('protocol-error', workerId=worker_id, error='Unknown command: ' + str(command_type))
        continue
    frame = int(command['frame'])
    claim_token = command['claimToken']
    temp_output_path = command['tempOutputPath']
    try:
        scene = bpy.data.scenes.get(command['sceneName'])
        if scene is None:
            raise RuntimeError('Scene not found: ' + command['sceneName'])
        scene.frame_set(frame)
        if command.get('engine'):
            scene.render.engine = command['engine']
        if command.get('resolutionX'):
            scene.render.resolution_x = int(command['resolutionX'])
        if command.get('resolutionY'):
            scene.render.resolution_y = int(command['resolutionY'])
        if command.get('resolutionPercentage'):
            scene.render.resolution_percentage = int(command['resolutionPercentage'])
        scene.render.image_settings.file_format = command.get('outputFormat') or 'PNG'
        scene.render.filepath = temp_output_path
        scene.render.use_file_extension = False
        os.makedirs(os.path.dirname(temp_output_path), exist_ok=True)
        emit('frame-started', workerId=worker_id, frame=frame, claimToken=claim_token)
        started = time.time()
        bpy.ops.render.render(write_still=True, scene=scene.name)
        emit('frame-completed', workerId=worker_id, frame=frame, claimToken=claim_token,
             renderDurationMs=int((time.time() - started) * 1000), tempOutputPath=temp_output_path)
    except Exception as exc:
        emit('frame-failed', workerId=worker_id, frame=frame, claimToken=claim_token,
             error=str(exc), traceback=traceback.format_exc())
"#
}

fn write_runtime_scripts(job_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(job_dir).map_err(|error| error.to_string())?;
    let bootstrap = job_dir.join("bootstrap.py");
    fs::write(&bootstrap, bootstrap_script()).map_err(|error| error.to_string())?;
    Ok(bootstrap)
}

fn write_persistent_worker_script(job_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(job_dir).map_err(|error| error.to_string())?;
    let worker = job_dir.join("worker.py");
    fs::write(&worker, persistent_worker_script()).map_err(|error| error.to_string())?;
    Ok(worker)
}

#[tauri::command]
pub async fn inspect_render_sources(
    blender_path: String,
    sources: Vec<RenderSourceRequest>,
) -> Result<Vec<RenderSourceInfo>, String> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let temp_script = std::env::temp_dir().join(format!("pm-render-inspect-{}.py", Uuid::new_v4()));
    fs::write(&temp_script, inspect_script()).map_err(|error| error.to_string())?;
    let mut result = Vec::with_capacity(sources.len());
    for source in sources {
        let output = tokio_command(&blender_path)
            .arg("-b")
            .arg(&source.path)
            .arg("--python")
            .arg(&temp_script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let scenes = stdout
                    .split_once(INSPECT_START)
                    .and_then(|(_, rest)| rest.split_once(INSPECT_END).map(|(json, _)| json.trim()))
                    .and_then(|value| serde_json::from_str::<Vec<RenderSceneInfo>>(value).ok());
                match scenes {
                    Some(scenes) => result.push(RenderSourceInfo {
                        path: source.path,
                        scenes,
                        error: None,
                    }),
                    None => result.push(RenderSourceInfo {
                        path: source.path,
                        scenes: Vec::new(),
                        error: Some("Blender 未返回可识别的场景信息".into()),
                    }),
                }
            }
            Ok(output) => result.push(RenderSourceInfo {
                path: source.path,
                scenes: Vec::new(),
                error: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            }),
            Err(error) => result.push(RenderSourceInfo {
                path: source.path,
                scenes: Vec::new(),
                error: Some(error.to_string()),
            }),
        }
    }
    let _ = fs::remove_file(temp_script);
    Ok(result)
}

fn format_extension(format: &str) -> &'static str {
    match format.to_ascii_uppercase().as_str() {
        "JPEG" => "jpg",
        "OPEN_EXR" | "OPEN_EXR_MULTILAYER" => "exr",
        "TIFF" => "tif",
        "BMP" => "bmp",
        "TARGA" | "TARGA_RAW" => "tga",
        "HDR" => "hdr",
        "WEBP" => "webp",
        _ => "png",
    }
}

fn normalize_output_format(format: &str) -> Result<String, String> {
    let normalized = format.trim().to_ascii_uppercase();
    if matches!(
        normalized.as_str(),
        "PNG" | "JPEG" | "OPEN_EXR" | "TIFF" | "WEBP"
    ) {
        Ok(normalized)
    } else {
        Err(format!("不支持的输出格式: {format}"))
    }
}

fn frame_output_path(
    output_dir: &Path,
    scene_name: &str,
    frame: i64,
    frame_end: i64,
    output_format: &str,
) -> PathBuf {
    let padding = frame_end.abs().to_string().len().max(4);
    output_dir.join(format!(
        "{}_{:0padding$}.{}",
        safe_name(scene_name),
        frame,
        format_extension(output_format),
        padding = padding
    ))
}

fn safe_name(value: &str) -> String {
    let result: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if result.is_empty() {
        "render".into()
    } else {
        result
    }
}

#[tauri::command]
pub async fn create_render_batch(
    app_handle: tauri::AppHandle,
    project_path: String,
    request: CreateRenderBatchRequest,
) -> Result<RenderBatchResult, String> {
    if request.jobs.is_empty() {
        return Err("至少需要一个渲染作业".into());
    }
    let mut conn = open_db(&project_path)?;
    let batch_id = Uuid::new_v4().to_string();
    let created = now();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let batch_position: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM render_batches WHERE project_path=?1",
            params![project_path],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO render_batches(id, project_path, name, status, position, created_at, updated_at) VALUES(?1, ?2, ?3, 'queued', ?4, ?5, ?5)",
        params![batch_id, project_path, request.name, batch_position, created],
    ).map_err(|error| error.to_string())?;
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let default_root = PathBuf::from(&project_path).join("renders");
    let output_root = request
        .output_root
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(default_root);
    let mut job_ids = Vec::new();
    for (position, job) in request.jobs.iter().enumerate() {
        if job.frame_step <= 0 || job.frame_end < job.frame_start {
            return Err(format!("{} 的帧范围无效", job.blend_path));
        }
        let job_id = Uuid::new_v4().to_string();
        let execution_mode = normalize_execution_mode(&job.execution_mode)?;
        let frame_order_mode = normalize_frame_order_mode(&job.frame_order_mode)?;
        let fingerprint = source_fingerprint(Path::new(&job.blend_path))?;
        let short_id = &job_id[..8];
        let blend_stem = Path::new(&job.blend_path)
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("blend");
        let output_dir = output_root
            .join(safe_name(blend_stem))
            .join(format!("{}-{}", timestamp, short_id));
        fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
        let format = job.output_format.clone().unwrap_or_else(|| "PNG".into());
        let spec = json!({
            "blendPath": job.blend_path,
            "sceneName": job.scene_name,
            "frameStart": job.frame_start,
            "frameEnd": job.frame_end,
            "frameStep": job.frame_step,
            "parallelism": job.parallelism.clamp(1, 8),
            "executionMode": execution_mode,
            "frameOrderMode": frame_order_mode,
            "resolutionX": job.resolution_x,
            "resolutionY": job.resolution_y,
            "resolutionPercentage": job.resolution_percentage.unwrap_or(100),
            "engine": job.engine,
            "outputFormat": format,
        });
        let name = format!("{} · {}", blend_stem, job.scene_name);
        tx.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,parallelism,execution_mode,frame_order_mode,output_dir,blender_path,python_path,pre_hook,post_hook,force_overwrite,max_retries,spec_json,position,created_at,source_hash,source_size,source_modified_at) VALUES(?1,?2,?3,?4,?5,?6,'paused',?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)",
            params![job_id,batch_id,project_path,name,job.blend_path,job.scene_name,job.frame_start,job.frame_end,job.frame_step,job.parallelism.clamp(1,8),execution_mode,frame_order_mode,output_dir.to_string_lossy(),request.blender_path,rusqlite::types::Null,request.pre_hook,request.post_hook,request.force_overwrite as i64,request.max_retries.max(0),spec.to_string(),position as i64,created,fingerprint.hash,fingerprint.size,fingerprint.modified_at],
        ).map_err(|error| error.to_string())?;
        for frame in (job.frame_start..=job.frame_end).step_by(job.frame_step as usize) {
            let output_path =
                frame_output_path(&output_dir, &job.scene_name, frame, job.frame_end, &format);
            tx.execute(
                "INSERT INTO render_frames(job_id,frame,status,output_path,updated_at) VALUES(?1,?2,'pending',?3,?4)",
                params![job_id, frame, output_path.to_string_lossy(), created],
            ).map_err(|error| error.to_string())?;
        }
        let job_dir = PathBuf::from(&project_path)
            .join(".pm_center/render_jobs")
            .join(&job_id);
        fs::create_dir_all(&job_dir).map_err(|error| error.to_string())?;
        fs::write(
            job_dir.join("job.json"),
            serde_json::to_vec_pretty(&spec).unwrap(),
        )
        .map_err(|error| error.to_string())?;
        job_ids.push(job_id);
    }
    tx.commit().map_err(|error| error.to_string())?;
    emit_queue(&app_handle, &project_path);
    Ok(RenderBatchResult { batch_id, job_ids })
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<RenderJob> {
    let total: i64 = row.get(19)?;
    let completed: i64 = row.get(20)?;
    let failed: i64 = row.get(21)?;
    let skipped: i64 = row.get(22)?;
    let frame_order_mode: String = row.get(34)?;
    let ready_workers: i64 = row.get(35)?;
    let active_workers: i64 = row.get(36)?;
    let configured_parallelism: i64 = row.get(28)?;
    let progress = if total > 0 {
        ((completed + failed + skipped) as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    Ok(RenderJob {
        id: row.get(0)?,
        batch_id: row.get(1)?,
        project_path: row.get(2)?,
        name: row.get(3)?,
        blend_path: row.get(4)?,
        scene_name: row.get(5)?,
        status: row.get(6)?,
        frame_start: row.get(7)?,
        frame_end: row.get(8)?,
        frame_step: row.get(9)?,
        parallelism: configured_parallelism,
        effective_parallelism: if frame_order_mode == "strict" {
            1
        } else if active_workers > 0 {
            active_workers
        } else {
            configured_parallelism
        },
        ready_workers,
        execution_mode: row.get(33)?,
        frame_order_mode,
        output_dir: row.get(10)?,
        blender_path: row.get(11)?,
        current_frame: row.get(12)?,
        error: row.get(13)?,
        archived: row.get::<_, i64>(14)? != 0,
        created_at: row.get(15)?,
        started_at: row.get(16)?,
        finished_at: row.get(17)?,
        total_frames: total,
        completed_frames: completed,
        failed_frames: failed,
        skipped_frames: skipped,
        progress,
        cpu_usage: row.get(23)?,
        memory_bytes: row.get(24)?,
        peak_cpu_usage: row.get(25)?,
        peak_memory_bytes: row.get(26)?,
        performance_updated_at: row.get(27)?,
        position: row.get(18)?,
        batch_name: row.get(29)?,
        batch_status: row.get(30)?,
        batch_position: row.get(31)?,
        attention_code: row.get(32)?,
    })
}

fn render_job_settings(job: &RenderJob, spec: &Value) -> RenderJobSettings {
    RenderJobSettings {
        scene_name: job.scene_name.clone(),
        frame_start: job.frame_start,
        frame_end: job.frame_end,
        frame_step: job.frame_step,
        parallelism: job.parallelism,
        execution_mode: job.execution_mode.clone(),
        frame_order_mode: job.frame_order_mode.clone(),
        resolution_percentage: spec
            .get("resolutionPercentage")
            .and_then(Value::as_i64)
            .unwrap_or(100),
        engine: spec
            .get("engine")
            .and_then(Value::as_str)
            .map(str::to_string),
        output_format: spec
            .get("outputFormat")
            .and_then(Value::as_str)
            .unwrap_or("PNG")
            .to_string(),
    }
}

const JOB_SELECT: &str = r#"SELECT j.id,j.batch_id,j.project_path,j.name,j.blend_path,j.scene_name,j.status,j.frame_start,j.frame_end,j.frame_step,j.output_dir,j.blender_path,j.current_frame,j.error,j.archived,j.created_at,j.started_at,j.finished_at,j.position,
    (SELECT COUNT(*) FROM render_frames f WHERE f.job_id=j.id) AS total,
    (SELECT COUNT(*) FROM render_frames f WHERE f.job_id=j.id AND f.status='completed'),
    (SELECT COUNT(*) FROM render_frames f WHERE f.job_id=j.id AND f.status='failed'),
    (SELECT COUNT(*) FROM render_frames f WHERE f.job_id=j.id AND f.status='skipped'),
    j.cpu_usage,j.memory_bytes,j.peak_cpu_usage,j.peak_memory_bytes,j.performance_updated_at,j.parallelism,
    b.name,b.status,b.position,j.attention_code,j.execution_mode,j.frame_order_mode,
    (SELECT COUNT(*) FROM render_workers w WHERE w.job_id=j.id AND w.state IN ('ready','rendering')),
    (SELECT COUNT(*) FROM render_workers w WHERE w.job_id=j.id AND w.state IN ('starting','ready','rendering'))
    FROM render_jobs j JOIN render_batches b ON b.id=j.batch_id"#;

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

fn weighted_quantile(values: &[f64], weights: &[f64], quantile: f64) -> f64 {
    let mut ordered: Vec<(f64, f64)> = values
        .iter()
        .copied()
        .zip(weights.iter().copied())
        .collect();
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    let total_weight: f64 = ordered.iter().map(|(_, weight)| weight).sum();
    let target = total_weight * quantile.clamp(0.0, 1.0);
    let mut accumulated = 0.0;
    for (value, weight) in &ordered {
        accumulated += weight;
        if accumulated >= target {
            return *value;
        }
    }
    ordered.last().map(|(value, _)| *value).unwrap_or(0.0)
}

fn estimate_render_eta(job: &RenderJob, frames: &[RenderFrame], now_ms: i64) -> RenderEta {
    let completed: Vec<(usize, f64)> = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            (frame.status == "completed")
                .then_some(frame.render_duration_ms.or(frame.duration_ms))
                .flatten()
                .filter(|duration| *duration > 0)
                .map(|duration| (index, duration as f64))
        })
        .collect();
    let sample_count = completed.len();

    let state = match job.status.as_str() {
        "completed" => Some(("completed", Some(0))),
        "paused" => Some(("paused", None)),
        "failed" | "cancelled" | "attention" => Some(("unavailable", None)),
        _ => None,
    };
    if let Some((status, remaining_ms)) = state {
        return RenderEta {
            status: status.to_string(),
            estimated_finish_at: remaining_ms.map(|_| job.finished_at.unwrap_or(now_ms)),
            remaining_ms,
            sample_count,
            confidence: "none".to_string(),
        };
    }

    if sample_count < 2 {
        return RenderEta {
            status: "calibrating".to_string(),
            estimated_finish_at: None,
            remaining_ms: None,
            sample_count,
            confidence: "low".to_string(),
        };
    }

    let durations: Vec<f64> = completed.iter().map(|(_, duration)| *duration).collect();
    let center = median(&durations);
    let deviations: Vec<f64> = durations
        .iter()
        .map(|duration| (duration - center).abs())
        .collect();
    let mad = median(&deviations);
    let huber_threshold = (mad * 1.5).max(center * 0.25).max(1_000.0);
    let later_center = median(&durations[1..]);
    let cold_start = durations[0] > later_center * 2.0;

    let weights: Vec<f64> = durations
        .iter()
        .enumerate()
        .map(|(index, duration)| {
            let age = sample_count - 1 - index;
            let mut weight = 0.78_f64.powf(age as f64);
            if cold_start && index == 0 {
                weight *= 0.15;
            }
            let residual = (duration - center).abs();
            if residual > huber_threshold {
                weight *= huber_threshold / residual;
            }
            weight.max(0.000_001)
        })
        .collect();
    let total_weight: f64 = weights.iter().sum();
    let baseline = durations
        .iter()
        .zip(&weights)
        .map(|(duration, weight)| duration * weight)
        .sum::<f64>()
        / total_weight;
    let mean_x = completed
        .iter()
        .zip(&weights)
        .map(|((index, _), weight)| *index as f64 * weight)
        .sum::<f64>()
        / total_weight;
    let regression_numerator = completed
        .iter()
        .zip(&weights)
        .map(|((index, duration), weight)| {
            weight * (*index as f64 - mean_x) * (duration - baseline)
        })
        .sum::<f64>();
    let regression_denominator = completed
        .iter()
        .zip(&weights)
        .map(|((index, _), weight)| weight * (*index as f64 - mean_x).powi(2))
        .sum::<f64>();
    let raw_slope = if regression_denominator > f64::EPSILON {
        regression_numerator / regression_denominator
    } else {
        0.0
    };
    let slope_limit = baseline * 0.12;
    let slope = raw_slope.clamp(-slope_limit, slope_limit);
    let minimum_prediction = baseline * 0.45;
    let maximum_prediction = baseline * 2.2;
    let percentile_80 = weighted_quantile(&durations, &weights, 0.8);
    let last_completed_index = completed
        .last()
        .map(|(index, _)| *index as f64)
        .unwrap_or(mean_x);
    const TREND_FORECAST_HORIZON: f64 = 8.0;

    let mut worker_loads = vec![0.0_f64; job.effective_parallelism.clamp(1, 8) as usize];
    for (index, frame) in frames.iter().enumerate() {
        if !matches!(frame.status.as_str(), "pending" | "running") {
            continue;
        }
        // A small measured slope becomes implausibly large when projected across a long job.
        // Keep learning the direction, but let its effect converge after a short horizon.
        let projected_index = (index as f64).min(last_completed_index + TREND_FORECAST_HORIZON);
        let predicted_total = (baseline + slope * (projected_index - mean_x))
            .clamp(minimum_prediction, maximum_prediction);
        let mut work = if frame.status == "running" {
            let elapsed = now_ms.saturating_sub(frame.updated_at).max(0) as f64;
            let conditional_total = predicted_total.max(percentile_80).max(elapsed * 1.08);
            (conditional_total - elapsed).max(0.0)
        } else {
            let mut pending_work = predicted_total;
            if frame.attempts > 0 {
                pending_work += if frame.attempts == 1 {
                    5_000.0
                } else {
                    15_000.0
                };
            }
            pending_work
        };
        if !work.is_finite() {
            work = baseline;
        }
        let target = worker_loads
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| left.total_cmp(right))
            .map(|(index, _)| index)
            .unwrap_or(0);
        worker_loads[target] += work;
    }

    let weighted_variance = durations
        .iter()
        .zip(&weights)
        .map(|(duration, weight)| weight * (duration - baseline).powi(2))
        .sum::<f64>()
        / total_weight;
    let variation = weighted_variance.sqrt() / baseline.max(1.0);
    let confidence = if sample_count >= 6 && variation <= 0.25 {
        "high"
    } else if sample_count >= 3 && variation <= 0.6 {
        "medium"
    } else {
        "low"
    };
    let remaining = worker_loads.into_iter().fold(0.0_f64, f64::max);
    let remaining_ms = remaining.ceil().clamp(0.0, i64::MAX as f64) as i64;

    RenderEta {
        status: "estimating".to_string(),
        estimated_finish_at: Some(now_ms.saturating_add(remaining_ms)),
        remaining_ms: Some(remaining_ms),
        sample_count,
        confidence: confidence.to_string(),
    }
}

#[tauri::command]
pub async fn list_render_jobs(
    project_path: String,
    include_archived: Option<bool>,
) -> Result<Vec<RenderJob>, String> {
    init_project_storage(&project_path)?;
    let conn = open_db(&project_path)?;
    let sql = format!(
        "{} WHERE (?1=1 OR j.archived=0) ORDER BY b.position ASC, j.position ASC, j.created_at ASC",
        JOB_SELECT
    );
    let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let jobs = stmt
        .query_map(
            params![include_archived.unwrap_or(false) as i64],
            row_to_job,
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(jobs)
}

#[tauri::command]
pub async fn get_render_job(
    project_path: String,
    job_id: String,
) -> Result<RenderJobDetail, String> {
    let conn = open_db(&project_path)?;
    let sql = format!("{} WHERE j.id=?1", JOB_SELECT);
    let job = conn
        .query_row(&sql, params![job_id], row_to_job)
        .map_err(|error| error.to_string())?;
    let spec_json: String = conn
        .query_row(
            "SELECT spec_json FROM render_jobs WHERE id=?1",
            params![job.id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let spec = serde_json::from_str(&spec_json).unwrap_or(Value::Null);
    let settings = render_job_settings(&job, &spec);
    let mut stmt = conn.prepare("SELECT job_id,frame,status,attempts,output_path,error,duration_ms,updated_at,render_duration_ms,worker_id,claim_token FROM render_frames WHERE job_id=?1 ORDER BY frame")
        .map_err(|error| error.to_string())?;
    let frames = stmt
        .query_map(params![&job.id], |row| {
            Ok(RenderFrame {
                job_id: row.get(0)?,
                frame: row.get(1)?,
                status: row.get(2)?,
                attempts: row.get(3)?,
                output_path: row.get(4)?,
                error: row.get(5)?,
                duration_ms: row.get(6)?,
                updated_at: row.get(7)?,
                render_duration_ms: row.get(8)?,
                worker_id: row.get(9)?,
                claim_token: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let eta = estimate_render_eta(&job, &frames, now());
    let log_path = PathBuf::from(&project_path)
        .join(".pm_center/render_jobs")
        .join(&job.id)
        .join("render.log");
    let log_tail = fs::read_to_string(log_path)
        .unwrap_or_default()
        .lines()
        .rev()
        .take(500)
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut performance_stmt = conn
        .prepare(
            "SELECT sampled_at,cpu_usage,memory_bytes FROM (
                SELECT sampled_at,cpu_usage,memory_bytes
                FROM render_performance_samples
                WHERE job_id=?1
                ORDER BY sampled_at DESC
                LIMIT 300
            ) ORDER BY sampled_at ASC",
        )
        .map_err(|error| error.to_string())?;
    let performance_samples = performance_stmt
        .query_map(params![job.id], |row| {
            Ok(RenderPerformanceSample {
                sampled_at: row.get(0)?,
                cpu_usage: row.get(1)?,
                memory_bytes: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut worker_stmt = conn
        .prepare("SELECT worker_id,ordinal,pid,state,current_frame,startup_ms,error,updated_at FROM render_workers WHERE job_id=?1 ORDER BY ordinal")
        .map_err(|error| error.to_string())?;
    let workers = worker_stmt
        .query_map(params![job.id], |row| {
            Ok(RenderWorkerState {
                worker_id: row.get(0)?,
                ordinal: row.get(1)?,
                pid: row
                    .get::<_, Option<i64>>(2)?
                    .map(|value| value.max(0) as u32),
                state: row.get(3)?,
                current_frame: row.get(4)?,
                startup_ms: row.get(5)?,
                error: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let startup_values: Vec<i64> = workers
        .iter()
        .filter_map(|worker| worker.startup_ms)
        .collect();
    let startup = RenderStartupStats {
        requested_workers: job.effective_parallelism,
        ready_workers: workers
            .iter()
            .filter(|worker| matches!(worker.state.as_str(), "ready" | "rendering"))
            .count() as i64,
        average_startup_ms: (!startup_values.is_empty())
            .then(|| startup_values.iter().sum::<i64>() / startup_values.len() as i64),
    };
    Ok(RenderJobDetail {
        job,
        settings,
        frames,
        log_tail,
        performance_samples,
        eta,
        workers,
        startup,
    })
}

fn update_render_job_settings(
    conn: &mut Connection,
    job_id: &str,
    request: &UpdateRenderJobRequest,
) -> Result<(bool, Value), String> {
    if request.scene_name.trim().is_empty() {
        return Err("场景不能为空".into());
    }
    if request.frame_step <= 0 || request.frame_end < request.frame_start {
        return Err("帧范围无效".into());
    }
    if !(1..=8).contains(&request.parallelism) {
        return Err("帧多开必须在 1 到 8 之间".into());
    }
    if !(1..=100).contains(&request.resolution_percentage) {
        return Err("分辨率比例必须在 1 到 100 之间".into());
    }
    let execution_mode = normalize_execution_mode(&request.execution_mode)?;
    let frame_order_mode = normalize_frame_order_mode(&request.frame_order_mode)?;
    let output_format = normalize_output_format(&request.output_format)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let (status, output_dir, blend_path, spec_json, old_scene_name): (
        String,
        String,
        String,
        String,
        String,
    ) = transaction
        .query_row(
            "SELECT status,output_dir,blend_path,spec_json,scene_name FROM render_jobs WHERE id=?1",
            params![job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    if matches!(
        status.as_str(),
        "starting" | "running" | "pausing" | "cancelling"
    ) {
        return Err("任务正在运行，请先暂停并等待当前帧结束后再修改".into());
    }

    let mut spec = serde_json::from_str::<Value>(&spec_json).unwrap_or_else(|_| json!({}));
    if !spec.is_object() {
        spec = json!({});
    }
    let old_resolution_percentage = spec
        .get("resolutionPercentage")
        .and_then(Value::as_i64)
        .unwrap_or(100);
    let old_engine = spec
        .get("engine")
        .and_then(Value::as_str)
        .map(str::to_string);
    let old_output_format = spec
        .get("outputFormat")
        .and_then(Value::as_str)
        .unwrap_or("PNG")
        .to_ascii_uppercase();
    let render_settings_changed = old_scene_name != request.scene_name
        || old_resolution_percentage != request.resolution_percentage
        || old_engine != request.engine
        || old_output_format != output_format;
    spec["blendPath"] = json!(blend_path);
    spec["sceneName"] = json!(request.scene_name);
    spec["frameStart"] = json!(request.frame_start);
    spec["frameEnd"] = json!(request.frame_end);
    spec["frameStep"] = json!(request.frame_step);
    spec["parallelism"] = json!(request.parallelism);
    spec["executionMode"] = json!(execution_mode);
    spec["frameOrderMode"] = json!(frame_order_mode);
    spec["resolutionPercentage"] = json!(request.resolution_percentage);
    spec["engine"] = json!(request.engine);
    spec["outputFormat"] = json!(output_format);

    let existing_frames = {
        let mut statement = transaction
            .prepare("SELECT frame,status,output_path FROM render_frames WHERE job_id=?1")
            .map_err(|error| error.to_string())?;
        let frames = statement
            .query_map(params![job_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        frames
    };
    let existing_by_frame: HashMap<i64, (String, String)> = existing_frames
        .iter()
        .map(|(frame, status, path)| (*frame, (status.clone(), path.clone())))
        .collect();
    let desired_frames: Vec<i64> = (request.frame_start..=request.frame_end)
        .step_by(request.frame_step as usize)
        .collect();
    let desired_set: HashSet<i64> = desired_frames.iter().copied().collect();

    for (frame, _, _) in &existing_frames {
        if desired_set.contains(frame) {
            continue;
        }
        transaction
            .execute(
                "DELETE FROM render_attempts WHERE job_id=?1 AND frame=?2",
                params![job_id, frame],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM render_artifacts WHERE job_id=?1 AND frame=?2",
                params![job_id, frame],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM render_frames WHERE job_id=?1 AND frame=?2",
                params![job_id, frame],
            )
            .map_err(|error| error.to_string())?;
    }

    let output_dir_path = Path::new(&output_dir);
    for frame in desired_frames {
        let output_path = frame_output_path(
            output_dir_path,
            &request.scene_name,
            frame,
            request.frame_end,
            &output_format,
        );
        let output_path = output_path.to_string_lossy().to_string();
        if let Some((old_status, old_output_path)) = existing_by_frame.get(&frame) {
            if !render_settings_changed {
                // Extending or narrowing a frame range must not renumber existing outputs
                // or rerender valid work. Only revive a completed record whose file vanished.
                let output_missing = matches!(old_status.as_str(), "completed" | "skipped")
                    && !valid_output(Path::new(old_output_path));
                if !output_missing {
                    continue;
                }
                transaction
                    .execute(
                    "UPDATE render_frames SET status='pending',attempts=0,error=NULL,duration_ms=NULL,render_duration_ms=NULL,force_render=0,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2",
                        params![job_id, frame, now()],
                    )
                    .map_err(|error| error.to_string())?;
                continue;
            }
            transaction
                .execute(
                    "DELETE FROM render_attempts WHERE job_id=?1 AND frame=?2",
                    params![job_id, frame],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "DELETE FROM render_artifacts WHERE job_id=?1 AND frame=?2",
                    params![job_id, frame],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE render_frames SET status='pending',attempts=0,output_path=?3,error=NULL,duration_ms=NULL,render_duration_ms=NULL,force_render=1,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?4 WHERE job_id=?1 AND frame=?2",
                    params![job_id, frame, output_path, now()],
                )
                .map_err(|error| error.to_string())?;
        } else {
            transaction
                .execute(
                    "INSERT INTO render_frames(job_id,frame,status,output_path,force_render,updated_at) VALUES(?1,?2,'pending',?3,?4,?5)",
                    params![job_id, frame, output_path, if render_settings_changed { 1 } else { 0 }, now()],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    let (pending_count, failed_count, completed_count, total_count): (i64, i64, i64, i64) =
        transaction
            .query_row(
                "SELECT SUM(CASE WHEN status='pending' THEN 1 ELSE 0 END),SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END),SUM(CASE WHEN status IN ('completed','skipped') THEN 1 ELSE 0 END),COUNT(*) FROM render_frames WHERE job_id=?1",
                params![job_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|error| error.to_string())?;
    let next_status = if pending_count > 0 {
        if status == "pending" {
            "pending"
        } else {
            "paused"
        }
    } else if failed_count > 0 {
        "failed"
    } else if total_count > 0 && completed_count == total_count {
        "completed"
    } else {
        status.as_str()
    };
    let requires_more_work = pending_count > 0;
    let clear_error = requires_more_work || next_status != status;
    let blend_stem = Path::new(&blend_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("blend");
    let name = format!("{} · {}", blend_stem, request.scene_name);
    transaction
        .execute(
            "UPDATE render_jobs SET name=?2,scene_name=?3,frame_start=?4,frame_end=?5,frame_step=?6,parallelism=?7,execution_mode=?8,frame_order_mode=?9,spec_json=?10,status=?11,current_frame=NULL,error=CASE WHEN ?12 THEN NULL ELSE error END,attention_code=NULL,finished_at=CASE WHEN ?13 THEN NULL ELSE finished_at END WHERE id=?1",
            params![job_id,name,request.scene_name,request.frame_start,request.frame_end,request.frame_step,request.parallelism,execution_mode,frame_order_mode,spec.to_string(),next_status,clear_error,requires_more_work],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok((next_status == "pending", spec))
}

#[tauri::command]
pub async fn update_render_job(
    app_handle: tauri::AppHandle,
    project_path: String,
    job_id: String,
    request: UpdateRenderJobRequest,
) -> Result<(), String> {
    let mut conn = open_db(&project_path)?;
    let (should_kick_scheduler, spec) = update_render_job_settings(&mut conn, &job_id, &request)?;
    let job_dir = PathBuf::from(&project_path)
        .join(".pm_center/render_jobs")
        .join(&job_id);
    fs::create_dir_all(&job_dir).map_err(|error| error.to_string())?;
    fs::write(
        job_dir.join("job.json"),
        serde_json::to_vec_pretty(&spec).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    emit_queue(&app_handle, &project_path);
    emit_progress(
        &app_handle,
        &project_path,
        &job_id,
        None,
        "settings-updated",
    );
    if should_kick_scheduler {
        kick_scheduler(app_handle, project_path);
    }
    Ok(())
}

fn emit_queue(app: &tauri::AppHandle, project_path: &str) {
    let _ = app.emit(
        "pm-center:render-queue-updated",
        json!({ "projectPath": project_path }),
    );
}

fn emit_progress(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    frame: Option<i64>,
    phase: &str,
) {
    let _ = app.emit(
        "pm-center:render-job-progress",
        json!({ "projectPath": project_path, "jobId": job_id, "frame": frame, "phase": phase }),
    );
}

fn register_runtime_project(project_path: &str) {
    let mut runtime = RUNTIME.lock().unwrap();
    if runtime.projects.insert(project_path.to_string()) {
        runtime.project_order.push(project_path.to_string());
    }
}

fn publish_worker_state(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    control: &Arc<Mutex<JobControl>>,
    worker: RenderWorkerState,
) {
    if let Ok(conn) = open_db(project_path) {
        let _ = conn.execute(
            "INSERT INTO render_workers(worker_id,job_id,ordinal,pid,state,current_frame,startup_ms,error,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(worker_id) DO UPDATE SET pid=excluded.pid,state=excluded.state,current_frame=excluded.current_frame,startup_ms=excluded.startup_ms,error=excluded.error,updated_at=excluded.updated_at",
            params![worker.worker_id, job_id, worker.ordinal, worker.pid.map(i64::from), worker.state, worker.current_frame, worker.startup_ms, worker.error, worker.updated_at],
        );
    }
    control
        .lock()
        .unwrap()
        .workers
        .insert(worker.worker_id.clone(), worker.clone());
    let _ = app.emit(
        "pm-center:render-worker-progress",
        json!({
            "projectPath": project_path,
            "jobId": job_id,
            "worker": worker,
        }),
    );
    emit_progress(app, project_path, job_id, worker.current_frame, "worker");
}

fn worker_state(
    worker_id: &str,
    ordinal: i64,
    pid: Option<u32>,
    state: &str,
    current_frame: Option<i64>,
    startup_ms: Option<i64>,
    error: Option<String>,
) -> RenderWorkerState {
    RenderWorkerState {
        worker_id: worker_id.to_string(),
        ordinal,
        pid,
        state: state.to_string(),
        current_frame,
        startup_ms,
        error,
        updated_at: now(),
    }
}

fn kick_scheduler(app: tauri::AppHandle, project_path: String) {
    register_runtime_project(&project_path);
    tauri::async_runtime::spawn(async move {
        let _scheduler_guard = SCHEDULER_LOCK.lock().await;
        let mut started_job = false;
        let concurrency = load_scheduler_settings(&app).concurrency.clamp(1, 8) as usize;
        loop {
            let _ = advance_batch_queue(&project_path);
            let running_total = RUNTIME.lock().unwrap().running.len();
            if running_total >= concurrency {
                break;
            }
            let next_job = open_db(&project_path).ok().and_then(|conn| {
                conn.query_row("SELECT j.id FROM render_jobs j JOIN render_batches b ON b.id=j.batch_id WHERE j.status='pending' AND j.archived=0 AND b.status='running' ORDER BY b.position ASC, j.position ASC, j.created_at ASC LIMIT 1", [], |row| row.get::<_, String>(0)).optional().ok().flatten()
            });
            let Some(job_id) = next_job else {
                break;
            };
            let claimed = open_db(&project_path)
                .and_then(|conn| {
                    conn.execute(
                        "UPDATE render_jobs SET status='starting' WHERE id=?1 AND status='pending'",
                        params![job_id],
                    )
                    .map_err(|error| error.to_string())
                })
                .unwrap_or(0);
            if claimed == 0 {
                continue;
            }
            let key = format!("{}\n{}", project_path, job_id);
            let control = Arc::new(Mutex::new(JobControl::default()));
            {
                let mut runtime = RUNTIME.lock().unwrap();
                if runtime.running.contains_key(&key) {
                    break;
                }
                runtime.running.insert(key.clone(), control.clone());
            }
            let app_clone = app.clone();
            let project_clone = project_path.clone();
            tauri::async_runtime::spawn(async move {
                let run_result =
                    run_job(&app_clone, &project_clone, &job_id, control.clone()).await;
                if let Err(error) = run_result {
                    let (cancel, pause, attention) = {
                        let value = control.lock().unwrap();
                        (value.cancel, value.pause, value.attention)
                    };
                    if !attention {
                        if let Ok(mut conn) = open_db(&project_clone) {
                            let status = conn
                                .query_row(
                                    "SELECT status FROM render_jobs WHERE id=?1",
                                    params![job_id],
                                    |row| row.get::<_, String>(0),
                                )
                                .unwrap_or_default();
                            if matches!(
                                status.as_str(),
                                "starting" | "running" | "pausing" | "cancelling"
                            ) {
                                let (terminal_status, reason) = if cancel {
                                    ("cancelled", "用户取消")
                                } else if pause {
                                    ("paused", "用户暂停")
                                } else {
                                    ("failed", error.as_str())
                                };
                                let _ =
                                    settle_runtime_job(&mut conn, &job_id, terminal_status, reason);
                                emit_progress(
                                    &app_clone,
                                    &project_clone,
                                    &job_id,
                                    None,
                                    terminal_status,
                                );
                            }
                        }
                    }
                }
                RUNTIME.lock().unwrap().running.remove(&key);
                emit_queue(&app_clone, &project_clone);
                kick_all_schedulers(app_clone);
            });
            started_job = true;
            break;
        }
        if started_job && RUNTIME.lock().unwrap().running.len() < concurrency {
            kick_all_schedulers(app.clone());
        }
    });
}

fn advance_batch_queue(project_path: &str) -> Result<(), String> {
    let conn = open_db(project_path)?;
    conn.execute(
        "UPDATE render_batches SET status=CASE WHEN EXISTS (SELECT 1 FROM render_jobs WHERE render_jobs.batch_id=render_batches.id AND render_jobs.archived=0 AND render_jobs.status='failed') THEN 'failed' WHEN EXISTS (SELECT 1 FROM render_jobs WHERE render_jobs.batch_id=render_batches.id AND render_jobs.archived=0 AND render_jobs.status='cancelled') THEN 'cancelled' ELSE 'completed' END,updated_at=?2 WHERE project_path=?1 AND status='running' AND NOT EXISTS (SELECT 1 FROM render_jobs WHERE render_jobs.batch_id=render_batches.id AND render_jobs.archived=0 AND render_jobs.status IN ('pending','starting','running','pausing','paused','attention'))",
        params![project_path, now()],
    )
    .map_err(|error| error.to_string())?;
    let running_batches: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM render_batches WHERE project_path=?1 AND status='running'",
            params![project_path],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if running_batches > 0 {
        return Ok(());
    }
    let next_batch = conn
        .query_row(
            "SELECT id FROM render_batches WHERE project_path=?1 AND status='queued' ORDER BY position ASC, created_at ASC LIMIT 1",
            params![project_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(batch_id) = next_batch else {
        return Ok(());
    };
    conn.execute(
        "UPDATE render_batches SET status='running',updated_at=?2 WHERE id=?1",
        params![batch_id, now()],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE render_jobs SET status='pending',error=NULL,finished_at=NULL WHERE batch_id=?1 AND status='paused' AND archived=0",
        params![batch_id],
    )
    .map_err(|error| error.to_string())?;
    let live_jobs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM render_jobs WHERE batch_id=?1 AND archived=0 AND status IN ('pending','starting','running','pausing','paused','attention')",
            params![batch_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if live_jobs == 0 {
        conn.execute(
            "UPDATE render_batches SET status=CASE WHEN EXISTS (SELECT 1 FROM render_jobs WHERE batch_id=?1 AND archived=0 AND status='failed') THEN 'failed' WHEN EXISTS (SELECT 1 FROM render_jobs WHERE batch_id=?1 AND archived=0 AND status='cancelled') THEN 'cancelled' ELSE 'completed' END,updated_at=?2 WHERE id=?1",
            params![batch_id, now()],
        )
        .map_err(|error| error.to_string())?;
        drop(conn);
        return advance_batch_queue(project_path);
    }
    Ok(())
}

fn kick_all_schedulers(app: tauri::AppHandle) {
    let projects = {
        let mut runtime = RUNTIME.lock().unwrap();
        if runtime.project_order.is_empty() {
            Vec::new()
        } else {
            let len = runtime.project_order.len();
            let start = runtime.project_cursor % len;
            runtime.project_cursor = (start + 1) % len;
            (0..len)
                .map(|offset| runtime.project_order[(start + offset) % len].clone())
                .collect::<Vec<_>>()
        }
    };
    for project_path in projects {
        kick_scheduler(app.clone(), project_path);
    }
}

#[derive(Debug, Clone)]
struct JobExecutionSpec {
    blend_path: String,
    scene_name: String,
    blender_path: String,
    pre_hook: Option<String>,
    post_hook: Option<String>,
    force_overwrite: bool,
    max_retries: i64,
    parallelism: i64,
    execution_mode: String,
    frame_order_mode: String,
    spec: Value,
}

fn load_execution_spec(conn: &Connection, job_id: &str) -> Result<JobExecutionSpec, String> {
    conn.query_row("SELECT blend_path,scene_name,blender_path,pre_hook,post_hook,force_overwrite,max_retries,spec_json,parallelism,execution_mode,frame_order_mode FROM render_jobs WHERE id=?1", params![job_id], |row| {
        let spec_json: String = row.get(7)?;
        Ok(JobExecutionSpec { blend_path: row.get(0)?, scene_name: row.get(1)?, blender_path: row.get(2)?, pre_hook: row.get(3)?, post_hook: row.get(4)?, force_overwrite: row.get::<_,i64>(5)? != 0, max_retries: row.get(6)?, spec: serde_json::from_str(&spec_json).unwrap_or(Value::Null), parallelism: row.get::<_,i64>(8)?.clamp(1,8), execution_mode: row.get(9)?, frame_order_mode: row.get(10)? })
    }).map_err(|error| error.to_string())
}

fn verify_job_source(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    blend_path: &str,
    control: &Arc<Mutex<JobControl>>,
) -> Result<(), String> {
    let _verification_guard = SOURCE_VERIFY_LOCK.lock().unwrap();
    if let Ok(metadata) = fs::metadata(blend_path) {
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or(0);
        if control
            .lock()
            .unwrap()
            .verified_source
            .as_ref()
            .is_some_and(|value| {
                value.size == metadata.len().min(i64::MAX as u64) as i64
                    && value.modified_at == modified_at
            })
        {
            return Ok(());
        }
    }
    let fingerprint = source_fingerprint(Path::new(blend_path))?;
    let conn = open_db(project_path)?;
    let (stored_hash, completed): (Option<String>, i64) = conn
        .query_row(
            "SELECT source_hash,(SELECT COUNT(*) FROM render_frames WHERE job_id=?1 AND status IN ('completed','skipped')) FROM render_jobs WHERE id=?1",
            params![job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    if stored_hash
        .as_deref()
        .is_none_or(|value| value == fingerprint.hash)
        || completed == 0
    {
        conn.execute(
            "UPDATE render_jobs SET source_hash=?2,source_size=?3,source_modified_at=?4,attention_code=NULL,error=CASE WHEN attention_code='source_changed' THEN NULL ELSE error END WHERE id=?1",
            params![job_id, fingerprint.hash, fingerprint.size, fingerprint.modified_at],
        )
        .map_err(|error| error.to_string())?;
        control.lock().unwrap().verified_source = Some(fingerprint);
        return Ok(());
    }
    conn.execute(
        "UPDATE render_jobs SET status='attention',attention_code='source_changed',error='源 Blender 文件已变化，请确认全部重新渲染后再继续',current_frame=NULL WHERE id=?1",
        params![job_id],
    )
    .map_err(|error| error.to_string())?;
    control.lock().unwrap().attention = true;
    emit_progress(app, project_path, job_id, None, "source-changed");
    Err("源 Blender 文件已变化".into())
}

async fn run_hook(
    app_handle: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    script: Option<&str>,
    phase: &str,
) -> Result<(), String> {
    let Some(script) = script.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let runtime = crate::plugin::prepare_pmc_python_runtime(app_handle)
        .map_err(|error| format!("{phase}脚本无法使用 PMC 内置 Python: {error}"))?;
    let mut command = tokio_command(&runtime.program);
    command
        .arg(script)
        .current_dir(project_path)
        .env("PM_RENDER_JOB_ID", job_id)
        .env("PMC_PROJECT_PATH", project_path);
    for (key, value) in runtime.env_vars {
        command.env(key, value);
    }
    let output = command
        .output()
        .await
        .map_err(|error| format!("{phase}脚本启动失败: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{phase}脚本失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

async fn run_job(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    control: Arc<Mutex<JobControl>>,
) -> Result<(), String> {
    let mut conn = open_db(project_path)?;
    let spec = load_execution_spec(&conn, job_id)?;
    conn.execute(
        "DELETE FROM render_workers WHERE job_id=?1",
        params![job_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute("UPDATE render_jobs SET status='starting',started_at=COALESCE(started_at,?2),finished_at=NULL,error=NULL,attention_code=NULL WHERE id=?1 AND status IN ('pending','starting')", params![job_id, now()]).map_err(|e| e.to_string())?;
    emit_progress(app, project_path, job_id, None, "starting");
    if let Err(error) = run_hook(app, project_path, job_id, spec.pre_hook.as_deref(), "前置").await
    {
        fail_job(&conn, job_id, &error)?;
        return Err(error);
    }
    let job_dir = PathBuf::from(project_path)
        .join(".pm_center/render_jobs")
        .join(job_id);
    let bootstrap = write_runtime_scripts(&job_dir)?;
    let persistent_worker = write_persistent_worker_script(&job_dir)?;
    let mut workers = Vec::new();
    let desired_workers = if spec.frame_order_mode == "strict" {
        1
    } else {
        spec.parallelism
    };
    for ordinal in 0..desired_workers {
        let app = app.clone();
        let project_path = project_path.to_string();
        let job_id = job_id.to_string();
        let spec = spec.clone();
        let bootstrap = bootstrap.clone();
        let persistent_worker = persistent_worker.clone();
        let job_dir = job_dir.clone();
        let control = control.clone();
        workers.push(tauri::async_runtime::spawn(async move {
            if ordinal > 0 {
                tokio::time::sleep(Duration::from_millis((ordinal as u64) * 75)).await;
            }
            run_job_worker(
                &app,
                &project_path,
                &job_id,
                &spec,
                &bootstrap,
                &persistent_worker,
                &job_dir,
                ordinal,
                control,
            )
            .await
        }));
    }
    let mut worker_error = None;
    for worker in workers {
        match worker.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                worker_error.get_or_insert(error);
            }
            Err(error) => {
                worker_error.get_or_insert_with(|| format!("渲染工作进程异常: {error}"));
            }
        };
    }
    let (cancel, pause, attention) = {
        let value = control.lock().unwrap();
        (value.cancel, value.pause, value.attention)
    };
    if cancel {
        settle_runtime_job(&mut conn, job_id, "cancelled", "用户取消")?;
        emit_progress(app, project_path, job_id, None, "cancelled");
        return Ok(());
    }
    if pause {
        settle_runtime_job(&mut conn, job_id, "paused", "用户暂停")?;
        emit_progress(app, project_path, job_id, None, "paused");
        return Ok(());
    }
    let temp_outputs = conn
        .prepare("SELECT temp_output_path FROM render_frames WHERE job_id=?1 AND status='running' AND temp_output_path IS NOT NULL")
        .and_then(|mut statement| {
            statement
                .query_map(params![job_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    for path in temp_outputs {
        let _ = fs::remove_file(path);
    }
    conn.execute(
        "DELETE FROM render_attempts WHERE job_id=?1 AND status='running'",
        params![job_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE render_frames SET status='pending',error=NULL,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?2 WHERE job_id=?1 AND status='running'",
        params![job_id, now()],
    )
    .map_err(|error| error.to_string())?;
    if attention {
        return Ok(());
    }
    if let Some(error) = worker_error {
        fail_job(&conn, job_id, &error)?;
        return Err(error);
    }
    let failed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM render_frames WHERE job_id=?1 AND status='failed'",
            params![job_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if let Err(error) = run_hook(app, project_path, job_id, spec.post_hook.as_deref(), "后置").await
    {
        fail_job(&conn, job_id, &error)?;
        return Err(error);
    }
    let status = if failed > 0 { "failed" } else { "completed" };
    let message = if failed > 0 {
        Some(format!("{failed} 帧渲染失败"))
    } else {
        None
    };
    conn.execute(
        "UPDATE render_jobs SET status=?2,current_frame=NULL,finished_at=?3,error=?4 WHERE id=?1",
        params![job_id, status, now(), message],
    )
    .map_err(|e| e.to_string())?;
    emit_progress(app, project_path, job_id, None, status);
    Ok(())
}

#[derive(Debug, Clone)]
struct ClaimedFrame {
    frame: i64,
    output_path: String,
    attempts: i64,
    force_render: bool,
    worker_id: String,
    claim_token: String,
    temp_output_path: String,
}

fn temporary_output_path(output_path: &str, claim_token: &str) -> Result<String, String> {
    let output = Path::new(output_path);
    let parent = output
        .parent()
        .ok_or_else(|| format!("输出路径没有父目录: {output_path}"))?;
    let file_name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("输出文件名无效: {output_path}"))?;
    let extension = output
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("tmp");
    Ok(parent
        .join(format!(".{file_name}.{claim_token}.pm-part.{extension}"))
        .to_string_lossy()
        .to_string())
}

fn claim_next_frame(
    conn: &mut Connection,
    job_id: &str,
    max_retries: i64,
    worker_id: &str,
) -> Result<Option<ClaimedFrame>, String> {
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let next = transaction
        .query_row(
            "SELECT frame,output_path,attempts,force_render FROM render_frames WHERE job_id=?1 AND status IN ('pending','failed') AND attempts<=?2 ORDER BY frame LIMIT 1",
            params![job_id, max_retries],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)? != 0)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let claimed = if let Some((frame, output_path, attempts, force_render)) = next {
        let claim_token = Uuid::new_v4().to_string();
        let temp_output_path = temporary_output_path(&output_path, &claim_token)?;
        let updated = transaction
            .execute(
                "UPDATE render_frames SET status='running',error=NULL,worker_id=?3,claim_token=?4,claimed_at=?5,temp_output_path=?6,updated_at=?5 WHERE job_id=?1 AND frame=?2 AND status IN ('pending','failed') AND claim_token IS NULL",
                params![job_id, frame, worker_id, claim_token, now(), temp_output_path],
            )
            .map_err(|error| error.to_string())?;
        (updated == 1).then_some(ClaimedFrame {
            frame,
            output_path,
            attempts,
            force_render,
            worker_id: worker_id.to_string(),
            claim_token,
            temp_output_path,
        })
    } else {
        None
    };
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(claimed)
}

fn begin_frame_attempt(
    conn: &Connection,
    job_id: &str,
    claim: &ClaimedFrame,
) -> Result<i64, String> {
    let attempt = claim.attempts + 1;
    conn.execute(
        "INSERT INTO render_attempts(job_id,frame,attempt,status,started_at,worker_id,claim_token,temp_output_path) VALUES(?1,?2,?3,'running',?4,?5,?6,?7)",
        params![job_id, claim.frame, attempt, now(), claim.worker_id, claim.claim_token, claim.temp_output_path],
    )
    .map_err(|error| error.to_string())?;
    Ok(attempt)
}

fn release_frame_claim(
    conn: &Connection,
    job_id: &str,
    claim: &ClaimedFrame,
) -> Result<(), String> {
    let _ = fs::remove_file(&claim.temp_output_path);
    conn.execute(
        "DELETE FROM render_attempts WHERE job_id=?1 AND frame=?2 AND claim_token=?3 AND status='running'",
        params![job_id, claim.frame, claim.claim_token],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE render_frames SET status='pending',error=NULL,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?4 WHERE job_id=?1 AND frame=?2 AND claim_token=?3",
        params![job_id, claim.frame, claim.claim_token, now()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn settle_runtime_job(
    conn: &mut Connection,
    job_id: &str,
    status: &str,
    reason: &str,
) -> Result<(), String> {
    if !matches!(status, "paused" | "cancelled" | "failed") {
        return Err(format!("不支持的任务收敛状态: {status}"));
    }
    let temp_outputs = {
        let mut statement = conn
            .prepare(
                "SELECT temp_output_path FROM render_frames WHERE job_id=?1 AND status IN ('running','committing') AND temp_output_path IS NOT NULL",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![job_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    let timestamp = now();
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE render_attempts SET status='aborted',finished_at=?2,error=?3 WHERE job_id=?1 AND status='running'",
            params![job_id, timestamp, reason],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE render_frames SET status='pending',error=NULL,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?2 WHERE job_id=?1 AND status IN ('running','committing')",
            params![job_id, timestamp],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE render_workers SET pid=NULL,state=?2,current_frame=NULL,error=CASE WHEN ?2='failed' THEN ?3 ELSE error END,updated_at=?4 WHERE job_id=?1 AND state IN ('starting','ready','rendering')",
            params![job_id, if status == "failed" { "failed" } else { "stopped" }, reason, timestamp],
        )
        .map_err(|error| error.to_string())?;
    let finished_at = (status != "paused").then_some(timestamp);
    let job_error = match status {
        "paused" => None,
        _ => Some(reason.to_string()),
    };
    transaction
        .execute(
            "UPDATE render_jobs SET status=?2,current_frame=NULL,finished_at=?3,error=?4,cpu_usage=0,memory_bytes=0,performance_updated_at=?5 WHERE id=?1",
            params![job_id, status, finished_at, job_error, timestamp],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    for path in temp_outputs {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn mark_claim_skipped(conn: &Connection, job_id: &str, claim: &ClaimedFrame) -> Result<(), String> {
    conn.execute(
        "UPDATE render_frames SET status='skipped',error=NULL,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?4 WHERE job_id=?1 AND frame=?2 AND claim_token=?3",
        params![job_id, claim.frame, claim.claim_token, now()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn atomic_replace_output(temp_path: &Path, output_path: &Path) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
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
        let to: Vec<u16> = output_path
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
            .map_err(|error| format!("提交渲染输出失败: {error}"))?;
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        fs::rename(temp_path, output_path).map_err(|error| format!("提交渲染输出失败: {error}"))
    }
}

fn complete_frame_claim(
    conn: &mut Connection,
    job_id: &str,
    claim: &ClaimedFrame,
    attempt: i64,
    render_duration_ms: i64,
    wall_duration_ms: i64,
    expected_dimensions: Option<(u32, u32)>,
) -> Result<bool, String> {
    if !valid_output_with_dimensions(Path::new(&claim.temp_output_path), expected_dimensions) {
        return Err("Blender 未生成有效输出文件".into());
    }
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let changed = transaction
        .execute(
            "UPDATE render_frames SET status='committing',updated_at=?4 WHERE job_id=?1 AND frame=?2 AND claim_token=?3 AND status='running'",
            params![job_id, claim.frame, claim.claim_token, now()],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    if changed == 0 {
        let _ = fs::remove_file(&claim.temp_output_path);
        return Ok(false);
    }
    if let Err(error) = atomic_replace_output(
        Path::new(&claim.temp_output_path),
        Path::new(&claim.output_path),
    ) {
        conn.execute(
            "UPDATE render_frames SET status='running',updated_at=?4 WHERE job_id=?1 AND frame=?2 AND claim_token=?3 AND status='committing'",
            params![job_id, claim.frame, claim.claim_token, now()],
        )
        .map_err(|db_error| db_error.to_string())?;
        return Err(error);
    }
    let size = fs::metadata(&claim.output_path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or(0);
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let committed = transaction
        .execute(
            "UPDATE render_frames SET status='completed',attempts=?4,duration_ms=?5,render_duration_ms=?6,error=NULL,force_render=0,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?7 WHERE job_id=?1 AND frame=?2 AND claim_token=?3 AND status='committing'",
            params![job_id, claim.frame, claim.claim_token, attempt, wall_duration_ms, render_duration_ms, now()],
        )
        .map_err(|error| error.to_string())?;
    if committed == 1 {
        transaction
            .execute(
                "UPDATE render_attempts SET status='completed',finished_at=?4,exit_code=0,render_duration_ms=?5 WHERE job_id=?1 AND frame=?2 AND claim_token=?3",
                params![job_id, claim.frame, claim.claim_token, now(), render_duration_ms],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO render_artifacts(job_id,frame,path,size_bytes,created_at) VALUES(?1,?2,?3,?4,?5)",
                params![job_id, claim.frame, claim.output_path, size, now()],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(committed == 1)
}

fn fail_frame_claim(
    conn: &Connection,
    job_id: &str,
    claim: &ClaimedFrame,
    attempt: i64,
    max_retries: i64,
    wall_duration_ms: i64,
    render_duration_ms: Option<i64>,
    error: &str,
) -> Result<bool, String> {
    let _ = fs::remove_file(&claim.temp_output_path);
    let final_failure = attempt > max_retries;
    conn.execute(
        "UPDATE render_frames SET status=?4,attempts=?5,duration_ms=?6,render_duration_ms=?7,error=?8,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?9 WHERE job_id=?1 AND frame=?2 AND claim_token=?3",
        params![job_id, claim.frame, claim.claim_token, if final_failure { "failed" } else { "pending" }, attempt, wall_duration_ms, render_duration_ms, error, now()],
    )
    .map_err(|db_error| db_error.to_string())?;
    conn.execute(
        "UPDATE render_attempts SET status='failed',finished_at=?4,exit_code=-1,error=?5,render_duration_ms=?6 WHERE job_id=?1 AND frame=?2 AND claim_token=?3",
        params![job_id, claim.frame, claim.claim_token, now(), error, render_duration_ms],
    )
    .map_err(|db_error| db_error.to_string())?;
    Ok(final_failure)
}

fn refresh_current_frame(conn: &Connection, job_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE render_jobs SET current_frame=(SELECT MIN(frame) FROM render_frames WHERE job_id=?1 AND status='running') WHERE id=?1",
        params![job_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn run_job_worker(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    spec: &JobExecutionSpec,
    bootstrap: &Path,
    persistent_script: &Path,
    job_dir: &Path,
    ordinal: i64,
    control: Arc<Mutex<JobControl>>,
) -> Result<(), String> {
    if spec.execution_mode == "persistent"
        && !wait_for_progressive_worker_admission(&control, ordinal).await
    {
        return Ok(());
    }
    let Some(mut slot) = acquire_process_slot(app, &control).await else {
        return Ok(());
    };
    let worker_id = format!(
        "{}-{}-{}",
        &job_id[..job_id.len().min(8)],
        ordinal + 1,
        &Uuid::new_v4().to_string()[..8]
    );
    if spec.execution_mode == "isolated" {
        return run_isolated_worker(
            app,
            project_path,
            job_id,
            spec,
            bootstrap,
            job_dir,
            &worker_id,
            ordinal,
            &mut slot,
            control,
        )
        .await;
    }
    let mut startup_failures = 0_i64;
    loop {
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            return Ok(());
        }
        if worker_exceeds_runtime_limit(&control, ordinal) {
            return Ok(());
        }
        verify_job_source(app, project_path, job_id, &spec.blend_path, &control)?;
        let outcome = match run_persistent_worker_process(
            app,
            project_path,
            job_id,
            spec,
            persistent_script,
            job_dir,
            &worker_id,
            ordinal,
            &mut slot,
            control.clone(),
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                control.lock().unwrap().no_more_work = true;
                return Err(error);
            }
        };
        match outcome {
            PersistentWorkerOutcome::Complete => return Ok(()),
            PersistentWorkerOutcome::Restart => startup_failures = 0,
            PersistentWorkerOutcome::StartupFailed(error) => {
                startup_failures += 1;
                if startup_failures > 2 {
                    return Err(format!("Blender Worker 连续启动失败: {error}"));
                }
                tokio::time::sleep(Duration::from_secs(if startup_failures == 1 {
                    5
                } else {
                    15
                }))
                .await;
            }
        }
    }
}

async fn run_isolated_worker(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    spec: &JobExecutionSpec,
    bootstrap: &Path,
    job_dir: &Path,
    worker_id: &str,
    ordinal: i64,
    slot: &mut ProcessSlotPermit,
    control: Arc<Mutex<JobControl>>,
) -> Result<(), String> {
    let mut conn = open_db(project_path)?;
    loop {
        let (cancel, pause, attention) = {
            let value = control.lock().unwrap();
            (value.cancel, value.pause, value.attention)
        };
        if cancel || pause || attention {
            return Ok(());
        }
        if slot.retire_if_over_limit(app) {
            return Ok(());
        }
        verify_job_source(app, project_path, job_id, &spec.blend_path, &control)?;
        let Some(claim) = claim_next_frame(&mut conn, job_id, spec.max_retries, worker_id)? else {
            return Ok(());
        };
        refresh_current_frame(&conn, job_id)?;
        if !spec.force_overwrite
            && !claim.force_render
            && valid_output(Path::new(&claim.output_path))
        {
            mark_claim_skipped(&conn, job_id, &claim)?;
            refresh_current_frame(&conn, job_id)?;
            emit_progress(app, project_path, job_id, Some(claim.frame), "skipped");
            continue;
        }
        if claim.attempts > 0 {
            tokio::time::sleep(Duration::from_secs(if claim.attempts == 1 {
                5
            } else {
                15
            }))
            .await;
        }
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            release_frame_claim(&conn, job_id, &claim)?;
            refresh_current_frame(&conn, job_id)?;
            return Ok(());
        }
        let started = now();
        let attempt = begin_frame_attempt(&conn, job_id, &claim)?;
        emit_progress(app, project_path, job_id, Some(claim.frame), "rendering");
        publish_worker_state(
            app,
            project_path,
            job_id,
            &control,
            worker_state(
                worker_id,
                ordinal,
                None,
                "rendering",
                Some(claim.frame),
                None,
                None,
            ),
        );
        let mut frame_spec = spec.spec.clone();
        frame_spec["frame"] = json!(claim.frame);
        frame_spec["outputPath"] = json!(claim.temp_output_path);
        frame_spec["sceneName"] = json!(spec.scene_name);
        let frame_spec_path =
            job_dir.join(format!("frame-{}-{}.json", claim.frame, claim.claim_token));
        fs::write(
            &frame_spec_path,
            serde_json::to_vec_pretty(&frame_spec).unwrap(),
        )
        .map_err(|error| error.to_string())?;
        let result = execute_frame(
            app,
            project_path,
            job_id,
            claim.frame,
            &spec.blender_path,
            &spec.blend_path,
            bootstrap,
            &frame_spec_path,
            job_dir,
            control.clone(),
        )
        .await;
        let duration = now() - started;
        let _ = fs::remove_file(frame_spec_path);
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            release_frame_claim(&conn, job_id, &claim)?;
            refresh_current_frame(&conn, job_id)?;
            return Ok(());
        }
        match result {
            Ok(render_duration_ms) => {
                match complete_frame_claim(
                    &mut conn,
                    job_id,
                    &claim,
                    attempt,
                    render_duration_ms,
                    duration,
                    expected_output_dimensions(spec),
                ) {
                    Ok(true) => emit_progress(
                        app,
                        project_path,
                        job_id,
                        Some(claim.frame),
                        "completed-frame",
                    ),
                    Ok(false) => {
                        emit_progress(app, project_path, job_id, Some(claim.frame), "stale-result")
                    }
                    Err(error) => {
                        let final_failure = fail_frame_claim(
                            &conn,
                            job_id,
                            &claim,
                            attempt,
                            spec.max_retries,
                            duration,
                            Some(render_duration_ms),
                            &error,
                        )?;
                        emit_progress(
                            app,
                            project_path,
                            job_id,
                            Some(claim.frame),
                            if final_failure {
                                "failed-frame"
                            } else {
                                "retrying"
                            },
                        );
                    }
                }
            }
            result => {
                let error = result
                    .err()
                    .unwrap_or_else(|| "Blender 未生成有效输出文件".into());
                let final_failure = fail_frame_claim(
                    &conn,
                    job_id,
                    &claim,
                    attempt,
                    spec.max_retries,
                    duration,
                    None,
                    &error,
                )?;
                emit_progress(
                    app,
                    project_path,
                    job_id,
                    Some(claim.frame),
                    if final_failure {
                        "failed-frame"
                    } else {
                        "retrying"
                    },
                );
            }
        }
        publish_worker_state(
            app,
            project_path,
            job_id,
            &control,
            worker_state(worker_id, ordinal, None, "ready", None, None, None),
        );
        refresh_current_frame(&conn, job_id)?;
    }
}

enum PersistentWorkerOutcome {
    Complete,
    Restart,
    StartupFailed(String),
}

fn is_worker_process_crash(error: &str) -> bool {
    error.starts_with("Blender Worker 异常退出")
}

fn append_rotating_render_log(path: &Path, prefix: &str, line: &str) {
    const LIMIT: u64 = 10 * 1024 * 1024;
    if fs::metadata(path)
        .map(|value| value.len() >= LIMIT)
        .unwrap_or(false)
    {
        let third = path.with_extension("log.3");
        let second = path.with_extension("log.2");
        let first = path.with_extension("log.1");
        let _ = fs::remove_file(&third);
        let _ = fs::rename(&second, &third);
        let _ = fs::rename(&first, &second);
        let _ = fs::rename(path, &first);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{prefix}] {line}");
    }
}

async fn run_persistent_worker_process(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    spec: &JobExecutionSpec,
    worker_script: &Path,
    job_dir: &Path,
    worker_id: &str,
    ordinal: i64,
    slot: &mut ProcessSlotPermit,
    control: Arc<Mutex<JobControl>>,
) -> Result<PersistentWorkerOutcome, String> {
    let startup_started = Instant::now();
    publish_worker_state(
        app,
        project_path,
        job_id,
        &control,
        worker_state(worker_id, ordinal, None, "starting", None, None, None),
    );
    let mut child = match tokio_command(&spec.blender_path)
        .arg("-b")
        .arg(&spec.blend_path)
        .arg("--python")
        .arg(worker_script)
        .arg("--")
        .arg("--worker-id")
        .arg(worker_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Ok(PersistentWorkerOutcome::StartupFailed(format!(
                "启动 Blender 失败: {error}"
            )))
        }
    };
    let pid = child.id();
    if let Some(pid) = pid {
        control.lock().unwrap().pids.insert(pid);
    }
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法连接 Blender Worker 标准输入".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法连接 Blender Worker 标准输出".to_string())?;
    let stderr = child.stderr.take();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let log_path = job_dir.join("render.log");
    let stdout_log = log_path.clone();
    let worker_label = format!("Worker {}", ordinal + 1);
    let worker_label_out = worker_label.clone();
    let app_out = app.clone();
    let project_out = project_path.to_string();
    let job_out = job_id.to_string();
    let stdout_task = tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            append_rotating_render_log(&stdout_log, &worker_label_out, &line);
            let structured = line
                .strip_prefix(EVENT_PREFIX)
                .and_then(|value| serde_json::from_str::<Value>(value).ok());
            if let Some(event) = structured.clone() {
                let _ = event_tx.send(event);
            }
            let _ = app_out.emit(
                "pm-center:render-log",
                json!({"projectPath":project_out,"jobId":job_out,"line":line,"event":structured}),
            );
        }
    });
    let stderr_task = stderr.map(|stderr| {
        let stderr_log = log_path.clone();
        let worker_label = worker_label.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                append_rotating_render_log(&stderr_log, &worker_label, &format!("[stderr] {line}"));
            }
        })
    });
    let mut sampler = RenderProcessSampler::new();
    let mut next_sample = Instant::now() + Duration::from_secs(1);
    let startup_ms = 'startup: loop {
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            terminate_child_process_tree(&mut child, pid).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            publish_worker_state(
                app,
                project_path,
                job_id,
                &control,
                worker_state(worker_id, ordinal, pid, "stopped", None, None, None),
            );
            return Ok(PersistentWorkerOutcome::Complete);
        }
        if worker_exceeds_runtime_limit(&control, ordinal) {
            let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n").await;
            let _ = stdin.flush().await;
            wait_or_terminate_child(&mut child, pid, Duration::from_secs(5)).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            publish_worker_state(
                app,
                project_path,
                job_id,
                &control,
                worker_state(
                    worker_id,
                    ordinal,
                    None,
                    "stopped",
                    None,
                    None,
                    Some("检测到 Blender 进程崩溃，当前运行已自动降为单 Worker".into()),
                ),
            );
            return Ok(PersistentWorkerOutcome::Complete);
        }
        if slot.retire_if_over_limit(app) {
            terminate_child_process_tree(&mut child, pid).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            publish_worker_state(
                app,
                project_path,
                job_id,
                &control,
                worker_state(worker_id, ordinal, pid, "stopped", None, None, None),
            );
            return Ok(PersistentWorkerOutcome::Complete);
        }
        if startup_started.elapsed() >= Duration::from_secs(600) {
            terminate_child_process_tree(&mut child, pid).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            return Ok(PersistentWorkerOutcome::StartupFailed(
                "加载项目超过 10 分钟".into(),
            ));
        }
        while let Ok(event) = event_rx.try_recv() {
            if event.get("type").and_then(Value::as_str) == Some("worker-ready") {
                break 'startup startup_started.elapsed().as_millis().min(i64::MAX as u128) as i64;
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            return Ok(PersistentWorkerOutcome::StartupFailed(format!(
                "Blender 提前退出，退出码 {}",
                status.code().unwrap_or(-1)
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let startup_ms = if startup_ms == 0 {
        startup_started.elapsed().as_millis().min(i64::MAX as u128) as i64
    } else {
        startup_ms
    };
    open_db(project_path)?
        .execute(
            "UPDATE render_jobs SET status='running' WHERE id=?1 AND status='starting'",
            params![job_id],
        )
        .map_err(|error| error.to_string())?;
    publish_worker_state(
        app,
        project_path,
        job_id,
        &control,
        worker_state(
            worker_id,
            ordinal,
            pid,
            "ready",
            None,
            Some(startup_ms),
            None,
        ),
    );

    let mut conn = open_db(project_path)?;
    loop {
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            terminate_child_process_tree(&mut child, pid).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            publish_worker_state(
                app,
                project_path,
                job_id,
                &control,
                worker_state(
                    worker_id,
                    ordinal,
                    pid,
                    "stopped",
                    None,
                    Some(startup_ms),
                    None,
                ),
            );
            return Ok(PersistentWorkerOutcome::Complete);
        }
        if worker_exceeds_runtime_limit(&control, ordinal) {
            let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n").await;
            let _ = stdin.flush().await;
            wait_or_terminate_child(&mut child, pid, Duration::from_secs(5)).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            publish_worker_state(
                app,
                project_path,
                job_id,
                &control,
                worker_state(
                    worker_id,
                    ordinal,
                    None,
                    "stopped",
                    None,
                    Some(startup_ms),
                    Some("检测到 Blender 进程崩溃，当前运行已自动降为单 Worker".into()),
                ),
            );
            return Ok(PersistentWorkerOutcome::Complete);
        }
        if slot.retire_if_over_limit(app) {
            let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n").await;
            let _ = stdin.flush().await;
            wait_or_terminate_child(&mut child, pid, Duration::from_secs(5)).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            publish_worker_state(
                app,
                project_path,
                job_id,
                &control,
                worker_state(
                    worker_id,
                    ordinal,
                    pid,
                    "stopped",
                    None,
                    Some(startup_ms),
                    None,
                ),
            );
            return Ok(PersistentWorkerOutcome::Complete);
        }
        let Some(claim) = claim_next_frame(&mut conn, job_id, spec.max_retries, worker_id)? else {
            control.lock().unwrap().no_more_work = true;
            let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n").await;
            let _ = stdin.flush().await;
            wait_or_terminate_child(&mut child, pid, Duration::from_secs(5)).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            publish_worker_state(
                app,
                project_path,
                job_id,
                &control,
                worker_state(
                    worker_id,
                    ordinal,
                    pid,
                    "stopped",
                    None,
                    Some(startup_ms),
                    None,
                ),
            );
            let _ = stdout_task.await;
            if let Some(task) = stderr_task {
                let _ = task.await;
            }
            return Ok(PersistentWorkerOutcome::Complete);
        };
        refresh_current_frame(&conn, job_id)?;
        if !spec.force_overwrite
            && !claim.force_render
            && valid_output(Path::new(&claim.output_path))
        {
            mark_claim_skipped(&conn, job_id, &claim)?;
            refresh_current_frame(&conn, job_id)?;
            emit_progress(app, project_path, job_id, Some(claim.frame), "skipped");
            continue;
        }
        if claim.attempts > 0 {
            tokio::time::sleep(Duration::from_secs(if claim.attempts == 1 {
                5
            } else {
                15
            }))
            .await;
        }
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            release_frame_claim(&conn, job_id, &claim)?;
            terminate_child_process_tree(&mut child, pid).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            return Ok(PersistentWorkerOutcome::Complete);
        }
        let attempt = begin_frame_attempt(&conn, job_id, &claim)?;
        let wall_started = now();
        let mut command = spec.spec.clone();
        command["type"] = json!("render");
        command["frame"] = json!(claim.frame);
        command["sceneName"] = json!(spec.scene_name);
        command["claimToken"] = json!(claim.claim_token);
        command["tempOutputPath"] = json!(claim.temp_output_path);
        let mut encoded = serde_json::to_vec(&command).map_err(|error| error.to_string())?;
        encoded.push(b'\n');
        let write_result = async {
            stdin.write_all(&encoded).await?;
            stdin.flush().await
        }
        .await;
        if let Err(error) = write_result {
            let message = format!("向 Blender Worker 发送任务失败: {error}");
            fail_frame_claim(
                &conn,
                job_id,
                &claim,
                attempt,
                spec.max_retries,
                now() - wall_started,
                None,
                &message,
            )?;
            terminate_child_process_tree(&mut child, pid).await;
            cleanup_worker_process(app, project_path, job_id, &control, pid);
            return Ok(PersistentWorkerOutcome::Restart);
        }
        publish_worker_state(
            app,
            project_path,
            job_id,
            &control,
            worker_state(
                worker_id,
                ordinal,
                pid,
                "rendering",
                Some(claim.frame),
                Some(startup_ms),
                None,
            ),
        );
        emit_progress(app, project_path, job_id, Some(claim.frame), "rendering");
        let frame_result: Result<i64, String> = 'frame_wait: loop {
            let interrupted = {
                let value = control.lock().unwrap();
                value.cancel || value.pause || value.attention
            };
            if interrupted {
                terminate_child_process_tree(&mut child, pid).await;
                release_frame_claim(&conn, job_id, &claim)?;
                cleanup_worker_process(app, project_path, job_id, &control, pid);
                return Ok(PersistentWorkerOutcome::Complete);
            }
            while let Ok(event) = event_rx.try_recv() {
                if event.get("claimToken").and_then(Value::as_str)
                    != Some(claim.claim_token.as_str())
                {
                    continue;
                }
                match event.get("type").and_then(Value::as_str) {
                    Some("frame-completed") => {
                        break 'frame_wait Ok(event
                            .get("renderDurationMs")
                            .and_then(Value::as_i64)
                            .unwrap_or_else(|| now() - wall_started))
                    }
                    Some("frame-failed") => {
                        break 'frame_wait Err(event
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("Blender 渲染失败")
                            .to_string())
                    }
                    _ => {}
                }
            }
            if let Ok(Some(status)) = child.try_wait() {
                break 'frame_wait Err(format!(
                    "Blender Worker 异常退出，退出码 {}",
                    status.code().unwrap_or(-1)
                ));
            }
            if Instant::now() >= next_sample {
                if let Some(pid) = pid {
                    if let Some(metrics) = sampler.sample(pid) {
                        let aggregate = update_worker_metrics(&control, pid, Some(metrics));
                        record_render_performance(
                            app,
                            project_path,
                            job_id,
                            claim.frame,
                            aggregate,
                        );
                    }
                }
                next_sample = Instant::now() + Duration::from_secs(2);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        match frame_result {
            Ok(render_duration_ms) => {
                match complete_frame_claim(
                    &mut conn,
                    job_id,
                    &claim,
                    attempt,
                    render_duration_ms,
                    now() - wall_started,
                    expected_output_dimensions(spec),
                ) {
                    Ok(true) => {
                        record_worker_frame_completion(&control);
                        emit_progress(
                            app,
                            project_path,
                            job_id,
                            Some(claim.frame),
                            "completed-frame",
                        );
                        publish_worker_state(
                            app,
                            project_path,
                            job_id,
                            &control,
                            worker_state(
                                worker_id,
                                ordinal,
                                pid,
                                "ready",
                                None,
                                Some(startup_ms),
                                None,
                            ),
                        );
                        refresh_current_frame(&conn, job_id)?;
                    }
                    Ok(false) => {
                        emit_progress(app, project_path, job_id, Some(claim.frame), "stale-result");
                        refresh_current_frame(&conn, job_id)?;
                    }
                    Err(error) => {
                        let final_failure = fail_frame_claim(
                            &conn,
                            job_id,
                            &claim,
                            attempt,
                            spec.max_retries,
                            now() - wall_started,
                            Some(render_duration_ms),
                            &error,
                        )?;
                        emit_progress(
                            app,
                            project_path,
                            job_id,
                            Some(claim.frame),
                            if final_failure {
                                "failed-frame"
                            } else {
                                "retrying"
                            },
                        );
                        terminate_child_process_tree(&mut child, pid).await;
                        cleanup_worker_process(app, project_path, job_id, &control, pid);
                        publish_worker_state(
                            app,
                            project_path,
                            job_id,
                            &control,
                            worker_state(
                                worker_id,
                                ordinal,
                                pid,
                                "failed",
                                None,
                                Some(startup_ms),
                                Some(error),
                            ),
                        );
                        return Ok(PersistentWorkerOutcome::Restart);
                    }
                }
            }
            Err(error) => {
                if spec.parallelism > 1 && is_worker_process_crash(&error) {
                    let activated = activate_single_worker_fallback(&control);
                    if activated || worker_exceeds_runtime_limit(&control, ordinal) {
                        release_frame_claim(&conn, job_id, &claim)?;
                        refresh_current_frame(&conn, job_id)?;
                        let warning = "检测到多个 Blender 进程发生驱动级崩溃，当前运行已自动降为单 Worker；此帧已重新排队且不计失败次数";
                        if activated {
                            conn.execute(
                                "UPDATE render_jobs SET error=?2 WHERE id=?1",
                                params![job_id, warning],
                            )
                            .map_err(|db_error| db_error.to_string())?;
                            emit_progress(
                                app,
                                project_path,
                                job_id,
                                Some(claim.frame),
                                "worker-auto-downgrade",
                            );
                        }
                        terminate_child_process_tree(&mut child, pid).await;
                        cleanup_worker_process(app, project_path, job_id, &control, pid);
                        publish_worker_state(
                            app,
                            project_path,
                            job_id,
                            &control,
                            worker_state(
                                worker_id,
                                ordinal,
                                None,
                                "stopped",
                                None,
                                Some(startup_ms),
                                Some(warning.into()),
                            ),
                        );
                        return Ok(if ordinal == 0 {
                            PersistentWorkerOutcome::Restart
                        } else {
                            PersistentWorkerOutcome::Complete
                        });
                    }
                }
                let final_failure = fail_frame_claim(
                    &conn,
                    job_id,
                    &claim,
                    attempt,
                    spec.max_retries,
                    now() - wall_started,
                    None,
                    &error,
                )?;
                emit_progress(
                    app,
                    project_path,
                    job_id,
                    Some(claim.frame),
                    if final_failure {
                        "failed-frame"
                    } else {
                        "retrying"
                    },
                );
                terminate_child_process_tree(&mut child, pid).await;
                cleanup_worker_process(app, project_path, job_id, &control, pid);
                publish_worker_state(
                    app,
                    project_path,
                    job_id,
                    &control,
                    worker_state(
                        worker_id,
                        ordinal,
                        pid,
                        "failed",
                        None,
                        Some(startup_ms),
                        Some(error),
                    ),
                );
                return Ok(PersistentWorkerOutcome::Restart);
            }
        }
    }
}

fn cleanup_worker_process(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    control: &Arc<Mutex<JobControl>>,
    pid: Option<u32>,
) {
    if let Some(pid) = pid {
        control.lock().unwrap().pids.remove(&pid);
        let aggregate = update_worker_metrics(control, pid, None);
        record_render_performance(app, project_path, job_id, 0, aggregate);
    }
}

fn fail_job(conn: &Connection, job_id: &str, error: &str) -> Result<(), String> {
    conn.execute("UPDATE render_jobs SET status='failed',current_frame=NULL,finished_at=?2,error=?3 WHERE id=?1", params![job_id,now(),error]).map_err(|e| e.to_string())?;
    Ok(())
}

fn expected_output_dimensions(spec: &JobExecutionSpec) -> Option<(u32, u32)> {
    let width = spec.spec.get("resolutionX")?.as_u64()?;
    let height = spec.spec.get("resolutionY")?.as_u64()?;
    let percentage = spec
        .spec
        .get("resolutionPercentage")
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .clamp(1, 100);
    let width = (width.saturating_mul(percentage) / 100).max(1);
    let height = (height.saturating_mul(percentage) / 100).max(1);
    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
}

fn valid_output(path: &Path) -> bool {
    valid_output_with_dimensions(path, None)
}

fn valid_output_with_dimensions(path: &Path, expected_dimensions: Option<(u32, u32)>) -> bool {
    let metadata_valid = fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 32)
        .unwrap_or(false);
    if !metadata_valid {
        return false;
    }
    let Ok(reader) = image::io::Reader::open(path) else {
        return false;
    };
    let Ok(reader) = reader.with_guessed_format() else {
        return false;
    };
    if let (Some(extension), Some(actual_format)) = (path.extension(), reader.format()) {
        if image::ImageFormat::from_extension(extension)
            .is_some_and(|expected| expected != actual_format)
        {
            return false;
        }
    }
    reader
        .into_dimensions()
        .map(|dimensions| {
            dimensions.0 > 0
                && dimensions.1 > 0
                && expected_dimensions.is_none_or(|expected| expected == dimensions)
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, Default)]
struct RenderProcessMetrics {
    cpu_usage: f64,
    memory_bytes: i64,
}

struct RenderProcessSampler {
    system: System,
}

impl RenderProcessSampler {
    fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    fn sample(&mut self, root_pid: u32) -> Option<RenderProcessMetrics> {
        self.system.refresh_processes();
        let root_pid = Pid::from_u32(root_pid);
        if !self.system.processes().contains_key(&root_pid) {
            return None;
        }

        let mut process_tree = HashSet::from([root_pid]);
        loop {
            let previous_count = process_tree.len();
            for (pid, process) in self.system.processes() {
                if process
                    .parent()
                    .is_some_and(|parent| process_tree.contains(&parent))
                {
                    process_tree.insert(*pid);
                }
            }
            if process_tree.len() == previous_count {
                break;
            }
        }

        let mut raw_cpu = 0.0_f64;
        let mut memory_bytes = 0_u64;
        for pid in process_tree {
            if let Some(process) = self.system.process(pid) {
                raw_cpu += f64::from(process.cpu_usage());
                memory_bytes = memory_bytes.saturating_add(process.memory());
            }
        }
        let logical_cpu_count = self.system.cpus().len().max(1) as f64;
        Some(RenderProcessMetrics {
            cpu_usage: (raw_cpu / logical_cpu_count).clamp(0.0, 100.0),
            memory_bytes: memory_bytes.min(i64::MAX as u64) as i64,
        })
    }
}

fn record_render_performance(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    frame: i64,
    metrics: RenderProcessMetrics,
) {
    let sampled_at = now();
    if let Ok(conn) = open_db(project_path) {
        let _ = conn.execute(
            "UPDATE render_jobs SET cpu_usage=?2,memory_bytes=?3,peak_cpu_usage=MAX(peak_cpu_usage,?2),peak_memory_bytes=MAX(peak_memory_bytes,?3),performance_updated_at=?4 WHERE id=?1",
            params![job_id, metrics.cpu_usage, metrics.memory_bytes, sampled_at],
        );
        if metrics.memory_bytes > 0 {
            let _ = conn.execute(
                "INSERT INTO render_performance_samples(job_id,sampled_at,cpu_usage,memory_bytes) VALUES(?1,?2,?3,?4)",
                params![job_id, sampled_at, metrics.cpu_usage, metrics.memory_bytes],
            );
        }
    }
    let _ = app.emit(
        "pm-center:render-performance",
        json!({
            "projectPath": project_path,
            "jobId": job_id,
            "frame": frame,
            "cpuUsage": metrics.cpu_usage,
            "memoryBytes": metrics.memory_bytes,
            "sampledAt": sampled_at,
        }),
    );
    emit_progress(app, project_path, job_id, Some(frame), "performance");
}

async fn execute_frame(
    app: &tauri::AppHandle,
    project_path: &str,
    job_id: &str,
    frame: i64,
    blender: &str,
    blend_path: &str,
    bootstrap: &Path,
    frame_spec: &Path,
    job_dir: &Path,
    control: Arc<Mutex<JobControl>>,
) -> Result<i64, String> {
    let execution_started = Instant::now();
    let mut child = tokio_command(blender)
        .arg("-b")
        .arg(blend_path)
        .arg("--python")
        .arg(bootstrap)
        .arg("--")
        .arg("--spec")
        .arg(frame_spec)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 Blender 失败: {e}"))?;
    let child_pid = child.id();
    if let Some(pid) = child_pid {
        control.lock().unwrap().pids.insert(pid);
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let log_path = job_dir.join("render.log");
    let app_out = app.clone();
    let project_out = project_path.to_string();
    let job_out = job_id.to_string();
    let log_out = log_path.clone();
    let render_duration = Arc::new(Mutex::new(None::<i64>));
    let render_duration_out = render_duration.clone();
    let stdout_task = tauri::async_runtime::spawn(async move {
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut file) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_out)
                {
                    let _ = writeln!(file, "{line}");
                }
                let structured = line
                    .strip_prefix(EVENT_PREFIX)
                    .and_then(|value| serde_json::from_str::<Value>(value).ok());
                if let Some(duration) = structured
                    .as_ref()
                    .filter(|event| {
                        event.get("type").and_then(Value::as_str) == Some("frame-completed")
                    })
                    .and_then(|event| event.get("durationMs").and_then(Value::as_i64))
                {
                    *render_duration_out.lock().unwrap() = Some(duration);
                }
                let _ = app_out.emit("pm-center:render-log", json!({"projectPath":project_out,"jobId":job_out,"frame":frame,"line":line,"event":structured}));
            }
        }
    });
    let log_err = log_path.clone();
    let stderr_task = tauri::async_runtime::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut file) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_err)
                {
                    let _ = writeln!(file, "[stderr] {line}");
                }
            }
        }
    });
    let mut sampler = RenderProcessSampler::new();
    let mut next_performance_sample = Instant::now() + Duration::from_secs(1);
    let status = loop {
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause || value.attention
        };
        if interrupted {
            if let Some(pid) = child_pid {
                terminate_pid_tree(pid);
            }
            let _ = child.kill().await;
            break child.wait().await.map_err(|e| e.to_string())?;
        }
        if Instant::now() >= next_performance_sample {
            if let Some(pid) = child_pid {
                if let Some(metrics) = sampler.sample(pid) {
                    let aggregate = update_worker_metrics(&control, pid, Some(metrics));
                    record_render_performance(app, project_path, job_id, frame, aggregate);
                }
            }
            next_performance_sample = Instant::now() + Duration::from_secs(2);
        }
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    };
    let aggregate = child_pid
        .map(|pid| update_worker_metrics(&control, pid, None))
        .unwrap_or_default();
    record_render_performance(app, project_path, job_id, frame, aggregate);
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    if status.success() {
        Ok(render_duration.lock().unwrap().unwrap_or_else(|| {
            execution_started
                .elapsed()
                .as_millis()
                .min(i64::MAX as u128) as i64
        }))
    } else {
        Err(format!("Blender 退出码 {}", status.code().unwrap_or(-1)))
    }
}

fn update_worker_metrics(
    control: &Arc<Mutex<JobControl>>,
    pid: u32,
    metrics: Option<RenderProcessMetrics>,
) -> RenderProcessMetrics {
    let mut value = control.lock().unwrap();
    if let Some(metrics) = metrics {
        value.metrics.insert(pid, metrics);
    } else {
        value.metrics.remove(&pid);
        value.pids.remove(&pid);
    }
    RenderProcessMetrics {
        cpu_usage: value.metrics.values().map(|item| item.cpu_usage).sum(),
        memory_bytes: value.metrics.values().map(|item| item.memory_bytes).sum(),
    }
}

fn control_for(project_path: &str, job_id: &str) -> Option<Arc<Mutex<JobControl>>> {
    RUNTIME
        .lock()
        .unwrap()
        .running
        .get(&format!("{}\n{}", project_path, job_id))
        .cloned()
}

fn terminate_pid_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = std_command("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std_command("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .output();
    }
}

async fn terminate_child_process_tree(child: &mut tokio::process::Child, pid: Option<u32>) {
    if let Some(pid) = pid {
        terminate_pid_tree(pid);
    }
    let _ = child.kill().await;
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
}

async fn wait_or_terminate_child(
    child: &mut tokio::process::Child,
    pid: Option<u32>,
    timeout: Duration,
) {
    if tokio::time::timeout(timeout, child.wait()).await.is_err() {
        terminate_child_process_tree(child, pid).await;
    }
}

fn terminate_control_processes(control: &Arc<Mutex<JobControl>>) {
    let pids: Vec<u32> = control.lock().unwrap().pids.iter().copied().collect();
    for pid in pids {
        terminate_pid_tree(pid);
    }
}

fn schedule_interruption_watchdog(
    app: tauri::AppHandle,
    project_path: String,
    job_id: String,
    expected_status: &'static str,
    terminal_status: &'static str,
    reason: &'static str,
) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let Ok(mut conn) = open_db(&project_path) else {
            return;
        };
        let status = conn
            .query_row(
                "SELECT status FROM render_jobs WHERE id=?1",
                params![job_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_default();
        if status != expected_status {
            return;
        }
        if settle_runtime_job(&mut conn, &job_id, terminal_status, reason).is_ok() {
            emit_progress(&app, &project_path, &job_id, None, terminal_status);
            emit_queue(&app, &project_path);
        }
    });
}

#[tauri::command]
pub async fn pause_render_job(
    app_handle: tauri::AppHandle,
    project_path: String,
    job_id: String,
) -> Result<(), String> {
    let mut conn = open_db(&project_path)?;
    if let Some(control) = control_for(&project_path, &job_id) {
        control.lock().unwrap().pause = true;
        terminate_control_processes(&control);
        conn.execute(
            "UPDATE render_jobs SET status='pausing' WHERE id=?1",
            params![job_id],
        )
        .map_err(|e| e.to_string())?;
        schedule_interruption_watchdog(
            app_handle.clone(),
            project_path.clone(),
            job_id.clone(),
            "pausing",
            "paused",
            "用户暂停",
        );
    } else {
        let status = conn
            .query_row(
                "SELECT status FROM render_jobs WHERE id=?1",
                params![job_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| "找不到渲染任务".to_string())?;
        if matches!(
            status.as_str(),
            "pending" | "starting" | "running" | "pausing" | "cancelling"
        ) {
            settle_runtime_job(&mut conn, &job_id, "paused", "用户暂停")?;
        }
    }
    emit_progress(&app_handle, &project_path, &job_id, None, "pause-requested");
    Ok(())
}

#[tauri::command]
pub async fn resume_render_job(
    app_handle: tauri::AppHandle,
    project_path: String,
    job_id: String,
) -> Result<(), String> {
    if control_for(&project_path, &job_id).is_some() {
        return Err("Blender Worker 正在退出，请等待清理完成后再继续".into());
    }
    let conn = open_db(&project_path)?;
    let (status, batch_id, batch_status): (String, String, String) = conn
        .query_row(
            "SELECT j.status,j.batch_id,b.status FROM render_jobs j JOIN render_batches b ON b.id=j.batch_id WHERE j.id=?1 AND j.project_path=?2",
            params![job_id, project_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| "找不到渲染任务".to_string())?;
    if status == "attention" {
        return Err("源 Blender 文件已变化，请先处理后再继续".into());
    }
    if !matches!(status.as_str(), "paused" | "failed" | "cancelled") {
        return Ok(());
    }
    conn.execute(
        "UPDATE render_frames SET status='pending',attempts=0,error=NULL WHERE job_id=?1 AND status='failed'",
        params![job_id],
    )
    .map_err(|e| e.to_string())?;
    if batch_status == "running" {
        conn.execute(
            "UPDATE render_jobs SET status='pending',error=NULL,finished_at=NULL WHERE id=?1",
            params![job_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE render_jobs SET status='paused',error=NULL,finished_at=NULL WHERE id=?1",
            params![job_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE render_batches SET status='queued',updated_at=?2 WHERE id=?1",
            params![batch_id, now()],
        )
        .map_err(|e| e.to_string())?;
    }
    kick_scheduler(app_handle, project_path);
    Ok(())
}

#[tauri::command]
pub async fn cancel_render_job(
    app_handle: tauri::AppHandle,
    project_path: String,
    job_id: String,
) -> Result<(), String> {
    let mut conn = open_db(&project_path)?;
    if let Some(control) = control_for(&project_path, &job_id) {
        control.lock().unwrap().cancel = true;
        terminate_control_processes(&control);
        conn.execute(
            "UPDATE render_jobs SET status='cancelling' WHERE id=?1",
            params![job_id],
        )
        .map_err(|e| e.to_string())?;
        schedule_interruption_watchdog(
            app_handle.clone(),
            project_path.clone(),
            job_id.clone(),
            "cancelling",
            "cancelled",
            "用户取消",
        );
    } else {
        let status = conn
            .query_row(
                "SELECT status FROM render_jobs WHERE id=?1",
                params![job_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| "找不到渲染任务".to_string())?;
        if !matches!(status.as_str(), "completed" | "cancelled") {
            settle_runtime_job(&mut conn, &job_id, "cancelled", "用户取消")?;
        }
    }
    emit_progress(
        &app_handle,
        &project_path,
        &job_id,
        None,
        "cancel-requested",
    );
    Ok(())
}

#[tauri::command]
pub async fn pause_render_queue(
    app_handle: tauri::AppHandle,
    project_path: String,
) -> Result<(), String> {
    let mut conn = open_db(&project_path)?;
    conn.execute(
        "UPDATE render_jobs SET status='paused' WHERE status='pending' AND archived=0",
        [],
    )
    .map_err(|e| e.to_string())?;
    let active_job_ids = {
        let mut statement = conn
            .prepare(
                "SELECT id FROM render_jobs WHERE project_path=?1 AND archived=0 AND status IN ('starting','running','pausing','cancelling')",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![project_path], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for job_id in active_job_ids {
        if let Some(control) = control_for(&project_path, &job_id) {
            control.lock().unwrap().pause = true;
            terminate_control_processes(&control);
            conn.execute(
                "UPDATE render_jobs SET status='pausing' WHERE id=?1",
                params![job_id],
            )
            .map_err(|error| error.to_string())?;
            schedule_interruption_watchdog(
                app_handle.clone(),
                project_path.clone(),
                job_id,
                "pausing",
                "paused",
                "用户暂停队列",
            );
        } else {
            settle_runtime_job(&mut conn, &job_id, "paused", "用户暂停队列")?;
        }
    }
    emit_queue(&app_handle, &project_path);
    Ok(())
}

#[tauri::command]
pub async fn resume_render_queue(
    app_handle: tauri::AppHandle,
    project_path: String,
) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    conn.execute(
        "UPDATE render_jobs SET status='pending',error=NULL,finished_at=NULL WHERE status='paused' AND archived=0 AND batch_id IN (SELECT id FROM render_batches WHERE project_path=?1 AND status='running')",
        params![project_path],
    )
    .map_err(|e| e.to_string())?;
    kick_scheduler(app_handle, project_path);
    Ok(())
}

#[tauri::command]
pub async fn queue_render_frames(
    project_path: String,
    job_id: String,
    frames: Vec<i64>,
    mode: String,
) -> Result<(), String> {
    if frames.is_empty() {
        return Err("请选择要处理的帧".into());
    }
    if !matches!(mode.as_str(), "retry" | "rerender") {
        return Err("帧操作模式必须是 retry 或 rerender".into());
    }
    queue_frames_for_render(&project_path, &job_id, &frames, &mode)
}

#[tauri::command]
pub async fn resolve_render_source_change(
    app_handle: tauri::AppHandle,
    project_path: String,
    job_id: String,
    action: String,
) -> Result<(), String> {
    if !matches!(action.as_str(), "recheck" | "acceptAndRerenderAll") {
        return Err("源文件处理方式无效".into());
    }
    let mut conn = open_db(&project_path)?;
    let (blend_path, stored_hash): (String, Option<String>) = conn
        .query_row(
            "SELECT blend_path,source_hash FROM render_jobs WHERE id=?1",
            params![job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let fingerprint = source_fingerprint(Path::new(&blend_path))?;
    if action == "recheck" {
        if stored_hash.as_deref() != Some(fingerprint.hash.as_str()) {
            return Err("源 Blender 文件仍与任务记录不一致".into());
        }
        conn.execute(
            "UPDATE render_jobs SET status='paused',attention_code=NULL,error=NULL WHERE id=?1 AND status='attention'",
            params![job_id],
        )
        .map_err(|error| error.to_string())?;
    } else {
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        transaction.execute(
            "UPDATE render_frames SET status='pending',attempts=0,error=NULL,duration_ms=NULL,render_duration_ms=NULL,force_render=1,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?2 WHERE job_id=?1",
            params![job_id, now()],
        ).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM render_attempts WHERE job_id=?1",
                params![job_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM render_artifacts WHERE job_id=?1",
                params![job_id],
            )
            .map_err(|error| error.to_string())?;
        transaction.execute(
            "UPDATE render_jobs SET status='paused',source_hash=?2,source_size=?3,source_modified_at=?4,attention_code=NULL,error=NULL,current_frame=NULL,finished_at=NULL WHERE id=?1",
            params![job_id, fingerprint.hash, fingerprint.size, fingerprint.modified_at],
        ).map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
    }
    emit_queue(&app_handle, &project_path);
    emit_progress(&app_handle, &project_path, &job_id, None, "source-resolved");
    Ok(())
}

fn queue_frames_for_render(
    project_path: &str,
    job_id: &str,
    frames: &[i64],
    mode: &str,
) -> Result<(), String> {
    let mut conn = open_db(project_path)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let status: String = transaction
        .query_row(
            "SELECT status FROM render_jobs WHERE id=?1",
            params![job_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if matches!(
        status.as_str(),
        "starting" | "running" | "pausing" | "cancelling"
    ) {
        return Err("任务正在运行，请先暂停后再操作帧".into());
    }
    for frame in frames {
        if mode == "retry" {
            transaction.execute(
                "UPDATE render_frames SET status='pending',attempts=0,error=NULL,force_render=0,render_duration_ms=NULL,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2 AND status='failed'",
                params![job_id, frame, now()],
            ).map_err(|error| error.to_string())?;
        } else {
            transaction.execute(
                "UPDATE render_frames SET status='pending',attempts=0,error=NULL,force_render=1,render_duration_ms=NULL,worker_id=NULL,claim_token=NULL,claimed_at=NULL,temp_output_path=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2",
                params![job_id, frame, now()],
            ).map_err(|error| error.to_string())?;
        }
        transaction
            .execute(
                "DELETE FROM render_attempts WHERE job_id=?1 AND frame=?2",
                params![job_id, frame],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.execute(
        "UPDATE render_jobs SET status='paused',error=NULL,attention_code=NULL,finished_at=NULL WHERE id=?1",
        params![job_id],
    ).map_err(|error| error.to_string())?;
    transaction.execute(
        "UPDATE render_batches SET status=CASE WHEN status='running' THEN 'running' ELSE 'queued' END,updated_at=?2 WHERE id=(SELECT batch_id FROM render_jobs WHERE id=?1)",
        params![job_id, now()],
    ).map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn retry_render_frames(
    project_path: String,
    job_id: String,
    frames: Option<Vec<i64>>,
) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    let selected = match frames.filter(|items| !items.is_empty()) {
        Some(frames) => frames,
        None => conn
            .prepare("SELECT frame FROM render_frames WHERE job_id=?1 AND status='failed'")
            .map_err(|error| error.to_string())?
            .query_map(params![job_id], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<i64>, _>>()
            .map_err(|error| error.to_string())?,
    };
    drop(conn);
    if selected.is_empty() {
        return Ok(());
    }
    queue_frames_for_render(&project_path, &job_id, &selected, "rerender")
}

#[tauri::command]
pub async fn skip_render_frames(
    project_path: String,
    job_id: String,
    frames: Vec<i64>,
) -> Result<(), String> {
    if frames.is_empty() {
        return Err("请选择要跳过的帧".into());
    }
    let mut conn = open_db(&project_path)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let status: String = transaction
        .query_row(
            "SELECT status FROM render_jobs WHERE id=?1",
            params![job_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if matches!(
        status.as_str(),
        "starting" | "running" | "pausing" | "cancelling"
    ) {
        return Err("任务正在运行，请先暂停后再跳过帧".into());
    }
    for frame in frames {
        transaction
            .execute(
                "UPDATE render_frames SET status='skipped',error=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2 AND status IN ('pending','failed')",
                params![job_id, frame, now()],
            )
            .map_err(|error| error.to_string())?;
    }
    let (pending_count, failed_count, completed_count, total_count): (i64, i64, i64, i64) = transaction
        .query_row(
            "SELECT SUM(CASE WHEN status='pending' THEN 1 ELSE 0 END),SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END),SUM(CASE WHEN status IN ('completed','skipped') THEN 1 ELSE 0 END),COUNT(*) FROM render_frames WHERE job_id=?1",
            params![job_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|error| error.to_string())?;
    let next_status = if pending_count > 0 {
        "paused"
    } else if failed_count > 0 {
        "failed"
    } else if total_count > 0 && completed_count == total_count {
        "completed"
    } else {
        status.as_str()
    };
    transaction
        .execute(
            "UPDATE render_jobs SET status=?2,current_frame=NULL,error=NULL,finished_at=CASE WHEN ?2='completed' THEN ?3 ELSE finished_at END WHERE id=?1",
            params![job_id, next_status, now()],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn reorder_render_job(
    project_path: String,
    job_id: String,
    before_job_id: Option<String>,
) -> Result<(), String> {
    reorder_render_job_in_db(&project_path, &job_id, before_job_id.as_deref())
}

fn reorder_render_job_in_db(
    project_path: &str,
    job_id: &str,
    before_job_id: Option<&str>,
) -> Result<(), String> {
    if before_job_id == Some(job_id) {
        return Ok(());
    }
    let mut conn = open_db(project_path)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let (source_batch_id, source_status): (String, String) = transaction
        .query_row(
            "SELECT j.batch_id,j.status FROM render_jobs j WHERE j.id=?1 AND j.project_path=?2",
            params![job_id, project_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "找不到要排序的渲染任务".to_string())?;
    if matches!(
        source_status.as_str(),
        "starting" | "running" | "pausing" | "cancelling"
    ) {
        return Err("正在运行的任务不可排序".into());
    }

    let target = before_job_id
        .as_deref()
        .filter(|id| *id != job_id)
        .map(|target_job_id| {
            transaction
                .query_row(
                    "SELECT j.batch_id,j.status FROM render_jobs j WHERE j.id=?1 AND j.project_path=?2",
                    params![target_job_id, project_path],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .map_err(|_| "找不到目标渲染任务".to_string())
        })
        .transpose()?;

    if let Some((target_batch_id, target_status)) = target.as_ref() {
        if source_batch_id != *target_batch_id {
            return Err("任务只能在所属批次内排序，请拖动批次标题调整批次顺序".into());
        }
        if matches!(
            target_status.as_str(),
            "starting" | "running" | "pausing" | "cancelling"
        ) {
            return Err("不能把任务插入正在运行的任务位置".into());
        }
    }
    let mut jobs = transaction
        .prepare("SELECT id FROM render_jobs WHERE batch_id=?1 ORDER BY position ASC, created_at ASC, id ASC")
        .map_err(|error| error.to_string())?
        .query_map(params![source_batch_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    jobs.retain(|id| id != job_id);
    let insert_at = before_job_id
        .and_then(|target_id| jobs.iter().position(|id| id == target_id))
        .unwrap_or(jobs.len());
    jobs.insert(insert_at, job_id.to_string());
    for (position, id) in jobs.iter().enumerate() {
        transaction
            .execute(
                "UPDATE render_jobs SET position=?2 WHERE id=?1",
                params![id, position as i64],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn reorder_render_batch(
    project_path: String,
    batch_id: String,
    before_batch_id: Option<String>,
) -> Result<(), String> {
    reorder_render_batch_in_db(&project_path, &batch_id, before_batch_id.as_deref())
}

fn reorder_render_batch_in_db(
    project_path: &str,
    batch_id: &str,
    before_batch_id: Option<&str>,
) -> Result<(), String> {
    if before_batch_id == Some(batch_id) {
        return Ok(());
    }
    let mut conn = open_db(project_path)?;
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let source_status: String = transaction
        .query_row(
            "SELECT status FROM render_batches WHERE id=?1 AND project_path=?2",
            params![batch_id, project_path],
            |row| row.get(0),
        )
        .map_err(|_| "找不到要排序的批次".to_string())?;
    if source_status != "queued" {
        return Err("只有等待中的批次可以排序".into());
    }
    let target_id = before_batch_id.filter(|id| *id != batch_id);
    if let Some(target_id) = target_id {
        let target_status: String = transaction
            .query_row(
                "SELECT status FROM render_batches WHERE id=?1 AND project_path=?2",
                params![target_id, project_path],
                |row| row.get(0),
            )
            .map_err(|_| "找不到目标批次".to_string())?;
        if target_status != "queued" {
            return Err("不能把等待批次插入正在运行或已完成批次的位置".into());
        }
    }
    let mut batches = transaction
        .prepare("SELECT id FROM render_batches WHERE project_path=?1 ORDER BY position ASC,created_at ASC,id ASC")
        .map_err(|error| error.to_string())?
        .query_map(params![project_path], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    batches.retain(|id| id != batch_id);
    let insert_at = target_id
        .and_then(|target| batches.iter().position(|id| id == target))
        .unwrap_or(batches.len());
    batches.insert(insert_at, batch_id.to_string());
    for (position, id) in batches.iter().enumerate() {
        transaction
            .execute(
                "UPDATE render_batches SET position=?2,updated_at=?3 WHERE id=?1",
                params![id, position as i64, now()],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn archive_render_job(
    project_path: String,
    job_id: String,
    archived: bool,
) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    conn.execute("UPDATE render_jobs SET archived=?2 WHERE id=?1 AND status NOT IN ('running','pausing','cancelling')", params![job_id,archived as i64]).map_err(|e|e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn list_render_presets(
    app_handle: tauri::AppHandle,
    project_path: String,
) -> Result<Vec<RenderPreset>, String> {
    let conn = open_db(&project_path)?;
    let mut stmt = conn.prepare("SELECT id,name,scope,settings_json,created_at,updated_at FROM render_presets ORDER BY scope,name").map_err(|e|e.to_string())?;
    let presets = stmt
        .query_map([], |row| {
            let raw: String = row.get(3)?;
            Ok(RenderPreset {
                id: row.get(0)?,
                name: row.get(1)?,
                scope: row.get(2)?,
                settings: serde_json::from_str(&raw).unwrap_or(Value::Null),
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut presets = presets;
    presets.extend(load_global_presets(&app_handle)?);
    presets.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then(left.name.cmp(&right.name))
    });
    Ok(presets)
}

fn global_presets_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join("render-presets.json"))
}

fn load_global_presets(app_handle: &tauri::AppHandle) -> Result<Vec<RenderPreset>, String> {
    let path = global_presets_path(app_handle)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&data).map_err(|error| format!("读取全局渲染预设失败: {error}"))
}

fn save_global_presets(
    app_handle: &tauri::AppHandle,
    presets: &[RenderPreset],
) -> Result<(), String> {
    let path = global_presets_path(app_handle)?;
    let temp = path.with_extension("json.tmp");
    fs::write(
        &temp,
        serde_json::to_vec_pretty(presets).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temp, path).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_render_preset(
    app_handle: tauri::AppHandle,
    project_path: String,
    id: Option<String>,
    name: String,
    scope: String,
    settings: Value,
) -> Result<RenderPreset, String> {
    let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    if scope == "global" {
        let mut presets = load_global_presets(&app_handle)?;
        let created_at = presets
            .iter()
            .find(|preset| preset.id == id)
            .map(|preset| preset.created_at)
            .unwrap_or(timestamp);
        let preset = RenderPreset {
            id: id.clone(),
            name,
            scope,
            settings,
            created_at,
            updated_at: timestamp,
        };
        presets.retain(|item| item.id != id);
        presets.push(preset.clone());
        save_global_presets(&app_handle, &presets)?;
        return Ok(preset);
    }
    let conn = open_db(&project_path)?;
    conn.execute("INSERT INTO render_presets(id,name,scope,settings_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,scope=excluded.scope,settings_json=excluded.settings_json,updated_at=excluded.updated_at", params![id,name,scope,settings.to_string(),timestamp]).map_err(|e|e.to_string())?;
    Ok(RenderPreset {
        id,
        name,
        scope,
        settings,
        created_at: timestamp,
        updated_at: timestamp,
    })
}

#[tauri::command]
pub async fn delete_render_preset(
    app_handle: tauri::AppHandle,
    project_path: String,
    id: String,
    scope: Option<String>,
) -> Result<(), String> {
    if scope.as_deref() == Some("global") {
        let mut presets = load_global_presets(&app_handle)?;
        presets.retain(|preset| preset.id != id);
        save_global_presets(&app_handle, &presets)?;
    } else {
        open_db(&project_path)?
            .execute("DELETE FROM render_presets WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_render_scheduler_settings(
    app_handle: tauri::AppHandle,
    _project_path: String,
) -> Result<SchedulerSettings, String> {
    Ok(load_scheduler_settings(&app_handle))
}

#[tauri::command]
pub async fn set_render_scheduler_settings(
    app_handle: tauri::AppHandle,
    project_path: String,
    settings: SchedulerSettings,
) -> Result<SchedulerSettings, String> {
    let settings = SchedulerSettings {
        concurrency: settings.concurrency.clamp(1, 8),
        max_blender_processes: settings.max_blender_processes.clamp(1, 16),
    };
    save_scheduler_settings(&app_handle, &settings)?;
    register_runtime_project(&project_path);
    PROCESS_BUDGET_NOTIFY.notify_waiters();
    kick_all_schedulers(app_handle);
    Ok(settings)
}

fn scheduler_settings_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join("render-scheduler.json"))
}

fn load_scheduler_settings(app_handle: &tauri::AppHandle) -> SchedulerSettings {
    scheduler_settings_path(app_handle)
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|data| serde_json::from_slice::<SchedulerSettings>(&data).ok())
        .map(|settings| SchedulerSettings {
            concurrency: settings.concurrency.clamp(1, 8),
            max_blender_processes: settings.max_blender_processes.clamp(1, 16),
        })
        .unwrap_or(SchedulerSettings {
            concurrency: 1,
            max_blender_processes: default_max_blender_processes(),
        })
}

fn save_scheduler_settings(
    app_handle: &tauri::AppHandle,
    settings: &SchedulerSettings,
) -> Result<(), String> {
    let path = scheduler_settings_path(app_handle)?;
    let temp = path.with_extension("json.tmp");
    fs::write(
        &temp,
        serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temp, path).map_err(|error| error.to_string())
}

#[derive(Debug, Clone)]
struct BatchPackageJob {
    id: String,
    name: String,
    scene_name: String,
    frame_start: i64,
    frame_end: i64,
    frame_step: i64,
    output_dir: PathBuf,
    output_format: String,
    expected_dimensions: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
struct PackageFramePlan {
    frames: Vec<Option<PathBuf>>,
    missing_frames: Vec<i64>,
    dimensions: (u32, u32),
}

#[derive(Debug, Clone, Copy)]
enum VideoPackageFormat {
    Mp4,
    Mov,
    Webm,
}

impl VideoPackageFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mp4" => Ok(Self::Mp4),
            "mov" => Ok(Self::Mov),
            "webm" => Ok(Self::Webm),
            _ => Err("只支持 MP4、MOV 或 WebM 视频打包格式".to_string()),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Webm => "webm",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4",
            Self::Mov => "MOV",
            Self::Webm => "WebM",
        }
    }
}

struct BatchPackageGuard {
    key: String,
}

impl Drop for BatchPackageGuard {
    fn drop(&mut self) {
        BATCH_PACKAGE_RUNNING.lock().unwrap().remove(&self.key);
    }
}

fn acquire_batch_package_guard(
    project_path: &str,
    batch_id: &str,
) -> Result<BatchPackageGuard, String> {
    let key = format!("{}\u{0}{}", project_path.to_ascii_lowercase(), batch_id);
    let mut running = BATCH_PACKAGE_RUNNING.lock().unwrap();
    if !running.insert(key.clone()) {
        return Err("该批次正在打包视频，请等待当前打包结束".to_string());
    }
    Ok(BatchPackageGuard { key })
}

fn collect_batch_package_jobs(
    conn: &Connection,
    project_path: &str,
    batch_id: &str,
) -> Result<(String, Vec<BatchPackageJob>), String> {
    let batch_name = conn
        .query_row(
            "SELECT name FROM render_batches WHERE id=?1 AND project_path=?2",
            params![batch_id, project_path],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| "找不到要打包的渲染批次".to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT id,name,scene_name,frame_start,frame_end,frame_step,output_dir,spec_json FROM render_jobs WHERE batch_id=?1 AND project_path=?2 ORDER BY position ASC,created_at ASC",
        )
        .map_err(|error| error.to_string())?;
    let jobs = statement
        .query_map(params![batch_id, project_path], |row| {
            let spec_json: String = row.get(6)?;
            let spec = serde_json::from_str::<Value>(&spec_json).unwrap_or(Value::Null);
            let output_format = spec
                .get("outputFormat")
                .and_then(Value::as_str)
                .unwrap_or("PNG")
                .to_string();
            let expected_dimensions = package_dimensions_from_spec(&spec);
            Ok(BatchPackageJob {
                id: row.get(0)?,
                name: row.get(1)?,
                scene_name: row.get(2)?,
                frame_start: row.get(3)?,
                frame_end: row.get(4)?,
                frame_step: row.get(5)?,
                output_dir: PathBuf::from(row.get::<_, String>(6)?),
                output_format,
                expected_dimensions,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if jobs.is_empty() {
        return Err("该批次没有可打包的渲染作业".to_string());
    }
    Ok((batch_name, jobs))
}

fn package_dimensions_from_spec(spec: &Value) -> Option<(u32, u32)> {
    let width = spec.get("resolutionX")?.as_u64()?;
    let height = spec.get("resolutionY")?.as_u64()?;
    let percentage = spec
        .get("resolutionPercentage")
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .clamp(1, 100);
    let width = u32::try_from((width.saturating_mul(percentage) / 100).max(1)).ok()?;
    let height = u32::try_from((height.saturating_mul(percentage) / 100).max(1)).ok()?;
    Some((width, height))
}

fn collect_job_package_frames(
    conn: &Connection,
    job: &BatchPackageJob,
) -> Result<PackageFramePlan, String> {
    if job.frame_step <= 0 || job.frame_end < job.frame_start {
        return Err(format!("{} 的帧范围无效", job.name));
    }
    let mut statement = conn
        .prepare("SELECT frame,output_path FROM render_frames WHERE job_id=?1 ORDER BY frame ASC")
        .map_err(|error| error.to_string())?;
    let actual = statement
        .query_map(params![job.id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<HashMap<i64, String>, _>>()
        .map_err(|error| error.to_string())?;

    // Video packaging intentionally uses the files that are actually present on disk.
    // A frame may be rendered by Blender or restored manually without its database state
    // being completed, so render_frames.status is not a reliable source of truth here.
    let mut missing_frames = Vec::new();
    let mut frames = Vec::new();
    for frame in (job.frame_start..=job.frame_end).step_by(job.frame_step as usize) {
        let path = actual.get(&frame).map(PathBuf::from).unwrap_or_else(|| {
            frame_output_path(
                &job.output_dir,
                &job.scene_name,
                frame,
                job.frame_end,
                &job.output_format,
            )
        });
        if valid_output(&path) {
            frames.push(Some(path));
        } else {
            missing_frames.push(frame);
            frames.push(None);
        }
    }

    let dimensions = job
        .expected_dimensions
        .or_else(|| {
            frames
                .iter()
                .flatten()
                .find_map(|path| image::image_dimensions(path).ok())
        })
        // Very old jobs may not have resolution fields. A fully-missing sequence still
        // needs a deterministic, playable result rather than a packaging failure.
        .unwrap_or((1920, 1080));
    Ok(PackageFramePlan {
        frames,
        missing_frames,
        dimensions: package_video_dimensions(dimensions),
    })
}

fn package_video_dimensions(dimensions: (u32, u32)) -> (u32, u32) {
    let even = |value: u32| value.max(2).saturating_add(value % 2);
    (even(dimensions.0), even(dimensions.1))
}

fn create_black_package_frame(path: &Path, dimensions: (u32, u32)) -> Result<(), String> {
    let image = image::RgbImage::from_pixel(dimensions.0, dimensions.1, image::Rgb([0, 0, 0]));
    image
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|error| format!("生成黑色补位帧失败: {error}"))
}

fn normalize_package_frame(
    source: &Path,
    target: &Path,
    dimensions: (u32, u32),
) -> Result<(), String> {
    let source_image = image::open(source)
        .map_err(|error| format!("无法读取打包图像 {}: {error}", source.display()))?;
    let source_dimensions = source_image.dimensions();
    if source_dimensions == dimensions {
        fs::copy(source, target).map_err(|error| format!("准备打包图像失败: {error}"))?;
        return Ok(());
    }
    let scale = (dimensions.0 as f64 / source_dimensions.0 as f64)
        .min(dimensions.1 as f64 / source_dimensions.1 as f64);
    let width = ((source_dimensions.0 as f64 * scale).round() as u32).max(1);
    let height = ((source_dimensions.1 as f64 * scale).round() as u32).max(1);
    let resized = source_image.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    let mut canvas =
        image::RgbaImage::from_pixel(dimensions.0, dimensions.1, image::Rgba([0, 0, 0, 255]));
    let offset_x = i64::from((dimensions.0 - width) / 2);
    let offset_y = i64::from((dimensions.1 - height) / 2);
    image::imageops::overlay(&mut canvas, &resized.to_rgba8(), offset_x, offset_y);
    canvas
        .save_with_format(target, image::ImageFormat::Png)
        .map_err(|error| format!("准备标准尺寸打包图像失败: {error}"))
}

fn prepare_package_frame_paths(
    frames: &[Option<PathBuf>],
    dimensions: (u32, u32),
    work_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(work_dir).map_err(|error| format!("创建视频补位目录失败: {error}"))?;
    let black_path = work_dir.join("black.png");
    create_black_package_frame(&black_path, dimensions)?;
    frames
        .iter()
        .enumerate()
        .map(|(index, source)| match source {
            None => Ok(black_path.clone()),
            Some(source) if image::image_dimensions(source).ok() == Some(dimensions) => {
                Ok(source.clone())
            }
            Some(source) => {
                let target = work_dir.join(format!("normalized-{index:06}.png"));
                normalize_package_frame(source, &target, dimensions)?;
                Ok(target)
            }
        })
        .collect()
}

fn ffconcat_path(path: &Path) -> Result<String, String> {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.contains('\n') || value.contains('\r') {
        return Err(format!("输出路径包含换行，无法打包: {}", path.display()));
    }
    Ok(value.replace('\'', r"\'"))
}

fn write_ffconcat_manifest(paths: &[PathBuf], fps: f64, path: &Path) -> Result<(), String> {
    let duration = 1.0 / fps;
    let mut content = String::from("ffconcat version 1.0\n");
    for frame_path in paths {
        content.push_str("file '");
        content.push_str(&ffconcat_path(frame_path)?);
        content.push_str("'\n");
        content.push_str(&format!("duration {duration:.12}\n"));
    }
    // ffconcat ignores the last duration; repeat the final image so it holds for one frame too.
    content.push_str("file '");
    content.push_str(&ffconcat_path(paths.last().expect("non-empty frame list"))?);
    content.push_str("'\n");
    fs::write(path, content).map_err(|error| format!("写入视频打包清单失败: {error}"))
}

fn package_render_batch_sync(
    project_path: &str,
    batch_id: &str,
    selected_job_id: Option<&str>,
    fps: f64,
    video_format: VideoPackageFormat,
    ffmpeg_path: &str,
) -> Result<RenderBatchPackageResult, String> {
    let conn = open_db(project_path)?;
    let (batch_name, mut jobs) = collect_batch_package_jobs(&conn, project_path, batch_id)?;
    if let Some(selected_job_id) = selected_job_id {
        jobs.retain(|job| job.id == selected_job_id);
        if jobs.is_empty() {
            return Err("找不到要打包的渲染任务".to_string());
        }
    }
    let planned_jobs = jobs
        .iter()
        .map(|job| collect_job_package_frames(&conn, job).map(|frames| (job.clone(), frames)))
        .collect::<Result<Vec<_>, _>>()?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");
    let output_dir = PathBuf::from(project_path).join("renders");
    fs::create_dir_all(&output_dir).map_err(|error| format!("创建视频输出目录失败: {error}"))?;

    let mut outputs = Vec::new();
    for (job, plan) in planned_jobs {
        let short_id = job.id.get(..8).unwrap_or(&job.id);
        let name = format!(
            "{}-{}-{short_id}-{}",
            safe_name(&batch_name),
            safe_name(&job.name),
            timestamp
        );
        let manifest_path = output_dir.join(format!(".{name}.ffconcat"));
        let output_path = output_dir.join(format!("{name}.{}", video_format.extension()));
        let work_dir = output_dir.join(format!(".{name}.frames"));
        let frames = prepare_package_frame_paths(&plan.frames, plan.dimensions, &work_dir)?;
        write_ffconcat_manifest(&frames, fps, &manifest_path)?;

        let mut command = std_command(ffmpeg_path);
        command
            .args([
                "-hide_banner",
                "-nostdin",
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
            ])
            .arg(&manifest_path)
            .args(["-an", "-r", &format!("{fps:.6}")]);
        match video_format {
            VideoPackageFormat::Mp4 | VideoPackageFormat::Mov => {
                command.args([
                    "-c:v",
                    "libx264",
                    "-preset",
                    "medium",
                    "-crf",
                    "18",
                    "-pix_fmt",
                    "yuv420p",
                    "-movflags",
                    "+faststart",
                ]);
            }
            VideoPackageFormat::Webm => {
                command.args([
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    "32",
                    "-b:v",
                    "0",
                    "-pix_fmt",
                    "yuv420p",
                ]);
            }
        }
        let output = command
            .arg(&output_path)
            .output()
            .map_err(|error| format!("启动 ffmpeg 失败: {error}"));
        let _ = fs::remove_file(&manifest_path);
        let _ = fs::remove_dir_all(&work_dir);
        let output = output?;
        if !output.status.success() {
            let _ = fs::remove_file(&output_path);
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!("ffmpeg 未能生成 {} 视频", video_format.label())
            } else {
                format!("ffmpeg 打包 {} 失败: {stderr}", job.name)
            });
        }
        if fs::metadata(&output_path)
            .map(|metadata| !metadata.is_file() || metadata.len() == 0)
            .unwrap_or(true)
        {
            return Err(format!("ffmpeg 未生成有效视频: {}", output_path.display()));
        }
        outputs.push(RenderBatchPackageOutput {
            job_id: job.id,
            job_name: job.name,
            output_path: output_path.to_string_lossy().to_string(),
            missing_frames: plan.missing_frames,
        });
    }
    Ok(RenderBatchPackageResult {
        output_dir: output_dir.to_string_lossy().to_string(),
        outputs,
    })
}

#[tauri::command]
pub async fn package_render_batch(
    project_path: String,
    batch_id: String,
    request: RenderBatchPackageRequest,
) -> Result<RenderBatchPackageResult, String> {
    if !request.fps.is_finite() || !(1.0..=240.0).contains(&request.fps) {
        return Err("帧率必须介于 1 到 240 fps 之间".to_string());
    }
    let video_format = VideoPackageFormat::parse(&request.format)?;
    let ffmpeg_path = resolve_ffmpeg_path(request.ffmpeg_path.as_deref())
        .ok_or_else(|| "未找到 ffmpeg。请在全局设置 > 工具路径中指定 ffmpeg.exe。".to_string())?;
    let _guard = acquire_batch_package_guard(&project_path, &batch_id)?;
    tokio::task::spawn_blocking(move || {
        package_render_batch_sync(
            &project_path,
            &batch_id,
            None,
            request.fps,
            video_format,
            &ffmpeg_path,
        )
    })
    .await
    .map_err(|error| format!("视频打包任务异常结束: {error}"))?
}

#[tauri::command]
pub async fn package_render_job(
    project_path: String,
    job_id: String,
    request: RenderBatchPackageRequest,
) -> Result<RenderBatchPackageResult, String> {
    if !request.fps.is_finite() || !(1.0..=240.0).contains(&request.fps) {
        return Err("帧率必须介于 1 到 240 fps 之间".to_string());
    }
    let video_format = VideoPackageFormat::parse(&request.format)?;
    let ffmpeg_path = resolve_ffmpeg_path(request.ffmpeg_path.as_deref())
        .ok_or_else(|| "未找到 ffmpeg。请在全局设置 > 工具路径中指定 ffmpeg.exe。".to_string())?;
    let batch_id = open_db(&project_path)?
        .query_row(
            "SELECT batch_id FROM render_jobs WHERE id=?1 AND project_path=?2",
            params![job_id, project_path],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| "找不到要打包的渲染任务".to_string())?;
    let _guard = acquire_batch_package_guard(&project_path, &batch_id)?;
    tokio::task::spawn_blocking(move || {
        package_render_batch_sync(
            &project_path,
            &batch_id,
            Some(&job_id),
            request.fps,
            video_format,
            &ffmpeg_path,
        )
    })
    .await
    .map_err(|error| format!("视频打包任务异常结束: {error}"))?
}

#[tauri::command]
pub async fn open_render_output(app_handle: tauri::AppHandle, path: String) -> Result<(), String> {
    tauri_plugin_opener::OpenerExt::opener(&app_handle)
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

pub fn shutdown_all() {
    let pids: Vec<u32> = RUNTIME
        .lock()
        .unwrap()
        .running
        .values()
        .flat_map(|control| {
            let mut value = control.lock().unwrap();
            value.cancel = true;
            value.pids.iter().copied().collect::<Vec<_>>()
        })
        .collect();
    for pid in pids {
        terminate_pid_tree(pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eta_job(status: &str) -> RenderJob {
        RenderJob {
            id: "eta-job".to_string(),
            batch_id: "eta-batch".to_string(),
            project_path: "C:/project".to_string(),
            name: "ETA test".to_string(),
            blend_path: "C:/project/test.blend".to_string(),
            scene_name: "Scene".to_string(),
            status: status.to_string(),
            frame_start: 1,
            frame_end: 20,
            frame_step: 1,
            parallelism: 1,
            effective_parallelism: 1,
            ready_workers: 0,
            execution_mode: "persistent".to_string(),
            frame_order_mode: "dynamic".to_string(),
            total_frames: 20,
            completed_frames: 0,
            failed_frames: 0,
            skipped_frames: 0,
            current_frame: None,
            progress: 0.0,
            output_dir: "C:/project/renders".to_string(),
            blender_path: "C:/Blender/blender.exe".to_string(),
            created_at: 0,
            started_at: Some(0),
            finished_at: None,
            error: None,
            archived: false,
            cpu_usage: 0.0,
            memory_bytes: 0,
            peak_cpu_usage: 0.0,
            peak_memory_bytes: 0,
            performance_updated_at: None,
            position: 0,
            batch_name: "ETA batch".to_string(),
            batch_status: "running".to_string(),
            batch_position: 0,
            attention_code: None,
        }
    }

    fn eta_frame(
        frame: i64,
        status: &str,
        duration_ms: Option<i64>,
        attempts: i64,
        updated_at: i64,
    ) -> RenderFrame {
        RenderFrame {
            job_id: "eta-job".to_string(),
            frame,
            status: status.to_string(),
            attempts,
            output_path: format!("frame-{frame}.png"),
            error: None,
            duration_ms,
            render_duration_ms: duration_ms,
            worker_id: None,
            claim_token: None,
            updated_at,
        }
    }

    fn frames_with_durations(durations: &[i64], pending: usize) -> Vec<RenderFrame> {
        let mut frames: Vec<RenderFrame> = durations
            .iter()
            .enumerate()
            .map(|(index, duration)| {
                eta_frame(index as i64 + 1, "completed", Some(*duration), 1, 0)
            })
            .collect();
        frames.extend((0..pending).map(|index| {
            eta_frame(
                durations.len() as i64 + index as i64 + 1,
                "pending",
                None,
                0,
                0,
            )
        }));
        frames
    }

    #[test]
    fn eta_waits_for_two_samples_and_downweights_cold_start() {
        let job = eta_job("running");
        let calibrating =
            estimate_render_eta(&job, &frames_with_durations(&[147_000], 8), 1_000_000);
        assert_eq!(calibrating.status, "calibrating");
        assert!(calibrating.estimated_finish_at.is_none());

        let estimated = estimate_render_eta(
            &job,
            &frames_with_durations(&[147_000, 19_000], 8),
            1_000_000,
        );
        assert_eq!(estimated.status, "estimating");
        let remaining = estimated.remaining_ms.unwrap();
        assert!(remaining > 8 * 10_000);
        assert!(remaining < 8 * 40_000);
    }

    #[test]
    fn eta_tracks_trend_without_being_dominated_by_an_outlier() {
        let job = eta_job("running");
        let stable = estimate_render_eta(
            &job,
            &frames_with_durations(&[10_000, 10_000, 10_000, 10_000], 3),
            1_000_000,
        );
        let rising = estimate_render_eta(
            &job,
            &frames_with_durations(&[10_000, 12_000, 14_000, 16_000], 3),
            1_000_000,
        );
        assert!(rising.remaining_ms.unwrap() > stable.remaining_ms.unwrap());

        let outlier = estimate_render_eta(
            &job,
            &frames_with_durations(&[20_000, 21_000, 120_000, 20_000, 21_000, 22_000], 5),
            1_000_000,
        );
        assert!(outlier.remaining_ms.unwrap() < 5 * 35_000);
    }

    #[test]
    fn eta_does_not_amplify_small_trend_across_a_long_job() {
        let job = eta_job("running");
        let eta = estimate_render_eta(
            &job,
            &frames_with_durations(
                &[
                    12_000, 12_000, 12_000, 13_000, 13_000, 13_000, 13_000, 13_000,
                ],
                148,
            ),
            1_000_000,
        );

        let remaining = eta.remaining_ms.unwrap();
        assert!(remaining > 148 * 11_000);
        assert!(remaining < 148 * 18_000);
    }

    #[test]
    fn eta_distributes_remaining_frames_across_job_workers() {
        let frames = frames_with_durations(&[10_000, 10_000, 10_000, 10_000], 12);
        let single = estimate_render_eta(&eta_job("running"), &frames, 1_000_000)
            .remaining_ms
            .unwrap();
        let mut parallel_job = eta_job("running");
        parallel_job.parallelism = 3;
        parallel_job.effective_parallelism = 3;
        let parallel = estimate_render_eta(&parallel_job, &frames, 1_000_000)
            .remaining_ms
            .unwrap();

        assert!(parallel <= single / 3 + 10_000);
        assert!(parallel < single / 2);
    }

    #[test]
    fn eta_keeps_overtime_current_frame_in_the_future() {
        let job = eta_job("running");
        let now_ms = 1_000_000;
        let mut frames = frames_with_durations(&[20_000, 21_000, 19_000], 0);
        frames.push(eta_frame(4, "running", None, 1, now_ms - 60_000));
        frames.push(eta_frame(5, "pending", None, 1, 0));
        let eta = estimate_render_eta(&job, &frames, now_ms);
        assert!(eta.estimated_finish_at.unwrap() > now_ms);
        assert!(eta.remaining_ms.unwrap() >= 5_000);
    }

    #[test]
    fn eta_reports_terminal_and_paused_states() {
        let frames = frames_with_durations(&[20_000, 21_000], 2);
        assert_eq!(
            estimate_render_eta(&eta_job("paused"), &frames, 1_000_000).status,
            "paused"
        );
        assert_eq!(
            estimate_render_eta(&eta_job("cancelled"), &frames, 1_000_000).status,
            "unavailable"
        );
        assert_eq!(
            estimate_render_eta(&eta_job("completed"), &frames, 1_000_000).remaining_ms,
            Some(0)
        );
    }

    #[test]
    fn reorders_jobs_and_batches_with_separate_operations() {
        let root = std::env::temp_dir().join(format!("pm-render-reorder-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let project_path = root.to_string_lossy().to_string();
        let conn = open_db(&project_path).unwrap();
        for (id, position) in [("batch-a", 0_i64), ("batch-b", 1_i64)] {
            conn.execute(
                "INSERT INTO render_batches(id,project_path,name,status,position,created_at,updated_at) VALUES(?1,?2,?1,'queued',?3,?3,?3)",
                params![id, project_path, position],
            )
            .unwrap();
        }
        for (id, batch_id, position) in [
            ("job-a-1", "batch-a", 0_i64),
            ("job-a-2", "batch-a", 1_i64),
            ("job-b-1", "batch-b", 0_i64),
        ] {
            conn.execute(
                "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,position,created_at) VALUES(?1,?2,?3,?1,'test.blend','Scene','paused',1,1,1,?3,'blender','{}',?4,0)",
                params![id, batch_id, project_path, position],
            )
            .unwrap();
        }
        drop(conn);

        reorder_render_job_in_db(&project_path, "job-a-2", Some("job-a-1")).unwrap();
        let conn = open_db(&project_path).unwrap();
        let same_batch: Vec<String> = conn
            .prepare("SELECT id FROM render_jobs WHERE batch_id='batch-a' ORDER BY position")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(same_batch, ["job-a-2", "job-a-1"]);
        drop(conn);

        let cross_batch_error =
            reorder_render_job_in_db(&project_path, "job-b-1", Some("job-a-1")).unwrap_err();
        assert!(cross_batch_error.contains("只能在所属批次内"));
        reorder_render_batch_in_db(&project_path, "batch-b", Some("batch-a")).unwrap();
        let conn = open_db(&project_path).unwrap();
        let batch_order: Vec<String> = conn
            .prepare("SELECT id FROM render_batches ORDER BY position")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(batch_order, ["batch-b", "batch-a"]);
        let positions: Vec<i64> = conn
            .prepare("SELECT position FROM render_batches ORDER BY position")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(positions, [0, 1]);
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn initializes_render_schema_and_recovers_running_jobs() {
        let root = std::env::temp_dir().join(format!("pm-render-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        init_project_storage(root.to_str().unwrap()).unwrap();
        let conn = open_db(root.to_str().unwrap()).unwrap();
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'render_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(tables >= 8);
        let performance_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='render_performance_samples'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(performance_table, 1);
        let performance_columns: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('render_jobs') WHERE name IN ('cpu_usage','memory_bytes','peak_cpu_usage','peak_memory_bytes','performance_updated_at')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(performance_columns, 5);
        let parallelism_column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('render_jobs') WHERE name='parallelism'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(parallelism_column, 1);
        let force_render_column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('render_frames') WHERE name='force_render'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(force_render_column, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editing_job_settings_preserves_or_invalidates_frames_as_needed() {
        let root = std::env::temp_dir().join(format!("pm-render-edit-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut conn = open_db(root.to_str().unwrap()).unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,created_at,updated_at) VALUES('edit-batch',?1,'Batch','completed',0,0)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        let output_dir = root.join("renders");
        fs::create_dir_all(&output_dir).unwrap();
        let spec = json!({
            "blendPath": root.join("test.blend"),
            "sceneName": "Scene",
            "frameStart": 1,
            "frameEnd": 2,
            "frameStep": 1,
            "parallelism": 1,
            "resolutionPercentage": 100,
            "engine": "CYCLES",
            "outputFormat": "PNG"
        });
        conn.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,parallelism,output_dir,blender_path,spec_json,position,created_at,finished_at) VALUES('edit-job','edit-batch',?1,'Job',?2,'Scene','completed',1,2,1,1,?3,'blender',?4,0,0,123)",
            params![root.to_string_lossy(), root.join("test.blend").to_string_lossy(), output_dir.to_string_lossy(), spec.to_string()],
        )
        .unwrap();
        for frame in 1..=2 {
            let output_path = frame_output_path(&output_dir, "Scene", frame, 2, "PNG");
            fs::create_dir_all(&output_dir).unwrap();
            image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
                .save(&output_path)
                .unwrap();
            conn.execute(
                "INSERT INTO render_frames(job_id,frame,status,attempts,output_path,duration_ms,updated_at) VALUES('edit-job',?1,'completed',1,?2,1000,0)",
                params![frame, output_path.to_string_lossy()],
            )
            .unwrap();
        }

        let parallel_only = UpdateRenderJobRequest {
            scene_name: "Scene".into(),
            frame_start: 1,
            frame_end: 2,
            frame_step: 1,
            parallelism: 2,
            execution_mode: "persistent".into(),
            frame_order_mode: "dynamic".into(),
            resolution_percentage: 100,
            engine: Some("CYCLES".into()),
            output_format: "PNG".into(),
        };
        let (should_kick, _) =
            update_render_job_settings(&mut conn, "edit-job", &parallel_only).unwrap();
        assert!(!should_kick);
        let preserved: (String, i64, i64, i64) = conn
            .query_row(
                "SELECT j.status,j.parallelism,f.force_render,j.finished_at FROM render_jobs j JOIN render_frames f ON f.job_id=j.id WHERE j.id='edit-job' AND f.frame=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(preserved, ("completed".into(), 2, 0, 123));

        let extended = UpdateRenderJobRequest {
            frame_end: 3,
            ..parallel_only.clone()
        };
        update_render_job_settings(&mut conn, "edit-job", &extended).unwrap();
        let range_update: Vec<(i64, String, i64, String)> = conn
            .prepare("SELECT frame,status,force_render,output_path FROM render_frames WHERE job_id='edit-job' ORDER BY frame")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(range_update.len(), 3);
        assert_eq!(range_update[0].0, 1);
        assert_eq!(range_update[0].1, "completed");
        assert_eq!(range_update[0].2, 0);
        assert_eq!(
            range_update[0].3,
            frame_output_path(&output_dir, "Scene", 1, 2, "PNG").to_string_lossy()
        );
        assert_eq!(range_update[1].1, "completed");
        assert_eq!(range_update[2].1, "pending");
        assert_eq!(range_update[2].2, 0);

        let changed = UpdateRenderJobRequest {
            frame_end: 3,
            resolution_percentage: 50,
            ..parallel_only.clone()
        };
        update_render_job_settings(&mut conn, "edit-job", &changed).unwrap();
        let reset: (String, i64, i64) = conn
            .query_row(
                "SELECT j.status,COUNT(*),SUM(f.force_render) FROM render_jobs j JOIN render_frames f ON f.job_id=j.id WHERE j.id='edit-job'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(reset, ("paused".into(), 3, 3));
        let pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM render_frames WHERE job_id='edit-job' AND status='pending'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, 3);

        conn.execute(
            "UPDATE render_jobs SET status='running' WHERE id='edit-job'",
            [],
        )
        .unwrap();
        let error = update_render_job_settings(&mut conn, "edit-job", &changed).unwrap_err();
        assert!(error.contains("请先暂停"));
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn batch_queue_starts_one_batch_at_a_time() {
        let root = std::env::temp_dir().join(format!("pm-render-batch-queue-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let conn = open_db(root.to_str().unwrap()).unwrap();
        for (id, created_at) in [("batch-a", 1_i64), ("batch-b", 2_i64)] {
            conn.execute(
                "INSERT INTO render_batches(id,project_path,name,status,created_at,updated_at) VALUES(?1,?2,?1,'queued',?3,?3)",
                params![id, root.to_string_lossy(), created_at],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,position,created_at) VALUES(?1,?2,?3,?1,'test.blend','Scene','paused',1,1,1,?3,'blender','{}',0,?4)",
                params![format!("job-{id}"), id, root.to_string_lossy(), created_at],
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE render_jobs SET status='cancelled' WHERE id='job-batch-a'",
            [],
        )
        .unwrap();

        advance_batch_queue(root.to_str().unwrap()).unwrap();
        let first: (String, String, String, String) = conn
            .query_row(
                "SELECT (SELECT status FROM render_batches WHERE id='batch-a'),(SELECT status FROM render_jobs WHERE id='job-batch-a'),(SELECT status FROM render_batches WHERE id='batch-b'),(SELECT status FROM render_jobs WHERE id='job-batch-b')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            first,
            (
                "cancelled".into(),
                "cancelled".into(),
                "running".into(),
                "pending".into()
            )
        );

        conn.execute(
            "UPDATE render_jobs SET status='completed' WHERE id='job-batch-b'",
            [],
        )
        .unwrap();
        advance_batch_queue(root.to_str().unwrap()).unwrap();
        let second: (String, String, String, String) = conn
            .query_row(
                "SELECT (SELECT status FROM render_batches WHERE id='batch-a'),(SELECT status FROM render_jobs WHERE id='job-batch-a'),(SELECT status FROM render_batches WHERE id='batch-b'),(SELECT status FROM render_jobs WHERE id='job-batch-b')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            second,
            (
                "cancelled".into(),
                "cancelled".into(),
                "completed".into(),
                "completed".into()
            )
        );
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archived_jobs_do_not_block_the_next_render_batch() {
        let root = std::env::temp_dir().join(format!("pm-render-archived-batch-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let conn = open_db(root.to_str().unwrap()).unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,position,created_at,updated_at) VALUES('archived-batch',?1,'Archived','running',0,0,0)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,position,created_at,updated_at) VALUES('next-batch',?1,'Next','queued',1,1,1)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        for (job_id, batch_id, archived) in [
            ("archived-job", "archived-batch", 1_i64),
            ("next-job", "next-batch", 0_i64),
        ] {
            conn.execute(
                "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,archived,position,created_at) VALUES(?1,?2,?3,?1,'test.blend','Scene','paused',1,1,1,?3,'blender','{}',?4,0,0)",
                params![job_id, batch_id, root.to_string_lossy(), archived],
            )
            .unwrap();
        }

        advance_batch_queue(root.to_str().unwrap()).unwrap();
        let state: (String, String, String, String) = conn
            .query_row(
                "SELECT (SELECT status FROM render_batches WHERE id='archived-batch'),(SELECT status FROM render_jobs WHERE id='archived-job'),(SELECT status FROM render_batches WHERE id='next-batch'),(SELECT status FROM render_jobs WHERE id='next-job')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "completed".into(),
                "paused".into(),
                "running".into(),
                "pending".into()
            )
        );
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retrying_frames_only_queues_work_until_started() {
        let root = std::env::temp_dir().join(format!("pm-render-retry-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let conn = open_db(root.to_str().unwrap()).unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,created_at,updated_at) VALUES('retry-batch',?1,'Retry','completed',0,0)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,position,created_at) VALUES('retry-job','retry-batch',?1,'Retry','test.blend','Scene','completed',1,1,1,?1,'blender','{}',0,0)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO render_frames(job_id,frame,status,output_path,updated_at) VALUES('retry-job',1,'completed',?1,0)",
            params![root.join("Scene_0001.png").to_string_lossy()],
        )
        .unwrap();
        drop(conn);

        tauri::async_runtime::block_on(queue_render_frames(
            root.to_string_lossy().to_string(),
            "retry-job".into(),
            vec![1],
            "rerender".into(),
        ))
        .unwrap();

        let conn = open_db(root.to_str().unwrap()).unwrap();
        let state: (String, String, String, i64) = conn
            .query_row(
                "SELECT (SELECT status FROM render_batches WHERE id='retry-batch'),(SELECT status FROM render_jobs WHERE id='retry-job'),(SELECT status FROM render_frames WHERE job_id='retry-job' AND frame=1),(SELECT force_render FROM render_frames WHERE job_id='retry-job' AND frame=1)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            ("queued".into(), "paused".into(), "pending".into(), 1)
        );
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn workers_claim_distinct_frames_atomically() {
        let root = std::env::temp_dir().join(format!("pm-render-claim-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut conn = open_db(root.to_str().unwrap()).unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,created_at,updated_at) VALUES('batch',?1,'Batch','queued',0,0)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,position,created_at) VALUES('job','batch',?1,'Job','test.blend','Scene','running',1,3,1,?1,'blender','{}',0,0)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        for frame in 1..=3 {
            conn.execute(
                "INSERT INTO render_frames(job_id,frame,status,output_path,updated_at) VALUES('job',?1,'pending',?2,0)",
                params![frame, root.join(format!("frame-{frame}.png")).to_string_lossy()],
            )
            .unwrap();
        }

        let first = claim_next_frame(&mut conn, "job", 2, "worker-a")
            .unwrap()
            .unwrap();
        let second = claim_next_frame(&mut conn, "job", 2, "worker-b")
            .unwrap()
            .unwrap();
        assert_eq!(first.frame, 1);
        assert_eq!(second.frame, 2);
        assert_ne!(first.claim_token, second.claim_token);
        assert_eq!(first.worker_id, "worker-a");
        assert_eq!(second.worker_id, "worker-b");
        let running: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM render_frames WHERE job_id='job' AND status='running'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(running, 2);
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_worker_token_cannot_commit_or_replace_output() {
        let root = std::env::temp_dir().join(format!("pm-render-stale-claim-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let project_path = root.to_string_lossy().to_string();
        let mut conn = open_db(&project_path).unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,created_at,updated_at) VALUES('batch',?1,'Batch','running',0,0)",
            params![project_path],
        ).unwrap();
        conn.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,position,created_at) VALUES('job','batch',?1,'Job','test.blend','Scene','running',1,1,1,?1,'blender','{}',0,0)",
            params![project_path],
        ).unwrap();
        let output = root.join("frame-1.png");
        conn.execute(
            "INSERT INTO render_frames(job_id,frame,status,output_path,updated_at) VALUES('job',1,'pending',?1,0)",
            params![output.to_string_lossy()],
        ).unwrap();
        let claim = claim_next_frame(&mut conn, "job", 2, "old-worker")
            .unwrap()
            .unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&claim.temp_output_path)
            .unwrap();
        conn.execute(
            "UPDATE render_frames SET claim_token='new-token',worker_id='new-worker' WHERE job_id='job' AND frame=1",
            [],
        ).unwrap();

        let committed =
            complete_frame_claim(&mut conn, "job", &claim, 1, 100, 120, Some((2, 2))).unwrap();
        assert!(!committed);
        assert!(!output.exists());
        assert!(!Path::new(&claim.temp_output_path).exists());
        let owner: (String, String, String) = conn.query_row(
            "SELECT status,worker_id,claim_token FROM render_frames WHERE job_id='job' AND frame=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(
            owner,
            ("running".into(), "new-worker".into(), "new-token".into())
        );
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn crash_recovery_commits_valid_temp_and_releases_running_frame() {
        let root = std::env::temp_dir().join(format!("pm-render-recovery-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let project_path = root.to_string_lossy().to_string();
        let conn = open_db(&project_path).unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,created_at,updated_at) VALUES('batch',?1,'Batch','running',0,0)",
            params![project_path],
        ).unwrap();
        conn.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,position,created_at) VALUES('job','batch',?1,'Job','test.blend','Scene','running',1,2,1,?1,'blender','{}',0,0)",
            params![project_path],
        ).unwrap();
        let committed_output = root.join("frame-1.png");
        let committing_temp = root.join(".frame-1.part.png");
        let running_output = root.join("frame-2.png");
        let running_temp = root.join(".frame-2.part.png");
        for path in [&committing_temp, &running_temp] {
            image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 30, 40, 255]))
                .save(path)
                .unwrap();
        }
        conn.execute(
            "INSERT INTO render_frames(job_id,frame,status,output_path,worker_id,claim_token,temp_output_path,updated_at) VALUES('job',1,'committing',?1,'worker-a','token-a',?2,0)",
            params![committed_output.to_string_lossy(), committing_temp.to_string_lossy()],
        ).unwrap();
        conn.execute(
            "INSERT INTO render_frames(job_id,frame,status,output_path,worker_id,claim_token,temp_output_path,updated_at) VALUES('job',2,'running',?1,'worker-b','token-b',?2,0)",
            params![running_output.to_string_lossy(), running_temp.to_string_lossy()],
        ).unwrap();

        recover_interrupted_frames(&conn).unwrap();
        let states: Vec<String> = conn
            .prepare("SELECT status FROM render_frames WHERE job_id='job' ORDER BY frame")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(states, ["completed", "pending"]);
        assert!(valid_output_with_dimensions(
            &committed_output,
            Some((2, 2))
        ));
        assert!(!committing_temp.exists());
        assert!(!running_temp.exists());
        assert!(!running_output.exists());
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn process_crash_fallback_retires_extra_workers_once() {
        let control = Arc::new(Mutex::new(JobControl::default()));
        {
            let mut value = control.lock().unwrap();
            value.completed_frames = 2;
            value.workers.insert(
                "worker-1".into(),
                worker_state("worker-1", 0, None, "ready", None, None, None),
            );
        }
        assert!(!progressive_worker_admitted(&control.lock().unwrap(), 1));
        {
            let mut value = control.lock().unwrap();
            value.completed_frames = PROGRESSIVE_WORKER_WARMUP_FRAMES;
        }
        assert!(progressive_worker_admitted(&control.lock().unwrap(), 1));
        {
            let mut value = control.lock().unwrap();
            value.completed_frames = PROGRESSIVE_WORKER_WARMUP_FRAMES * 2;
            value.workers.insert(
                "worker-2".into(),
                worker_state("worker-2", 1, None, "rendering", Some(4), None, None),
            );
        }
        assert!(progressive_worker_admitted(&control.lock().unwrap(), 2));
        assert!(activate_single_worker_fallback(&control));
        assert!(!activate_single_worker_fallback(&control));
        assert!(!worker_exceeds_runtime_limit(&control, 0));
        assert!(worker_exceeds_runtime_limit(&control, 1));
        assert!(worker_exceeds_runtime_limit(&control, 7));
        assert!(is_worker_process_crash(
            "Blender Worker 异常退出，退出码 11"
        ));
        assert!(!is_worker_process_crash("Blender 渲染失败"));
    }

    #[test]
    fn pause_settlement_releases_claims_and_clears_stale_workers() {
        let root = std::env::temp_dir().join(format!("pm-render-pause-settle-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let project_path = root.to_string_lossy().to_string();
        let mut conn = open_db(&project_path).unwrap();
        conn.execute(
            "INSERT INTO render_batches(id,project_path,name,status,created_at,updated_at) VALUES('batch',?1,'Batch','running',0,0)",
            params![project_path],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,output_dir,blender_path,spec_json,position,created_at,current_frame,cpu_usage,memory_bytes) VALUES('job','batch',?1,'Job','test.blend','Scene','pausing',1,2,1,?1,'blender','{}',0,0,1,50,1024)",
            params![project_path],
        )
        .unwrap();
        let first_temp = root.join("frame-1.part.png");
        let second_temp = root.join("frame-2.part.png");
        fs::write(&first_temp, b"partial").unwrap();
        fs::write(&second_temp, b"partial").unwrap();
        for (frame, status, token, temp) in [
            (1_i64, "running", "token-a", &first_temp),
            (2_i64, "committing", "token-b", &second_temp),
        ] {
            conn.execute(
                "INSERT INTO render_frames(job_id,frame,status,output_path,worker_id,claim_token,temp_output_path,updated_at) VALUES('job',?1,?2,?3,'worker',?4,?5,0)",
                params![frame, status, root.join(format!("frame-{frame}.png")).to_string_lossy(), token, temp.to_string_lossy()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO render_attempts(job_id,frame,attempt,status,started_at,worker_id,claim_token,temp_output_path) VALUES('job',?1,1,'running',0,'worker',?2,?3)",
                params![frame, token, temp.to_string_lossy()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO render_workers(worker_id,job_id,ordinal,pid,state,current_frame,updated_at) VALUES('worker','job',0,12345,'rendering',1,0)",
            [],
        )
        .unwrap();

        settle_runtime_job(&mut conn, "job", "paused", "用户暂停").unwrap();

        let job: (String, Option<i64>, f64, i64, Option<String>) = conn
            .query_row(
                "SELECT status,current_frame,cpu_usage,memory_bytes,error FROM render_jobs WHERE id='job'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        assert_eq!(job, ("paused".into(), None, 0.0, 0, None));
        let pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM render_frames WHERE job_id='job' AND status='pending' AND claim_token IS NULL AND worker_id IS NULL AND temp_output_path IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, 2);
        let aborted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM render_attempts WHERE job_id='job' AND status='aborted' AND finished_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(aborted, 2);
        let worker: (String, Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT state,pid,current_frame FROM render_workers WHERE worker_id='worker'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(worker, ("stopped".into(), None, None));
        assert!(!first_temp.exists());
        assert!(!second_temp.exists());
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn maps_formats_and_sanitizes_names() {
        assert_eq!(format_extension("OPEN_EXR"), "exr");
        assert_eq!(safe_name("Scene 01/主"), "Scene_01__");
        let root = std::env::temp_dir().join(format!("pm-render-image-check-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("frame.png");
        image::RgbaImage::from_pixel(2, 3, image::Rgba([0, 0, 0, 255]))
            .save(&output)
            .unwrap();
        assert!(valid_output_with_dimensions(&output, Some((2, 3))));
        assert!(!valid_output_with_dimensions(&output, Some((3, 2))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_frame_plan_uses_images_on_disk_and_black_fills_missing_frames() {
        let root = std::env::temp_dir().join(format!("pm-render-package-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let existing = root.join("Scene_0001.png");
        image::RgbImage::from_pixel(4, 2, image::Rgb([12, 34, 56]))
            .save(&existing)
            .unwrap();

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE render_frames (job_id TEXT, frame INTEGER, status TEXT, output_path TEXT)",
        )
        .unwrap();
        // The frame deliberately remains pending: packaging must use the real image,
        // rather than treating the scheduler state as a missing frame.
        conn.execute(
            "INSERT INTO render_frames(job_id,frame,status,output_path) VALUES('job',1,'pending',?1)",
            params![existing.to_string_lossy()],
        )
        .unwrap();
        let job = BatchPackageJob {
            id: "job".into(),
            name: "Job".into(),
            scene_name: "Scene".into(),
            frame_start: 1,
            frame_end: 2,
            frame_step: 1,
            output_dir: root.clone(),
            output_format: "PNG".into(),
            expected_dimensions: Some((4, 2)),
        };

        let plan = collect_job_package_frames(&conn, &job).unwrap();
        assert_eq!(plan.missing_frames, vec![2]);
        assert_eq!(plan.frames[0].as_deref(), Some(existing.as_path()));
        assert_eq!(plan.frames[1], None);

        let prepared =
            prepare_package_frame_paths(&plan.frames, plan.dimensions, &root.join("work")).unwrap();
        assert_eq!(prepared[0], existing);
        assert_eq!(image::image_dimensions(&prepared[1]).unwrap(), (4, 2));
        assert!(image::open(&prepared[1])
            .unwrap()
            .to_rgb8()
            .pixels()
            .all(|pixel| pixel.0 == [0, 0, 0]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persistent_worker_renders_ten_frames_with_one_blend_load_when_configured() {
        let Ok(blender) = std::env::var("PM_CENTER_BLENDER_TEST") else {
            return;
        };
        let root = std::env::temp_dir().join(format!("pm-render-smoke-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let blend_path = root.join("source.blend");
        let worker_script = root.join("worker.py");
        fs::write(&worker_script, persistent_worker_script()).unwrap();

        let create_code = format!(
            "import bpy; s=bpy.context.scene; s.render.resolution_x=64; s.render.resolution_y=64; s.render.resolution_percentage=100; bpy.ops.wm.save_as_mainfile(filepath=r'{}')",
            blend_path.to_string_lossy()
        );
        let create_status = std_command(&blender)
            .args([
                "--background",
                "--factory-startup",
                "--python-expr",
                &create_code,
            ])
            .status()
            .unwrap();
        assert!(create_status.success());
        let source_before = fs::read(&blend_path).unwrap();

        let mut child = std_command(&blender)
            .arg("-b")
            .arg(&blend_path)
            .arg("--python")
            .arg(&worker_script)
            .arg("--")
            .arg("--worker-id")
            .arg("smoke-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdout_reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut output = String::new();
            let mut reader = std::io::BufReader::new(stdout);
            reader.read_to_string(&mut output).unwrap();
            output
        });
        let stderr_reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut output = String::new();
            let mut reader = std::io::BufReader::new(stderr);
            reader.read_to_string(&mut output).unwrap();
            output
        });
        let mut stdin = child.stdin.take().unwrap();
        let output_paths = (1..=10)
            .map(|frame| {
                let output = root.join(format!("frame-{frame:04}.png"));
                writeln!(
                    stdin,
                    "{}",
                    json!({
                        "type": "render",
                        "frame": frame,
                        "claimToken": format!("token-{frame}"),
                        "tempOutputPath": output,
                        "sceneName": "Scene",
                        "outputFormat": "PNG",
                        "resolutionX": 64,
                        "resolutionY": 64,
                        "resolutionPercentage": 100,
                        "engine": null
                    })
                )
                .unwrap();
                output
            })
            .collect::<Vec<_>>();
        writeln!(stdin, "{}", json!({ "type": "shutdown" })).unwrap();
        drop(stdin);
        let render_status = child.wait().unwrap();
        let stdout = stdout_reader.join().unwrap();
        let stderr = stderr_reader.join().unwrap();
        assert!(render_status.success());
        assert_eq!(
            stdout.matches("Read blend:").count() + stderr.matches("Read blend:").count(),
            1
        );
        assert_eq!(stdout.matches("\"type\": \"worker-ready\"").count(), 1);
        assert_eq!(stdout.matches("\"type\": \"frame-completed\"").count(), 10);
        assert!(output_paths
            .iter()
            .all(|path| valid_output_with_dimensions(path, Some((64, 64)))));
        assert_eq!(source_before, fs::read(&blend_path).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn video_package_formats_and_concat_paths_are_normalized() {
        assert!(matches!(
            VideoPackageFormat::parse("mp4"),
            Ok(VideoPackageFormat::Mp4)
        ));
        assert!(matches!(
            VideoPackageFormat::parse("WEBM"),
            Ok(VideoPackageFormat::Webm)
        ));
        assert!(VideoPackageFormat::parse("avi").is_err());
        assert_eq!(
            ffconcat_path(Path::new(r"C:\renders\shot's_0001.png")).unwrap(),
            r"C:/renders/shot\'s_0001.png"
        );
    }
}
