use base64::Engine;
use chrono::Utc;
use pmc_platform::{
    parse_component_manifest, parse_package_header, validate_component_graph, ComponentManifestV1,
    ComponentRuntime, DigestAlgorithm, PackageKind, PageTemplateContribution,
    ShellTemplateContribution, ThemePresetContribution, ValidateContract,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::{Pid, System};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
use walkdir::WalkDir;
use zip::ZipArchive;

const COMPONENT_STATE_SCHEMA_VERSION: u16 = 1;
const COMPONENT_MANIFEST_FILE: &str = "component.json";
const MAX_COMPONENT_FILES: usize = 4096;
const MAX_COMPONENT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CAPTURED_LOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_OPERATION_HISTORY: usize = 100;
const WORKER_READY_TIMEOUT: Duration = Duration::from_secs(120);
const WORKER_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComponentRuntimeErrorCode {
    ComponentNotInstalled,
    ComponentAlreadyInstalled,
    ComponentManifestInvalid,
    ComponentPackageInvalid,
    ComponentRuntimeUnsupported,
    ComponentOperationConflict,
    ComponentOperationNotFound,
    ComponentOperationCancelled,
    ComponentOperationTimedOut,
    ComponentProcessFailed,
    ComponentProtocolError,
    ComponentIoError,
    ComponentDependencyConflict,
    ComponentCapabilityRequired,
    ComponentHostUnavailable,
    ComponentHostAbiMismatch,
    ComponentHostCrashed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRuntimeError {
    pub code: ComponentRuntimeErrorCode,
    pub message: String,
    pub details: Vec<String>,
}

impl ComponentRuntimeError {
    pub fn new(code: ComponentRuntimeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }

    pub fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

impl std::fmt::Display for ComponentRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ComponentRuntimeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentInstallSource {
    Bundled,
    Local,
    Marketplace,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledComponentSummary {
    pub manifest: ComponentManifestV1,
    pub source: ComponentInstallSource,
    pub package_path: Option<String>,
    pub removable: bool,
    pub host_adapter: Option<String>,
    pub active_operation_count: usize,
    pub worker: Option<ComponentWorkerSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentWorkerSummary {
    pub component_id: String,
    pub pid: u32,
    pub started_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTemplateOwner {
    pub component_id: String,
    pub component_name: String,
    pub component_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellTemplateCatalogItem {
    pub template: ShellTemplateContribution,
    pub owner: PresentationTemplateOwner,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTemplateCatalogItem {
    pub template: PageTemplateContribution,
    pub owner: PresentationTemplateOwner,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePresetCatalogItem {
    pub preset: ThemePresetContribution,
    pub owner: PresentationTemplateOwner,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTemplateCatalog {
    pub shell_templates: Vec<ShellTemplateCatalogItem>,
    pub page_templates: Vec<PageTemplateCatalogItem>,
    pub theme_presets: Vec<ThemePresetCatalogItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentOperationStatus {
    Starting,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentOperationSummary {
    pub operation_id: String,
    pub component_id: String,
    pub component_version: String,
    pub command: String,
    pub runtime: ComponentRuntime,
    pub status: ComponentOperationStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<u64>,
    pub pid: Option<u32>,
    pub message: String,
    pub log_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRuntimeOverview {
    pub root_path: String,
    pub state_path: String,
    pub installed_components: Vec<InstalledComponentSummary>,
    pub available_bundled_components: Vec<ComponentManifestV1>,
    pub active_operations: Vec<ComponentOperationSummary>,
    pub recent_operations: Vec<ComponentOperationSummary>,
    pub templates: PresentationTemplateCatalog,
    pub legacy_python_action_compatible: bool,
    pub component_host_available: bool,
    pub component_host_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallComponentRequest {
    pub source_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallComponentPackageRequest {
    pub package_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPackageInspection {
    pub package_path: String,
    pub valid: bool,
    pub component_id: Option<String>,
    pub component_name: Option<String>,
    pub component_version: Option<String>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub package_digest: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInvocationRequest {
    pub component_id: String,
    pub module_id: String,
    pub command: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub capability_request: Option<super::CapabilityTokenRequest>,
    #[serde(default)]
    pub capability_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInvocationResult {
    pub operation_id: String,
    pub component_id: String,
    pub component_version: String,
    pub command: String,
    pub runtime: ComponentRuntime,
    pub output: Value,
    pub logs: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredComponentRuntimeState {
    schema_version: u16,
    #[serde(default)]
    removed_bundled: BTreeSet<String>,
}

impl Default for StoredComponentRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: COMPONENT_STATE_SCHEMA_VERSION,
            removed_bundled: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct InstalledComponent {
    manifest: ComponentManifestV1,
    source: ComponentInstallSource,
    package_root: Option<PathBuf>,
}

struct OperationControl {
    summary: Mutex<ComponentOperationSummary>,
    cancelled: AtomicBool,
    pid: AtomicU32,
}

#[derive(Clone)]
struct WorkerHandle {
    sender: mpsc::Sender<WorkerMessage>,
    pid: u32,
}

enum WorkerMessage {
    Invoke {
        request: Value,
        response: oneshot::Sender<Result<ProcessResponse, ComponentRuntimeError>>,
    },
    Shutdown,
}

struct ProcessResponse {
    output: Value,
    logs: Vec<String>,
}

pub struct ComponentRuntimeManager {
    app_handle: AppHandle,
    root_path: PathBuf,
    packages_path: PathBuf,
    state_path: PathBuf,
    logs_path: PathBuf,
    bundled: BTreeMap<String, ComponentManifestV1>,
    state: Mutex<StoredComponentRuntimeState>,
    catalog: Mutex<BTreeMap<String, InstalledComponent>>,
    operations: Mutex<HashMap<String, Arc<OperationControl>>>,
    history: Mutex<VecDeque<ComponentOperationSummary>>,
    workers: tokio::sync::Mutex<HashMap<String, WorkerHandle>>,
    worker_states: Arc<Mutex<BTreeMap<String, ComponentWorkerSummary>>>,
    mutation_lock: tokio::sync::Mutex<()>,
}

impl ComponentRuntimeManager {
    pub fn new(
        app_data_dir: &Path,
        app_handle: AppHandle,
        bundled_manifests: Vec<ComponentManifestV1>,
    ) -> Result<Arc<Self>, ComponentRuntimeError> {
        let root_path = app_data_dir.join("components");
        let packages_path = root_path.join("packages");
        let logs_path = root_path.join("logs");
        let state_path = root_path.join("runtime-state.json");
        fs::create_dir_all(&packages_path)
            .and_then(|_| fs::create_dir_all(&logs_path))
            .map_err(|error| io_error("创建组件运行时目录失败", error))?;

        for manifest in &bundled_manifests {
            manifest.validate_contract().map_err(contract_error)?;
        }
        validate_component_graph(&bundled_manifests).map_err(contract_error)?;
        let bundled = bundled_manifests
            .into_iter()
            .map(|manifest| (manifest.id.clone(), manifest))
            .collect::<BTreeMap<_, _>>();
        let state = load_state(&state_path)?;
        let catalog = load_catalog(&packages_path, &bundled, &state)?;
        validate_component_graph(
            &catalog
                .values()
                .map(|component| component.manifest.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(contract_error)?;

        Ok(Arc::new(Self {
            app_handle,
            root_path,
            packages_path,
            state_path,
            logs_path,
            bundled,
            state: Mutex::new(state),
            catalog: Mutex::new(catalog),
            operations: Mutex::new(HashMap::new()),
            history: Mutex::new(VecDeque::new()),
            workers: tokio::sync::Mutex::new(HashMap::new()),
            worker_states: Arc::new(Mutex::new(BTreeMap::new())),
            mutation_lock: tokio::sync::Mutex::new(()),
        }))
    }

    pub fn manifests(&self) -> Vec<ComponentManifestV1> {
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .values()
            .map(|component| component.manifest.clone())
            .collect()
    }

    pub fn manifest(&self, component_id: &str) -> Option<ComponentManifestV1> {
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(component_id)
            .map(|component| component.manifest.clone())
    }

    pub fn ensure_installed(&self, component_id: &str) -> Result<(), ComponentRuntimeError> {
        if self.manifest(component_id).is_some() {
            Ok(())
        } else {
            Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentNotInstalled,
                format!("组件 {component_id} 未安装"),
            ))
        }
    }

    pub fn overview(&self) -> ComponentRuntimeOverview {
        let active = self
            .operations
            .lock()
            .expect("component operation mutex poisoned");
        let active_counts = active
            .values()
            .fold(BTreeMap::new(), |mut counts, control| {
                let component_id = control
                    .summary
                    .lock()
                    .expect("component operation summary mutex poisoned")
                    .component_id
                    .clone();
                *counts.entry(component_id).or_insert(0usize) += 1;
                counts
            });
        let mut active_operations = active
            .values()
            .map(|control| {
                control
                    .summary
                    .lock()
                    .expect("component operation summary mutex poisoned")
                    .clone()
            })
            .collect::<Vec<_>>();
        drop(active);
        active_operations.sort_by_key(|operation| operation.started_at);

        let workers = self
            .worker_states
            .lock()
            .expect("component worker state mutex poisoned")
            .clone();
        let catalog = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned");
        let mut installed_components = catalog
            .values()
            .map(|component| InstalledComponentSummary {
                manifest: component.manifest.clone(),
                source: component.source,
                package_path: component
                    .package_root
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                removable: component
                    .manifest
                    .extensions
                    .get("removable")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                host_adapter: host_adapter(&component.manifest).map(str::to_string),
                active_operation_count: active_counts
                    .get(&component.manifest.id)
                    .copied()
                    .unwrap_or_default(),
                worker: workers.get(&component.manifest.id).cloned(),
            })
            .collect::<Vec<_>>();
        installed_components.sort_by(|left, right| left.manifest.name.cmp(&right.manifest.name));
        let installed_ids = catalog.keys().cloned().collect::<BTreeSet<_>>();
        let templates = template_catalog(catalog.values());
        drop(catalog);

        let mut available_bundled_components = self
            .bundled
            .values()
            .filter(|manifest| !installed_ids.contains(&manifest.id))
            .cloned()
            .collect::<Vec<_>>();
        available_bundled_components.sort_by(|left, right| left.name.cmp(&right.name));

        let host_path = component_host_path();
        ComponentRuntimeOverview {
            root_path: self.root_path.to_string_lossy().into_owned(),
            state_path: self.state_path.to_string_lossy().into_owned(),
            installed_components,
            available_bundled_components,
            active_operations,
            recent_operations: self
                .history
                .lock()
                .expect("component operation history mutex poisoned")
                .iter()
                .cloned()
                .collect(),
            templates,
            legacy_python_action_compatible: true,
            component_host_available: host_path.is_some(),
            component_host_path: host_path.map(|path| path.to_string_lossy().into_owned()),
        }
    }

    pub async fn install_from_directory(
        &self,
        request: &InstallComponentRequest,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        let _guard = self.mutation_lock.lock().await;
        let source = fs::canonicalize(&request.source_path)
            .map_err(|error| io_error("组件来源目录不可用", error))?;
        if !source.is_dir() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "R9 本地安装要求选择包含 component.json 的目录",
            ));
        }
        let manifest = read_component_manifest(&source.join(COMPONENT_MANIFEST_FILE))?;
        let staging_root = self.root_path.join(".staging");
        fs::create_dir_all(&staging_root)
            .map_err(|error| io_error("创建组件安装暂存目录失败", error))?;
        let staging = staging_root.join(format!("{}-{}", manifest.id, Uuid::new_v4().simple()));
        copy_component_tree(&source, &staging)?;
        let staged_manifest = read_component_manifest(&staging.join(COMPONENT_MANIFEST_FILE))?;
        if staged_manifest.id != manifest.id || staged_manifest.version != manifest.version {
            let _ = fs::remove_dir_all(&staging);
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "暂存后的组件清单与来源不一致",
            ));
        }
        validate_entry(&staging, &staged_manifest)?;
        self.commit_staged_component(staging, staged_manifest, ComponentInstallSource::Local)
            .await
    }

    async fn commit_staged_component(
        &self,
        staging: PathBuf,
        staged_manifest: ComponentManifestV1,
        source: ComponentInstallSource,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        let mut next_manifests = self.manifests();
        next_manifests.retain(|item| item.id != staged_manifest.id);
        next_manifests.push(staged_manifest.clone());
        validate_component_graph(&next_manifests).map_err(contract_error)?;

        let target = self.packages_path.join(&staged_manifest.id);
        let backup = self.root_path.join(".backup").join(format!(
            "{}-{}",
            staged_manifest.id,
            Uuid::new_v4().simple()
        ));
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error("创建组件备份目录失败", error))?;
        }
        let had_target = target.exists();
        if had_target {
            fs::rename(&target, &backup).map_err(|error| io_error("暂存旧组件版本失败", error))?;
        }
        if let Err(error) = fs::rename(&staging, &target) {
            if had_target {
                let _ = fs::rename(&backup, &target);
            }
            let _ = fs::remove_dir_all(&staging);
            return Err(io_error("提交组件安装失败", error));
        }
        let _ = fs::remove_dir_all(&backup);

        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .insert(
                staged_manifest.id.clone(),
                InstalledComponent {
                    manifest: staged_manifest.clone(),
                    source,
                    package_root: Some(target),
                },
            );
        if self.bundled.contains_key(&staged_manifest.id) {
            let mut state = self.state.lock().expect("component state mutex poisoned");
            state.removed_bundled.remove(&staged_manifest.id);
            persist_state(&self.state_path, &state)?;
        }
        Ok(staged_manifest)
    }

    pub fn inspect_package(
        &self,
        package_path: &str,
    ) -> Result<ComponentPackageInspection, ComponentRuntimeError> {
        inspect_component_package(Path::new(package_path))
    }

    pub async fn install_from_package(
        &self,
        request: &InstallComponentPackageRequest,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        let _guard = self.mutation_lock.lock().await;
        let package_path = fs::canonicalize(&request.package_path)
            .map_err(|error| io_error("组件包不可用", error))?;
        let staging_root = self.root_path.join(".staging");
        fs::create_dir_all(&staging_root)
            .map_err(|error| io_error("创建组件安装暂存目录失败", error))?;
        let staging = staging_root.join(format!("package-{}", Uuid::new_v4().simple()));
        if let Err(error) = extract_component_package(&package_path, &staging) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let manifest = match read_component_manifest(&staging.join(COMPONENT_MANIFEST_FILE)) {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        };
        if let Err(error) = validate_entry(&staging, &manifest) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        self.commit_staged_component(staging, manifest, ComponentInstallSource::Marketplace)
            .await
    }

    pub async fn uninstall(
        &self,
        component_id: &str,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        let _guard = self.mutation_lock.lock().await;
        if self.active_operation_count(component_id) > 0 {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentOperationConflict,
                "组件仍有运行中的操作，请先取消或等待完成",
            ));
        }
        let dependent_components = self
            .manifests()
            .into_iter()
            .filter(|manifest| manifest.id != component_id)
            .filter(|manifest| {
                manifest
                    .requires_components
                    .iter()
                    .any(|dependency| dependency.id == component_id)
            })
            .map(|manifest| manifest.id)
            .collect::<Vec<_>>();
        if !dependent_components.is_empty() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentDependencyConflict,
                "仍有已安装组件依赖此组件，不能卸载",
            )
            .with_details(dependent_components));
        }
        self.shutdown_worker(component_id).await;
        let installed = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(component_id)
            .cloned()
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentNotInstalled,
                    format!("组件 {component_id} 未安装"),
                )
            })?;

        if installed.source == ComponentInstallSource::Bundled {
            let mut state = self.state.lock().expect("component state mutex poisoned");
            state.removed_bundled.insert(component_id.to_string());
            persist_state(&self.state_path, &state)?;
        } else if let Some(package_root) = &installed.package_root {
            let trash_root = self.root_path.join(".trash");
            fs::create_dir_all(&trash_root)
                .map_err(|error| io_error("创建组件卸载暂存目录失败", error))?;
            let trash = trash_root.join(format!("{}-{}", component_id, Uuid::new_v4().simple()));
            fs::rename(package_root, &trash)
                .map_err(|error| io_error("撤下组件目录失败", error))?;
            let _ = fs::remove_dir_all(trash);
        }
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .remove(component_id);
        Ok(installed.manifest)
    }

    pub async fn reinstall_bundled(
        &self,
        component_id: &str,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        let _guard = self.mutation_lock.lock().await;
        let manifest = self.bundled.get(component_id).cloned().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentNotInstalled,
                format!("{component_id} 不是安装器随附组件"),
            )
        })?;
        let mut next_manifests = self.manifests();
        next_manifests.retain(|item| item.id != component_id);
        next_manifests.push(manifest.clone());
        validate_component_graph(&next_manifests).map_err(contract_error)?;
        {
            let mut state = self.state.lock().expect("component state mutex poisoned");
            state.removed_bundled.remove(component_id);
            persist_state(&self.state_path, &state)?;
        }
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .insert(
                component_id.into(),
                InstalledComponent {
                    manifest: manifest.clone(),
                    source: ComponentInstallSource::Bundled,
                    package_root: None,
                },
            );
        Ok(manifest)
    }

    pub async fn invoke(
        &self,
        request: ComponentInvocationRequest,
    ) -> Result<ComponentInvocationResult, ComponentRuntimeError> {
        let installed = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(&request.component_id)
            .cloned()
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentNotInstalled,
                    format!("组件 {} 未安装", request.component_id),
                )
            })?;
        let operation_id = request
            .operation_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let max_parallelism = installed.manifest.resources.max_parallelism.unwrap_or(1) as usize;
        if self.active_operation_count(&installed.manifest.id) >= max_parallelism {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentOperationConflict,
                format!("组件并行上限为 {max_parallelism}"),
            ));
        }
        let timeout_ms = request
            .timeout_ms
            .or(installed.manifest.resources.timeout_ms)
            .unwrap_or(300_000)
            .min(installed.manifest.resources.timeout_ms.unwrap_or(u64::MAX));
        let started_at = Utc::now().timestamp_millis();
        let control = Arc::new(OperationControl {
            summary: Mutex::new(ComponentOperationSummary {
                operation_id: operation_id.clone(),
                component_id: installed.manifest.id.clone(),
                component_version: installed.manifest.version.clone(),
                command: request.command.clone(),
                runtime: installed.manifest.runtime,
                status: ComponentOperationStatus::Starting,
                started_at,
                finished_at: None,
                duration_ms: None,
                pid: None,
                message: "正在准备组件".into(),
                log_path: None,
            }),
            cancelled: AtomicBool::new(false),
            pid: AtomicU32::new(0),
        });
        {
            let mut operations = self
                .operations
                .lock()
                .expect("component operation mutex poisoned");
            if operations.contains_key(&operation_id) {
                return Err(ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentOperationConflict,
                    format!("operationId 已存在: {operation_id}"),
                ));
            }
            operations.insert(operation_id.clone(), control.clone());
        }
        self.update_operation(&control, ComponentOperationStatus::Running, "组件正在运行");

        let payload = json!({
            "protocol": "nexora.component.v1",
            "operationId": operation_id,
            "componentId": installed.manifest.id,
            "componentVersion": installed.manifest.version,
            "command": request.command,
            "input": request.input,
        });
        let mut future = Box::pin(self.invoke_installed(&installed, &control, payload));
        let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms));
        tokio::pin!(deadline);
        let mut memory_interval = tokio::time::interval(Duration::from_secs(1));
        memory_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let memory_limit_bytes = installed
            .manifest
            .resources
            .max_memory_mb
            .map(|value| value.saturating_mul(1024 * 1024));
        let result = loop {
            tokio::select! {
                result = &mut future => break result,
                _ = &mut deadline => {
                    let pid = control.pid.load(Ordering::SeqCst);
                    if pid != 0 {
                        crate::process_utils::terminate_pid_tree(pid);
                    }
                    break Err(ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentOperationTimedOut,
                        format!("组件操作超过 {timeout_ms} ms"),
                    ));
                }
                _ = memory_interval.tick(), if memory_limit_bytes.is_some() => {
                    let pid = control.pid.load(Ordering::SeqCst);
                    let limit = memory_limit_bytes.unwrap_or(u64::MAX);
                    if pid != 0 && process_tree_memory_bytes(pid).is_some_and(|bytes| bytes > limit) {
                        crate::process_utils::terminate_pid_tree(pid);
                        break Err(ComponentRuntimeError::new(
                            ComponentRuntimeErrorCode::ComponentProcessFailed,
                            format!("组件进程树内存超过 {} MiB 限制", limit / 1024 / 1024),
                        ));
                    }
                }
            }
        };
        let finished_at = Utc::now().timestamp_millis();
        let duration_ms = finished_at.saturating_sub(started_at) as u64;
        let cancelled = control.cancelled.load(Ordering::SeqCst);
        let (status, message) = if cancelled {
            (
                ComponentOperationStatus::Cancelled,
                "操作已取消".to_string(),
            )
        } else if let Err(error) = &result {
            if error.code == ComponentRuntimeErrorCode::ComponentOperationTimedOut {
                (ComponentOperationStatus::TimedOut, error.message.clone())
            } else {
                (ComponentOperationStatus::Failed, error.message.clone())
            }
        } else {
            (
                ComponentOperationStatus::Completed,
                "操作已完成".to_string(),
            )
        };
        let log_path = match &result {
            Ok(response) => self
                .write_operation_log(&operation_id, &installed.manifest.id, &response.logs)
                .ok(),
            Err(error) if !error.details.is_empty() => self
                .write_operation_log(&operation_id, &installed.manifest.id, &error.details)
                .ok(),
            Err(_) => None,
        };
        let mut completed_summary = {
            let mut summary = control
                .summary
                .lock()
                .expect("component operation summary mutex poisoned");
            summary.status = status;
            summary.finished_at = Some(finished_at);
            summary.duration_ms = Some(duration_ms);
            summary.message = message;
            summary.log_path = log_path.clone();
            summary.clone()
        };
        completed_summary.pid = match control.pid.load(Ordering::SeqCst) {
            0 => None,
            pid => Some(pid),
        };
        self.operations
            .lock()
            .expect("component operation mutex poisoned")
            .remove(&operation_id);
        let mut history = self
            .history
            .lock()
            .expect("component operation history mutex poisoned");
        history.push_front(completed_summary.clone());
        history.truncate(MAX_OPERATION_HISTORY);
        drop(history);
        let _ = self
            .app_handle
            .emit("nexora:component-operation", &completed_summary);

        if cancelled {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentOperationCancelled,
                "组件操作已取消",
            ));
        }
        let response = result?;
        Ok(ComponentInvocationResult {
            operation_id,
            component_id: installed.manifest.id.clone(),
            component_version: installed.manifest.version.clone(),
            command: completed_summary.command,
            runtime: installed.manifest.runtime,
            output: response.output,
            logs: response.logs,
            duration_ms,
        })
    }

    pub fn cancel(&self, operation_id: &str) -> Result<(), ComponentRuntimeError> {
        let control = self
            .operations
            .lock()
            .expect("component operation mutex poisoned")
            .get(operation_id)
            .cloned()
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentOperationNotFound,
                    format!("未找到运行中的操作 {operation_id}"),
                )
            })?;
        control.cancelled.store(true, Ordering::SeqCst);
        let pid = control.pid.load(Ordering::SeqCst);
        if pid != 0 {
            crate::process_utils::terminate_pid_tree(pid);
        }
        self.update_operation(
            &control,
            ComponentOperationStatus::Cancelled,
            "正在终止组件进程",
        );
        Ok(())
    }

    pub async fn shutdown(&self) {
        let operation_ids = self
            .operations
            .lock()
            .expect("component operation mutex poisoned")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for operation_id in operation_ids {
            let _ = self.cancel(&operation_id);
        }
        let worker_ids = self
            .workers
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for component_id in worker_ids {
            self.shutdown_worker(&component_id).await;
        }
    }

    async fn invoke_installed(
        &self,
        installed: &InstalledComponent,
        control: &Arc<OperationControl>,
        payload: Value,
    ) -> Result<ProcessResponse, ComponentRuntimeError> {
        match host_adapter(&installed.manifest) {
            Some("builtin-rust") => {
                invoke_blendio_adapter(&installed.manifest, control, payload).await
            }
            Some("builtin-catalog") => invoke_catalog_adapter(&installed.manifest, payload),
            Some(other) => Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
                format!("未知宿主适配器: {other}"),
            )),
            None => match installed.manifest.runtime {
                ComponentRuntime::PythonAction => {
                    self.invoke_one_shot(installed, control, payload, true)
                        .await
                }
                ComponentRuntime::PythonWorker => {
                    self.invoke_worker(installed, control, payload).await
                }
                ComponentRuntime::NativeProcess => {
                    self.invoke_one_shot(installed, control, payload, false)
                        .await
                }
                ComponentRuntime::DataPack => invoke_data_pack(installed, payload),
                ComponentRuntime::NativeLibrary => {
                    self.invoke_native_library(installed, control, payload).await
                }
                ComponentRuntime::BuiltinRust => Err(ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
                    "builtin-rust 必须声明受信任 hostAdapter",
                )),
            },
        }
    }

    async fn invoke_native_library(
        &self,
        installed: &InstalledComponent,
        control: &Arc<OperationControl>,
        payload: Value,
    ) -> Result<ProcessResponse, ComponentRuntimeError> {
        let host = component_host_path().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostUnavailable,
                "未找到 pmc-component-host，native-library 不会在 Nexora 主进程中加载",
            )
        })?;
        let package_root = installed.package_root.as_ref().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "native-library 组件缺少安装目录",
            )
        })?;
        let entry = resolve_entry(package_root, &installed.manifest)?;
        let mut command = crate::process_utils::tokio_command(&host);
        command
            .args(["--dll", &entry.to_string_lossy(), "--component-id", &installed.manifest.id])
            .current_dir(package_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostUnavailable,
                format!("启动隔离组件宿主失败: {error}"),
            )
        })?;
        let pid = child.id().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostCrashed,
                "无法获取隔离组件宿主 PID",
            )
        })?;
        self.set_operation_pid(control, pid);
        let mut stdin = child.stdin.take().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostCrashed,
                "隔离组件宿主 stdin 不可用",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostCrashed,
                "隔离组件宿主 stdout 不可用",
            )
        })?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let ready = tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
            .await
            .map_err(|_| {
                crate::process_utils::terminate_pid_tree(pid);
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentHostAbiMismatch,
                    "隔离组件宿主 ABI 握手超时",
                )
            })?
            .map_err(|error| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentHostCrashed,
                    format!("读取隔离组件宿主握手失败: {error}"),
                )
            })?;
        let ready_value = serde_json::from_str::<Value>(line.trim()).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostAbiMismatch,
                format!("隔离组件宿主握手不是有效 JSON: {error}"),
            )
        })?;
        if ready == 0 || ready_value.get("type").and_then(Value::as_str) != Some("host-ready") {
            crate::process_utils::terminate_pid_tree(pid);
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostAbiMismatch,
                ready_value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("隔离组件宿主未通过 ABI 握手"),
            ));
        }
        let mut request = payload;
        if let Some(object) = request.as_object_mut() {
            object.insert("type".into(), Value::String("invoke".into()));
        }
        let mut bytes = serde_json::to_vec(&request).map_err(protocol_error)?;
        bytes.push(b'\n');
        stdin.write_all(&bytes).await.map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostCrashed,
                format!("向隔离组件宿主发送请求失败: {error}"),
            )
        })?;
        stdin.flush().await.map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostCrashed,
                format!("刷新隔离组件宿主请求失败: {error}"),
            )
        })?;
        line.clear();
        let read = tokio::time::timeout(Duration::from_secs(30), reader.read_line(&mut line))
            .await
            .map_err(|_| {
                crate::process_utils::terminate_pid_tree(pid);
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentOperationTimedOut,
                    "隔离组件宿主响应超时",
                )
            })?
            .map_err(|error| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentHostCrashed,
                    format!("读取隔离组件宿主响应失败: {error}"),
                )
            })?;
        let response = serde_json::from_str::<Value>(line.trim()).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostCrashed,
                format!("隔离组件宿主响应不是有效 JSON: {error}"),
            )
        })?;
        let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n").await;
        crate::process_utils::terminate_pid_tree(pid);
        control.pid.store(0, Ordering::SeqCst);
        if read == 0 || response.get("type").and_then(Value::as_str) == Some("error") {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostCrashed,
                response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("隔离组件宿主返回失败"),
            ));
        }
        Ok(ProcessResponse {
            output: response.get("result").cloned().unwrap_or(response),
            logs: vec![format!("host pid={pid}")],
        })
    }

    async fn invoke_one_shot(
        &self,
        installed: &InstalledComponent,
        control: &Arc<OperationControl>,
        payload: Value,
        python: bool,
    ) -> Result<ProcessResponse, ComponentRuntimeError> {
        let package_root = installed.package_root.as_ref().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "组件缺少安装目录",
            )
        })?;
        let entry = resolve_entry(package_root, &installed.manifest)?;
        let (program, args, env) = if python {
            let runtime =
                crate::plugin::prepare_pmc_python_runtime(&self.app_handle).map_err(|error| {
                    ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentProcessFailed,
                        error,
                    )
                })?;
            (
                runtime.program,
                vec![entry.to_string_lossy().into_owned()],
                runtime.env_vars,
            )
        } else {
            (
                entry.to_string_lossy().into_owned(),
                Vec::new(),
                HashMap::new(),
            )
        };
        let mut command = crate::process_utils::tokio_command(&program);
        command
            .args(args)
            .current_dir(package_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command.spawn().map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                format!("启动组件进程失败: {error}"),
            )
        })?;
        let pid = child.id().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                "无法获取组件进程 PID",
            )
        })?;
        self.set_operation_pid(control, pid);
        if let Some(mut stdin) = child.stdin.take() {
            let mut bytes = serde_json::to_vec(&payload).map_err(protocol_error)?;
            bytes.push(b'\n');
            stdin.write_all(&bytes).await.map_err(|error| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentProtocolError,
                    format!("向组件写入请求失败: {error}"),
                )
            })?;
        }
        let output = child.wait_with_output().await.map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                format!("等待组件进程失败: {error}"),
            )
        })?;
        control.pid.store(0, Ordering::SeqCst);
        let stdout = truncate_log(String::from_utf8_lossy(&output.stdout).into_owned());
        let stderr = truncate_log(String::from_utf8_lossy(&output.stderr).into_owned());
        let mut logs = stdout.lines().map(str::to_string).collect::<Vec<_>>();
        logs.extend(stderr.lines().map(|line| format!("stderr: {line}")));
        if !output.status.success() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                format!("组件进程退出码 {}", output.status.code().unwrap_or(-1)),
            )
            .with_details(logs));
        }
        Ok(ProcessResponse {
            output: parse_protocol_output(&stdout)?,
            logs,
        })
    }

    async fn invoke_worker(
        &self,
        installed: &InstalledComponent,
        control: &Arc<OperationControl>,
        payload: Value,
    ) -> Result<ProcessResponse, ComponentRuntimeError> {
        let component_id = installed.manifest.id.clone();
        let mut attempt = 0;
        loop {
            attempt += 1;
            let handle = self.worker_handle(installed).await?;
            self.set_operation_pid(control, handle.pid);
            let (response_tx, response_rx) = oneshot::channel();
            if handle
                .sender
                .send(WorkerMessage::Invoke {
                    request: payload.clone(),
                    response: response_tx,
                })
                .await
                .is_err()
            {
                self.workers.lock().await.remove(&component_id);
                if attempt < 2 {
                    continue;
                }
                return Err(ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentProcessFailed,
                    "Python Worker 已退出",
                ));
            }
            match response_rx.await {
                Ok(Ok(result)) => return Ok(result),
                Ok(Err(error)) if attempt < 2 => {
                    self.shutdown_worker(&component_id).await;
                    if control.cancelled.load(Ordering::SeqCst) {
                        return Err(error);
                    }
                }
                Ok(Err(error)) => return Err(error),
                Err(_) if attempt < 2 => {
                    self.shutdown_worker(&component_id).await;
                }
                Err(_) => {
                    return Err(ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentProcessFailed,
                        "Python Worker 未返回结果",
                    ));
                }
            }
        }
    }

    async fn worker_handle(
        &self,
        installed: &InstalledComponent,
    ) -> Result<WorkerHandle, ComponentRuntimeError> {
        let mut workers = self.workers.lock().await;
        if let Some(worker) = workers.get(&installed.manifest.id) {
            return Ok(worker.clone());
        }
        let package_root = installed.package_root.as_ref().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "Python Worker 缺少安装目录",
            )
        })?;
        let entry = resolve_entry(package_root, &installed.manifest)?;
        let runtime =
            crate::plugin::prepare_pmc_python_runtime(&self.app_handle).map_err(|error| {
                ComponentRuntimeError::new(ComponentRuntimeErrorCode::ComponentProcessFailed, error)
            })?;
        let mut command = crate::process_utils::tokio_command(&runtime.program);
        command
            .arg(entry)
            .current_dir(package_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, value) in runtime.env_vars {
            command.env(key, value);
        }
        let mut child = command.spawn().map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                format!("启动 Python Worker 失败: {error}"),
            )
        })?;
        let pid = child.id().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                "无法获取 Python Worker PID",
            )
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                "Python Worker stdin 不可用",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                "Python Worker stdout 不可用",
            )
        })?;
        let stderr = child.stderr.take();
        let mut reader = BufReader::new(stdout);
        let mut ready_line = String::new();
        let ready_read =
            tokio::time::timeout(WORKER_READY_TIMEOUT, reader.read_line(&mut ready_line))
                .await
                .map_err(|_| {
                    crate::process_utils::terminate_pid_tree(pid);
                    ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentOperationTimedOut,
                        "Python Worker 启动超时",
                    )
                })?
                .map_err(|error| {
                    ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentProtocolError,
                        format!("读取 Python Worker 就绪事件失败: {error}"),
                    )
                })?;
        if ready_read == 0 || !worker_ready(&ready_line) {
            crate::process_utils::terminate_pid_tree(pid);
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                format!("Python Worker 未发送 worker-ready: {}", ready_line.trim()),
            ));
        }
        let (sender, receiver) = mpsc::channel(16);
        let component_id = installed.manifest.id.clone();
        let worker_states = self.worker_states.clone();
        worker_states
            .lock()
            .expect("component worker state mutex poisoned")
            .insert(
                component_id.clone(),
                ComponentWorkerSummary {
                    component_id: component_id.clone(),
                    pid,
                    started_at: Utc::now().timestamp_millis(),
                    status: "ready".into(),
                },
            );
        tokio::spawn(worker_loop(
            component_id.clone(),
            pid,
            child,
            stdin,
            reader,
            stderr,
            receiver,
            worker_states,
        ));
        let handle = WorkerHandle { sender, pid };
        workers.insert(component_id, handle.clone());
        Ok(handle)
    }

    async fn shutdown_worker(&self, component_id: &str) {
        if let Some(worker) = self.workers.lock().await.remove(component_id) {
            let _ = worker.sender.send(WorkerMessage::Shutdown).await;
            tokio::time::sleep(Duration::from_millis(250)).await;
            crate::process_utils::terminate_pid_tree(worker.pid);
        }
        self.worker_states
            .lock()
            .expect("component worker state mutex poisoned")
            .remove(component_id);
    }

    fn active_operation_count(&self, component_id: &str) -> usize {
        self.operations
            .lock()
            .expect("component operation mutex poisoned")
            .values()
            .filter(|control| {
                control
                    .summary
                    .lock()
                    .expect("component operation summary mutex poisoned")
                    .component_id
                    == component_id
            })
            .count()
    }

    fn set_operation_pid(&self, control: &Arc<OperationControl>, pid: u32) {
        control.pid.store(pid, Ordering::SeqCst);
        control
            .summary
            .lock()
            .expect("component operation summary mutex poisoned")
            .pid = Some(pid);
    }

    fn update_operation(
        &self,
        control: &Arc<OperationControl>,
        status: ComponentOperationStatus,
        message: &str,
    ) {
        let summary = {
            let mut summary = control
                .summary
                .lock()
                .expect("component operation summary mutex poisoned");
            summary.status = status;
            summary.message = message.into();
            summary.clone()
        };
        let _ = self.app_handle.emit("nexora:component-operation", summary);
    }

    fn write_operation_log(
        &self,
        operation_id: &str,
        component_id: &str,
        logs: &[String],
    ) -> Result<String, ComponentRuntimeError> {
        fs::create_dir_all(&self.logs_path)
            .map_err(|error| io_error("创建组件日志目录失败", error))?;
        let path = self.logs_path.join(format!(
            "{}-{}.log",
            safe_name(component_id),
            safe_name(operation_id)
        ));
        let mut content = logs.join("\n");
        if content.len() > MAX_CAPTURED_LOG_BYTES {
            content.truncate(MAX_CAPTURED_LOG_BYTES);
            content.push_str("\n[日志已截断]");
        }
        fs::write(&path, content).map_err(|error| io_error("写入组件日志失败", error))?;
        cleanup_old_logs(&self.logs_path);
        Ok(path.to_string_lossy().into_owned())
    }
}

