use crate::process_utils::{std_command, tokio_command};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderBatchResult {
    pub batch_id: String,
    pub job_ids: Vec<String>,
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
    pub updated_at: i64,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderJobSettings {
    pub scene_name: String,
    pub frame_start: i64,
    pub frame_end: i64,
    pub frame_step: i64,
    pub parallelism: i64,
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
}

#[derive(Default)]
struct JobControl {
    cancel: bool,
    pause: bool,
    pids: HashSet<u32>,
    metrics: HashMap<u32, RenderProcessMetrics>,
}

#[derive(Default)]
struct RuntimeState {
    running: HashMap<String, Arc<Mutex<JobControl>>>,
    projects: HashSet<String>,
}

lazy_static::lazy_static! {
    static ref RUNTIME: Mutex<RuntimeState> = Mutex::new(RuntimeState::default());
    static ref RECOVERED_PROJECTS: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    static ref SCHEDULER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
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
            force_render INTEGER NOT NULL DEFAULT 0,
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
            error TEXT
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
        "render_frames",
        "force_render",
        "INTEGER NOT NULL DEFAULT 0",
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
    conn.execute(
        "UPDATE render_jobs SET status = 'paused', error = '应用上次退出时任务仍在运行，请手动继续', cpu_usage=0, memory_bytes=0 WHERE status IN ('starting', 'running', 'pausing', 'cancelling')",
        [],
    ).map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE render_frames SET status = 'pending', error = NULL WHERE status = 'running'",
        [],
    )
    .map_err(|error| error.to_string())?;
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

fn write_runtime_scripts(job_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(job_dir).map_err(|error| error.to_string())?;
    let bootstrap = job_dir.join("bootstrap.py");
    if !bootstrap.exists() {
        fs::write(&bootstrap, bootstrap_script()).map_err(|error| error.to_string())?;
    }
    Ok(bootstrap)
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
    tx.execute(
        "INSERT INTO render_batches(id, project_path, name, status, created_at, updated_at) VALUES(?1, ?2, ?3, 'queued', ?4, ?4)",
        params![batch_id, project_path, request.name, created],
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
            "resolutionX": job.resolution_x,
            "resolutionY": job.resolution_y,
            "resolutionPercentage": job.resolution_percentage.unwrap_or(100),
            "engine": job.engine,
            "outputFormat": format,
        });
        let name = format!("{} · {}", blend_stem, job.scene_name);
        tx.execute(
            "INSERT INTO render_jobs(id,batch_id,project_path,name,blend_path,scene_name,status,frame_start,frame_end,frame_step,parallelism,output_dir,blender_path,python_path,pre_hook,post_hook,force_overwrite,max_retries,spec_json,position,created_at) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
            params![job_id,batch_id,project_path,name,job.blend_path,job.scene_name,job.frame_start,job.frame_end,job.frame_step,job.parallelism.clamp(1,8),output_dir.to_string_lossy(),request.blender_path,rusqlite::types::Null,request.pre_hook,request.post_hook,request.force_overwrite as i64,request.max_retries.max(0),spec.to_string(),position as i64,created],
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
    kick_scheduler(app_handle, project_path.clone());
    Ok(RenderBatchResult { batch_id, job_ids })
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<RenderJob> {
    let total: i64 = row.get(19)?;
    let completed: i64 = row.get(20)?;
    let failed: i64 = row.get(21)?;
    let skipped: i64 = row.get(22)?;
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
        parallelism: row.get(28)?,
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
    })
}

