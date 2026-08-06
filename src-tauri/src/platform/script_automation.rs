use super::capability_gateway::{
    CapabilityDecision, CapabilityDecisionRequest, CapabilityGateway, CapabilityOperation,
    CapabilityRequestStatus, CapabilityScopeKind, CapabilityScopeRequest, CapabilitySubjectKind,
    CapabilityTokenRequest,
};
use super::component_runtime::ComponentInstallSource;
use super::{ComponentInvocationRequest, ComponentRuntimeManager, ModuleManager};
use crate::platform::profile_runtime::WorkspaceProfileRuntime;
use base64::Engine;
use chrono::{Datelike, Timelike, Utc};
use pmc_platform::{
    parse_component_manifest, AutomationCapabilityOperation, AutomationCommandContribution,
    AutomationContextRequirement, AutomationExecutionSemantics, AutomationProjectContext,
    AutomationTriggerBinding, Capability, ComponentManifestV1, ComponentRuntime,
    ProfileAutomationBinding, WorkspaceProfileV1,
};
use regex::{Captures, Regex};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Listener};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;
use walkdir::WalkDir;

pub const SCRIPT_AUTOMATION_MODULE_ID: &str = "builtin.script-automation";
pub const SCRIPT_AUTOMATION_TOOL_ID: &str = "builtin.script-automation.studio-tool";
pub const SCRIPT_AUTOMATION_SURFACE_ID: &str = "builtin.script-automation.studio-surface";
const DATABASE_SCHEMA_VERSION: i64 = 4;
const TRUST_SCHEMA_VERSION: u16 = 1;
const BRIDGE_PROTOCOL: &str = "nexora.automation.bridge.v1";
const MAX_BRIDGE_REQUEST_BYTES: usize = 1024 * 1024;
const RUN_EVENT: &str = "nexora:automation-run-changed";
const LOG_EVENT: &str = "nexora:automation-log";
const PERMISSION_EVENT: &str = "nexora:automation-permission-required";
const ATTENTION_EVENT: &str = "nexora:automation-attention";
const SCRIPT_SURFACE_EVENT: &str = "nexora:script-surface-event";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutomationRunStatus {
    Queued,
    Preparing,
    Running,
    WaitingPermission,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    Attention,
}