async fn worker_loop(
    component_id: String,
    pid: u32,
    mut child: tokio::process::Child,
    mut stdin: tokio::process::ChildStdin,
    mut stdout: BufReader<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    mut receiver: mpsc::Receiver<WorkerMessage>,
    worker_states: Arc<Mutex<BTreeMap<String, ComponentWorkerSummary>>>,
) {
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                    break;
                }
            }
        });
    }
    while let Some(message) = receiver.recv().await {
        match message {
            WorkerMessage::Shutdown => {
                let _ = stdin.write_all(b"{\"type\":\"shutdown\"}\n").await;
                break;
            }
            WorkerMessage::Invoke { request, response } => {
                if let Some(state) = worker_states
                    .lock()
                    .expect("component worker state mutex poisoned")
                    .get_mut(&component_id)
                {
                    state.status = "busy".into();
                }
                let result = invoke_worker_message(&mut stdin, &mut stdout, request).await;
                if let Some(state) = worker_states
                    .lock()
                    .expect("component worker state mutex poisoned")
                    .get_mut(&component_id)
                {
                    state.status = if result.is_ok() { "ready" } else { "error" }.into();
                }
                let _ = response.send(result);
            }
        }
    }
    crate::process_utils::terminate_pid_tree(pid);
    let _ = child.kill().await;
    let _ = child.wait().await;
    worker_states
        .lock()
        .expect("component worker state mutex poisoned")
        .remove(&component_id);
}