fn render_job_settings(job: &RenderJob, spec: &Value) -> RenderJobSettings {
    RenderJobSettings {
        scene_name: job.scene_name.clone(),
        frame_start: job.frame_start,
        frame_end: job.frame_end,
        frame_step: job.frame_step,
        parallelism: job.parallelism,
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
    j.cpu_usage,j.memory_bytes,j.peak_cpu_usage,j.peak_memory_bytes,j.performance_updated_at,j.parallelism
    FROM render_jobs j"#;

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
                .then_some(frame.duration_ms)
                .flatten()
                .filter(|duration| *duration > 0)
                .map(|duration| (index, duration as f64))
        })
        .collect();
    let sample_count = completed.len();

    let state = match job.status.as_str() {
        "completed" => Some(("completed", Some(0))),
        "paused" => Some(("paused", None)),
        "failed" | "cancelled" => Some(("unavailable", None)),
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

    let mut worker_loads = vec![0.0_f64; job.parallelism.clamp(1, 8) as usize];
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
        "{} WHERE (?1=1 OR j.archived=0) ORDER BY j.position ASC, j.created_at DESC",
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
    let mut stmt = conn.prepare("SELECT job_id,frame,status,attempts,output_path,error,duration_ms,updated_at FROM render_frames WHERE job_id=?1 ORDER BY frame")
        .map_err(|error| error.to_string())?;
    let frames = stmt
        .query_map(params![job.id], |row| {
            Ok(RenderFrame {
                job_id: row.get(0)?,
                frame: row.get(1)?,
                status: row.get(2)?,
                attempts: row.get(3)?,
                output_path: row.get(4)?,
                error: row.get(5)?,
                duration_ms: row.get(6)?,
                updated_at: row.get(7)?,
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
    Ok(RenderJobDetail {
        job,
        settings,
        frames,
        log_tail,
        performance_samples,
        eta,
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
                        "UPDATE render_frames SET status='pending',attempts=0,error=NULL,duration_ms=NULL,force_render=0,updated_at=?3 WHERE job_id=?1 AND frame=?2",
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
                    "UPDATE render_frames SET status='pending',attempts=0,output_path=?3,error=NULL,duration_ms=NULL,force_render=1,updated_at=?4 WHERE job_id=?1 AND frame=?2",
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
            "UPDATE render_jobs SET name=?2,scene_name=?3,frame_start=?4,frame_end=?5,frame_step=?6,parallelism=?7,spec_json=?8,status=?9,current_frame=NULL,error=CASE WHEN ?10 THEN NULL ELSE error END,finished_at=CASE WHEN ?11 THEN NULL ELSE finished_at END WHERE id=?1",
            params![job_id,name,request.scene_name,request.frame_start,request.frame_end,request.frame_step,request.parallelism,spec.to_string(),next_status,clear_error,requires_more_work],
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

fn kick_scheduler(app: tauri::AppHandle, project_path: String) {
    RUNTIME
        .lock()
        .unwrap()
        .projects
        .insert(project_path.clone());
    tauri::async_runtime::spawn(async move {
        let _scheduler_guard = SCHEDULER_LOCK.lock().await;
        loop {
            let concurrency = load_scheduler_settings(&app).concurrency.clamp(1, 8) as usize;
            let running_total = RUNTIME.lock().unwrap().running.len();
            if running_total >= concurrency {
                break;
            }
            let next_job = open_db(&project_path).ok().and_then(|conn| {
                conn.query_row("SELECT id FROM render_jobs WHERE status='pending' AND archived=0 ORDER BY position ASC, created_at ASC LIMIT 1", [], |row| row.get::<_, String>(0)).optional().ok().flatten()
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
                let _ = run_job(&app_clone, &project_clone, &job_id, control).await;
                RUNTIME.lock().unwrap().running.remove(&key);
                emit_queue(&app_clone, &project_clone);
                kick_all_schedulers(app_clone);
            });
        }
    });
}

fn kick_all_schedulers(app: tauri::AppHandle) {
    let projects: Vec<String> = RUNTIME.lock().unwrap().projects.iter().cloned().collect();
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
    spec: Value,
}

fn load_execution_spec(conn: &Connection, job_id: &str) -> Result<JobExecutionSpec, String> {
    conn.query_row("SELECT blend_path,scene_name,blender_path,pre_hook,post_hook,force_overwrite,max_retries,spec_json,parallelism FROM render_jobs WHERE id=?1", params![job_id], |row| {
        let spec_json: String = row.get(7)?;
        Ok(JobExecutionSpec { blend_path: row.get(0)?, scene_name: row.get(1)?, blender_path: row.get(2)?, pre_hook: row.get(3)?, post_hook: row.get(4)?, force_overwrite: row.get::<_,i64>(5)? != 0, max_retries: row.get(6)?, spec: serde_json::from_str(&spec_json).unwrap_or(Value::Null), parallelism: row.get::<_,i64>(8)?.clamp(1,8) })
    }).map_err(|error| error.to_string())
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
    let conn = open_db(project_path)?;
    let spec = load_execution_spec(&conn, job_id)?;
    conn.execute("UPDATE render_jobs SET status='running',started_at=COALESCE(started_at,?2),finished_at=NULL,error=NULL WHERE id=?1 AND status IN ('pending','starting')", params![job_id, now()]).map_err(|e| e.to_string())?;
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
    let mut workers = Vec::new();
    for _ in 0..spec.parallelism {
        let app = app.clone();
        let project_path = project_path.to_string();
        let job_id = job_id.to_string();
        let spec = spec.clone();
        let bootstrap = bootstrap.clone();
        let job_dir = job_dir.clone();
        let control = control.clone();
        workers.push(tauri::async_runtime::spawn(async move {
            run_job_worker(
                &app,
                &project_path,
                &job_id,
                &spec,
                &bootstrap,
                &job_dir,
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
    let (cancel, pause) = {
        let value = control.lock().unwrap();
        (value.cancel, value.pause)
    };
    conn.execute(
        "UPDATE render_frames SET status='pending',error=NULL WHERE job_id=?1 AND status='running'",
        params![job_id],
    )
    .map_err(|error| error.to_string())?;
    if cancel {
        conn.execute("UPDATE render_jobs SET status='cancelled',current_frame=NULL,finished_at=?2,error='用户取消' WHERE id=?1", params![job_id, now()]).map_err(|error| error.to_string())?;
        emit_progress(app, project_path, job_id, None, "cancelled");
        return Ok(());
    }
    if pause {
        conn.execute(
            "UPDATE render_jobs SET status='paused',current_frame=NULL WHERE id=?1",
            params![job_id],
        )
        .map_err(|error| error.to_string())?;
        emit_progress(app, project_path, job_id, None, "paused");
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

fn claim_next_frame(
    conn: &mut Connection,
    job_id: &str,
    max_retries: i64,
) -> Result<Option<(i64, String, i64, bool)>, String> {
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
    if let Some((frame, _, _, _)) = &next {
        transaction
            .execute(
                "UPDATE render_frames SET status='running',error=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2",
                params![job_id, frame, now()],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(next)
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
    job_dir: &Path,
    control: Arc<Mutex<JobControl>>,
) -> Result<(), String> {
    let mut conn = open_db(project_path)?;
    loop {
        let (cancel, pause) = {
            let value = control.lock().unwrap();
            (value.cancel, value.pause)
        };
        if cancel || pause {
            return Ok(());
        }
        let Some((frame, output_path, attempts, force_render)) =
            claim_next_frame(&mut conn, job_id, spec.max_retries)?
        else {
            return Ok(());
        };
        refresh_current_frame(&conn, job_id)?;
        if !spec.force_overwrite && !force_render && valid_output(Path::new(&output_path)) {
            conn.execute("UPDATE render_frames SET status='skipped',error=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2", params![job_id, frame, now()]).map_err(|error| error.to_string())?;
            refresh_current_frame(&conn, job_id)?;
            emit_progress(app, project_path, job_id, Some(frame), "skipped");
            continue;
        }
        if attempts > 0 {
            tokio::time::sleep(Duration::from_secs(if attempts == 1 { 5 } else { 15 })).await;
        }
        let interrupted = {
            let value = control.lock().unwrap();
            value.cancel || value.pause
        };
        if interrupted {
            conn.execute("UPDATE render_frames SET status='pending',error=NULL,updated_at=?3 WHERE job_id=?1 AND frame=?2", params![job_id,frame,now()]).map_err(|error| error.to_string())?;
            refresh_current_frame(&conn, job_id)?;
            return Ok(());
        }
        let started = now();
        let attempt = attempts + 1;
        conn.execute("UPDATE render_frames SET attempts=?3,error=NULL,updated_at=?4 WHERE job_id=?1 AND frame=?2", params![job_id, frame, attempt, started]).map_err(|error| error.to_string())?;
        conn.execute("INSERT INTO render_attempts(job_id,frame,attempt,status,started_at) VALUES(?1,?2,?3,'running',?4)", params![job_id,frame,attempt,started]).map_err(|error| error.to_string())?;
        emit_progress(app, project_path, job_id, Some(frame), "rendering");
        let mut frame_spec = spec.spec.clone();
        frame_spec["frame"] = json!(frame);
        frame_spec["outputPath"] = json!(output_path);
        frame_spec["sceneName"] = json!(spec.scene_name);
        let frame_spec_path = job_dir.join(format!("frame-{frame}.json"));
        fs::write(
            &frame_spec_path,
            serde_json::to_vec_pretty(&frame_spec).unwrap(),
        )
        .map_err(|error| error.to_string())?;
        let result = execute_frame(
            app,
            project_path,
            job_id,
            frame,
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
        match result {
            Ok(()) if valid_output(Path::new(&output_path)) => {
                let size = fs::metadata(&output_path)
                    .map(|metadata| metadata.len() as i64)
                    .unwrap_or(0);
                conn.execute("UPDATE render_frames SET status='completed',duration_ms=?3,error=NULL,force_render=0,updated_at=?4 WHERE job_id=?1 AND frame=?2", params![job_id,frame,duration,now()]).map_err(|error| error.to_string())?;
                conn.execute("UPDATE render_attempts SET status='completed',finished_at=?4,exit_code=0 WHERE job_id=?1 AND frame=?2 AND attempt=?3", params![job_id,frame,attempt,now()]).map_err(|error| error.to_string())?;
                conn.execute("INSERT INTO render_artifacts(job_id,frame,path,size_bytes,created_at) VALUES(?1,?2,?3,?4,?5)", params![job_id,frame,output_path,size,now()]).map_err(|error| error.to_string())?;
                emit_progress(app, project_path, job_id, Some(frame), "completed-frame");
            }
            result => {
                let error = result
                    .err()
                    .unwrap_or_else(|| "Blender 未生成有效输出文件".into());
                let final_failure = attempt > spec.max_retries;
                conn.execute("UPDATE render_frames SET status=?3,duration_ms=?4,error=?5,updated_at=?6 WHERE job_id=?1 AND frame=?2", params![job_id,frame,if final_failure{"failed"}else{"pending"},duration,error,now()]).map_err(|db_error| db_error.to_string())?;
                conn.execute("UPDATE render_attempts SET status='failed',finished_at=?4,exit_code=-1,error=?5 WHERE job_id=?1 AND frame=?2 AND attempt=?3", params![job_id,frame,attempt,now(),error]).map_err(|db_error| db_error.to_string())?;
                emit_progress(
                    app,
                    project_path,
                    job_id,
                    Some(frame),
                    if final_failure {
                        "failed-frame"
                    } else {
                        "retrying"
                    },
                );
            }
        }
        refresh_current_frame(&conn, job_id)?;
    }
}

fn fail_job(conn: &Connection, job_id: &str, error: &str) -> Result<(), String> {
    conn.execute("UPDATE render_jobs SET status='failed',current_frame=NULL,finished_at=?2,error=?3 WHERE id=?1", params![job_id,now(),error]).map_err(|e| e.to_string())?;
    Ok(())
}

fn valid_output(path: &Path) -> bool {
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
    reader
        .into_dimensions()
        .map(|(width, height)| width > 0 && height > 0)
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
) -> Result<(), String> {
    use std::io::Write;
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
        if control.lock().unwrap().cancel {
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
        Ok(())
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

#[tauri::command]
pub async fn pause_render_job(project_path: String, job_id: String) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    if let Some(control) = control_for(&project_path, &job_id) {
        control.lock().unwrap().pause = true;
        conn.execute(
            "UPDATE render_jobs SET status='pausing' WHERE id=?1",
            params![job_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE render_jobs SET status='paused' WHERE id=?1 AND status='pending'",
            params![job_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn resume_render_job(
    app_handle: tauri::AppHandle,
    project_path: String,
    job_id: String,
) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    conn.execute(
        "UPDATE render_frames SET status='pending',attempts=0,error=NULL WHERE job_id=?1 AND status='failed'",
        params![job_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("UPDATE render_jobs SET status='pending',error=NULL,finished_at=NULL WHERE id=?1 AND status IN ('paused','failed','cancelled')", params![job_id]).map_err(|e| e.to_string())?;
    kick_scheduler(app_handle, project_path);
    Ok(())
}

#[tauri::command]
pub async fn cancel_render_job(project_path: String, job_id: String) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    if let Some(control) = control_for(&project_path, &job_id) {
        control.lock().unwrap().cancel = true;
        conn.execute(
            "UPDATE render_jobs SET status='cancelling' WHERE id=?1",
            params![job_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute("UPDATE render_jobs SET status='cancelled',finished_at=?2,error='用户取消' WHERE id=?1 AND status IN ('pending','paused','failed')", params![job_id,now()]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn pause_render_queue(project_path: String) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    conn.execute(
        "UPDATE render_jobs SET status='paused' WHERE status='pending' AND archived=0",
        [],
    )
    .map_err(|e| e.to_string())?;
    let controls: Vec<_> = RUNTIME
        .lock()
        .unwrap()
        .running
        .iter()
        .filter(|(key, _)| key.starts_with(&format!("{}\n", project_path)))
        .map(|(_, v)| v.clone())
        .collect();
    for control in controls {
        control.lock().unwrap().pause = true;
    }
    Ok(())
}

#[tauri::command]
pub async fn resume_render_queue(
    app_handle: tauri::AppHandle,
    project_path: String,
) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    conn.execute(
        "UPDATE render_jobs SET status='pending',error=NULL WHERE status='paused' AND archived=0",
        [],
    )
    .map_err(|e| e.to_string())?;
    kick_scheduler(app_handle, project_path);
    Ok(())
}

#[tauri::command]
pub async fn retry_render_frames(
    app_handle: tauri::AppHandle,
    project_path: String,
    job_id: String,
    frames: Option<Vec<i64>>,
) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    match frames.filter(|items| !items.is_empty()) {
        Some(frames) => {
            for frame in frames {
                conn.execute("UPDATE render_frames SET status='pending',attempts=0,error=NULL,force_render=1,updated_at=?3 WHERE job_id=?1 AND frame=?2", params![job_id,frame,now()]).map_err(|e|e.to_string())?;
            }
        }
        None => {
            conn.execute("UPDATE render_frames SET status='pending',attempts=0,error=NULL,force_render=1,updated_at=?2 WHERE job_id=?1 AND status='failed'", params![job_id,now()]).map_err(|e|e.to_string())?;
        }
    }
    conn.execute(
        "UPDATE render_jobs SET status='pending',error=NULL,finished_at=NULL WHERE id=?1",
        params![job_id],
    )
    .map_err(|e| e.to_string())?;
    kick_scheduler(app_handle, project_path);
    Ok(())
}

#[tauri::command]
pub async fn reorder_render_job(
    project_path: String,
    job_id: String,
    position: i64,
) -> Result<(), String> {
    let conn = open_db(&project_path)?;
    conn.execute(
        "UPDATE render_jobs SET position=?2 WHERE id=?1",
        params![job_id, position],
    )
    .map_err(|e| e.to_string())?;
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
    };
    save_scheduler_settings(&app_handle, &settings)?;
    RUNTIME.lock().unwrap().projects.insert(project_path);
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
        })
        .unwrap_or(SchedulerSettings { concurrency: 1 })
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
        assert_eq!(range_update[0].3, frame_output_path(&output_dir, "Scene", 1, 2, "PNG").to_string_lossy());
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

        let first = claim_next_frame(&mut conn, "job", 2).unwrap().unwrap();
        let second = claim_next_frame(&mut conn, "job", 2).unwrap().unwrap();
        assert_eq!(first.0, 1);
        assert_eq!(second.0, 2);
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
    fn maps_formats_and_sanitizes_names() {
        assert_eq!(format_extension("OPEN_EXR"), "exr");
        assert_eq!(safe_name("Scene 01/主"), "Scene_01__");
    }

    #[test]
    fn renders_one_frame_with_real_blender_when_configured() {
        let Ok(blender) = std::env::var("PM_CENTER_BLENDER_TEST") else {
            return;
        };
        let root = std::env::temp_dir().join(format!("pm-render-smoke-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let blend_path = root.join("source.blend");
        let bootstrap = root.join("bootstrap.py");
        let spec_path = root.join("spec.json");
        let output_path = root.join("frame.png");
        fs::write(&bootstrap, bootstrap_script()).unwrap();

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

        fs::write(
            &spec_path,
            serde_json::to_vec(&json!({
                "sceneName": "Scene",
                "frame": 1,
                "outputPath": output_path,
                "outputFormat": "PNG",
                "resolutionX": 64,
                "resolutionY": 64,
                "resolutionPercentage": 100,
                "engine": null
            }))
            .unwrap(),
        )
        .unwrap();
        let render_status = std_command(&blender)
            .arg("-b")
            .arg(&blend_path)
            .arg("--python")
            .arg(&bootstrap)
            .arg("--")
            .arg("--spec")
            .arg(&spec_path)
            .status()
            .unwrap();
        assert!(render_status.success());
        assert!(valid_output(&output_path));
        assert_eq!(source_before, fs::read(&blend_path).unwrap());
        let _ = fs::remove_dir_all(root);
    }
}