impl AutomationRunStatus {
    fn parse(value: &str) -> Self {
        match value {
            "preparing" => Self::Preparing,
            "running" => Self::Running,
            "waiting-permission" => Self::WaitingPermission,
            "cancelling" => Self::Cancelling,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "attention" => Self::Attention,
            _ => Self::Queued,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRun {
    pub id: String,
    pub component_id: String,
    pub component_name: String,
    pub component_version: String,
    pub command: String,
    pub command_name: String,
    pub profile_id: String,
    pub profile_revision: u64,
    pub project_path: Option<String>,
    pub trigger_kind: String,
    pub trigger_id: Option<String>,
    pub status: AutomationRunStatus,
    pub progress: Option<f64>,
    pub progress_message: Option<String>,
    pub execution_semantics: AutomationExecutionSemantics,
    pub attempt: u16,
    pub max_attempts: u16,
    pub operation_id: Option<String>,
    pub content_digest: String,
    pub input: Value,
    pub output: Option<Value>,
    pub logs: Vec<String>,
    pub error: Option<String>,
    pub permission_request: Option<Value>,
    pub attempts: Vec<AutomationRunAttempt>,
    pub attention: Option<AutomationAttention>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRunAttempt {
    pub attempt: u16,
    pub operation_id: String,
    pub status: String,
    pub logs: Vec<String>,
    pub error: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationAttention {
    pub id: String,
    pub kind: String,
    pub message: String,
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAutomationRunRequest {
    pub component_id: String,
    pub command: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default = "default_manual_trigger")]
    pub trigger_kind: String,
    #[serde(default)]
    pub trigger_id: Option<String>,
    #[serde(default)]
    pub capability_scope: Option<CapabilityScopeRequest>,
}

fn default_manual_trigger() -> String {
    "manual".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRunFilter {
    pub profile_id: Option<String>,
    pub project_path: Option<String>,
    pub status: Option<AutomationRunStatus>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRuntimeSnapshot {
    pub running: bool,
    pub database_path: String,
    pub trusted_development_directories: Vec<String>,
    pub active_count: usize,
    pub waiting_permission_count: usize,
    pub attention_count: usize,
    pub recent_runs: Vec<AutomationRun>,
    pub available_components: Vec<AutomationComponentSummary>,
    pub legacy_workflows_executable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationComponentSummary {
    pub component_id: String,
    pub component_name: String,
    pub component_version: String,
    pub commands: Vec<AutomationCommandContribution>,
    pub events: Vec<String>,
    pub surfaces: Vec<pmc_platform::ScriptSurfaceContribution>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveAutomationAttentionRequest {
    pub run_id: String,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationEventRequest {
    pub event: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub dedupe_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptComponentValidation {
    pub valid: bool,
    pub source_path: String,
    pub trusted: bool,
    pub content_digest: Option<String>,
    pub manifest: Option<ComponentManifestV1>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScriptComponentTemplateRequest {
    pub parent_path: String,
    pub component_id: String,
    pub name: String,
    #[serde(default)]
    pub include_surface: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAutomationBindingRequest {
    pub profile_id: String,
    pub expected_revision: u64,
    pub binding: ProfileAutomationBinding,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAutomationBindingRequest {
    pub profile_id: String,
    pub expected_revision: u64,
    pub binding_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptSurfaceDocument {
    pub component_id: String,
    pub surface_id: String,
    pub title: String,
    pub nonce: String,
    pub allowed_commands: Vec<String>,
    pub html: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptDevelopmentFile {
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptDevelopmentDocument {
    pub path: String,
    pub content: String,
    pub content_digest: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveScriptDevelopmentFileRequest {
    pub source_path: String,
    pub relative_path: String,
    pub content: String,
    #[serde(default)]
    pub expected_content_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrustedDevelopmentDirectories {
    schema_version: u16,
    directories: Vec<String>,
}

impl Default for TrustedDevelopmentDirectories {
    fn default() -> Self {
        Self {
            schema_version: TRUST_SCHEMA_VERSION,
            directories: Vec::new(),
        }
    }
}

struct AutomationHost {
    manager: Arc<ModuleManager>,
    profiles: Arc<WorkspaceProfileRuntime>,
    gateway: Arc<CapabilityGateway>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomationBridgeRequest {
    protocol: String,
    token: String,
    operation: String,
    #[serde(default)]
    payload: Value,
}

pub struct ScriptAutomationRuntime {
    database: Mutex<Connection>,
    database_path: PathBuf,
    trust_path: PathBuf,
    trusted_directories: Mutex<TrustedDevelopmentDirectories>,
    app_handle: AppHandle,
    components: Arc<ComponentRuntimeManager>,
    host: Mutex<Option<AutomationHost>>,
    running: AtomicBool,
    accepting_triggers: AtomicBool,
    app_started_emitted: AtomicBool,
    open_projects: Mutex<BTreeSet<String>>,
    scheduler: Mutex<Option<JoinHandle<()>>>,
    listeners: Mutex<Vec<tauri::EventId>>,
    bridged_operations: Mutex<BTreeMap<String, BTreeSet<String>>>,
}

pub struct AutomationTriggerIntakeGuard {
    runtime: Arc<ScriptAutomationRuntime>,
}

impl Drop for AutomationTriggerIntakeGuard {
    fn drop(&mut self) {
        self.runtime
            .accepting_triggers
            .store(true, Ordering::SeqCst);
    }
}

impl ScriptAutomationRuntime {
    pub fn new(
        app_data_dir: &Path,
        app_handle: AppHandle,
        components: Arc<ComponentRuntimeManager>,
    ) -> Result<Arc<Self>, String> {
        fs::create_dir_all(app_data_dir).map_err(|error| error.to_string())?;
        let database_path = app_data_dir.join("automation_runtime.db");
        let connection = Connection::open(&database_path).map_err(|error| error.to_string())?;
        migrate_database(&connection)?;
        recover_interrupted_runs(&connection)?;
        let trust_path = app_data_dir.join("script-development-trust.json");
        let trusted_directories = read_trust_state(&trust_path);
        Ok(Arc::new(Self {
            database: Mutex::new(connection),
            database_path,
            trust_path,
            trusted_directories: Mutex::new(trusted_directories),
            app_handle,
            components,
            host: Mutex::new(None),
            running: AtomicBool::new(false),
            accepting_triggers: AtomicBool::new(false),
            app_started_emitted: AtomicBool::new(false),
            open_projects: Mutex::new(BTreeSet::new()),
            scheduler: Mutex::new(None),
            listeners: Mutex::new(Vec::new()),
            bridged_operations: Mutex::new(BTreeMap::new()),
        }))
    }

    pub fn attach_host(
        &self,
        manager: Arc<ModuleManager>,
        profiles: Arc<WorkspaceProfileRuntime>,
        gateway: Arc<CapabilityGateway>,
    ) {
        *self.host.lock().expect("automation host mutex poisoned") = Some(AutomationHost {
            manager,
            profiles,
            gateway,
        });
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.accepting_triggers.store(true, Ordering::SeqCst);
        self.install_event_forwarders();
        let runtime = self.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            while runtime.is_running() {
                interval.tick().await;
                if let Err(error) = runtime.process_schedules() {
                    eprintln!("[script-automation] schedule error: {error}");
                }
            }
        });
        *self
            .scheduler
            .lock()
            .expect("automation scheduler mutex poisoned") = Some(handle);
        self.resume_queued_runs();
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let was_accepting = self.accepting_triggers.swap(false, Ordering::SeqCst);
        let unsafe_active = match self.has_unsafe_active_runs() {
            Ok(value) => value,
            Err(error) => {
                self.accepting_triggers
                    .store(was_accepting, Ordering::SeqCst);
                return Err(error);
            }
        };
        if unsafe_active {
            self.accepting_triggers
                .store(was_accepting, Ordering::SeqCst);
            return Err("存在不可安全取消的非幂等脚本运行，已阻止停用脚本自动化模块".into());
        }
        if !self.running.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        if let Some(handle) = self
            .scheduler
            .lock()
            .expect("automation scheduler mutex poisoned")
            .take()
        {
            handle.abort();
        }
        for id in self
            .listeners
            .lock()
            .expect("automation listeners mutex poisoned")
            .drain(..)
        {
            self.app_handle.unlisten(id);
        }
        let active = {
            let connection = self
                .database
                .lock()
                .expect("automation database mutex poisoned");
            let mut statement = connection
                .prepare(
                    "SELECT id, operation_id, semantics FROM automation_runs
                     WHERE status IN ('preparing','running','cancelling')",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            rows
        };
        let mut operation_ids = active
            .iter()
            .filter_map(|(_, operation_id, _)| operation_id.clone())
            .collect::<Vec<_>>();
        for (run_id, operation_id, semantics) in active {
            if let Some(operation_id) = operation_id {
                operation_ids.extend(self.cancel_component_operation_tree(&operation_id));
            }
            let (status, error) = if semantics == "non-idempotent" {
                ("attention", "模块停用时操作结果未知，未自动重放")
            } else {
                ("cancelled", "脚本自动化模块已停用")
            };
            self.update_terminal(&run_id, status, None, Some(error.into()), Vec::new())?;
        }
        for _ in 0..50 {
            if operation_ids
                .iter()
                .all(|operation_id| !self.components.is_operation_active(operation_id))
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if operation_ids
            .iter()
            .any(|operation_id| self.components.is_operation_active(operation_id))
        {
            return Err("等待脚本组件进程退出超时".into());
        }
        Ok(())
    }

    pub fn health_message(&self) -> String {
        let snapshot = self.snapshot().ok();
        match snapshot {
            Some(snapshot) => format!(
                "运行 {} 个，等待授权 {} 个，需处理 {} 个",
                snapshot.active_count, snapshot.waiting_permission_count, snapshot.attention_count
            ),
            None => "自动化数据库不可用".into(),
        }
    }

    pub fn emit_app_started_once(self: &Arc<Self>) -> Result<usize, String> {
        if self.app_started_emitted.swap(true, Ordering::SeqCst) {
            return Ok(0);
        }
        match self.emit_event(AutomationEventRequest {
            event: "app.started".into(),
            payload: json!({ "startedAt": Utc::now().timestamp_millis() }),
            project_path: None,
            dedupe_key: Some(format!("app.started:{}", Uuid::new_v4())),
        }) {
            Ok(count) => Ok(count),
            Err(error) => {
                self.app_started_emitted.store(false, Ordering::SeqCst);
                Err(error)
            }
        }
    }

    pub fn snapshot(&self) -> Result<AutomationRuntimeSnapshot, String> {
        let recent_runs = self.list_runs(AutomationRunFilter {
            limit: Some(100),
            ..AutomationRunFilter::default()
        })?;
        let active_count = recent_runs
            .iter()
            .filter(|run| {
                matches!(
                    run.status,
                    AutomationRunStatus::Queued
                        | AutomationRunStatus::Preparing
                        | AutomationRunStatus::Running
                        | AutomationRunStatus::Cancelling
                )
            })
            .count();
        let waiting_permission_count = recent_runs
            .iter()
            .filter(|run| run.status == AutomationRunStatus::WaitingPermission)
            .count();
        let attention_count = recent_runs
            .iter()
            .filter(|run| run.status == AutomationRunStatus::Attention)
            .count();
        Ok(AutomationRuntimeSnapshot {
            running: self.is_running(),
            database_path: self.database_path.to_string_lossy().into_owned(),
            trusted_development_directories: self.trusted_directories(),
            active_count,
            waiting_permission_count,
            attention_count,
            recent_runs,
            available_components: self.available_components()?,
            legacy_workflows_executable: false,
        })
    }

    pub fn available_components(&self) -> Result<Vec<AutomationComponentSummary>, String> {
        let (_, effective) = self.current_profile_and_components()?;
        let mut items = self
            .components
            .manifests()
            .into_iter()
            .filter(|manifest| {
                effective.contains(&manifest.id)
                    && (!manifest.contributes.automation_commands.is_empty()
                        || !manifest.contributes.script_surfaces.is_empty())
            })
            .map(|manifest| AutomationComponentSummary {
                component_id: manifest.id,
                component_name: manifest.name,
                component_version: manifest.version,
                commands: manifest.contributes.automation_commands,
                events: manifest
                    .contributes
                    .automation_events
                    .into_iter()
                    .map(|event| event.event)
                    .collect(),
                surfaces: manifest.contributes.script_surfaces,
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.component_name.cmp(&right.component_name));
        Ok(items)
    }

    pub fn list_runs(&self, filter: AutomationRunFilter) -> Result<Vec<AutomationRun>, String> {
        let connection = self
            .database
            .lock()
            .expect("automation database mutex poisoned");
        let mut statement = connection
            .prepare("SELECT * FROM automation_runs ORDER BY created_at DESC LIMIT ?1")
            .map_err(|error| error.to_string())?;
        let limit = filter.limit.unwrap_or(200).clamp(1, 1000) as i64;
        let mut runs = statement
            .query_map([limit], map_run)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        for run in &mut runs {
            enrich_run(&connection, run)?;
        }
        runs.retain(|run| {
            filter
                .profile_id
                .as_ref()
                .is_none_or(|value| &run.profile_id == value)
                && filter
                    .project_path
                    .as_ref()
                    .is_none_or(|value| run.project_path.as_ref().is_some_and(|path| path == value))
                && filter.status.is_none_or(|value| run.status == value)
        });
        Ok(runs)
    }

    pub fn get_run(&self, run_id: &str) -> Result<AutomationRun, String> {
        let connection = self
            .database
            .lock()
            .expect("automation database mutex poisoned");
        let mut run = connection
            .query_row(
                "SELECT * FROM automation_runs WHERE id=?1",
                [run_id],
                map_run,
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("自动化运行不存在: {run_id}"))?;
        enrich_run(&connection, &mut run)?;
        Ok(run)
    }

    pub fn start_run(
        self: &Arc<Self>,
        request: StartAutomationRunRequest,
    ) -> Result<AutomationRun, String> {
        if !self.is_running() {
            return Err("脚本自动化模块未启用".into());
        }
        if !self.accepting_triggers.load(Ordering::SeqCst) {
            return Err("脚本自动化正在切换装配方案，暂不接收新运行".into());
        }
        let (profile, effective) = self.current_profile_and_components()?;
        if !effective.contains(&request.component_id) {
            return Err(format!("组件未进入当前装配方案: {}", request.component_id));
        }
        let manifest = self
            .components
            .manifest(&request.component_id)
            .ok_or_else(|| format!("组件未安装: {}", request.component_id))?;
        self.ensure_component_execution_trusted(&manifest)?;
        let command = automation_command(&manifest, &request.command)?;
        let mut request = request;
        if command.context_requirement == AutomationContextRequirement::Global {
            request.project_path = None;
        }
        if matches!(
            command.context_requirement,
            AutomationContextRequirement::ProjectRequired
        ) && request.project_path.as_deref().is_none_or(str::is_empty)
        {
            return Err("该脚本需要项目上下文，请先打开项目或指定项目".into());
        }
        if request
            .project_path
            .as_deref()
            .is_some_and(|path| !Path::new(path).is_dir())
        {
            return Err("自动化项目上下文目录不存在或不可用".into());
        }
        validate_json_schema_value(&request.input, &command.input_schema, "输入")?;
        let run_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let semantics = semantics_name(command.execution_semantics);
        let max_attempts = command.max_attempts.max(1).min(100);
        let digest = self
            .components
            .content_digest(&manifest.id)
            .unwrap_or_else(|| blake3::hash(manifest.id.as_bytes()).to_hex().to_string());
        let capability_scope = request.capability_scope.unwrap_or_else(|| {
            default_capability_scope(command.required_capability, request.project_path.as_deref())
        });
        let connection = self
            .database
            .lock()
            .expect("automation database mutex poisoned");
        if let Some(limit) = command.max_parallelism {
            let active: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM automation_runs
                     WHERE component_id=?1 AND command=?2
                       AND status IN ('queued','preparing','running','waiting-permission','cancelling')",
                    params![manifest.id, command.command],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if active >= limit as i64 {
                return Err(format!("自动化命令并发上限为 {limit}"));
            }
        }
        connection
            .execute(
                "INSERT INTO automation_runs(
                    id,component_id,component_name,component_version,command,command_name,
                    profile_id,profile_revision,project_path,trigger_kind,trigger_id,status,
                    semantics,attempt,max_attempts,operation_id,content_digest,input_json,
                    output_json,logs_json,error,permission_request_json,manifest_json,
                    capability_scope_json,created_at,started_at,finished_at,updated_at
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'queued',?12,0,?13,NULL,?14,?15,NULL,'[]',NULL,NULL,?16,?17,?18,NULL,NULL,?18)",
                params![
                    run_id,
                    manifest.id,
                    manifest.name,
                    manifest.version,
                    command.command,
                    command.name,
                    profile.id,
                    profile.revision as i64,
                    request.project_path,
                    request.trigger_kind,
                    request.trigger_id,
                    semantics,
                    max_attempts as i64,
                    digest,
                    request.input.to_string(),
                    serde_json::to_string(&manifest).map_err(|error| error.to_string())?,
                    serde_json::to_string(&capability_scope).map_err(|error| error.to_string())?,
                    now,
                ],
            )
            .map_err(|error| error.to_string())?;
        drop(connection);
        let run = self.get_run(&run_id)?;
        self.emit_run(&run);
        self.spawn_prepare(run_id.clone(), None);
        Ok(run)
    }

    fn spawn_prepare(
        self: &Arc<Self>,
        run_id: String,
        granted: Option<(CapabilityTokenRequest, String)>,
    ) {
        let runtime = self.clone();
        tokio::spawn(async move {
            Box::pin(runtime.prepare_and_execute(&run_id, granted)).await;
        });
    }

    async fn prepare_and_execute(
        self: &Arc<Self>,
        run_id: &str,
        granted: Option<(CapabilityTokenRequest, String)>,
    ) {
        if !self.is_running() {
            let _ = self.update_terminal(
                run_id,
                "cancelled",
                None,
                Some("脚本自动化模块未启用".into()),
                Vec::new(),
            );
            return;
        }
        if granted.is_none() {
            if let Err(error) = self.update_status(run_id, "preparing", None, None) {
                eprintln!("[script-automation] prepare {run_id}: {error}");
                return;
            }
        }
        let run = match self.get_run(run_id) {
            Ok(run) => run,
            Err(error) => {
                eprintln!("[script-automation] load {run_id}: {error}");
                return;
            }
        };
        let manifest = match self.components.manifest(&run.component_id) {
            Some(manifest) => manifest,
            None => {
                let _ = self.update_terminal(
                    run_id,
                    "failed",
                    None,
                    Some("组件已卸载，无法执行".into()),
                    Vec::new(),
                );
                return;
            }
        };
        if let Err(error) = self.ensure_component_execution_trusted(&manifest) {
            let _ = self.update_terminal(run_id, "failed", None, Some(error), Vec::new());
            return;
        }
        if self.components.content_digest(&manifest.id).as_deref() != Some(&run.content_digest) {
            let _ = self.update_terminal(
                run_id,
                "attention",
                None,
                Some("组件内容在运行前已变化，已阻止使用不同代码继续".into()),
                Vec::new(),
            );
            return;
        }
        let command = match automation_command(&manifest, &run.command) {
            Ok(command) => command,
            Err(error) => {
                let _ = self.update_terminal(run_id, "failed", None, Some(error), Vec::new());
                return;
            }
        };
        let (capability_request, capability_token) = match granted {
            Some((request, token)) => (Some(request), Some(token)),
            None => match self.request_capability(&run, &command) {
                Ok(Some((request, token))) => (Some(request), Some(token)),
                Ok(None) if command.required_capability.is_some() => return,
                Ok(None) => (None, None),
                Err(error) => {
                    let _ = self.update_terminal(run_id, "failed", None, Some(error), Vec::new());
                    return;
                }
            },
        };
        self.execute_component(
            run_id,
            &run,
            &manifest,
            &command,
            capability_request,
            capability_token,
        )
        .await;
    }

    fn request_capability(
        &self,
        run: &AutomationRun,
        command: &AutomationCommandContribution,
    ) -> Result<Option<(CapabilityTokenRequest, String)>, String> {
        let Some(capability) = command.required_capability else {
            return Ok(None);
        };
        let host = self.host.lock().expect("automation host mutex poisoned");
        let host = host.as_ref().ok_or("脚本自动化宿主尚未初始化")?;
        let scope = self
            .database
            .lock()
            .expect("automation database mutex poisoned")
            .query_row(
                "SELECT capability_scope_json FROM automation_runs WHERE id=?1",
                [&run.id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        let request = CapabilityTokenRequest {
            subject_kind: CapabilitySubjectKind::Component,
            module_id: SCRIPT_AUTOMATION_MODULE_ID.into(),
            component_id: Some(run.component_id.clone()),
            capability,
            operation: capability_operation(command.capability_operation),
            reason: format!("自动化脚本“{}”执行 {}", run.command_name, run.command),
            scope: serde_json::from_str(&scope).unwrap_or_default(),
        };
        let response = host
            .gateway
            .request_token(request.clone())
            .map_err(|error| error.message)?;
        match response.status {
            CapabilityRequestStatus::Granted => {
                Ok(response.token.map(|token| (request, token.value)))
            }
            CapabilityRequestStatus::ApprovalRequired => {
                let approval =
                    serde_json::to_value(response.approval).map_err(|error| error.to_string())?;
                self.database
                    .lock()
                    .expect("automation database mutex poisoned")
                    .execute(
                        "UPDATE automation_runs SET status='waiting-permission', permission_request_json=?2, updated_at=?3 WHERE id=?1",
                        params![run.id, approval.to_string(), Utc::now().timestamp_millis()],
                    )
                    .map_err(|error| error.to_string())?;
                let updated = self.get_run(&run.id)?;
                self.emit_run(&updated);
                let _ = self.app_handle.emit(PERMISSION_EVENT, &updated);
                Ok(None)
            }
            CapabilityRequestStatus::Denied => Err(response.message),
        }
    }

    async fn execute_component(
        self: &Arc<Self>,
        run_id: &str,
        run: &AutomationRun,
        manifest: &ComponentManifestV1,
        command: &AutomationCommandContribution,
        capability_request: Option<CapabilityTokenRequest>,
        capability_token: Option<String>,
    ) {
        if let (Some(request), Some(token)) =
            (capability_request.as_ref(), capability_token.as_deref())
        {
            let consume_result = {
                let host = self.host.lock().expect("automation host mutex poisoned");
                let Some(host) = host.as_ref() else {
                    let _ = self.update_terminal(
                        run_id,
                        "failed",
                        None,
                        Some("脚本自动化宿主尚未初始化".into()),
                        Vec::new(),
                    );
                    return;
                };
                host.gateway.consume_token(token, request)
            };
            if let Err(error) = consume_result {
                let _ =
                    self.update_terminal(run_id, "failed", None, Some(error.message), Vec::new());
                return;
            }
        }
        let operation_id = Uuid::new_v4().to_string();
        let attempt = run.attempt.saturating_add(1);
        if let Err(error) = self.update_running(run_id, &operation_id, attempt) {
            eprintln!("[script-automation] start {run_id}: {error}");
            return;
        }
        let (bridge_endpoint, bridge_token, bridge_shutdown, bridge_handle) = match self
            .start_bridge(
                run.clone(),
                manifest.clone(),
                operation_id.clone(),
                command.required_capability,
            )
            .await
        {
            Ok(bridge) => bridge,
            Err(error) => {
                let _ = self.update_terminal(
                    run_id,
                    "failed",
                    None,
                    Some(format!("启动 Nexora SDK 桥失败: {error}")),
                    Vec::new(),
                );
                return;
            }
        };
        let payload = json!({
            "payload": run.input,
            "nexora": {
                "protocol": "nexora.automation.v1",
                "runId": run.id,
                "operationId": operation_id,
                "attempt": attempt,
                "profileId": run.profile_id,
                "profileRevision": run.profile_revision,
                "projectPath": run.project_path,
                "trigger": { "kind": run.trigger_kind, "id": run.trigger_id },
                "component": { "id": run.component_id, "version": run.component_version },
                "bridge": {
                    "protocol": BRIDGE_PROTOCOL,
                    "endpoint": bridge_endpoint,
                    "token": bridge_token,
                },
            }
        });
        let result = self
            .components
            .invoke(ComponentInvocationRequest {
                component_id: run.component_id.clone(),
                module_id: SCRIPT_AUTOMATION_MODULE_ID.into(),
                command: run.command.clone(),
                input: payload,
                operation_id: Some(operation_id.clone()),
                timeout_ms: command.timeout_ms,
                capability_request,
                capability_token,
            })
            .await;
        let _ = bridge_shutdown.send(());
        if tokio::time::timeout(Duration::from_secs(2), bridge_handle)
            .await
            .is_err()
        {
            let _ = self.cancel_bridged_operations(&operation_id);
        }
        match result {
            Ok(result) => {
                for line in &result.logs {
                    let _ = self
                        .app_handle
                        .emit(LOG_EVENT, json!({ "runId": run_id, "line": line }));
                }
                match validate_json_schema_value(&result.output, &command.output_schema, "输出") {
                    Ok(()) => {
                        let _ = self.update_terminal(
                            run_id,
                            "completed",
                            Some(result.output),
                            None,
                            result.logs,
                        );
                    }
                    Err(error) => {
                        let _ =
                            self.update_terminal(run_id, "failed", None, Some(error), result.logs);
                    }
                }
            }
            Err(error) => {
                let cancelled = matches!(
                    error.code,
                    super::ComponentRuntimeErrorCode::ComponentOperationCancelled
                ) || self
                    .get_run(run_id)
                    .is_ok_and(|current| current.status == AutomationRunStatus::Cancelling);
                if cancelled {
                    let _ = self.update_terminal(
                        run_id,
                        "cancelled",
                        None,
                        Some("运行已取消".into()),
                        error.details,
                    );
                    return;
                }
                let safe_retry = matches!(
                    command.execution_semantics,
                    AutomationExecutionSemantics::Pure | AutomationExecutionSemantics::Idempotent
                ) && attempt < run.max_attempts;
                if safe_retry && self.is_running() {
                    let delay = 5u64
                        .saturating_mul(2u64.saturating_pow((attempt - 1).into()))
                        .min(60);
                    let _ = self.update_status(
                        run_id,
                        "queued",
                        Some(format!("第 {attempt} 次失败，{delay} 秒后重试")),
                        Some(error.details),
                    );
                    let runtime = self.clone();
                    let id = run_id.to_string();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                        runtime.spawn_prepare(id, None);
                    });
                } else {
                    let _ = self.update_terminal(
                        run_id,
                        "failed",
                        None,
                        Some(error.message),
                        error.details,
                    );
                }
            }
        }
    }

    async fn start_bridge(
        self: &Arc<Self>,
        run: AutomationRun,
        manifest: ComponentManifestV1,
        root_operation_id: String,
        granted_capability: Option<Capability>,
    ) -> Result<(String, String, oneshot::Sender<()>, JoinHandle<()>), String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let endpoint = format!("{}:{}", address.ip(), address.port());
        let token = Uuid::new_v4().to_string();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let runtime = self.clone();
        let expected_token = token.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
                        let runtime = runtime.clone();
                        let run = run.clone();
                        let manifest = manifest.clone();
                        let root_operation_id = root_operation_id.clone();
                        let expected_token = expected_token.clone();
                        tokio::spawn(async move {
                            runtime
                                .serve_bridge_connection(
                                    stream,
                                    &run,
                                    &manifest,
                                    &root_operation_id,
                                    granted_capability,
                                    &expected_token,
                                )
                                .await;
                        });
                    }
                }
            }
        });
        Ok((endpoint, token, shutdown_tx, handle))
    }

    async fn serve_bridge_connection(
        self: Arc<Self>,
        stream: TcpStream,
        run: &AutomationRun,
        manifest: &ComponentManifestV1,
        root_operation_id: &str,
        granted_capability: Option<Capability>,
        expected_token: &str,
    ) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        let response = match tokio::time::timeout(
            Duration::from_secs(10),
            reader.read_line(&mut line),
        )
        .await
        {
            Err(_) => Err("SDK 桥请求读取超时".to_string()),
            Ok(Err(error)) => Err(format!("读取 SDK 桥请求失败: {error}")),
            Ok(Ok(0)) => Err("SDK 桥连接未发送请求".to_string()),
            Ok(Ok(_)) if line.len() > MAX_BRIDGE_REQUEST_BYTES => {
                Err("SDK 桥请求超过 1 MiB 限制".to_string())
            }
            Ok(Ok(_)) => match serde_json::from_str::<AutomationBridgeRequest>(line.trim()) {
                Err(error) => Err(format!("SDK 桥请求不是有效 JSON: {error}")),
                Ok(request)
                    if request.protocol != BRIDGE_PROTOCOL || request.token != expected_token =>
                {
                    Err("SDK 桥协议或会话令牌无效".to_string())
                }
                Ok(request) => {
                    self.handle_bridge_request(
                        run,
                        manifest,
                        root_operation_id,
                        granted_capability,
                        request,
                    )
                    .await
                }
            },
        };
        let payload = match response {
            Ok(value) => json!({"ok": true, "result": value}),
            Err(error) => json!({"ok": false, "error": error}),
        };
        if let Ok(mut bytes) = serde_json::to_vec(&payload) {
            bytes.push(b'\n');
            let _ = writer.write_all(&bytes).await;
            let _ = writer.shutdown().await;
        }
    }

    async fn handle_bridge_request(
        &self,
        run: &AutomationRun,
        manifest: &ComponentManifestV1,
        root_operation_id: &str,
        granted_capability: Option<Capability>,
        request: AutomationBridgeRequest,
    ) -> Result<Value, String> {
        let current = self.get_run(&run.id)?;
        if !matches!(
            current.status,
            AutomationRunStatus::Preparing | AutomationRunStatus::Running
        ) {
            return Err("自动化运行已不再接受 SDK 请求".into());
        }
        match request.operation.as_str() {
            "component.invoke" => {
                let component_id = required_bridge_string(&request.payload, "componentId")?;
                let command = required_bridge_string(&request.payload, "command")?;
                validate_bridge_command(&command)?;
                if component_id == manifest.id {
                    return Err("SDK 依赖桥不能递归调用当前组件".into());
                }
                let declared = manifest
                    .requires_components
                    .iter()
                    .chain(manifest.optional_components.iter())
                    .any(|dependency| dependency.id == component_id);
                if !declared {
                    return Err(format!("组件未声明依赖: {component_id}"));
                }
                let (_, effective) = self.current_profile_and_components()?;
                if !effective.contains(&component_id) {
                    return Err(format!(
                        "依赖组件未进入当前 Profile 有效闭包: {component_id}"
                    ));
                }
                let target = self
                    .components
                    .manifest(&component_id)
                    .ok_or_else(|| format!("依赖组件未安装: {component_id}"))?;
                if !target.capabilities.is_empty() {
                    let capability = request
                        .payload
                        .get("capability")
                        .cloned()
                        .ok_or("调用该依赖必须声明 capability")?;
                    let capability = serde_json::from_value::<Capability>(capability)
                        .map_err(|_| "依赖 capability 无效".to_string())?;
                    if granted_capability != Some(capability) {
                        return Err("依赖调用只能使用本次自动化运行已经授权的 capability".into());
                    }
                    if !manifest.capabilities.contains(&capability)
                        || !target.capabilities.contains(&capability)
                    {
                        return Err("调用方或依赖组件未在清单中声明该 capability".into());
                    }
                }
                let operation_id = format!("{root_operation_id}:dependency:{}", Uuid::new_v4());
                self.register_bridged_operation(root_operation_id, &operation_id);
                let result = self
                    .components
                    .invoke(ComponentInvocationRequest {
                        component_id: component_id.clone(),
                        module_id: SCRIPT_AUTOMATION_MODULE_ID.into(),
                        command: command.clone(),
                        input: request.payload.get("input").cloned().unwrap_or(Value::Null),
                        operation_id: Some(operation_id.clone()),
                        timeout_ms: request
                            .payload
                            .get("timeoutMs")
                            .and_then(Value::as_u64)
                            .map(|value| value.clamp(100, 600_000)),
                        capability_request: None,
                        capability_token: None,
                    })
                    .await;
                self.finish_bridged_operation(root_operation_id, &operation_id);
                result
                    .map(|result| {
                        json!({
                            "componentId": result.component_id,
                            "componentVersion": result.component_version,
                            "command": result.command,
                            "output": result.output,
                            "logs": result.logs,
                            "durationMs": result.duration_ms,
                        })
                    })
                    .map_err(|error| format!("依赖组件调用失败: {}", error.message))
            }
            "state.get" => {
                let scope_key = bridge_scope_key(run, &request.payload)?;
                let key = required_bridge_string(&request.payload, "key")?;
                validate_state_key(&key)?;
                let connection = self
                    .database
                    .lock()
                    .expect("automation database mutex poisoned");
                let state = connection
                    .query_row(
                        "SELECT state_json FROM automation_component_state WHERE component_id=?1 AND scope_key=?2",
                        params![run.component_id, scope_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .and_then(|value| serde_json::from_str::<Value>(&value).ok())
                    .unwrap_or_else(|| json!({}));
                Ok(state.get(&key).cloned().unwrap_or(Value::Null))
            }
            "state.set" => {
                let scope_key = bridge_scope_key(run, &request.payload)?;
                let key = required_bridge_string(&request.payload, "key")?;
                validate_state_key(&key)?;
                let value = request.payload.get("value").cloned().unwrap_or(Value::Null);
                let connection = self
                    .database
                    .lock()
                    .expect("automation database mutex poisoned");
                let mut state = connection
                    .query_row(
                        "SELECT state_json FROM automation_component_state WHERE component_id=?1 AND scope_key=?2",
                        params![run.component_id, scope_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .and_then(|stored| serde_json::from_str::<Value>(&stored).ok())
                    .unwrap_or_else(|| json!({}));
                let object = state
                    .as_object_mut()
                    .ok_or("组件状态根值损坏，必须是 JSON 对象")?;
                object.insert(key, value.clone());
                connection
                    .execute(
                        "INSERT INTO automation_component_state(component_id,scope_key,state_json,updated_at)
                         VALUES(?1,?2,?3,?4)
                         ON CONFLICT(component_id,scope_key) DO UPDATE SET state_json=excluded.state_json,updated_at=excluded.updated_at",
                        params![run.component_id, scope_key, state.to_string(), Utc::now().timestamp_millis()],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(value)
            }
            "state.delete" => {
                let scope_key = bridge_scope_key(run, &request.payload)?;
                let key = required_bridge_string(&request.payload, "key")?;
                validate_state_key(&key)?;
                let connection = self
                    .database
                    .lock()
                    .expect("automation database mutex poisoned");
                let stored = connection
                    .query_row(
                        "SELECT state_json FROM automation_component_state WHERE component_id=?1 AND scope_key=?2",
                        params![run.component_id, scope_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                let removed = stored
                    .and_then(|stored| serde_json::from_str::<Value>(&stored).ok())
                    .and_then(|mut state| {
                        let removed = state.as_object_mut()?.remove(&key);
                        let _ = connection.execute(
                            "UPDATE automation_component_state SET state_json=?3,updated_at=?4 WHERE component_id=?1 AND scope_key=?2",
                            params![run.component_id, scope_key, state.to_string(), Utc::now().timestamp_millis()],
                        );
                        removed
                    });
                Ok(removed.unwrap_or(Value::Null))
            }
            "surface.emit" => {
                let surface_id = required_bridge_string(&request.payload, "surfaceId")?;
                if !manifest
                    .contributes
                    .script_surfaces
                    .iter()
                    .any(|surface| surface.id == surface_id)
                {
                    return Err(format!("组件未声明页面: {surface_id}"));
                }
                let event = required_bridge_string(&request.payload, "event")?;
                validate_bridge_command(&event)?;
                let payload = json!({
                    "componentId": run.component_id,
                    "surfaceId": surface_id,
                    "event": event,
                    "payload": request.payload.get("payload").cloned().unwrap_or(Value::Null),
                    "runId": run.id,
                });
                let _ = self.app_handle.emit(SCRIPT_SURFACE_EVENT, &payload);
                Ok(payload)
            }
            _ => Err(format!("SDK 桥不支持操作: {}", request.operation)),
        }
    }

    fn register_bridged_operation(&self, root_operation_id: &str, operation_id: &str) {
        self.bridged_operations
            .lock()
            .expect("automation bridge operation mutex poisoned")
            .entry(root_operation_id.into())
            .or_default()
            .insert(operation_id.into());
    }

    fn finish_bridged_operation(&self, root_operation_id: &str, operation_id: &str) {
        let mut operations = self
            .bridged_operations
            .lock()
            .expect("automation bridge operation mutex poisoned");
        if let Some(children) = operations.get_mut(root_operation_id) {
            children.remove(operation_id);
            if children.is_empty() {
                operations.remove(root_operation_id);
            }
        }
    }

    fn cancel_bridged_operations(&self, root_operation_id: &str) -> Vec<String> {
        let children = self
            .bridged_operations
            .lock()
            .expect("automation bridge operation mutex poisoned")
            .remove(root_operation_id)
            .unwrap_or_default();
        let mut cancelled = Vec::with_capacity(children.len());
        for operation_id in children {
            let _ = self.components.cancel(&operation_id);
            cancelled.push(operation_id);
        }
        cancelled
    }

    fn cancel_component_operation_tree(&self, operation_id: &str) -> Vec<String> {
        let _ = self.components.cancel(operation_id);
        self.cancel_bridged_operations(operation_id)
    }

    pub fn cancel_run(&self, run_id: &str) -> Result<AutomationRun, String> {
        let run = self.get_run(run_id)?;
        if matches!(
            run.status,
            AutomationRunStatus::Completed
                | AutomationRunStatus::Failed
                | AutomationRunStatus::Cancelled
        ) {
            return Ok(run);
        }
        if let Some(operation_id) = &run.operation_id {
            self.update_status(run_id, "cancelling", None, None)?;
            self.cancel_component_operation_tree(operation_id);
        } else {
            self.update_terminal(
                run_id,
                "cancelled",
                None,
                Some("运行在启动前被取消".into()),
                Vec::new(),
            )?;
        }
        self.get_run(run_id)
    }

    pub fn retry_run(self: &Arc<Self>, run_id: &str) -> Result<AutomationRun, String> {
        let run = self.get_run(run_id)?;
        if !matches!(
            run.status,
            AutomationRunStatus::Failed
                | AutomationRunStatus::Cancelled
                | AutomationRunStatus::Attention
        ) {
            return Err("只有失败、取消或需处理的运行可以重试".into());
        }
        if run.execution_semantics == AutomationExecutionSemantics::NonIdempotent {
            return Err(
                "非幂等运行不能自动重放；请确认外部结果后将其标记为失败或保留处理记录".into(),
            );
        }
        if run.status == AutomationRunStatus::Attention {
            self.resolve_attention_record(run_id, "retry-safe")?;
        }
        self.database
            .lock()
            .expect("automation database mutex poisoned")
            .execute(
                "UPDATE automation_runs SET status='queued', operation_id=NULL, output_json=NULL,
                 error=NULL, permission_request_json=NULL, started_at=NULL, finished_at=NULL,
                 progress=NULL, progress_message=NULL, updated_at=?2 WHERE id=?1",
                params![run_id, Utc::now().timestamp_millis()],
            )
            .map_err(|error| error.to_string())?;
        self.spawn_prepare(run_id.to_string(), None);
        self.get_run(run_id)
    }

    pub fn resolve_attention(
        self: &Arc<Self>,
        request: ResolveAutomationAttentionRequest,
    ) -> Result<AutomationRun, String> {
        let run = self.get_run(&request.run_id)?;
        match request.action.as_str() {
            "allowOnce" | "allowAlways" | "deny" => {
                if run.status != AutomationRunStatus::WaitingPermission {
                    return Err("该运行当前没有等待权限请求".into());
                }
                let request_id = run
                    .permission_request
                    .as_ref()
                    .and_then(|value| value.get("requestId"))
                    .and_then(Value::as_str)
                    .ok_or("权限请求缺少 requestId")?;
                let decision = match request.action.as_str() {
                    "allowOnce" => CapabilityDecision::AllowOnce,
                    "allowAlways" => CapabilityDecision::AllowAlways,
                    _ => CapabilityDecision::Deny,
                };
                let host = self.host.lock().expect("automation host mutex poisoned");
                let host = host.as_ref().ok_or("脚本自动化宿主尚未初始化")?;
                let response = host
                    .gateway
                    .decide(CapabilityDecisionRequest {
                        request_id: request_id.into(),
                        decision,
                    })
                    .map_err(|error| error.message)?;
                if response.status == CapabilityRequestStatus::Denied {
                    self.update_terminal(
                        &run.id,
                        "failed",
                        None,
                        Some("用户拒绝了脚本所需权限".into()),
                        Vec::new(),
                    )?;
                } else {
                    let token = response.token.ok_or("权限网关未返回令牌")?.value;
                    let manifest = self
                        .components
                        .manifest(&run.component_id)
                        .ok_or("组件已卸载")?;
                    let command = automation_command(&manifest, &run.command)?;
                    let capability = command.required_capability.ok_or("命令未声明权限")?;
                    let scope_json = self
                        .database
                        .lock()
                        .expect("automation database mutex poisoned")
                        .query_row(
                            "SELECT capability_scope_json FROM automation_runs WHERE id=?1",
                            [&run.id],
                            |row| row.get::<_, String>(0),
                        )
                        .map_err(|error| error.to_string())?;
                    let capability_request = CapabilityTokenRequest {
                        subject_kind: CapabilitySubjectKind::Component,
                        module_id: SCRIPT_AUTOMATION_MODULE_ID.into(),
                        component_id: Some(run.component_id.clone()),
                        capability,
                        operation: capability_operation(command.capability_operation),
                        reason: format!("自动化脚本“{}”执行 {}", run.command_name, run.command),
                        scope: serde_json::from_str(&scope_json).unwrap_or_default(),
                    };
                    self.database.lock().expect("automation database mutex poisoned").execute(
                        "UPDATE automation_runs SET status='queued', permission_request_json=NULL, updated_at=?2 WHERE id=?1",
                        params![run.id, Utc::now().timestamp_millis()],
                    ).map_err(|error| error.to_string())?;
                    self.spawn_prepare(run.id.clone(), Some((capability_request, token)));
                }
            }
            "retrySafe" => return self.retry_run(&request.run_id),
            "markFailed" => {
                self.resolve_attention_record(&request.run_id, "mark-failed")?;
                self.update_terminal(
                    &request.run_id,
                    "failed",
                    None,
                    Some("用户将结果不确定的运行标记为失败".into()),
                    Vec::new(),
                )?;
            }
            "cancel" => {
                if run.status == AutomationRunStatus::Attention {
                    self.resolve_attention_record(&request.run_id, "cancel")?;
                }
                return self.cancel_run(&request.run_id);
            }
            _ => return Err("未知的自动化处理动作".into()),
        }
        self.get_run(&request.run_id)
    }

    pub fn emit_event(self: &Arc<Self>, request: AutomationEventRequest) -> Result<usize, String> {
        if !self.is_running() {
            return Ok(0);
        }
        if !self.accepting_triggers.load(Ordering::SeqCst) {
            return Ok(0);
        }
        validate_event_name(&request.event)?;
        if let Some(project_path) = request
            .project_path
            .as_ref()
            .filter(|path| !path.is_empty())
        {
            let mut open_projects = self
                .open_projects
                .lock()
                .expect("automation project mutex poisoned");
            match request.event.as_str() {
                "project.opened" => {
                    open_projects.insert(project_path.clone());
                }
                "project.closed" => {
                    open_projects.remove(project_path);
                }
                _ => {}
            }
        }
        let dedupe_key = request
            .dedupe_key
            .unwrap_or_else(|| format!("{}:{}", request.event, Uuid::new_v4()));
        let inserted = self
            .database
            .lock()
            .expect("automation database mutex poisoned")
            .execute(
                "INSERT OR IGNORE INTO automation_events(event_id,event_name,dedupe_key,payload_json,occurred_at)
                 VALUES(?1,?2,?3,?4,?5)",
                params![
                    Uuid::new_v4().to_string(),
                    request.event,
                    dedupe_key,
                    request.payload.to_string(),
                    Utc::now().timestamp_millis()
                ],
            )
            .map_err(|error| error.to_string())?;
        if inserted == 0 {
            return Ok(0);
        }
        let profile = self.current_profile()?;
        let bindings = profile
            .automation_bindings
            .iter()
            .filter(|binding| {
                binding.enabled
                    && matches!(
                        &binding.trigger,
                        AutomationTriggerBinding::Event { event } if event == &request.event
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut started = 0;
        for binding in bindings {
            let project_path =
                resolve_binding_project(&profile, &binding, request.project_path.as_deref());
            let input = merge_binding_input(binding.input.clone(), &request.payload);
            match self.start_run(StartAutomationRunRequest {
                component_id: binding.component_id.clone(),
                command: binding.command.clone(),
                input: input.clone(),
                project_path: project_path.clone(),
                trigger_kind: "event".into(),
                trigger_id: Some(request.event.clone()),
                capability_scope: None,
            }) {
                Ok(_) => started += 1,
                Err(error) => {
                    let _ = self.record_trigger_failure(
                        &profile,
                        &binding,
                        input,
                        project_path,
                        "event",
                        Some(request.event.clone()),
                        error,
                    );
                }
            }
        }
        Ok(started)
    }

    fn process_schedules(self: &Arc<Self>) -> Result<(), String> {
        if !self.is_running() {
            return Ok(());
        }
        if !self.accepting_triggers.load(Ordering::SeqCst) {
            return Ok(());
        }
        let now = chrono::Local::now();
        let minute_key = now.format("%Y-%m-%dT%H:%M").to_string();
        let profile = self.current_profile()?;
        let bindings = profile
            .automation_bindings
            .iter()
            .filter(|binding| binding.enabled)
            .filter_map(|binding| match &binding.trigger {
                AutomationTriggerBinding::Schedule { cron } if cron_matches(cron, now) => {
                    Some(binding.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for binding in bindings {
            let project_paths =
                if binding.project_context == AutomationProjectContext::EachOpenProject {
                    self.open_projects
                        .lock()
                        .expect("automation project mutex poisoned")
                        .iter()
                        .cloned()
                        .map(Some)
                        .collect::<Vec<_>>()
                } else {
                    vec![resolve_binding_project(&profile, &binding, None)]
                };
            for project_path in project_paths {
                let context_key = project_path.as_deref().unwrap_or("global");
                let inserted = self.database.lock().expect("automation database mutex poisoned").execute(
                    "INSERT OR IGNORE INTO automation_schedule_ticks(profile_id,binding_id,minute_key,context_key,triggered_at)
                     VALUES(?1,?2,?3,?4,?5)",
                    params![profile.id, binding.id, minute_key, context_key, Utc::now().timestamp_millis()],
                ).map_err(|error| error.to_string())?;
                if inserted == 0 {
                    continue;
                }
                let input = binding.input.clone();
                if let Err(error) = self.start_run(StartAutomationRunRequest {
                    component_id: binding.component_id.clone(),
                    command: binding.command.clone(),
                    input: input.clone(),
                    project_path: project_path.clone(),
                    trigger_kind: "schedule".into(),
                    trigger_id: Some(binding.id.clone()),
                    capability_scope: None,
                }) {
                    let _ = self.record_trigger_failure(
                        &profile,
                        &binding,
                        input,
                        project_path,
                        "schedule",
                        Some(binding.id.clone()),
                        error,
                    );
                }
            }
        }
        Ok(())
    }

    fn record_trigger_failure(
        &self,
        profile: &WorkspaceProfileV1,
        binding: &ProfileAutomationBinding,
        input: Value,
        project_path: Option<String>,
        trigger_kind: &str,
        trigger_id: Option<String>,
        error: String,
    ) -> Result<(), String> {
        let manifest = self.components.manifest(&binding.component_id);
        let command = manifest
            .as_ref()
            .and_then(|manifest| automation_command(manifest, &binding.command).ok());
        let now = Utc::now().timestamp_millis();
        let run_id = Uuid::new_v4().to_string();
        let component_name = manifest
            .as_ref()
            .map(|manifest| manifest.name.clone())
            .unwrap_or_else(|| binding.component_id.clone());
        let component_version = manifest
            .as_ref()
            .map(|manifest| manifest.version.clone())
            .unwrap_or_else(|| "unknown".into());
        let command_name = command
            .as_ref()
            .map(|command| command.name.clone())
            .unwrap_or_else(|| binding.command.clone());
        let semantics = command
            .as_ref()
            .map(|command| semantics_name(command.execution_semantics))
            .unwrap_or("non-idempotent");
        let max_attempts = command
            .as_ref()
            .map(|command| command.max_attempts.max(1))
            .unwrap_or(1);
        let digest = self
            .components
            .content_digest(&binding.component_id)
            .unwrap_or_else(|| {
                blake3::hash(binding.component_id.as_bytes())
                    .to_hex()
                    .to_string()
            });
        let manifest_json = manifest
            .as_ref()
            .and_then(|manifest| serde_json::to_string(manifest).ok())
            .unwrap_or_else(|| "{}".into());
        self.database
            .lock()
            .expect("automation database mutex poisoned")
            .execute(
                "INSERT INTO automation_runs(
                    id,component_id,component_name,component_version,command,command_name,
                    profile_id,profile_revision,project_path,trigger_kind,trigger_id,status,
                    semantics,attempt,max_attempts,operation_id,content_digest,input_json,
                    output_json,logs_json,error,permission_request_json,manifest_json,
                    capability_scope_json,created_at,started_at,finished_at,updated_at
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'failed',?12,0,?13,NULL,?14,?15,NULL,'[]',?16,NULL,?17,'{}',?18,NULL,?18,?18)",
                params![
                    run_id,
                    binding.component_id,
                    component_name,
                    component_version,
                    binding.command,
                    command_name,
                    profile.id,
                    profile.revision as i64,
                    project_path,
                    trigger_kind,
                    trigger_id,
                    semantics,
                    max_attempts as i64,
                    digest,
                    input.to_string(),
                    error,
                    manifest_json,
                    now,
                ],
            )
            .map_err(|error| error.to_string())?;
        let run = self.get_run(&run_id)?;
        self.emit_run(&run);
        Ok(())
    }

    pub fn validate_development_component(&self, source_path: &str) -> ScriptComponentValidation {
        let mut report = ScriptComponentValidation {
            valid: false,
            source_path: source_path.into(),
            trusted: self.is_trusted_directory(source_path),
            content_digest: None,
            manifest: None,
            warnings: vec![
                "Python 组件属于受信任代码，仍拥有当前 Windows 用户的系统权限。".into(),
                "Capability 只约束 Nexora SDK 与宿主接口，不能沙箱 Python 标准库。".into(),
            ],
            errors: Vec::new(),
        };
        let source = match fs::canonicalize(source_path) {
            Ok(path) if path.is_dir() => path,
            Ok(_) => {
                report.errors.push("选择的位置不是目录".into());
                return report;
            }
            Err(error) => {
                report.errors.push(format!("目录不可用: {error}"));
                return report;
            }
        };
        let text = match fs::read_to_string(source.join("component.json")) {
            Ok(text) => text,
            Err(error) => {
                report
                    .errors
                    .push(format!("无法读取 component.json: {error}"));
                return report;
            }
        };
        let manifest = match parse_component_manifest(&text) {
            Ok(manifest) => manifest,
            Err(error) => {
                report
                    .errors
                    .push(format!("{}（{}）", error.message, error.path));
                return report;
            }
        };
        if manifest.contributes.automation_commands.is_empty()
            && manifest.contributes.script_surfaces.is_empty()
        {
            report
                .errors
                .push("组件没有 automationCommands 或 scriptSurfaces".into());
        }
        if let Some(entry) = manifest.entry.as_deref() {
            if !source.join(entry).is_file() {
                report.errors.push(format!("运行入口不存在: {entry}"));
            }
        }
        for surface in &manifest.contributes.script_surfaces {
            if !source.join(&surface.entry).is_file() {
                report
                    .errors
                    .push(format!("脚本页面入口不存在: {}", surface.entry));
            }
        }
        report.content_digest = hash_directory(&source).ok();
        report.valid = report.errors.is_empty();
        report.manifest = Some(manifest);
        report
    }

    pub fn trust_development_directory(&self, source_path: &str) -> Result<Vec<String>, String> {
        let validation = self.validate_development_component(source_path);
        if !validation.valid {
            return Err(validation.errors.join("；"));
        }
        let canonical = fs::canonicalize(source_path)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .into_owned();
        let mut state = self
            .trusted_directories
            .lock()
            .expect("script trust mutex poisoned");
        if !state
            .directories
            .iter()
            .any(|path| path.eq_ignore_ascii_case(&canonical))
        {
            state.directories.push(canonical);
            state.directories.sort();
            persist_json(&self.trust_path, &*state)?;
        }
        Ok(state.directories.clone())
    }

    pub fn untrust_development_directory(&self, source_path: &str) -> Result<Vec<String>, String> {
        let canonical = fs::canonicalize(source_path)
            .unwrap_or_else(|_| PathBuf::from(source_path))
            .to_string_lossy()
            .into_owned();
        let mut state = self
            .trusted_directories
            .lock()
            .expect("script trust mutex poisoned");
        state
            .directories
            .retain(|path| !path.eq_ignore_ascii_case(&canonical));
        persist_json(&self.trust_path, &*state)?;
        Ok(state.directories.clone())
    }

    pub fn trusted_directories(&self) -> Vec<String> {
        self.trusted_directories
            .lock()
            .expect("script trust mutex poisoned")
            .directories
            .clone()
    }

    pub fn is_trusted_directory(&self, source_path: &str) -> bool {
        let canonical = fs::canonicalize(source_path)
            .unwrap_or_else(|_| PathBuf::from(source_path))
            .to_string_lossy()
            .into_owned();
        self.trusted_directories
            .lock()
            .expect("script trust mutex poisoned")
            .directories
            .iter()
            .any(|path| path.eq_ignore_ascii_case(&canonical))
    }

    fn ensure_component_execution_trusted(
        &self,
        manifest: &ComponentManifestV1,
    ) -> Result<(), String> {
        if !matches!(
            manifest.runtime,
            ComponentRuntime::PythonAction | ComponentRuntime::PythonWorker
        ) || self.components.install_source(&manifest.id) != Some(ComponentInstallSource::Local)
        {
            return Ok(());
        }
        let installed_root = self
            .components
            .package_root(&manifest.id)
            .ok_or("无法读取本地 Python 组件安装目录")?;
        let installed_digest = hash_directory(&installed_root)?;
        let trusted = self.trusted_directories().into_iter().any(|path| {
            let validation = self.validate_development_component(&path);
            validation.valid
                && validation.trusted
                && validation.manifest.as_ref().is_some_and(|candidate| {
                    candidate.id == manifest.id && candidate.version == manifest.version
                })
                && validation.content_digest.as_deref() == Some(installed_digest.as_str())
        });
        if trusted {
            Ok(())
        } else {
            Err("本地 Python 组件没有匹配的受信任开发目录，或目录内容已变化；请重新校验、信任并热重载".into())
        }
    }

    pub fn create_template(
        &self,
        request: CreateScriptComponentTemplateRequest,
    ) -> Result<String, String> {
        validate_component_id(&request.component_id)?;
        if request.name.trim().is_empty() {
            return Err("组件名称不能为空".into());
        }
        let root = PathBuf::from(&request.parent_path).join(&request.component_id);
        if root.exists() {
            return Err("目标组件目录已经存在".into());
        }
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let mut contributes = json!({
            "automationCommands": [{
                "id": format!("{}.run", request.component_id),
                "command": "run",
                "name": "运行",
                "description": "执行脚本组件示例命令。",
                "contextRequirement": "either",
                "executionSemantics": "idempotent",
                "maxAttempts": 2,
                "maxParallelism": 1,
                "inputSchema": {"type":"object"},
                "outputSchema": {"type":"object"}
            }],
            "automationEvents": [
                {
                    "id": format!("{}.app-started", request.component_id),
                    "event": "app.started",
                    "name": "应用启动完成"
                },
                {
                    "id": format!("{}.file-changed", request.component_id),
                    "event": "file.changed",
                    "name": "项目文件变化"
                }
            ]
        });
        if request.include_surface {
            contributes["scriptSurfaces"] = json!([{
                "id": format!("{}.surface", request.component_id),
                "name": request.name,
                "entry": "ui/index.html",
                "placements": ["workspace", "dialog"],
                "allowedCommands": ["run"]
            }]);
        }
        let manifest = json!({
            "schemaVersion": 1,
            "id": request.component_id,
            "name": request.name,
            "description": "由 Nexora 脚本开发者工作台创建。",
            "version": "0.1.0",
            "apiVersion": "1",
            "runtime": "python-action",
            "role": "feature",
            "distribution": "local",
            "uiMode": if request.include_surface { "contributed" } else { "none" },
            "platforms": ["windows-x64"],
            "entry": "main.py",
            "capabilities": [],
            "requiresComponents": [],
            "contributes": contributes,
            "resources": {"maxMemoryMb": 512, "maxParallelism": 1, "timeoutMs": 300000}
        });
        fs::write(
            root.join("component.json"),
            serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            root.join("main.py"),
            include_str!("../../resources/script-sdk/template_main.py"),
        )
        .map_err(|error| error.to_string())?;
        if request.include_surface {
            fs::create_dir_all(root.join("ui")).map_err(|error| error.to_string())?;
            fs::write(
                root.join("ui/index.html"),
                include_str!("../../resources/script-sdk/template_surface.html"),
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(root.to_string_lossy().into_owned())
    }

    pub fn list_development_files(
        &self,
        source_path: &str,
    ) -> Result<Vec<ScriptDevelopmentFile>, String> {
        let root = canonical_development_root(source_path)?;
        let mut files = WalkDir::new(&root)
            .follow_links(false)
            .max_depth(8)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .filter_map(|entry| {
                let relative = entry.path().strip_prefix(&root).ok()?;
                if relative.components().any(|part| {
                    matches!(part, Component::Normal(name) if name == "__pycache__" || name == ".git" || name == "target")
                }) {
                    return None;
                }
                let extension = entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if !matches!(extension.as_str(), "json" | "py" | "html" | "css" | "js" | "md" | "txt") {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                Some(ScriptDevelopmentFile {
                    path: relative.to_string_lossy().replace('\\', "/"),
                    size_bytes: metadata.len(),
                    modified_at: metadata
                        .modified()
                        .ok()
                        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
                        .unwrap_or_default(),
                })
            })
            .collect::<Vec<_>>();
        files.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(files)
    }

    pub fn read_development_file(
        &self,
        source_path: &str,
        relative_path: &str,
    ) -> Result<ScriptDevelopmentDocument, String> {
        let root = canonical_development_root(source_path)?;
        let path = safe_join(&root, relative_path)?;
        let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
            return Err("开发文件不存在或超过 2 MB 编辑上限".into());
        }
        let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        Ok(ScriptDevelopmentDocument {
            path: relative_path.replace('\\', "/"),
            content_digest: blake3::hash(content.as_bytes()).to_hex().to_string(),
            content,
        })
    }

    pub fn save_development_file(
        &self,
        request: SaveScriptDevelopmentFileRequest,
    ) -> Result<ScriptDevelopmentDocument, String> {
        if request.content.len() > 2 * 1024 * 1024 {
            return Err("开发文件超过 2 MB 编辑上限".into());
        }
        let root = canonical_development_root(&request.source_path)?;
        let path = safe_join(&root, &request.relative_path)?;
        let current = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let current_digest = blake3::hash(current.as_bytes()).to_hex().to_string();
        if request
            .expected_content_digest
            .as_deref()
            .is_some_and(|expected| expected != current_digest)
        {
            return Err("文件已被其他编辑或热重载修改，请重新加载后再保存".into());
        }
        let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
        fs::write(&temporary, request.content.as_bytes()).map_err(|error| error.to_string())?;
        if path.exists() {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
        fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
        self.read_development_file(&request.source_path, &request.relative_path)
    }

    pub fn script_surface_document(
        &self,
        component_id: &str,
        surface_id: &str,
    ) -> Result<ScriptSurfaceDocument, String> {
        let (_, effective) = self.current_profile_and_components()?;
        if !effective.contains(component_id) {
            return Err("脚本页面所属组件未进入当前装配方案".into());
        }
        let manifest = self.components.manifest(component_id).ok_or("组件未安装")?;
        let surface = manifest
            .contributes
            .script_surfaces
            .iter()
            .find(|surface| surface.id == surface_id)
            .ok_or("脚本页面不存在")?;
        let root = self
            .components
            .package_root(component_id)
            .ok_or("脚本页面组件没有安装目录")?;
        let entry = safe_join(&root, &surface.entry)?;
        let source = fs::read_to_string(&entry).map_err(|error| error.to_string())?;
        if source.to_ascii_lowercase().contains("<base") {
            return Err("脚本页面不能声明 base 标签".into());
        }
        let nonce = Uuid::new_v4().simple().to_string();
        let bridge = format!(
            r#"<script nonce="{nonce}">
window.nexora = Object.freeze({{
  invoke(command, input = {{}}) {{
    return new Promise((resolve, reject) => {{
      const requestId = crypto.randomUUID();
      const listener = (event) => {{
        const data = event.data || {{}};
        if (data.type !== 'nexora-script-result' || data.requestId !== requestId) return;
        window.removeEventListener('message', listener);
        data.ok ? resolve(data.result) : reject(new Error(data.error || '调用失败'));
      }};
      window.addEventListener('message', listener);
      parent.postMessage({{type:'nexora-script-invoke', nonce:'{nonce}', requestId, command, input}}, '*');
    }});
  }},
  onEvent(listener) {{
    if (typeof listener !== 'function') throw new TypeError('listener must be a function');
    const handler = (event) => {{
      const data = event.data || {{}};
      if (data.type === 'nexora-script-event' && data.nonce === '{nonce}') listener(data.event);
    }};
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }}
}});
</script>"#
        );
        let csp = format!(
            r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data: blob:; style-src 'unsafe-inline'; script-src 'nonce-{nonce}'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'">"#
        );
        let resource_root = entry.parent().ok_or("脚本页面入口缺少父目录")?;
        let source = prepare_script_surface_source(resource_root, &source, &nonce)?;
        let html = if let Some(index) = source.to_ascii_lowercase().find("</head>") {
            format!("{}{}{}{}", &source[..index], csp, bridge, &source[index..])
        } else {
            format!("<!doctype html><html><head>{csp}{bridge}</head><body>{source}</body></html>")
        };
        Ok(ScriptSurfaceDocument {
            component_id: component_id.into(),
            surface_id: surface_id.into(),
            title: surface.name.clone(),
            nonce,
            allowed_commands: surface.allowed_commands.clone(),
            html,
        })
    }

    pub fn has_unsafe_active_run_for_component(&self, component_id: &str) -> Result<bool, String> {
        let count: i64 = self
            .database
            .lock()
            .expect("automation database mutex poisoned")
            .query_row(
                "SELECT COUNT(*) FROM automation_runs WHERE component_id=?1
                 AND semantics='non-idempotent' AND status IN ('preparing','running','cancelling')",
                [component_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(count > 0)
    }

    pub async fn prepare_component_uninstall(&self, component_id: &str) -> Result<(), String> {
        if self.has_unsafe_active_run_for_component(component_id)? {
            return Err(
                "组件仍有不可安全重放的非幂等脚本运行，请等待完成或在任务中心处理后再卸载".into(),
            );
        }
        let active = {
            let connection = self
                .database
                .lock()
                .expect("automation database mutex poisoned");
            let mut statement = connection
                .prepare(
                    "SELECT id,operation_id FROM automation_runs
                     WHERE component_id=?1
                       AND status IN ('queued','preparing','running','waiting-permission','cancelling')",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([component_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            rows
        };
        let mut operation_ids = Vec::new();
        for (run_id, operation_id) in active {
            if let Some(operation_id) = operation_id {
                operation_ids.push(operation_id.clone());
                operation_ids.extend(self.cancel_component_operation_tree(&operation_id));
            }
            self.update_terminal(
                &run_id,
                "cancelled",
                None,
                Some("组件卸载前已安全取消".into()),
                Vec::new(),
            )?;
        }
        for _ in 0..50 {
            if self.components.active_operation_count(component_id) == 0
                && operation_ids
                    .iter()
                    .all(|operation_id| !self.components.is_operation_active(operation_id))
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err("等待脚本组件进程退出超时，未执行卸载".into())
    }

    pub fn has_unsafe_active_runs(&self) -> Result<bool, String> {
        let count: i64 = self
            .database
            .lock()
            .expect("automation database mutex poisoned")
            .query_row(
                "SELECT COUNT(*) FROM automation_runs WHERE semantics='non-idempotent'
                 AND status IN ('preparing','running','cancelling')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(count > 0)
    }

    pub async fn prepare_profile_switch(
        self: &Arc<Self>,
    ) -> Result<AutomationTriggerIntakeGuard, String> {
        self.accepting_triggers.store(false, Ordering::SeqCst);
        let guard = AutomationTriggerIntakeGuard {
            runtime: self.clone(),
        };
        if self.has_unsafe_active_runs()? {
            return Err(
                "存在不可安全取消的非幂等脚本运行，请等待完成或在任务中心处理后再切换装配方案"
                    .into(),
            );
        }
        let active = {
            let connection = self
                .database
                .lock()
                .expect("automation database mutex poisoned");
            let mut statement = connection
                .prepare(
                    "SELECT id,operation_id FROM automation_runs
                     WHERE status IN ('queued','preparing','running','waiting-permission','cancelling')",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            rows
        };
        let mut operation_ids = active
            .iter()
            .filter_map(|(_, operation_id)| operation_id.clone())
            .collect::<Vec<_>>();
        for (run_id, operation_id) in active {
            if let Some(operation_id) = operation_id {
                operation_ids.extend(self.cancel_component_operation_tree(&operation_id));
            }
            self.update_terminal(
                &run_id,
                "cancelled",
                None,
                Some("装配方案切换前已安全取消".into()),
                Vec::new(),
            )?;
        }
        for _ in 0..50 {
            if operation_ids
                .iter()
                .all(|operation_id| !self.components.is_operation_active(operation_id))
            {
                return Ok(guard);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        return Err("等待脚本组件进程退出超时，未切换装配方案".into());
    }

    fn current_profile(&self) -> Result<WorkspaceProfileV1, String> {
        self.current_profile_and_components()
            .map(|(profile, _)| profile)
    }

    fn current_profile_and_components(
        &self,
    ) -> Result<(WorkspaceProfileV1, BTreeSet<String>), String> {
        let host = self.host.lock().expect("automation host mutex poisoned");
        let host = host.as_ref().ok_or("脚本自动化宿主尚未初始化")?;
        let manifests = host
            .manager
            .overview()
            .modules
            .into_iter()
            .filter(|module| !module.diagnostic)
            .map(|module| module.manifest)
            .collect::<Vec<_>>();
        let (profile_id, effective) = host
            .profiles
            .current_effective_component_ids(&manifests)
            .map_err(|error| error.message)?;
        let profile = host
            .profiles
            .profile_document(&profile_id)
            .map_err(|error| error.message)?;
        Ok((profile, effective))
    }

    fn update_running(&self, run_id: &str, operation_id: &str, attempt: u16) -> Result<(), String> {
        let now = Utc::now().timestamp_millis();
        let connection = self
            .database
            .lock()
            .expect("automation database mutex poisoned");
        connection
            .execute(
                "UPDATE automation_runs SET status='running', operation_id=?2, attempt=?3,
                 started_at=COALESCE(started_at,?4), updated_at=?4, error=NULL,
                 permission_request_json=NULL, progress=0, progress_message=NULL WHERE id=?1",
                params![run_id, operation_id, attempt as i64, now],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO automation_run_attempts(
                   run_id,attempt,operation_id,status,logs_json,error,started_at,finished_at
                 ) VALUES(?1,?2,?3,'running','[]',NULL,?4,NULL)
                 ON CONFLICT(run_id,attempt) DO UPDATE SET
                   operation_id=excluded.operation_id,status='running',logs_json='[]',error=NULL,
                   started_at=excluded.started_at,finished_at=NULL",
                params![run_id, attempt as i64, operation_id, now],
            )
            .map_err(|error| error.to_string())?;
        drop(connection);
        let run = self.get_run(run_id)?;
        self.emit_run(&run);
        Ok(())
    }

    fn update_status(
        &self,
        run_id: &str,
        status: &str,
        error: Option<String>,
        logs: Option<Vec<String>>,
    ) -> Result<(), String> {
        let logs_json =
            logs.map(|value| serde_json::to_string(&value).unwrap_or_else(|_| "[]".into()));
        let connection = self
            .database
            .lock()
            .expect("automation database mutex poisoned");
        connection
            .execute(
                "UPDATE automation_runs SET status=?2, error=COALESCE(?3,error),
                 logs_json=COALESCE(?4,logs_json), updated_at=?5,
                 progress=CASE WHEN ?2='queued' THEN NULL ELSE progress END,
                 progress_message=CASE WHEN ?2='queued' THEN NULL ELSE progress_message END
                 WHERE id=?1",
                params![
                    run_id,
                    status,
                    error,
                    logs_json,
                    Utc::now().timestamp_millis()
                ],
            )
            .map_err(|error| error.to_string())?;
        if status == "queued" {
            connection
                .execute(
                    "UPDATE automation_run_attempts SET status='failed', logs_json=COALESCE(?2,logs_json),
                     error=COALESCE(?3,error), finished_at=?4
                     WHERE run_id=?1 AND attempt=(SELECT attempt FROM automation_runs WHERE id=?1)",
                    params![run_id, logs_json, error, Utc::now().timestamp_millis()],
                )
                .map_err(|error| error.to_string())?;
        } else if status == "cancelling" {
            connection
                .execute(
                    "UPDATE automation_run_attempts SET status='cancelling'
                     WHERE run_id=?1 AND attempt=(SELECT attempt FROM automation_runs WHERE id=?1)",
                    [run_id],
                )
                .map_err(|error| error.to_string())?;
        }
        drop(connection);
        let run = self.get_run(run_id)?;
        self.emit_run(&run);
        Ok(())
    }

    fn update_terminal(
        &self,
        run_id: &str,
        status: &str,
        output: Option<Value>,
        error: Option<String>,
        logs: Vec<String>,
    ) -> Result<(), String> {
        let now = Utc::now().timestamp_millis();
        let connection = self
            .database
            .lock()
            .expect("automation database mutex poisoned");
        connection
            .execute(
                "UPDATE automation_runs SET status=?2, output_json=?3, logs_json=?4, error=?5,
                 operation_id=NULL, permission_request_json=NULL, finished_at=?6, updated_at=?6,
                 progress=CASE WHEN ?2='completed' THEN 1 ELSE progress END
                 WHERE id=?1",
                params![
                    run_id,
                    status,
                    output.map(|value| value.to_string()),
                    serde_json::to_string(&logs).unwrap_or_else(|_| "[]".into()),
                    error,
                    now
                ],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE automation_run_attempts SET status=?2, logs_json=?3, error=?4, finished_at=?5
                 WHERE run_id=?1 AND attempt=(SELECT attempt FROM automation_runs WHERE id=?1)",
                params![
                    run_id,
                    status,
                    serde_json::to_string(&logs).unwrap_or_else(|_| "[]".into()),
                    error,
                    now
                ],
            )
            .map_err(|error| error.to_string())?;
        if status == "attention" {
            connection
                .execute(
                    "INSERT INTO automation_attention(id,run_id,kind,message,status,resolution,created_at,resolved_at)
                     VALUES(?1,?2,'uncertain-result',?3,'open',NULL,?4,NULL)",
                    params![
                        Uuid::new_v4().to_string(),
                        run_id,
                        error.as_deref().unwrap_or("非幂等操作结果未知"),
                        now
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        drop(connection);
        let run = self.get_run(run_id)?;
        self.emit_run(&run);
        if run.status == AutomationRunStatus::Attention {
            let _ = self.app_handle.emit(ATTENTION_EVENT, &run);
        }
        Ok(())
    }

    fn emit_run(&self, run: &AutomationRun) {
        let _ = self.app_handle.emit(RUN_EVENT, run);
        if run.trigger_kind == "surface" {
            if let Some(surface_id) = &run.trigger_id {
                let _ = self.app_handle.emit(
                    SCRIPT_SURFACE_EVENT,
                    json!({
                        "componentId": run.component_id,
                        "surfaceId": surface_id,
                        "event": "run.changed",
                        "payload": { "run": run },
                        "runId": run.id,
                    }),
                );
            }
        }
    }

    fn update_progress_for_operation(
        &self,
        operation_id: &str,
        progress: f64,
        message: Option<String>,
    ) -> Result<(), String> {
        let run_id = {
            let connection = self
                .database
                .lock()
                .expect("automation database mutex poisoned");
            let run_id = connection
                .query_row(
                    "SELECT id FROM automation_runs WHERE operation_id=?1 AND status='running'",
                    [operation_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            let Some(run_id) = run_id else {
                return Ok(());
            };
            connection
                .execute(
                    "UPDATE automation_runs SET progress=?2,progress_message=?3,updated_at=?4 WHERE id=?1",
                    params![
                        run_id,
                        progress.clamp(0.0, 1.0),
                        message,
                        Utc::now().timestamp_millis()
                    ],
                )
                .map_err(|error| error.to_string())?;
            run_id
        };
        let run = self.get_run(&run_id)?;
        self.emit_run(&run);
        Ok(())
    }

    fn resolve_attention_record(&self, run_id: &str, resolution: &str) -> Result<(), String> {
        self.database
            .lock()
            .expect("automation database mutex poisoned")
            .execute(
                "UPDATE automation_attention SET status='resolved',resolution=?2,resolved_at=?3
                 WHERE id=(
                   SELECT id FROM automation_attention
                   WHERE run_id=?1 AND status='open' ORDER BY created_at DESC LIMIT 1
                 )",
                params![run_id, resolution, Utc::now().timestamp_millis()],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn resume_queued_runs(self: &Arc<Self>) {
        let run_ids = {
            let connection = self
                .database
                .lock()
                .expect("automation database mutex poisoned");
            let mut statement = match connection
                .prepare("SELECT id FROM automation_runs WHERE status='queued' ORDER BY created_at")
            {
                Ok(statement) => statement,
                Err(_) => return,
            };
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .collect::<Vec<_>>()
        };
        for run_id in run_ids {
            self.spawn_prepare(run_id, None);
        }
    }

    fn install_event_forwarders(self: &Arc<Self>) {
        let mappings = [
            ("pm-center:project-fs-change", "file.changed"),
            ("task-completed", "task.completed"),
            ("task-error", "task.failed"),
            ("pm-center:lan-transfer-changed", "lan.file-received"),
            ("pm-center:render-batch-created", "render.batch-created"),
            ("pm-center:render-batch-completed", "render.batch-completed"),
            ("pm-center:render-job-progress", "render.job-completed"),
        ];
        let mut ids = self
            .listeners
            .lock()
            .expect("automation listeners mutex poisoned");
        let weak: Weak<Self> = Arc::downgrade(self);
        let progress_id =
            self.app_handle
                .listen("nexora:component-operation-progress", move |event| {
                    let Some(runtime) = weak.upgrade() else {
                        return;
                    };
                    let Ok(payload) = serde_json::from_str::<Value>(event.payload()) else {
                        return;
                    };
                    let Some(operation_id) = payload.get("operationId").and_then(Value::as_str)
                    else {
                        return;
                    };
                    let Some(progress) = payload.get("progress").and_then(Value::as_f64) else {
                        return;
                    };
                    let message = payload
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let _ = runtime.update_progress_for_operation(operation_id, progress, message);
                });
        ids.push(progress_id);
        for (source, target) in mappings {
            let weak: Weak<Self> = Arc::downgrade(self);
            let target = target.to_string();
            let id = self.app_handle.listen(source, move |event| {
                let Some(runtime) = weak.upgrade() else {
                    return;
                };
                let payload = serde_json::from_str::<Value>(event.payload())
                    .unwrap_or_else(|_| Value::String(event.payload().into()));
                if target == "lan.file-received"
                    && (payload.get("direction").and_then(Value::as_str) != Some("incoming")
                        || payload.get("status").and_then(Value::as_str) != Some("completed"))
                {
                    return;
                }
                if target == "render.job-completed"
                    && payload.get("phase").and_then(Value::as_str) != Some("completed")
                {
                    return;
                }
                if target == "task.completed"
                    && payload.get("exitCode").and_then(Value::as_i64) != Some(0)
                {
                    return;
                }
                let project_path = payload
                    .get("projectPath")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let dedupe_key = payload
                    .get("id")
                    .or_else(|| payload.get("taskId"))
                    .or_else(|| payload.get("jobId"))
                    .and_then(Value::as_str)
                    .map(|id| format!("{}:{id}", target));
                let _ = runtime.emit_event(AutomationEventRequest {
                    event: target.clone(),
                    payload,
                    project_path,
                    dedupe_key,
                });
            });
            ids.push(id);
        }
    }
}

fn required_bridge_string(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("SDK 桥请求缺少 {key}"))
}

fn validate_bridge_command(value: &str) -> Result<(), String> {
    if value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._:-".contains(character))
    {
        return Err(format!("SDK 桥标识符无效: {value}"));
    }
    Ok(())
}

fn validate_state_key(value: &str) -> Result<(), String> {
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err("组件状态键无效或超过 256 字符".into());
    }
    Ok(())
}

fn bridge_scope_key(run: &AutomationRun, payload: &Value) -> Result<String, String> {
    match payload
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("global")
    {
        "global" => Ok("global".into()),
        "project" => {
            let project_path = run
                .project_path
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or("项目级组件状态需要有效项目上下文")?;
            let path =
                fs::canonicalize(project_path).unwrap_or_else(|_| PathBuf::from(project_path));
            let normalized = path.to_string_lossy().replace('\\', "/");
            #[cfg(windows)]
            let normalized = normalized.to_lowercase();
            Ok(format!("project:{normalized}"))
        }
        scope => Err(format!("不支持的组件状态作用域: {scope}")),
    }
}

fn migrate_database(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS automation_schema_meta(
               id INTEGER PRIMARY KEY CHECK(id=1), schema_version INTEGER NOT NULL, updated_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO automation_schema_meta(id,schema_version,updated_at) VALUES(1,1,0);
             CREATE TABLE IF NOT EXISTS automation_runs(
               id TEXT PRIMARY KEY,
               component_id TEXT NOT NULL,
               component_name TEXT NOT NULL,
               component_version TEXT NOT NULL,
               command TEXT NOT NULL,
               command_name TEXT NOT NULL,
               profile_id TEXT NOT NULL,
               profile_revision INTEGER NOT NULL,
               project_path TEXT,
               trigger_kind TEXT NOT NULL,
               trigger_id TEXT,
               status TEXT NOT NULL,
               semantics TEXT NOT NULL,
               attempt INTEGER NOT NULL,
               max_attempts INTEGER NOT NULL,
               operation_id TEXT,
               content_digest TEXT NOT NULL,
               input_json TEXT NOT NULL,
               output_json TEXT,
               logs_json TEXT NOT NULL,
               error TEXT,
               permission_request_json TEXT,
               manifest_json TEXT NOT NULL,
               capability_scope_json TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               started_at INTEGER,
               finished_at INTEGER,
               updated_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_automation_runs_created ON automation_runs(created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_automation_runs_status ON automation_runs(status, updated_at);
             CREATE TABLE IF NOT EXISTS automation_events(
               event_id TEXT PRIMARY KEY,
               event_name TEXT NOT NULL,
               dedupe_key TEXT NOT NULL UNIQUE,
               payload_json TEXT NOT NULL,
               occurred_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS automation_schedule_ticks(
               profile_id TEXT NOT NULL,
               binding_id TEXT NOT NULL,
               minute_key TEXT NOT NULL,
               context_key TEXT NOT NULL DEFAULT 'global',
               triggered_at INTEGER NOT NULL,
               PRIMARY KEY(profile_id,binding_id,minute_key,context_key)
             );
             CREATE TABLE IF NOT EXISTS automation_component_state(
               component_id TEXT NOT NULL,
               scope_key TEXT NOT NULL,
               state_json TEXT NOT NULL,
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(component_id,scope_key)
             );",
        )
        .map_err(|error| error.to_string())?;
    let mut version: i64 = connection
        .query_row(
            "SELECT schema_version FROM automation_schema_meta WHERE id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if version == 1 {
        connection.execute_batch(
            "ALTER TABLE automation_schedule_ticks RENAME TO automation_schedule_ticks_v1;
             CREATE TABLE automation_schedule_ticks(
               profile_id TEXT NOT NULL,
               binding_id TEXT NOT NULL,
               minute_key TEXT NOT NULL,
               context_key TEXT NOT NULL DEFAULT 'global',
               triggered_at INTEGER NOT NULL,
               PRIMARY KEY(profile_id,binding_id,minute_key,context_key)
             );
             INSERT INTO automation_schedule_ticks(profile_id,binding_id,minute_key,context_key,triggered_at)
             SELECT profile_id,binding_id,minute_key,'global',triggered_at FROM automation_schedule_ticks_v1;
             DROP TABLE automation_schedule_ticks_v1;
             UPDATE automation_schema_meta SET schema_version=2,updated_at=strftime('%s','now')*1000 WHERE id=1;"
        ).map_err(|error| error.to_string())?;
        version = 2;
    }
    if version == 2 {
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS automation_run_attempts(
                   run_id TEXT NOT NULL,
                   attempt INTEGER NOT NULL,
                   operation_id TEXT NOT NULL,
                   status TEXT NOT NULL,
                   logs_json TEXT NOT NULL,
                   error TEXT,
                   started_at INTEGER NOT NULL,
                   finished_at INTEGER,
                   PRIMARY KEY(run_id,attempt),
                   FOREIGN KEY(run_id) REFERENCES automation_runs(id) ON DELETE CASCADE
                 );
                 CREATE TABLE IF NOT EXISTS automation_attention(
                   id TEXT PRIMARY KEY,
                   run_id TEXT NOT NULL,
                   kind TEXT NOT NULL,
                   message TEXT NOT NULL,
                   status TEXT NOT NULL,
                   resolution TEXT,
                   created_at INTEGER NOT NULL,
                   resolved_at INTEGER,
                   FOREIGN KEY(run_id) REFERENCES automation_runs(id) ON DELETE CASCADE
                 );
                 CREATE INDEX IF NOT EXISTS idx_automation_attempts_run ON automation_run_attempts(run_id,attempt);
                 CREATE INDEX IF NOT EXISTS idx_automation_attention_run ON automation_attention(run_id,created_at DESC);
                 UPDATE automation_schema_meta SET schema_version=3,updated_at=strftime('%s','now')*1000 WHERE id=1;",
            )
            .map_err(|error| error.to_string())?;
        version = 3;
    }
    if version == 3 {
        connection
            .execute_batch(
                "ALTER TABLE automation_runs ADD COLUMN progress REAL;
                 ALTER TABLE automation_runs ADD COLUMN progress_message TEXT;
                 UPDATE automation_schema_meta SET schema_version=4,updated_at=strftime('%s','now')*1000 WHERE id=1;",
            )
            .map_err(|error| error.to_string())?;
        version = 4;
    }
    if version != DATABASE_SCHEMA_VERSION {
        return Err(format!("不支持 automation_runtime.db schema {version}"));
    }
    Ok(())
}

fn recover_interrupted_runs(connection: &Connection) -> Result<(), String> {
    let now = Utc::now().timestamp_millis();
    connection
        .execute(
            "UPDATE automation_runs SET status='queued', permission_request_json=NULL,
             progress=NULL,progress_message=NULL,
             error='应用异常退出，原权限请求已失效，运行已重新排队', updated_at=?1
             WHERE status='waiting-permission'",
            [now],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE automation_runs SET status='queued', operation_id=NULL,
             progress=NULL,progress_message=NULL,
             error='应用异常退出，幂等运行已重新排队', updated_at=?1
             WHERE status IN ('preparing','running','cancelling') AND semantics IN ('pure','idempotent')",
            [now],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE automation_runs SET status='attention', operation_id=NULL,
             error='应用异常退出，非幂等操作结果未知，未自动重放', updated_at=?1
             WHERE status IN ('preparing','running','cancelling') AND semantics='non-idempotent'",
            [now],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE automation_run_attempts SET status='interrupted',
             error=COALESCE(error,'应用异常退出'),finished_at=COALESCE(finished_at,?1)
             WHERE finished_at IS NULL",
            [now],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO automation_attention(id,run_id,kind,message,status,resolution,created_at,resolved_at)
             SELECT lower(hex(randomblob(16))),id,'uncertain-result',
                    '应用异常退出，非幂等操作结果未知，未自动重放','open',NULL,?1,NULL
             FROM automation_runs
             WHERE status='attention'
               AND NOT EXISTS(
                 SELECT 1 FROM automation_attention attention
                 WHERE attention.run_id=automation_runs.id AND attention.status='open'
               )",
            [now],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn map_run(row: &Row<'_>) -> rusqlite::Result<AutomationRun> {
    let input_json: String = row.get("input_json")?;
    let output_json: Option<String> = row.get("output_json")?;
    let logs_json: String = row.get("logs_json")?;
    let permission_json: Option<String> = row.get("permission_request_json")?;
    Ok(AutomationRun {
        id: row.get("id")?,
        component_id: row.get("component_id")?,
        component_name: row.get("component_name")?,
        component_version: row.get("component_version")?,
        command: row.get("command")?,
        command_name: row.get("command_name")?,
        profile_id: row.get("profile_id")?,
        profile_revision: row.get::<_, i64>("profile_revision")?.max(0) as u64,
        project_path: row.get("project_path")?,
        trigger_kind: row.get("trigger_kind")?,
        trigger_id: row.get("trigger_id")?,
        status: AutomationRunStatus::parse(&row.get::<_, String>("status")?),
        progress: row.get("progress")?,
        progress_message: row.get("progress_message")?,
        execution_semantics: parse_semantics(&row.get::<_, String>("semantics")?),
        attempt: row.get::<_, i64>("attempt")?.max(0) as u16,
        max_attempts: row.get::<_, i64>("max_attempts")?.max(1) as u16,
        operation_id: row.get("operation_id")?,
        content_digest: row.get("content_digest")?,
        input: serde_json::from_str(&input_json).unwrap_or(Value::Null),
        output: output_json.and_then(|value| serde_json::from_str(&value).ok()),
        logs: serde_json::from_str(&logs_json).unwrap_or_default(),
        error: row.get("error")?,
        permission_request: permission_json.and_then(|value| serde_json::from_str(&value).ok()),
        attempts: Vec::new(),
        attention: None,
        created_at: row.get("created_at")?,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn enrich_run(connection: &Connection, run: &mut AutomationRun) -> Result<(), String> {
    let mut attempt_statement = connection
        .prepare(
            "SELECT attempt,operation_id,status,logs_json,error,started_at,finished_at
             FROM automation_run_attempts WHERE run_id=?1 ORDER BY attempt",
        )
        .map_err(|error| error.to_string())?;
    run.attempts = attempt_statement
        .query_map([&run.id], |row| {
            let logs_json = row.get::<_, String>(3)?;
            Ok(AutomationRunAttempt {
                attempt: row.get::<_, i64>(0)?.max(0) as u16,
                operation_id: row.get(1)?,
                status: row.get(2)?,
                logs: serde_json::from_str(&logs_json).unwrap_or_default(),
                error: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(attempt_statement);

    run.attention = connection
        .query_row(
            "SELECT id,kind,message,status,resolution,created_at,resolved_at
             FROM automation_attention WHERE run_id=?1 ORDER BY created_at DESC LIMIT 1",
            [&run.id],
            |row| {
                Ok(AutomationAttention {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    message: row.get(2)?,
                    status: row.get(3)?,
                    resolution: row.get(4)?,
                    created_at: row.get(5)?,
                    resolved_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn automation_command(
    manifest: &ComponentManifestV1,
    command: &str,
) -> Result<AutomationCommandContribution, String> {
    manifest
        .contributes
        .automation_commands
        .iter()
        .find(|candidate| candidate.command == command || candidate.id == command)
        .cloned()
        .ok_or_else(|| format!("组件 {} 未声明自动化命令 {command}", manifest.id))
}

fn semantics_name(value: AutomationExecutionSemantics) -> &'static str {
    match value {
        AutomationExecutionSemantics::Pure => "pure",
        AutomationExecutionSemantics::Idempotent => "idempotent",
        AutomationExecutionSemantics::NonIdempotent => "non-idempotent",
    }
}

fn parse_semantics(value: &str) -> AutomationExecutionSemantics {
    match value {
        "pure" => AutomationExecutionSemantics::Pure,
        "idempotent" => AutomationExecutionSemantics::Idempotent,
        _ => AutomationExecutionSemantics::NonIdempotent,
    }
}

fn capability_operation(value: AutomationCapabilityOperation) -> CapabilityOperation {
    match value {
        AutomationCapabilityOperation::Read => CapabilityOperation::Read,
        AutomationCapabilityOperation::Write => CapabilityOperation::Write,
        AutomationCapabilityOperation::Delete => CapabilityOperation::Delete,
        AutomationCapabilityOperation::Execute => CapabilityOperation::Execute,
        AutomationCapabilityOperation::Connect => CapabilityOperation::Connect,
        AutomationCapabilityOperation::Notify => CapabilityOperation::Notify,
    }
}

fn default_capability_scope(
    capability: Option<Capability>,
    project_path: Option<&str>,
) -> CapabilityScopeRequest {
    let project_scoped = matches!(
        capability,
        Some(
            Capability::ProjectFilesRead
                | Capability::ProjectFilesWrite
                | Capability::ProjectMetadataRead
                | Capability::ProjectMetadataWrite
                | Capability::CacheInspect
                | Capability::CacheMaintain
        )
    );
    if project_scoped {
        CapabilityScopeRequest {
            kind: CapabilityScopeKind::Project,
            root_path: project_path.map(str::to_string),
            relative_path: None,
        }
    } else {
        CapabilityScopeRequest::default()
    }
}

fn validate_event_name(event: &str) -> Result<(), String> {
    const EVENTS: &[&str] = &[
        "app.started",
        "project.opened",
        "project.closed",
        "file.changed",
        "lan.file-received",
        "render.batch-created",
        "render.batch-completed",
        "render.job-completed",
        "task.completed",
        "task.failed",
    ];
    if EVENTS.contains(&event) {
        Ok(())
    } else {
        Err(format!("事件不在脚本自动化白名单中: {event}"))
    }
}

fn validate_json_schema_value(value: &Value, schema: &Value, label: &str) -> Result<(), String> {
    let Some(schema_object) = schema.as_object() else {
        return if schema.is_null() {
            Ok(())
        } else {
            Err(format!("{label} Schema 必须是 JSON 对象"))
        };
    };
    if schema_object.is_empty() {
        return Ok(());
    }
    if let Some(allowed) = schema_object.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return Err(format!("{label}不在 Schema enum 允许值中"));
        }
    }
    if let Some(expected) = schema_object.get("type").and_then(Value::as_str) {
        let matches = match expected {
            "null" => value.is_null(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            _ => return Err(format!("{label} Schema 使用了暂不支持的 type: {expected}")),
        };
        if !matches {
            return Err(format!("{label}不符合 Schema type={expected}"));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema_object.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{label}缺少必需字段: {key}"));
                }
            }
        }
        if let Some(properties) = schema_object.get("properties").and_then(Value::as_object) {
            for (key, property_schema) in properties {
                if let Some(property_value) = object.get(key) {
                    validate_json_schema_value(
                        property_value,
                        property_schema,
                        &format!("{label}.{key}"),
                    )?;
                }
            }
        }
    }
    if let (Some(items), Some(values)) = (schema_object.get("items"), value.as_array()) {
        for (index, item) in values.iter().enumerate() {
            validate_json_schema_value(item, items, &format!("{label}[{index}]"))?;
        }
    }
    Ok(())
}

fn merge_binding_input(binding_input: Value, event_payload: &Value) -> Value {
    match binding_input {
        Value::Object(mut object) => {
            object.insert("event".into(), event_payload.clone());
            Value::Object(object)
        }
        other => json!({ "input": other, "event": event_payload }),
    }
}

fn resolve_binding_project(
    profile: &WorkspaceProfileV1,
    binding: &ProfileAutomationBinding,
    event_project: Option<&str>,
) -> Option<String> {
    match binding.project_context {
        AutomationProjectContext::EventProject => event_project.map(str::to_string),
        AutomationProjectContext::ProfileVariable => binding
            .project_variable
            .as_ref()
            .and_then(|name| profile.variables.get(name))
            .cloned(),
        AutomationProjectContext::None => None,
        AutomationProjectContext::ActiveProject | AutomationProjectContext::EachOpenProject => {
            event_project.map(str::to_string)
        }
    }
}

fn cron_matches(cron: &str, now: chrono::DateTime<chrono::Local>) -> bool {
    let fields = cron.split_whitespace().collect::<Vec<_>>();
    fields.len() == 5
        && cron_field_matches(fields[0], now.minute())
        && cron_field_matches(fields[1], now.hour())
        && cron_field_matches(fields[2], now.day())
        && cron_field_matches(fields[3], now.month())
        && cron_field_matches(fields[4], now.weekday().num_days_from_sunday())
}

fn cron_field_matches(field: &str, value: u32) -> bool {
    field.split(',').any(|part| {
        if part == "*" {
            return true;
        }
        if let Some(step) = part
            .strip_prefix("*/")
            .and_then(|value| value.parse::<u32>().ok())
        {
            return step > 0 && value % step == 0;
        }
        if let Some((start, end)) = part.split_once('-') {
            return start
                .parse::<u32>()
                .ok()
                .zip(end.parse::<u32>().ok())
                .is_some_and(|(start, end)| value >= start && value <= end);
        }
        part.parse::<u32>().ok() == Some(value)
    })
}

fn read_trust_state(path: &Path) -> TrustedDevelopmentDirectories {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .filter(|state: &TrustedDevelopmentDirectories| {
            state.schema_version == TRUST_SCHEMA_VERSION
        })
        .unwrap_or_default()
}

fn persist_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn hash_directory(root: &Path) -> Result<String, String> {
    let mut files = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && !entry.path().components().any(|component| {
                    matches!(component, Component::Normal(value) if value == ".git" || value == "__pycache__")
                })
                && entry.path().extension().and_then(|value| value.to_str()) != Some("pyc")
        })
        .map(|entry| entry.path().to_path_buf())
        .collect::<Vec<_>>();
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        hasher.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
        hasher.update(&fs::read(path).map_err(|error| error.to_string())?);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("脚本资源路径必须是包内普通相对路径".into());
    }
    let joined = root.join(relative);
    if joined.is_file() {
        Ok(joined)
    } else {
        Err("脚本资源不存在".into())
    }
}

fn canonical_development_root(source_path: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(source_path).map_err(|error| error.to_string())?;
    if !root.is_dir() || !root.join("component.json").is_file() {
        return Err("开发目录必须包含 component.json".into());
    }
    Ok(root)
}

fn prepare_script_surface_source(root: &Path, source: &str, nonce: &str) -> Result<String, String> {
    let script_source = Regex::new(
        r#"(?is)<script(?P<before>[^>]*?)\s+src=[\"'](?P<src>[^\"']+)[\"'](?P<after>[^>]*)>\s*</script>"#,
    ).map_err(|error| error.to_string())?;
    let mut failure = None;
    let inlined = script_source
        .replace_all(source, |captures: &Captures<'_>| {
            let src = captures
                .name("src")
                .map(|value| value.as_str())
                .unwrap_or_default();
            match safe_join(root, src)
                .and_then(|path| fs::read_to_string(path).map_err(|error| error.to_string()))
            {
                Ok(content) => format!("<script nonce=\"{nonce}\">{content}</script>"),
                Err(error) => {
                    failure = Some(format!("无法加载脚本页面资源 {src}: {error}"));
                    String::new()
                }
            }
        })
        .into_owned();
    if let Some(error) = failure {
        return Err(error);
    }
    let style_source = Regex::new(
        r#"(?is)<link(?P<attrs>[^>]*?)href=[\"'](?P<href>[^\"']+)[\"'](?P<tail>[^>]*)>"#,
    )
    .map_err(|error| error.to_string())?;
    let mut failure = None;
    let inlined = style_source
        .replace_all(&inlined, |captures: &Captures<'_>| {
            let attrs = format!(
                "{} {}",
                captures
                    .name("attrs")
                    .map(|value| value.as_str())
                    .unwrap_or_default(),
                captures
                    .name("tail")
                    .map(|value| value.as_str())
                    .unwrap_or_default(),
            );
            if !attrs.to_ascii_lowercase().contains("stylesheet") {
                return captures
                    .get(0)
                    .map(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
            }
            let href = captures
                .name("href")
                .map(|value| value.as_str())
                .unwrap_or_default();
            match safe_join(root, href)
                .and_then(|path| fs::read_to_string(path).map_err(|error| error.to_string()))
            {
                Ok(content) => format!("<style>{content}</style>"),
                Err(error) => {
                    failure = Some(format!("无法加载脚本页面样式 {href}: {error}"));
                    String::new()
                }
            }
        })
        .into_owned();
    if let Some(error) = failure {
        return Err(error);
    }
    let image_source = Regex::new(
        r#"(?is)<img(?P<before>[^>]*?)\s+src=[\"'](?P<src>[^\"']+)[\"'](?P<after>[^>]*)>"#,
    )
    .map_err(|error| error.to_string())?;
    let mut failure = None;
    let inlined = image_source
        .replace_all(&inlined, |captures: &Captures<'_>| {
            let src = captures
                .name("src")
                .map(|value| value.as_str())
                .unwrap_or_default();
            if src.starts_with("data:") || src.starts_with("blob:") || src.contains("://") {
                return captures
                    .get(0)
                    .map(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
            }
            match safe_join(root, src).and_then(|path| {
                let mime = mime_guess::from_path(&path)
                    .first_raw()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let bytes = fs::read(path).map_err(|error| error.to_string())?;
                if bytes.len() > 4 * 1024 * 1024 {
                    return Err("脚本页面内联图片超过 4 MB".into());
                }
                Ok(format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ))
            }) {
                Ok(data_url) => format!(
                    "<img{} src=\"{}\"{}>",
                    captures
                        .name("before")
                        .map(|value| value.as_str())
                        .unwrap_or_default(),
                    data_url,
                    captures
                        .name("after")
                        .map(|value| value.as_str())
                        .unwrap_or_default()
                ),
                Err(error) => {
                    failure = Some(format!("无法加载脚本页面图片 {src}: {error}"));
                    String::new()
                }
            }
        })
        .into_owned();
    if let Some(error) = failure {
        return Err(error);
    }
    let script_tags =
        Regex::new(r#"(?i)<script(?P<attrs>[^>]*)>"#).map_err(|error| error.to_string())?;
    Ok(script_tags
        .replace_all(&inlined, |captures: &Captures<'_>| {
            let attrs = captures
                .name("attrs")
                .map(|value| value.as_str())
                .unwrap_or_default();
            if attrs.to_ascii_lowercase().contains("nonce=") {
                captures
                    .get(0)
                    .map(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string()
            } else {
                format!("<script nonce=\"{nonce}\"{attrs}>")
            }
        })
        .into_owned())
}

fn validate_component_id(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.contains('.')
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '-')
        });
    if valid {
        Ok(())
    } else {
        Err("组件 ID 需要使用小写字母、数字、点和短横线，并至少包含一个点".into())
    }
}

pub fn all_script_capabilities() -> Vec<Capability> {
    vec![
        Capability::AppProfileRead,
        Capability::AppProfileWrite,
        Capability::AppSettingsRead,
        Capability::AppSettingsWrite,
        Capability::NotificationSend,
        Capability::ClipboardRead,
        Capability::ClipboardWrite,
        Capability::FilesystemDialogOpen,
        Capability::FilesystemExternalRead,
        Capability::FilesystemExternalWrite,
        Capability::ProjectOpen,
        Capability::ProjectFilesRead,
        Capability::ProjectFilesWrite,
        Capability::ProjectMetadataRead,
        Capability::ProjectMetadataWrite,
        Capability::CacheInspect,
        Capability::CacheMaintain,
        Capability::TaskRun,
        Capability::TaskCancel,
        Capability::PythonExecute,
        Capability::PythonPackagesManage,
        Capability::ProcessSpawn,
        Capability::NetworkHttpRequest,
        Capability::NetworkLanDiscover,
        Capability::NetworkLanMessage,
        Capability::NetworkLanTransfer,
        Capability::NetworkServerConnect,
        Capability::RenderInspect,
        Capability::RenderQueueRead,
        Capability::RenderQueueWrite,
        Capability::RenderWorkerExecute,
        Capability::RenderResultCommit,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_field_cron_supports_steps_ranges_and_lists() {
        let now = chrono::Local::now();
        assert!(cron_matches("* * * * *", now));
        assert!(cron_field_matches("*/5", 10));
        assert!(cron_field_matches("1-10", 6));
        assert!(cron_field_matches("1,3,8", 3));
        assert!(!cron_field_matches("1,3,8", 4));
    }

    #[test]
    fn automation_json_schema_checks_required_fields_and_types() {
        let schema = json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string"},
                "flags": {"type": "array", "items": {"type": "boolean"}}
            }
        });
        assert!(validate_json_schema_value(
            &json!({"path":"scene.blend","flags":[true,false]}),
            &schema,
            "输入"
        )
        .is_ok());
        assert!(validate_json_schema_value(&json!({"flags":[]}), &schema, "输入").is_err());
        assert!(validate_json_schema_value(&json!({"path":1}), &schema, "输入").is_err());
    }

    #[test]
    fn sdk_bridge_rejects_unsafe_identifiers_and_project_state_without_context() {
        assert!(validate_bridge_command("inspect.scene").is_ok());
        assert!(validate_bridge_command("invoke component").is_err());
        assert!(validate_state_key("lastAudit").is_ok());
        assert!(validate_state_key("bad\nkey").is_err());
        let run = AutomationRun {
            id: "run-1".into(),
            component_id: "test.script".into(),
            component_name: "Test".into(),
            component_version: "1.0.0".into(),
            command: "run".into(),
            command_name: "Run".into(),
            profile_id: "test.profile".into(),
            profile_revision: 1,
            project_path: None,
            trigger_kind: "manual".into(),
            trigger_id: None,
            status: AutomationRunStatus::Running,
            progress: None,
            progress_message: None,
            execution_semantics: AutomationExecutionSemantics::Idempotent,
            attempt: 1,
            max_attempts: 1,
            operation_id: Some("operation-1".into()),
            content_digest: "digest".into(),
            input: Value::Null,
            output: None,
            logs: Vec::new(),
            error: None,
            permission_request: None,
            attempts: Vec::new(),
            attention: None,
            created_at: 0,
            started_at: None,
            finished_at: None,
            updated_at: 0,
        };
        assert!(bridge_scope_key(&run, &json!({"scope":"project"})).is_err());
    }

    #[test]
    fn script_surface_inlines_local_assets_and_nonces_scripts() {
        let root = std::env::temp_dir().join(format!("nexora-script-surface-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("app.js"), "window.surfaceReady = true;").unwrap();
        fs::write(root.join("app.css"), "body { color: red; }").unwrap();
        let source = r#"<html><head><link rel="stylesheet" href="app.css"></head><body><script src="app.js"></script><script>window.inlineReady = true;</script></body></html>"#;
        let prepared = prepare_script_surface_source(&root, source, "nonce-1").unwrap();
        assert!(prepared.contains("<style>body { color: red; }</style>"));
        assert!(prepared.contains("<script nonce=\"nonce-1\">window.surfaceReady = true;</script>"));
        assert!(prepared.contains("<script nonce=\"nonce-1\">window.inlineReady = true;</script>"));
        assert!(!prepared.contains("src=\"app.js\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn database_migration_is_idempotent_and_recovers_runs() {
        let path = std::env::temp_dir().join(format!("nexora-automation-{}.db", Uuid::new_v4()));
        let connection = Connection::open(&path).unwrap();
        migrate_database(&connection).unwrap();
        migrate_database(&connection).unwrap();
        let version: i64 = connection
            .query_row(
                "SELECT schema_version FROM automation_schema_meta WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, DATABASE_SCHEMA_VERSION);
        let columns = connection
            .prepare("PRAGMA table_info(automation_schedule_ticks)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "context_key"));
        let run_columns = connection
            .prepare("PRAGMA table_info(automation_runs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(run_columns.iter().any(|column| column == "progress"));
        assert!(run_columns
            .iter()
            .any(|column| column == "progress_message"));
        let attempt_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='automation_run_attempts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let attention_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='automation_attention'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_table, 1);
        assert_eq!(attention_table, 1);
        drop(connection);
        let _ = fs::remove_file(path);
    }
}