async fn invoke_worker_message(
    stdin: &mut tokio::process::ChildStdin,
    stdout: &mut BufReader<tokio::process::ChildStdout>,
    request: Value,
) -> Result<ProcessResponse, ComponentRuntimeError> {
    let operation_id = request
        .get("operationId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut bytes = serde_json::to_vec(&request).map_err(protocol_error)?;
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentProtocolError,
            format!("向 Python Worker 写入请求失败: {error}"),
        )
    })?;
    stdin.flush().await.map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentProtocolError,
            format!("刷新 Python Worker 请求失败: {error}"),
        )
    })?;
    let mut logs = Vec::new();
    loop {
        let mut line = String::new();
        let read = tokio::time::timeout(WORKER_HEARTBEAT_TIMEOUT, stdout.read_line(&mut line))
            .await
            .map_err(|_| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentOperationTimedOut,
                    "Python Worker 超过心跳时限未响应",
                )
            })?
            .map_err(|error| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentProtocolError,
                    format!("读取 Python Worker 输出失败: {error}"),
                )
            })?;
        if read == 0 {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                "Python Worker 已退出",
            ));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        logs.push(trimmed.to_string());
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let response_operation = value.get("operationId").and_then(Value::as_str);
        if response_operation != Some(operation_id.as_str()) {
            continue;
        }
        let message_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(message_type, "progress" | "heartbeat" | "log") {
            continue;
        }
        if value.get("ok").and_then(Value::as_bool) == Some(false)
            || matches!(message_type, "failed" | "error")
        {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Python Worker 返回失败")
                    .to_string(),
            )
            .with_details(logs));
        }
        let output = value.get("result").cloned().unwrap_or(value);
        return Ok(ProcessResponse { output, logs });
    }
}

async fn invoke_blendio_adapter(
    manifest: &ComponentManifestV1,
    control: &Arc<OperationControl>,
    payload: Value,
) -> Result<ProcessResponse, ComponentRuntimeError> {
    if manifest.id != super::builtin_components::PM_BLENDIO_COMPONENT_ID {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
            format!("组件 {} 没有内置 Rust 适配器", manifest.id),
        ));
    }
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let input = payload.get("input").cloned().unwrap_or(Value::Null);
    let path = input_path(&input)?;
    let cancelled = control.cancelled.load(Ordering::SeqCst);
    if cancelled {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentOperationCancelled,
            "操作已取消",
        ));
    }
    let command_for_worker = command.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<Value, ComponentRuntimeError> {
        let path_buf = PathBuf::from(&path);
        match command_for_worker.as_str() {
            "inspect" | "read-render-settings" => {
                let file = blendio::BlendFile::open(&path_buf).map_err(component_process_error)?;
                let summary = blendio::summarize(&file).map_err(component_process_error)?;
                let external = blendio::collect_external_data_with_base(&file, Some(&path_buf))
                    .map_err(component_process_error)?;
                Ok(json!({ "summary": summary, "externalData": external }))
            }
            "collect-external-data" => {
                let file = blendio::BlendFile::open(&path_buf).map_err(component_process_error)?;
                let external = blendio::collect_external_data_with_base(&file, Some(&path_buf))
                    .map_err(component_process_error)?;
                serde_json::to_value(external).map_err(protocol_error)
            }
            "extract-preview" => {
                let preview = blendio::extract_preview_from_path(&path_buf)
                    .map_err(component_process_error)?;
                let Some(preview) = preview else {
                    return Ok(Value::Null);
                };
                let image = image::RgbaImage::from_raw(preview.width, preview.height, preview.rgba)
                    .ok_or_else(|| {
                        ComponentRuntimeError::new(
                            ComponentRuntimeErrorCode::ComponentProcessFailed,
                            "Blender 预览像素无效",
                        )
                    })?;
                let mut bytes = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(image)
                    .write_to(&mut bytes, image::ImageFormat::Png)
                    .map_err(component_process_error)?;
                Ok(json!({
                    "width": preview.width,
                    "height": preview.height,
                    "pngBase64": base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()),
                }))
            }
            "edit-render-settings" => {
                let scene_selector = serde_json::from_value::<blendio::SceneSelector>(
                    input.get("sceneSelector").cloned().unwrap_or(Value::Null),
                )
                .map_err(protocol_error)?;
                let edit = serde_json::from_value::<blendio::SceneRenderEdit>(
                    input.get("edit").cloned().unwrap_or(Value::Null),
                )
                .map_err(protocol_error)?;
                let options = input
                    .get("options")
                    .cloned()
                    .map(serde_json::from_value::<blendio::WriteOptions>)
                    .transpose()
                    .map_err(protocol_error)?
                    .unwrap_or_default();
                let mut session = blendio::BlendEditSession::open(&path_buf)
                    .map_err(component_process_error)?;
                session
                    .edit_scene_render(scene_selector, edit)
                    .map_err(component_process_error)?;
                let report = session.commit(options).map_err(component_process_error)?;
                serde_json::to_value(report).map_err(protocol_error)
            }
            other => Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                format!("BlenderIO 不支持命令 {other}"),
            )),
        }
    })
    .await
    .map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentProcessFailed,
            format!("BlenderIO 适配器任务失败: {error}"),
        )
    })??;
    if control.cancelled.load(Ordering::SeqCst) {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentOperationCancelled,
            "操作已取消，旧结果已丢弃",
        ));
    }
    Ok(ProcessResponse {
        output: result,
        logs: vec![format!("BlenderIO {command} completed")],
    })
}

fn invoke_catalog_adapter(
    manifest: &ComponentManifestV1,
    payload: Value,
) -> Result<ProcessResponse, ComponentRuntimeError> {
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("describe");
    if !matches!(command, "describe" | "list") {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentProtocolError,
            format!("内置资料目录不支持命令 {command}"),
        ));
    }
    Ok(ProcessResponse {
        output: json!({
            "componentId": manifest.id,
            "contributes": manifest.contributes,
        }),
        logs: Vec::new(),
    })
}

fn invoke_data_pack(
    installed: &InstalledComponent,
    payload: Value,
) -> Result<ProcessResponse, ComponentRuntimeError> {
    let root = installed.package_root.as_ref().ok_or_else(|| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "资料包缺少安装目录",
        )
    })?;
    let data_root = resolve_entry(root, &installed.manifest)?;
    let command = payload
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("describe");
    let input = payload.get("input").cloned().unwrap_or(Value::Null);
    let output = match command {
        "describe" => {
            let (files, bytes) = directory_stats(&data_root)?;
            json!({ "files": files, "bytes": bytes, "entry": installed.manifest.entry })
        }
        "list" => {
            let relative = input.get("path").and_then(Value::as_str).unwrap_or("");
            let directory = resolve_data_path(&data_root, relative)?;
            if !directory.is_dir() {
                return Err(ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentPackageInvalid,
                    "资料包 list 目标不是目录",
                ));
            }
            let mut entries = fs::read_dir(directory)
                .map_err(|error| io_error("读取资料包目录失败", error))?
                .filter_map(Result::ok)
                .map(|entry| {
                    let metadata = entry.metadata().ok();
                    json!({
                        "name": entry.file_name().to_string_lossy(),
                        "isDirectory": metadata.as_ref().is_some_and(|metadata| metadata.is_dir()),
                        "bytes": metadata.map(|metadata| metadata.len()).unwrap_or_default(),
                    })
                })
                .collect::<Vec<_>>();
            entries.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
            Value::Array(entries)
        }
        "read-json" | "read-text" => {
            let relative = input.get("path").and_then(Value::as_str).ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentProtocolError,
                    "资料包读取命令缺少 path",
                )
            })?;
            let path = resolve_data_path(&data_root, relative)?;
            let bytes = fs::read(&path).map_err(|error| io_error("读取资料包文件失败", error))?;
            if bytes.len() > 2 * 1024 * 1024 {
                return Err(ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentPackageInvalid,
                    "资料包单次读取限制为 2 MiB",
                ));
            }
            if command == "read-json" {
                serde_json::from_slice(&bytes).map_err(protocol_error)?
            } else {
                Value::String(String::from_utf8(bytes).map_err(protocol_error)?)
            }
        }
        _ => {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                format!("资料包不支持命令 {command}"),
            ))
        }
    };
    Ok(ProcessResponse {
        output,
        logs: Vec::new(),
    })
}

fn load_catalog(
    packages_path: &Path,
    bundled: &BTreeMap<String, ComponentManifestV1>,
    state: &StoredComponentRuntimeState,
) -> Result<BTreeMap<String, InstalledComponent>, ComponentRuntimeError> {
    let mut catalog = bundled
        .values()
        .filter(|manifest| !state.removed_bundled.contains(&manifest.id))
        .filter(|manifest| {
            manifest
                .extensions
                .get("autoInstall")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
        .map(|manifest| {
            (
                manifest.id.clone(),
                InstalledComponent {
                    manifest: manifest.clone(),
                    source: ComponentInstallSource::Bundled,
                    package_root: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    if !packages_path.exists() {
        return Ok(catalog);
    }
    for entry in fs::read_dir(packages_path).map_err(|error| io_error("扫描组件目录失败", error))?
    {
        let entry = entry.map_err(|error| io_error("读取组件目录项失败", error))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join(COMPONENT_MANIFEST_FILE);
        if !manifest_path.is_file() {
            continue;
        }
        let manifest = read_component_manifest(&manifest_path)?;
        validate_entry(&path, &manifest)?;
        catalog.insert(
            manifest.id.clone(),
            InstalledComponent {
                manifest,
                source: ComponentInstallSource::Local,
                package_root: Some(path),
            },
        );
    }
    Ok(catalog)
}

fn read_component_manifest(path: &Path) -> Result<ComponentManifestV1, ComponentRuntimeError> {
    let input =
        fs::read_to_string(path).map_err(|error| io_error("读取 component.json 失败", error))?;
    parse_component_manifest(&input).map_err(contract_error)
}

fn inspect_component_package(path: &Path) -> Result<ComponentPackageInspection, ComponentRuntimeError> {
    let file = fs::File::open(path).map_err(|error| io_error("打开组件归档失败", error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("组件包不是有效 ZIP 归档: {error}"),
        )
    })?;
    let mut seen = BTreeSet::new();
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    let mut digest = blake3::Hasher::new();
    let mut manifest_bytes = None;
    let mut package_header_bytes = None;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("读取组件归档条目失败: {error}"),
            )
        })?;
        let name = entry.name().to_string();
        validate_archive_entry_name(&name)?;
        if !seen.insert(name.clone()) {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("组件包包含重复条目: {name}"),
            ));
        }
        if entry.is_dir() {
            continue;
        }
        if entry.encrypted()
            || entry
                .unix_mode()
                .map(|mode| mode & 0o170000 == 0o120000)
                .unwrap_or(false)
        {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("组件包不允许加密条目或符号链接: {name}"),
            ));
        }
        file_count += 1;
        if file_count > MAX_COMPONENT_FILES {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "组件包文件数超过安全限制",
            ));
        }
        let declared_size = entry.size();
        if declared_size > 128 * 1024 * 1024 {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("组件包单文件超过安全限制: {name}"),
            ));
        }
        let mut bytes = Vec::with_capacity(declared_size.min(8 * 1024 * 1024) as usize);
        entry.read_to_end(&mut bytes).map_err(|error| io_error("读取组件归档内容失败", error))?;
        if bytes.len() > 128 * 1024 * 1024 {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("组件包单文件实际大小超过安全限制: {name}"),
            ));
        }
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        if total_bytes > MAX_COMPONENT_BYTES {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "组件包解压总大小超过安全限制",
            ));
        }
        digest.update(name.as_bytes());
        digest.update(&bytes);
        if name == COMPONENT_MANIFEST_FILE {
            manifest_bytes = Some(bytes);
        } else if name == "manifest.json" {
            package_header_bytes = Some(bytes);
        }
    }
    let manifest_input = manifest_bytes.ok_or_else(|| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "组件包根目录缺少 component.json",
        )
    })?;
    let mut warnings = Vec::new();
    if let Some(header_bytes) = package_header_bytes {
        let header = parse_package_header(
            std::str::from_utf8(&header_bytes).map_err(|error| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentPackageInvalid,
                    format!("组件包 manifest.json 不是 UTF-8: {error}"),
                )
            })?,
        )
        .map_err(contract_error)?;
        if header.kind != PackageKind::ComponentPack
            || header.payload.path != COMPONENT_MANIFEST_FILE
            || header.payload.digest.algorithm != DigestAlgorithm::Blake3
            || header.payload.size_bytes != manifest_input.len() as u64
            || header.payload.digest.value != blake3::hash(&manifest_input).to_hex().to_string()
        {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "组件包 manifest.json 的类型、路径、大小或 BLAKE3 摘要不匹配",
            ));
        }
    } else {
        warnings.push("未提供 PMC_PACKAGE 包头，按兼容模式检查 component.json".into());
    }
    let manifest = parse_component_manifest(
        std::str::from_utf8(&manifest_input).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("component.json 不是 UTF-8: {error}"),
            )
        })?,
    )
    .map_err(contract_error)?;
    Ok(ComponentPackageInspection {
        package_path: path.to_string_lossy().into_owned(),
        valid: true,
        component_id: Some(manifest.id),
        component_name: Some(manifest.name),
        component_version: Some(manifest.version),
        file_count,
        total_bytes,
        package_digest: Some(digest.finalize().to_hex().to_string()),
        warnings,
    })
}

fn extract_component_package(path: &Path, destination: &Path) -> Result<(), ComponentRuntimeError> {
    let _ = inspect_component_package(path)?;
    let file = fs::File::open(path).map_err(|error| io_error("打开组件归档失败", error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("组件包不是有效 ZIP 归档: {error}"),
        )
    })?;
    fs::create_dir_all(destination).map_err(|error| io_error("创建组件解压暂存目录失败", error))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("读取组件归档条目失败: {error}"),
            )
        })?;
        let name = entry.name().to_string();
        validate_archive_entry_name(&name)?;
        let target = destination.join(Path::new(&name));
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| io_error("创建组件解压目录失败", error))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error("创建组件解压目录失败", error))?;
        }
        let mut output = fs::File::create(&target).map_err(|error| io_error("创建组件解压文件失败", error))?;
        std::io::copy(&mut entry, &mut output).map_err(|error| io_error("解压组件文件失败", error))?;
    }
    Ok(())
}

fn validate_archive_entry_name(name: &str) -> Result<(), ComponentRuntimeError> {
    let path = Path::new(name);
    if name.is_empty()
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\\')
        || name.contains(':')
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("组件包路径不安全: {name}"),
        ));
    }
    Ok(())
}

fn validate_entry(
    root: &Path,
    manifest: &ComponentManifestV1,
) -> Result<(), ComponentRuntimeError> {
    let Some(entry) = &manifest.entry else {
        return Ok(());
    };
    let path = resolve_relative_path(root, entry)?;
    if host_adapter(manifest).is_none() && !path.exists() {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("组件入口不存在: {}", path.display()),
        ));
    }
    Ok(())
}

fn resolve_entry(
    root: &Path,
    manifest: &ComponentManifestV1,
) -> Result<PathBuf, ComponentRuntimeError> {
    let entry = manifest.entry.as_deref().ok_or_else(|| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "组件清单缺少 entry",
        )
    })?;
    let path = resolve_relative_path(root, entry)?;
    if !path.exists() {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("组件入口不存在: {}", path.display()),
        ));
    }
    Ok(path)
}

fn resolve_relative_path(root: &Path, relative: &str) -> Result<PathBuf, ComponentRuntimeError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("组件相对路径不安全: {relative}"),
        ));
    }
    Ok(root.join(path))
}

fn resolve_data_path(root: &Path, relative: &str) -> Result<PathBuf, ComponentRuntimeError> {
    let path = resolve_relative_path(root, relative)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|error| io_error("资料包入口不可用", error))?;
    let canonical_path =
        fs::canonicalize(&path).map_err(|error| io_error("资料包路径不可用", error))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "资料包路径越界",
        ));
    }
    Ok(canonical_path)
}

fn copy_component_tree(source: &Path, destination: &Path) -> Result<(), ComponentRuntimeError> {
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    fs::create_dir_all(destination).map_err(|error| io_error("创建组件暂存目录失败", error))?;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("扫描组件来源失败: {error}"),
            )
        })?;
        if entry.path() == source {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|error| io_error("读取组件文件失败", error))?;
        if metadata.file_type().is_symlink() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "组件目录不能包含符号链接",
            ));
        }
        let relative = entry.path().strip_prefix(source).map_err(|_| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "组件来源路径无法规范化",
            )
        })?;
        let target = destination.join(relative);
        if metadata.is_dir() {
            fs::create_dir_all(&target).map_err(|error| io_error("创建组件子目录失败", error))?;
            continue;
        }
        file_count += 1;
        total_bytes = total_bytes.saturating_add(metadata.len());
        if file_count > MAX_COMPONENT_FILES || total_bytes > MAX_COMPONENT_BYTES {
            let _ = fs::remove_dir_all(destination);
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                "组件目录超过 R9 本地安装大小限制",
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error("创建组件文件目录失败", error))?;
        }
        fs::copy(entry.path(), &target).map_err(|error| io_error("复制组件文件失败", error))?;
    }
    Ok(())
}

fn template_catalog<'a>(
    components: impl Iterator<Item = &'a InstalledComponent>,
) -> PresentationTemplateCatalog {
    let mut catalog = PresentationTemplateCatalog::default();
    for component in components {
        let owner = PresentationTemplateOwner {
            component_id: component.manifest.id.clone(),
            component_name: component.manifest.name.clone(),
            component_version: component.manifest.version.clone(),
        };
        catalog.shell_templates.extend(
            component
                .manifest
                .contributes
                .shell_templates
                .iter()
                .cloned()
                .map(|template| ShellTemplateCatalogItem {
                    template,
                    owner: owner.clone(),
                }),
        );
        catalog.page_templates.extend(
            component
                .manifest
                .contributes
                .page_templates
                .iter()
                .cloned()
                .map(|template| PageTemplateCatalogItem {
                    template,
                    owner: owner.clone(),
                }),
        );
        catalog.theme_presets.extend(
            component
                .manifest
                .contributes
                .theme_presets
                .iter()
                .cloned()
                .map(|preset| ThemePresetCatalogItem {
                    preset,
                    owner: owner.clone(),
                }),
        );
    }
    catalog
        .shell_templates
        .sort_by(|left, right| left.template.id.cmp(&right.template.id));
    catalog
        .page_templates
        .sort_by(|left, right| left.template.id.cmp(&right.template.id));
    catalog
        .theme_presets
        .sort_by(|left, right| left.preset.id.cmp(&right.preset.id));
    catalog
}

fn load_state(path: &Path) -> Result<StoredComponentRuntimeState, ComponentRuntimeError> {
    if !path.exists() {
        return Ok(StoredComponentRuntimeState::default());
    }
    let input = fs::read_to_string(path).map_err(|error| io_error("读取组件状态失败", error))?;
    let state =
        serde_json::from_str::<StoredComponentRuntimeState>(&input).map_err(protocol_error)?;
    if state.schema_version != COMPONENT_STATE_SCHEMA_VERSION {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("不支持组件状态版本 {}", state.schema_version),
        ));
    }
    Ok(state)
}

fn persist_state(
    path: &Path,
    state: &StoredComponentRuntimeState,
) -> Result<(), ComponentRuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error("创建组件状态目录失败", error))?;
    }
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    let mut bytes = serde_json::to_vec_pretty(state).map_err(protocol_error)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| io_error("创建组件状态临时文件失败", error))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| io_error("写入组件状态失败", error))?;
    let backup = path.with_extension(format!("{}.bak", Uuid::new_v4().simple()));
    let result = (|| {
        if path.exists() {
            fs::rename(path, &backup).map_err(|error| io_error("备份旧组件状态失败", error))?;
        }
        if let Err(error) = fs::rename(&temporary, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(io_error("提交组件状态失败", error));
        }
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| io_error("清理组件状态备份失败", error))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn input_path(input: &Value) -> Result<String, ComponentRuntimeError> {
    input
        .get("path")
        .or_else(|| input.get("file"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                "组件命令缺少 path/file 参数",
            )
        })
}

fn host_adapter(manifest: &ComponentManifestV1) -> Option<&str> {
    manifest
        .extensions
        .get("hostAdapter")
        .and_then(Value::as_str)
}

fn component_host_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("NEXORA_COMPONENT_HOST_PATH") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join(if cfg!(target_os = "windows") {
                "pmc-component-host.exe"
            } else {
                "pmc-component-host"
            }));
            let resource_name = if cfg!(target_os = "windows") {
                "pmc-component-host.exe"
            } else {
                "pmc-component-host"
            };
            candidates.push(parent.join("resources").join("component-host").join(resource_name));
            candidates.push(parent.join("_up_").join("resources").join("component-host").join(resource_name));
            if let Some(debug_root) = parent.parent() {
                candidates.push(debug_root.join(if cfg!(target_os = "windows") {
                    "pmc-component-host.exe"
                } else {
                    "pmc-component-host"
                }));
            }
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn worker_ready(line: &str) -> bool {
    serde_json::from_str::<Value>(line.trim()).is_ok_and(|value| {
        value.get("type").and_then(Value::as_str) == Some("worker-ready")
            || value.get("status").and_then(Value::as_str) == Some("ready")
    })
}

fn parse_protocol_output(stdout: &str) -> Result<Value, ComponentRuntimeError> {
    for line in stdout.lines().rev() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if value.get("ok").and_then(Value::as_bool) == Some(false) {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("组件返回失败")
                    .to_string(),
            ));
        }
        return Ok(value.get("result").cloned().unwrap_or(value));
    }
    if stdout.trim().is_empty() {
        Ok(Value::Null)
    } else {
        Ok(Value::String(stdout.trim().to_string()))
    }
}

fn truncate_log(mut value: String) -> String {
    if value.len() > MAX_CAPTURED_LOG_BYTES {
        value.truncate(MAX_CAPTURED_LOG_BYTES);
        value.push_str("\n[输出已截断]");
    }
    value
}

fn cleanup_old_logs(logs_path: &Path) {
    let Ok(entries) = fs::read_dir(logs_path) else {
        return;
    };
    let mut files = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((metadata.modified().ok(), entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in files.into_iter().skip(MAX_OPERATION_HISTORY) {
        let _ = fs::remove_file(path);
    }
}

fn directory_stats(path: &Path) -> Result<(usize, u64), ComponentRuntimeError> {
    if path.is_file() {
        return fs::metadata(path)
            .map(|metadata| (1, metadata.len()))
            .map_err(|error| io_error("读取资料包大小失败", error));
    }
    let mut files = 0usize;
    let mut bytes = 0u64;
    for entry in WalkDir::new(path) {
        let entry = entry.map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentIoError,
                format!("扫描资料包失败: {error}"),
            )
        })?;
        let metadata = entry
            .metadata()
            .map_err(|error| io_error("读取资料包文件失败", error))?;
        if metadata.is_file() {
            files += 1;
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok((files, bytes))
}

fn process_tree_memory_bytes(root_pid: u32) -> Option<u64> {
    let mut system = System::new_all();
    system.refresh_processes();
    let root_pid = Pid::from_u32(root_pid);
    if !system.processes().contains_key(&root_pid) {
        return None;
    }
    let mut process_tree = HashSet::from([root_pid]);
    loop {
        let previous_count = process_tree.len();
        for (pid, process) in system.processes() {
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
    Some(process_tree.into_iter().fold(0u64, |total, pid| {
        total.saturating_add(
            system
                .process(pid)
                .map(|process| process.memory())
                .unwrap_or(0),
        )
    }))
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn component_process_error(error: impl std::fmt::Display) -> ComponentRuntimeError {
    ComponentRuntimeError::new(
        ComponentRuntimeErrorCode::ComponentProcessFailed,
        error.to_string(),
    )
}

fn protocol_error(error: impl std::fmt::Display) -> ComponentRuntimeError {
    ComponentRuntimeError::new(
        ComponentRuntimeErrorCode::ComponentProtocolError,
        error.to_string(),
    )
}

fn contract_error(error: impl std::fmt::Display) -> ComponentRuntimeError {
    ComponentRuntimeError::new(
        ComponentRuntimeErrorCode::ComponentManifestInvalid,
        error.to_string(),
    )
}

fn io_error(message: &str, error: impl std::fmt::Display) -> ComponentRuntimeError {
    ComponentRuntimeError::new(
        ComponentRuntimeErrorCode::ComponentIoError,
        format!("{message}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn relative_component_paths_reject_parent_segments() {
        let root = std::env::temp_dir();
        let error = resolve_relative_path(&root, "../outside.exe").unwrap_err();
        assert_eq!(
            error.code,
            ComponentRuntimeErrorCode::ComponentPackageInvalid
        );
    }

    #[test]
    fn protocol_output_uses_last_json_result() {
        let output =
            parse_protocol_output("loading\n{\"ok\":true,\"result\":{\"value\":7}}\n").unwrap();
        assert_eq!(output["value"], 7);
    }

    #[test]
    fn archive_paths_reject_windows_and_parent_traversal() {
        for path in ["../outside", "bin\\tool.exe", "C:/tool.exe", "/absolute"] {
            assert!(validate_archive_entry_name(path).is_err(), "{path}");
        }
        assert!(validate_archive_entry_name("bin/tool.exe").is_ok());
    }

    #[test]
    fn component_package_inspection_reads_manifest_and_digest() {
        let path = std::env::temp_dir().join(format!("nexora-test-{}.pmc-pack", Uuid::new_v4()));
        let file = fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "id": "test.pack",
            "name": "测试组件包",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "builtin-rust",
            "platforms": ["any"]
        });
        writer.start_file("component.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file("data/readme.txt", options).unwrap();
        writer.write_all(b"safe").unwrap();
        writer.finish().unwrap();

        let inspection = inspect_component_package(&path).unwrap();
        assert!(inspection.valid);
        assert_eq!(inspection.component_id.as_deref(), Some("test.pack"));
        assert_eq!(inspection.file_count, 2);
        assert!(inspection.package_digest.is_some());
        let _ = fs::remove_file(path);
    }
}
