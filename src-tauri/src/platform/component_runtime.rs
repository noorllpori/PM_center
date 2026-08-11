use super::presentation_templates::{
    load_presentation_template_preview, validate_presentation_component,
    InterfaceTemplateDiagnostic, InterfaceTemplateLayoutValidation, PresentationTemplatePreview,
    PresentationTemplatePreviewRequest,
};
use base64::Engine;
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use pmc_platform::{
    parse_component_manifest, parse_package_header, validate_component_graph, ComponentManifestV1,
    ComponentRuntime, ComponentSurfaceInstanceMode, DigestAlgorithm, PackageKind,
    PageTemplateContribution, PlatformTarget, ProfileTemplateSlotBinding,
    ShellTemplateContribution, TemplateSlotAccepts, TemplateSlotDefinition, TemplateSlotLayout,
    TemplateSlotMultiplicity, ThemePresetContribution, ValidateContract, WorkspaceProfileV1,
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
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
use walkdir::WalkDir;
use zip::ZipArchive;

const COMPONENT_STATE_SCHEMA_VERSION: u16 = 1;
const COMPONENT_TRUST_SCHEMA_VERSION: u16 = 1;
const COMPONENT_MANIFEST_FILE: &str = "component.json";
const COMPONENT_TRUST_FILE: &str = "trusted-publishers.json";
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
    ComponentPackageUntrusted,
    ComponentPackageSignatureInvalid,
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
    pub disabled_components: Vec<InstalledComponentSummary>,
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
    pub content_digest: Option<String>,
    pub publisher: Option<ComponentPackagePublisher>,
    pub license: Option<String>,
    pub trust: ComponentPackageTrust,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPackagePublisher {
    pub id: String,
    pub display_name: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentPackageTrustStatus {
    IntegrityOnly,
    SignedUntrusted,
    Trusted,
    InvalidSignature,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPackageTrust {
    pub status: ComponentPackageTrustStatus,
    pub signature_present: bool,
    pub signature_valid: bool,
    pub installable: bool,
    pub message: String,
}

/// An archive that can be embedded in a portable workspace.  The path is
/// deliberately kept inside the component runtime; callers only receive a
/// validated, immutable archive path rather than a component directory.
#[derive(Debug, Clone)]
pub struct WorkspaceComponentArchive {
    pub component_id: String,
    pub version: Option<String>,
    pub version_requirement: String,
    pub package_path: Option<PathBuf>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceComponentInstallResult {
    /// Components that did not exist before this import. They are the only
    /// components that may be removed if profile persistence subsequently
    /// fails.
    pub installed_component_ids: Vec<String>,
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
    #[serde(default)]
    disabled_components: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTrustedPublisher {
    id: String,
    display_name: String,
    public_key: String,
    trusted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredComponentTrustState {
    schema_version: u16,
    publishers: Vec<StoredTrustedPublisher>,
}

impl Default for StoredComponentTrustState {
    fn default() -> Self {
        Self {
            schema_version: COMPONENT_TRUST_SCHEMA_VERSION,
            publishers: Vec::new(),
        }
    }
}

impl Default for StoredComponentRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: COMPONENT_STATE_SCHEMA_VERSION,
            removed_bundled: BTreeSet::new(),
            disabled_components: BTreeSet::new(),
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
    cancellation_path: PathBuf,
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

pub(crate) struct ProcessResponse {
    pub(crate) output: Value,
    pub(crate) logs: Vec<String>,
}

pub struct ComponentRuntimeManager {
    app_handle: AppHandle,
    root_path: PathBuf,
    packages_path: PathBuf,
    archives_path: PathBuf,
    state_path: PathBuf,
    trust_path: PathBuf,
    logs_path: PathBuf,
    bundled: BTreeMap<String, ComponentManifestV1>,
    state: Mutex<StoredComponentRuntimeState>,
    trusted_publishers: Mutex<BTreeMap<String, StoredTrustedPublisher>>,
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
        let archives_path = root_path.join("archives");
        let logs_path = root_path.join("logs");
        let state_path = root_path.join("runtime-state.json");
        let trust_path = root_path.join(COMPONENT_TRUST_FILE);
        fs::create_dir_all(&packages_path)
            .and_then(|_| fs::create_dir_all(&archives_path))
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
        let trusted_publishers = load_trusted_publishers(&trust_path)?
            .publishers
            .into_iter()
            .map(|publisher| (publisher.id.clone(), publisher))
            .collect::<BTreeMap<_, _>>();
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
            archives_path,
            state_path,
            trust_path,
            logs_path,
            bundled,
            state: Mutex::new(state),
            trusted_publishers: Mutex::new(trusted_publishers),
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

    /// Manifests that still have an installation on disk, including globally
    /// disabled components. Profiles use this catalog for contract validation
    /// so disabling a component does not turn a valid profile into a missing-
    /// component recovery failure on the next launch.
    pub fn installed_manifests(&self) -> Vec<ComponentManifestV1> {
        let mut manifests = self
            .manifests()
            .into_iter()
            .map(|manifest| (manifest.id.clone(), manifest))
            .collect::<BTreeMap<_, _>>();
        let disabled = self
            .state
            .lock()
            .expect("component state mutex poisoned")
            .disabled_components
            .clone();
        for component_id in disabled {
            if let Some(manifest) = self.stored_manifest(&component_id) {
                manifests.insert(component_id, manifest);
            }
        }
        manifests.into_values().collect()
    }

    pub fn manifest(&self, component_id: &str) -> Option<ComponentManifestV1> {
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(component_id)
            .map(|component| component.manifest.clone())
    }

    pub fn stored_manifest(&self, component_id: &str) -> Option<ComponentManifestV1> {
        self.manifest(component_id)
            .or_else(|| self.bundled.get(component_id).cloned())
            .or_else(|| {
                read_component_manifest(
                    &self
                        .packages_path
                        .join(component_id)
                        .join(COMPONENT_MANIFEST_FILE),
                )
                .ok()
            })
    }

    pub fn install_source(&self, component_id: &str) -> Option<ComponentInstallSource> {
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(component_id)
            .map(|component| component.source)
    }

    pub fn package_root(&self, component_id: &str) -> Option<PathBuf> {
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(component_id)
            .and_then(|component| component.package_root.clone())
    }

    pub fn content_digest(&self, component_id: &str) -> Option<String> {
        let catalog = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned");
        let component = catalog.get(component_id)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(serde_json::to_string(&component.manifest).ok()?.as_bytes());
        if let Some(root) = &component.package_root {
            let mut files = WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| entry.path().to_path_buf())
                .collect::<Vec<_>>();
            files.sort();
            for path in files {
                let relative = path.strip_prefix(root).ok()?;
                hasher.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
                let mut file = fs::File::open(path).ok()?;
                let mut buffer = [0u8; 64 * 1024];
                loop {
                    let read = file.read(&mut buffer).ok()?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                }
            }
        }
        Some(hasher.finalize().to_hex().to_string())
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

        let state = self
            .state
            .lock()
            .expect("component state mutex poisoned")
            .clone();
        let mut disabled_ids = state.disabled_components.clone();
        disabled_ids.extend(state.removed_bundled.iter().cloned());
        disabled_ids.extend(
            self.bundled
                .keys()
                .filter(|component_id| !installed_ids.contains(*component_id))
                .cloned(),
        );
        let mut disabled_components = disabled_ids
            .iter()
            .filter_map(|component_id| {
                let (manifest, source, package_root) =
                    if let Some(manifest) = self.bundled.get(component_id) {
                        (manifest.clone(), ComponentInstallSource::Bundled, None)
                    } else {
                        let package_root = self.packages_path.join(component_id);
                        let manifest =
                            read_component_manifest(&package_root.join(COMPONENT_MANIFEST_FILE))
                                .ok()?;
                        let source = if self.archive_path_for(&manifest).is_file() {
                            ComponentInstallSource::Marketplace
                        } else {
                            ComponentInstallSource::Local
                        };
                        (manifest, source, Some(package_root))
                    };
                Some(InstalledComponentSummary {
                    removable: manifest
                        .extensions
                        .get("removable")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    host_adapter: host_adapter(&manifest).map(str::to_string),
                    manifest,
                    source,
                    package_path: package_root.map(|path| path.to_string_lossy().into_owned()),
                    active_operation_count: 0,
                    worker: None,
                })
            })
            .collect::<Vec<_>>();
        disabled_components.sort_by(|left, right| left.manifest.name.cmp(&right.manifest.name));

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
            disabled_components,
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
        validate_platform_compatibility(&staged_manifest)?;
        validate_native_library_entry(&staging, &staged_manifest)?;
        validate_presentation_component(&staging, &staged_manifest)?;
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
        {
            let mut state = self.state.lock().expect("component state mutex poisoned");
            state.disabled_components.remove(&staged_manifest.id);
            if self.bundled.contains_key(&staged_manifest.id) {
                state.removed_bundled.remove(&staged_manifest.id);
            }
            persist_state(&self.state_path, &state)?;
        }
        Ok(staged_manifest)
    }

    pub fn inspect_package(
        &self,
        package_path: &str,
    ) -> Result<ComponentPackageInspection, ComponentRuntimeError> {
        let trusted = self
            .trusted_publishers
            .lock()
            .expect("component trust mutex poisoned")
            .clone();
        inspect_component_package_with_trust(Path::new(package_path), &trusted)
    }

    pub fn read_checked_package_manifest(
        &self,
        package_path: &str,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        // `inspect_package` has already completed the archive path, size and
        // digest checks. This variant lets a workspace preview show an
        // untrusted publisher as a blocking item instead of failing before a
        // user can see what the archive contains. It must never be used to
        // execute or install the package.
        let _ = self.inspect_package(package_path)?;
        read_component_manifest_from_package(Path::new(package_path))
    }

    fn archive_path_for(&self, manifest: &ComponentManifestV1) -> PathBuf {
        self.archives_path.join(archive_file_name(manifest))
    }

    fn cache_component_package(
        &self,
        package_path: &Path,
        manifest: &ComponentManifestV1,
    ) -> Result<(), ComponentRuntimeError> {
        fs::create_dir_all(&self.archives_path)
            .map_err(|error| io_error("创建组件归档缓存目录失败", error))?;
        let destination = self.archive_path_for(manifest);
        copy_file_atomic(package_path, &destination)
    }

    /// Returns the exact, previously verified archives required by a profile.
    /// A directory-installed or bundled component intentionally remains a
    /// reference: Nexora must never synthesize a redistributable package from
    /// code whose publisher/signature information is unavailable.
    pub fn workspace_component_archives(
        &self,
        requested: &[(String, String)],
    ) -> Vec<WorkspaceComponentArchive> {
        let catalog = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .clone();
        let mut required = BTreeMap::<String, String>::new();
        let mut pending = requested.to_vec();

        while let Some((component_id, version_requirement)) = pending.pop() {
            if required.contains_key(&component_id) {
                continue;
            }
            required.insert(component_id.clone(), version_requirement);
            if let Some(component) = catalog.get(&component_id) {
                for dependency in &component.manifest.requires_components {
                    pending.push((
                        dependency.id.clone(),
                        dependency.version_requirement.clone(),
                    ));
                }
            }
        }

        required
            .into_iter()
            .map(|(component_id, version_requirement)| {
                let Some(component) = catalog.get(&component_id) else {
                    return WorkspaceComponentArchive {
                        component_id,
                        version: None,
                        version_requirement,
                        package_path: None,
                        reason: Some("此组件当前未安装，只保留依赖引用。".into()),
                    };
                };
                if component
                    .manifest
                    .extensions
                    .get("redistributable")
                    .and_then(Value::as_bool)
                    == Some(false)
                {
                    return WorkspaceComponentArchive {
                        component_id,
                        version: Some(component.manifest.version.clone()),
                        version_requirement,
                        package_path: None,
                        reason: Some("组件声明为不可重新分发，只保留依赖引用。".into()),
                    };
                }
                let archive_path = self.archive_path_for(&component.manifest);
                let validated_archive = archive_path
                    .is_file()
                    .then(|| {
                        self.inspect_package(&archive_path.to_string_lossy())
                            .ok()
                            .filter(|inspection| {
                                inspection.component_id.as_deref() == Some(component_id.as_str())
                                    && inspection.component_version.as_deref()
                                        == Some(component.manifest.version.as_str())
                                    && inspection.trust.installable
                            })
                            .map(|_| archive_path.clone())
                    })
                    .flatten();
                WorkspaceComponentArchive {
                    component_id,
                    version: Some(component.manifest.version.clone()),
                    version_requirement,
                    package_path: validated_archive,
                    reason: if self.archive_path_for(&component.manifest).is_file()
                        && self
                            .inspect_package(&archive_path.to_string_lossy())
                            .ok()
                            .is_some_and(|inspection| {
                                inspection.component_id.as_deref()
                                    == Some(component.manifest.id.as_str())
                                    && inspection.component_version.as_deref()
                                        == Some(component.manifest.version.as_str())
                                    && inspection.trust.installable
                            }) {
                        None
                    } else if archive_path.is_file() {
                        Some("已缓存的组件包不再匹配当前版本或信任状态，只保留依赖引用。".into())
                    } else if component.source == ComponentInstallSource::Bundled {
                        Some("随 Nexora 提供的组件没有独立分发包，只保留依赖引用。".into())
                    } else {
                        Some("此组件不是从已验证的 .pmc-pack 安装，只保留依赖引用。".into())
                    },
                }
            })
            .collect()
    }

    /// Installs a group of embedded component packages as one operation. This
    /// intentionally rejects version replacement: a portable workspace may
    /// add missing components, but may not silently replace working local
    /// components. Existing identical versions are reused.
    pub async fn install_workspace_packages_atomically(
        &self,
        package_paths: &[PathBuf],
    ) -> Result<WorkspaceComponentInstallResult, ComponentRuntimeError> {
        if package_paths.is_empty() {
            return Ok(WorkspaceComponentInstallResult {
                installed_component_ids: Vec::new(),
            });
        }
        let _guard = self.mutation_lock.lock().await;
        let transaction_root = self
            .root_path
            .join(".workspace-staging")
            .join(Uuid::new_v4().simple().to_string());
        let staged_root = transaction_root.join("components");
        let cached_root = transaction_root.join("archives");
        fs::create_dir_all(&staged_root)
            .and_then(|_| fs::create_dir_all(&cached_root))
            .map_err(|error| io_error("创建装配空间组件暂存目录失败", error))?;

        let result = (|| {
            let mut staged = Vec::<(ComponentManifestV1, PathBuf, PathBuf)>::new();
            let mut seen = BTreeSet::new();
            let catalog = self
                .catalog
                .lock()
                .expect("component catalog mutex poisoned")
                .clone();

            for package_path in package_paths {
                let package_path = fs::canonicalize(package_path)
                    .map_err(|error| io_error("装配空间内的组件包不可读取", error))?;
                let inspection = self.inspect_package(&package_path.to_string_lossy())?;
                ensure_package_installable(&inspection)?;
                let component_id = inspection.component_id.clone().ok_or_else(|| {
                    ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentPackageInvalid,
                        "组件包缺少组件 ID",
                    )
                })?;
                let component_version = inspection.component_version.clone().ok_or_else(|| {
                    ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentPackageInvalid,
                        "组件包缺少组件版本",
                    )
                })?;
                if !seen.insert(component_id.clone()) {
                    return Err(ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentPackageInvalid,
                        format!("装配空间包含重复组件包: {component_id}"),
                    ));
                }
                if let Some(installed) = catalog.get(&component_id) {
                    if installed.manifest.version != component_version {
                        return Err(ComponentRuntimeError::new(
                            ComponentRuntimeErrorCode::ComponentDependencyConflict,
                            format!(
                                "本机已有组件 {component_id} {}，装配空间要求 {}；为避免静默替换已停止导入",
                                installed.manifest.version, component_version
                            ),
                        ));
                    }
                    continue;
                }

                let component_staging = staged_root.join(&component_id);
                extract_component_package(&package_path, &component_staging)?;
                let manifest =
                    read_component_manifest(&component_staging.join(COMPONENT_MANIFEST_FILE))?;
                validate_entry(&component_staging, &manifest)?;
                validate_platform_compatibility(&manifest)?;
                validate_native_library_entry(&component_staging, &manifest)?;
                validate_presentation_component(&component_staging, &manifest)?;
                if manifest.id != component_id || manifest.version != component_version {
                    return Err(ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentPackageInvalid,
                        "组件包检查结果与解压后的 component.json 不一致",
                    ));
                }
                let archive_staging = cached_root.join(archive_file_name(&manifest));
                copy_file_atomic(&package_path, &archive_staging)?;
                staged.push((manifest, component_staging, archive_staging));
            }

            let mut next_manifests = catalog
                .values()
                .map(|component| component.manifest.clone())
                .collect::<Vec<_>>();
            next_manifests.extend(staged.iter().map(|(manifest, _, _)| manifest.clone()));
            validate_component_graph(&next_manifests).map_err(contract_error)?;

            let mut committed = Vec::<(ComponentManifestV1, PathBuf, PathBuf)>::new();
            for (manifest, _, _) in &staged {
                if self.packages_path.join(&manifest.id).exists() {
                    return Err(ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentDependencyConflict,
                        format!("组件目录在预检后发生变化: {}", manifest.id),
                    ));
                }
            }
            for (manifest, component_staging, archive_staging) in &staged {
                let target = self.packages_path.join(&manifest.id);
                if let Err(error) = fs::rename(component_staging, &target) {
                    for (_, component_target, _) in committed.iter().rev() {
                        let _ = fs::remove_dir_all(component_target);
                    }
                    return Err(io_error("提交装配空间组件失败", error));
                }
                committed.push((manifest.clone(), target, archive_staging.clone()));
            }

            for (manifest, _, archive_staging) in &committed {
                let archive_target = self.archive_path_for(manifest);
                if let Err(error) = replace_file(archive_staging, &archive_target) {
                    for (_, component_target, _) in committed.iter().rev() {
                        let _ = fs::remove_dir_all(component_target);
                    }
                    return Err(error);
                }
            }

            let mut catalog_guard = self
                .catalog
                .lock()
                .expect("component catalog mutex poisoned");
            for (manifest, target, _) in &committed {
                catalog_guard.insert(
                    manifest.id.clone(),
                    InstalledComponent {
                        manifest: manifest.clone(),
                        source: ComponentInstallSource::Marketplace,
                        package_root: Some(target.clone()),
                    },
                );
            }
            Ok(WorkspaceComponentInstallResult {
                installed_component_ids: committed
                    .into_iter()
                    .map(|(manifest, _, _)| manifest.id)
                    .collect(),
            })
        })();

        if result.is_err() {
            // Targets created above are removed by the failing branch when
            // possible. The staging root is disposable in every case.
            let _ = fs::remove_dir_all(&transaction_root);
        } else {
            let _ = fs::remove_dir_all(&transaction_root);
        }
        result
    }

    pub async fn rollback_workspace_package_install(
        &self,
        component_ids: &[String],
    ) -> Result<(), ComponentRuntimeError> {
        if component_ids.is_empty() {
            return Ok(());
        }
        let _guard = self.mutation_lock.lock().await;
        for component_id in component_ids {
            if self.active_operation_count(component_id) > 0 {
                return Err(ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentOperationConflict,
                    format!("组件 {component_id} 在导入回滚时已开始运行"),
                ));
            }
        }
        let mut catalog = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned");
        for component_id in component_ids {
            let Some(installed) = catalog.remove(component_id) else {
                continue;
            };
            if let Some(path) = installed.package_root {
                fs::remove_dir_all(path)
                    .map_err(|error| io_error("回滚装配空间组件失败", error))?;
            }
            let archive = self.archive_path_for(&installed.manifest);
            let _ = fs::remove_file(archive);
        }
        Ok(())
    }

    pub fn trust_package_publisher(
        &self,
        package_path: &str,
    ) -> Result<ComponentPackagePublisher, ComponentRuntimeError> {
        let inspection = self.inspect_package(package_path)?;
        let publisher = inspection.publisher.clone().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageSignatureInvalid,
                "组件包未提供可验证的发布者签名，不能建立信任",
            )
        })?;
        if !inspection.trust.signature_valid {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageSignatureInvalid,
                "组件包签名无效，不能信任发布者",
            ));
        }
        let mut trusted = self
            .trusted_publishers
            .lock()
            .expect("component trust mutex poisoned");
        let previous = trusted.get(&publisher.id).cloned();
        trusted.insert(
            publisher.id.clone(),
            StoredTrustedPublisher {
                id: publisher.id.clone(),
                display_name: publisher.display_name.clone(),
                public_key: publisher.public_key.clone(),
                trusted_at: Utc::now().timestamp_millis(),
            },
        );
        if let Err(error) = persist_trusted_publishers(
            &self.trust_path,
            &StoredComponentTrustState {
                schema_version: COMPONENT_TRUST_SCHEMA_VERSION,
                publishers: trusted.values().cloned().collect(),
            },
        ) {
            if let Some(previous) = previous {
                trusted.insert(publisher.id.clone(), previous);
            } else {
                trusted.remove(&publisher.id);
            }
            return Err(error);
        }
        Ok(publisher)
    }

    pub fn presentation_template_preview(
        &self,
        request: &PresentationTemplatePreviewRequest,
    ) -> Result<PresentationTemplatePreview, ComponentRuntimeError> {
        let component = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(request.component_id.trim())
            .cloned()
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentNotInstalled,
                    format!("组件 {} 未安装", request.component_id),
                )
            })?;
        let root = component.package_root.as_ref().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
                "随程序提供的兼容模板由宿主直接渲染，不提供外部 HTML 预览",
            )
        })?;
        load_presentation_template_preview(root, &component.manifest, &request.template_id)
    }

    /// Validate the active interface template against the installed component
    /// catalog before a profile is saved. Keeping this here lets validation use
    /// the same parsed descriptor that will render at runtime.
    pub fn validate_interface_layout(
        &self,
        profile: &WorkspaceProfileV1,
    ) -> InterfaceTemplateLayoutValidation {
        let template_id = profile
            .shell_layout
            .shell_template
            .as_ref()
            .map(|binding| canonical_interface_template_id(&binding.id))
            .unwrap_or_else(|| match profile.shell_layout.navigation_kind {
                pmc_platform::ShellNavigationKind::TopBar => "nexora.shell.top-bar",
                pmc_platform::ShellNavigationKind::SideBar => "nexora.shell.side-bar",
                pmc_platform::ShellNavigationKind::Minimal => "nexora.shell.minimal",
            });
        let catalog = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .clone();
        let mut diagnostics = Vec::new();
        let slots = if template_id.starts_with("nexora.shell.") {
            builtin_interface_template_slots(template_id)
        } else {
            let owner = catalog.values().find(|component| {
                component
                    .manifest
                    .contributes
                    .shell_templates
                    .iter()
                    .any(|template| template.id == template_id)
            });
            match owner.and_then(|component| {
                component
                    .package_root
                    .as_ref()
                    .map(|root| (root, &component.manifest))
            }) {
                Some((root, manifest)) => {
                    match load_presentation_template_preview(root, manifest, template_id) {
                        Ok(preview) => preview.slots,
                        Err(error) => {
                            diagnostics.push(interface_diagnostic(
                                "TEMPLATE_INVALID",
                                "error",
                                "$.shellLayout.shellTemplate",
                                format!("界面模板无法装载：{}", error.message),
                            ));
                            Vec::new()
                        }
                    }
                }
                None => {
                    diagnostics.push(interface_diagnostic(
                        "TEMPLATE_MISSING",
                        "error",
                        "$.shellLayout.shellTemplate",
                        format!("界面模板未安装或不可读取：{template_id}"),
                    ));
                    Vec::new()
                }
            }
        };
        let state = profile
            .shell_layout
            .interface_template_states
            .iter()
            .find(|state| state.template_id == template_id);
        let bindings = state
            .map(|state| state.slot_bindings.as_slice())
            .unwrap_or_default();

        for slot in &slots {
            let slot_bindings = bindings
                .iter()
                .filter(|binding| binding.enabled && binding.slot_id == slot.id)
                .collect::<Vec<_>>();
            if slot.multiplicity == TemplateSlotMultiplicity::One && slot_bindings.len() > 1 {
                diagnostics.push(interface_diagnostic(
                    "SLOT_MULTIPLICITY_EXCEEDED",
                    "error",
                    &format!("$.shellLayout.interfaceTemplateStates[{template_id}].slotBindings"),
                    format!("插槽 {} 只允许一个内容", slot.name),
                ));
            }
        }

        let slots_by_id = slots
            .iter()
            .map(|slot| (slot.id.as_str(), slot))
            .collect::<HashMap<_, _>>();
        let mut singleton_surfaces = HashSet::new();
        for (index, binding) in bindings.iter().enumerate() {
            if !binding.enabled {
                continue;
            }
            let path = format!(
                "$.shellLayout.interfaceTemplateStates[{template_id}].slotBindings[{index}]"
            );
            let Some(slot) = slots_by_id.get(binding.slot_id.as_str()) else {
                diagnostics.push(interface_diagnostic(
                    "SLOT_NOT_FOUND",
                    "error",
                    &path,
                    format!("绑定引用了当前模板不存在的插槽：{}", binding.slot_id),
                ));
                continue;
            };
            validate_template_surface_binding(
                binding,
                slot,
                &catalog,
                &mut singleton_surfaces,
                &path,
                &mut diagnostics,
            );
        }
        InterfaceTemplateLayoutValidation {
            valid: !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"),
            diagnostics,
        }
    }

    pub fn interface_template_preview_by_id(
        &self,
        template_id: &str,
    ) -> Result<PresentationTemplatePreview, ComponentRuntimeError> {
        let template_id = canonical_interface_template_id(template_id.trim());
        let component_id = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .values()
            .find(|component| {
                component
                    .manifest
                    .contributes
                    .shell_templates
                    .iter()
                    .any(|template| template.id == template_id)
            })
            .map(|component| component.manifest.id.clone())
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentNotInstalled,
                    format!("界面模板未安装：{template_id}"),
                )
            })?;
        self.presentation_template_preview(&PresentationTemplatePreviewRequest {
            component_id,
            template_id: template_id.into(),
        })
    }

    /// Creates a separate data-pack source directory. Installed packages are
    /// never changed in place; the copy must be trusted and installed through
    /// the normal development-component path before it can be used.
    pub fn duplicate_interface_template_for_development(
        &self,
        template_id: &str,
        target_directory: &str,
    ) -> Result<String, ComponentRuntimeError> {
        let template_id = canonical_interface_template_id(template_id.trim());
        let component = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .values()
            .find(|component| {
                component
                    .manifest
                    .contributes
                    .shell_templates
                    .iter()
                    .any(|template| template.id == template_id)
            })
            .cloned()
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentNotInstalled,
                    format!("界面模板未安装：{template_id}"),
                )
            })?;
        let target_parent = PathBuf::from(target_directory);
        fs::create_dir_all(&target_parent)
            .map_err(|error| io_error("创建模板开发目录失败", error))?;
        let target_parent = fs::canonicalize(&target_parent)
            .map_err(|error| io_error("模板开发目录不可用", error))?;
        if !target_parent.is_dir() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentIoError,
                "模板开发目标必须是目录",
            ));
        }
        let copy_suffix = Uuid::new_v4().simple().to_string();
        let component_id = format!("local.template-copy-{}", &copy_suffix[..10]);
        let copied_template_id = format!("{component_id}.shell");
        let destination = target_parent.join(&component_id);
        let rewrite_result = if let Some(source) = component.package_root {
            copy_component_tree(&source, &destination)?;
            rewrite_copied_shell_template(
                &destination,
                template_id,
                &component_id,
                &copied_template_id,
            )
        } else if component.manifest.id == "nexora.presentation.templates" {
            create_builtin_interface_template_copy(
                &destination,
                &component_id,
                &copied_template_id,
                component
                    .manifest
                    .contributes
                    .shell_templates
                    .iter()
                    .find(|template| template.id == template_id)
                    .map(|template| template.name.as_str())
                    .unwrap_or("Nexora 界面模板"),
            )
        } else {
            Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
                "此模板没有可复制的 data-pack 来源",
            ))
        };
        if let Err(error) = rewrite_result {
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }
        Ok(destination.to_string_lossy().into_owned())
    }

    pub async fn install_from_package(
        &self,
        request: &InstallComponentPackageRequest,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        let _guard = self.mutation_lock.lock().await;
        let package_path = fs::canonicalize(&request.package_path)
            .map_err(|error| io_error("组件包不可用", error))?;
        let inspection = self.inspect_package(&package_path.to_string_lossy())?;
        ensure_package_installable(&inspection)?;
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
        if let Err(error) = validate_platform_compatibility(&manifest) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(error) = validate_native_library_entry(&staging, &manifest) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Err(error) = validate_presentation_component(&staging, &manifest) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let installed = self
            .commit_staged_component(staging, manifest, ComponentInstallSource::Marketplace)
            .await?;
        // Keeping the original verified archive is what allows a later
        // self-contained workspace export to preserve the publisher signature.
        // A cache failure is reported in the log but never turns a completed
        // component installation into a false failure.
        if let Err(error) = self.cache_component_package(&package_path, &installed) {
            eprintln!(
                "[components] 已安装 {}, 但无法缓存原始 .pmc-pack：{}",
                installed.id, error.message
            );
        }
        Ok(installed)
    }

    pub async fn disable(
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
                "仍有已启用组件依赖此组件，不能禁用",
            )
            .with_details(dependent_components));
        }
        self.shutdown_worker(component_id).await;
        super::file_operations::release_component_streams(component_id);
        let installed = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(component_id)
            .cloned()
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentNotInstalled,
                    format!("组件 {component_id} 未启用"),
                )
            })?;

        {
            let mut state = self.state.lock().expect("component state mutex poisoned");
            state.disabled_components.insert(component_id.to_string());
            if installed.source == ComponentInstallSource::Bundled {
                state.removed_bundled.insert(component_id.to_string());
            }
            persist_state(&self.state_path, &state)?;
        }
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .remove(component_id);
        Ok(installed.manifest)
    }

    pub async fn enable(
        &self,
        component_id: &str,
    ) -> Result<ComponentManifestV1, ComponentRuntimeError> {
        let _guard = self.mutation_lock.lock().await;
        if self.manifest(component_id).is_some() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentAlreadyInstalled,
                format!("组件 {component_id} 已启用"),
            ));
        }
        let package_root = self.packages_path.join(component_id);
        let manifest_path = package_root.join(COMPONENT_MANIFEST_FILE);
        let installed = if manifest_path.is_file() {
            let manifest = read_component_manifest(&manifest_path)?;
            validate_entry(&package_root, &manifest)?;
            validate_platform_compatibility(&manifest)?;
            validate_native_library_entry(&package_root, &manifest)?;
            validate_presentation_component(&package_root, &manifest)?;
            let source = if self.archive_path_for(&manifest).is_file() {
                ComponentInstallSource::Marketplace
            } else {
                ComponentInstallSource::Local
            };
            InstalledComponent {
                manifest,
                source,
                package_root: Some(package_root),
            }
        } else if let Some(manifest) = self.bundled.get(component_id).cloned() {
            InstalledComponent {
                manifest,
                source: ComponentInstallSource::Bundled,
                package_root: None,
            }
        } else {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentNotInstalled,
                format!("组件 {component_id} 的安装文件不存在"),
            ));
        };
        let mut next_manifests = self.manifests();
        next_manifests.retain(|item| item.id != component_id);
        next_manifests.push(installed.manifest.clone());
        validate_component_graph(&next_manifests).map_err(contract_error)?;
        {
            let mut state = self.state.lock().expect("component state mutex poisoned");
            state.disabled_components.remove(component_id);
            state.removed_bundled.remove(component_id);
            persist_state(&self.state_path, &state)?;
        }
        let manifest = installed.manifest.clone();
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .insert(component_id.into(), installed);
        Ok(manifest)
    }

    pub async fn delete(
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
            .installed_manifests()
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
                "仍有已安装组件依赖此组件，不能删除",
            )
            .with_details(dependent_components));
        }
        self.shutdown_worker(component_id).await;
        super::file_operations::release_component_streams(component_id);
        let installed = self
            .catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .get(component_id)
            .cloned()
            .or_else(|| {
                let package_root = self.packages_path.join(component_id);
                let manifest =
                    read_component_manifest(&package_root.join(COMPONENT_MANIFEST_FILE)).ok()?;
                Some(InstalledComponent {
                    source: if self.archive_path_for(&manifest).is_file() {
                        ComponentInstallSource::Marketplace
                    } else {
                        ComponentInstallSource::Local
                    },
                    manifest,
                    package_root: Some(package_root),
                })
            })
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentNotInstalled,
                    format!("组件 {component_id} 未安装"),
                )
            })?;
        if installed.source == ComponentInstallSource::Bundled || installed.package_root.is_none() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentDependencyConflict,
                "随安装包组件不能删除，请改用禁用",
            ));
        }
        if let Some(package_root) = &installed.package_root {
            let trash_root = self.root_path.join(".trash");
            fs::create_dir_all(&trash_root)
                .map_err(|error| io_error("创建组件删除暂存目录失败", error))?;
            let trash = trash_root.join(format!("{}-{}", component_id, Uuid::new_v4().simple()));
            fs::rename(package_root, &trash)
                .map_err(|error| io_error("撤下组件目录失败", error))?;
            let _ = fs::remove_dir_all(trash);
        }
        let _ = fs::remove_file(self.archive_path_for(&installed.manifest));
        self.catalog
            .lock()
            .expect("component catalog mutex poisoned")
            .remove(component_id);
        {
            let mut state = self.state.lock().expect("component state mutex poisoned");
            state.disabled_components.remove(component_id);
            if self.bundled.contains_key(component_id) {
                state.disabled_components.insert(component_id.to_string());
                state.removed_bundled.insert(component_id.to_string());
            }
            persist_state(&self.state_path, &state)?;
        }
        Ok(installed.manifest)
    }

    pub fn is_bundled(&self, component_id: &str) -> bool {
        self.bundled.contains_key(component_id)
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
        let cancellation_root = self.root_path.join(".cancellation");
        fs::create_dir_all(&cancellation_root).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentIoError,
                format!("创建组件取消目录失败: {error}"),
            )
        })?;
        let cancellation_path = cancellation_root.join(safe_name(&operation_id));
        let _ = fs::remove_file(&cancellation_path);
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
            cancellation_path: cancellation_path.clone(),
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
            "cancellationFile": cancellation_path,
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
        let _ = fs::remove_file(&control.cancellation_path);
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
        let _ = fs::write(&control.cancellation_path, b"cancelled\n");
        let pid = control.pid.load(Ordering::SeqCst);
        if pid != 0 {
            let control = control.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                let current_pid = control.pid.load(Ordering::SeqCst);
                if control.cancelled.load(Ordering::SeqCst) && current_pid != 0 {
                    crate::process_utils::terminate_pid_tree(current_pid);
                }
            });
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
            Some("nexora-file-operations") => {
                super::file_operations::invoke(&self.app_handle, &self.root_path, payload).await
            }
            Some("bundled-resource-process") => {
                self.invoke_bundled_resource_process(installed, control, payload)
                    .await
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
                    self.invoke_native_library(installed, control, payload)
                        .await
                }
                ComponentRuntime::BuiltinRust => Err(ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
                    "builtin-rust 必须声明受信任 hostAdapter",
                )),
            },
        }
    }

    async fn invoke_bundled_resource_process(
        &self,
        installed: &InstalledComponent,
        control: &Arc<OperationControl>,
        payload: Value,
    ) -> Result<ProcessResponse, ComponentRuntimeError> {
        if installed.manifest.id != super::builtin_components::PM_BLENDIO_COMPONENT_ID {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
                format!("组件 {} 没有随程序分发的受监督进程", installed.manifest.id),
            ));
        }
        let entry = blendio_service_path().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostUnavailable,
                "未找到 pmc-blendio-service；BlenderIO 不会回退到 Nexora 主进程执行",
            )
        })?;
        let parent = entry.parent().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentHostUnavailable,
                "BlenderIO 服务资源路径无效",
            )
        })?;
        let file_name = entry
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentHostUnavailable,
                    "BlenderIO 服务文件名无效",
                )
            })?;
        let mut process_component = installed.clone();
        process_component.package_root = Some(parent.to_path_buf());
        process_component.manifest.entry = Some(file_name.into());
        process_component.manifest.runtime = ComponentRuntime::NativeProcess;
        process_component.manifest.extensions.remove("hostAdapter");
        self.invoke_one_shot(&process_component, control, payload, false)
            .await
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
            .args([
                "--dll",
                &entry.to_string_lossy(),
                "--component-id",
                &installed.manifest.id,
            ])
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
        let mut env = env;
        extend_script_sdk_pythonpath(&self.app_handle, &mut env);
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
        let stdout = child.stdout.take().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                "组件进程 stdout 不可用",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProtocolError,
                "组件进程 stderr 不可用",
            )
        })?;
        let stdout_task = tokio::spawn(capture_component_stream(
            stdout,
            None,
            Some(self.app_handle.clone()),
            payload
                .get("operationId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            installed.manifest.id.clone(),
        ));
        let stderr_task = tokio::spawn(capture_component_stream(
            stderr,
            Some("stderr: ".into()),
            None,
            String::new(),
            installed.manifest.id.clone(),
        ));
        let status = child.wait().await.map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                format!("等待组件进程失败: {error}"),
            )
        })?;
        control.pid.store(0, Ordering::SeqCst);
        let stdout = stdout_task.await.unwrap_or_default();
        let stderr = stderr_task.await.unwrap_or_default();
        let mut logs = stdout.lines().map(str::to_string).collect::<Vec<_>>();
        logs.extend(stderr.lines().map(str::to_string));
        if !status.success() {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentProcessFailed,
                format!("组件进程退出码 {}", status.code().unwrap_or(-1)),
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
        let mut env = runtime.env_vars;
        extend_script_sdk_pythonpath(&self.app_handle, &mut env);
        for (key, value) in env {
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
            self.app_handle.clone(),
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

    pub fn active_operation_count(&self, component_id: &str) -> usize {
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

    pub fn is_operation_active(&self, operation_id: &str) -> bool {
        self.operations
            .lock()
            .expect("component operation mutex poisoned")
            .contains_key(operation_id)
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
    app_handle: AppHandle,
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
                let result = invoke_worker_message(
                    &app_handle,
                    &component_id,
                    &mut stdin,
                    &mut stdout,
                    request,
                )
                .await;
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
    app_handle: &AppHandle,
    component_id: &str,
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
            let _ = app_handle.emit(
                "nexora:component-operation-progress",
                json!({
                    "operationId": operation_id,
                    "componentId": component_id,
                    "type": message_type,
                    "progress": value.get("progress").cloned().unwrap_or(Value::Null),
                    "message": value.get("message").cloned().unwrap_or(Value::Null),
                    "payload": value,
                }),
            );
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

fn canonical_interface_template_id(id: &str) -> &str {
    match id {
        "builtin.shell.top-bar" => "nexora.shell.top-bar",
        "builtin.shell.side-bar" => "nexora.shell.side-bar",
        "builtin.shell.compact" => "nexora.shell.minimal",
        _ => id,
    }
}

fn builtin_interface_template_slots(template_id: &str) -> Vec<TemplateSlotDefinition> {
    let slot = |id: &str, name: &str, accepts: Vec<TemplateSlotAccepts>, required: bool| {
        TemplateSlotDefinition {
            id: id.into(),
            name: name.into(),
            accepts,
            multiplicity: if id == "primary" {
                TemplateSlotMultiplicity::Many
            } else {
                TemplateSlotMultiplicity::One
            },
            layout: if id == "primary" {
                TemplateSlotLayout::Stack
            } else {
                TemplateSlotLayout::Single
            },
            required,
            collapse_when_empty: !required,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            extensions: Default::default(),
        }
    };
    if template_id == "nexora.shell.blank-home" {
        return vec![TemplateSlotDefinition {
            id: "primary".into(),
            name: "主页".into(),
            accepts: vec![TemplateSlotAccepts::ActiveSurface],
            multiplicity: TemplateSlotMultiplicity::One,
            layout: TemplateSlotLayout::Single,
            required: false,
            collapse_when_empty: false,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            extensions: Default::default(),
        }];
    }
    vec![
        slot("tabs", "标签", vec![TemplateSlotAccepts::Tabs], false),
        slot(
            "navigation",
            "导航",
            vec![TemplateSlotAccepts::Navigation],
            false,
        ),
        slot(
            "toolbar",
            "项目工具",
            vec![TemplateSlotAccepts::Toolbar],
            false,
        ),
        slot(
            "primary",
            "主内容",
            vec![
                TemplateSlotAccepts::ActiveSurface,
                TemplateSlotAccepts::ComponentSurface,
            ],
            true,
        ),
        slot("status", "状态", vec![TemplateSlotAccepts::Status], false),
    ]
}

fn interface_diagnostic(
    code: impl Into<String>,
    severity: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> InterfaceTemplateDiagnostic {
    InterfaceTemplateDiagnostic {
        code: code.into(),
        severity: severity.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn validate_template_surface_binding(
    binding: &ProfileTemplateSlotBinding,
    slot: &TemplateSlotDefinition,
    catalog: &BTreeMap<String, InstalledComponent>,
    singleton_surfaces: &mut HashSet<String>,
    path: &str,
    diagnostics: &mut Vec<InterfaceTemplateDiagnostic>,
) {
    if !matches!(
        binding.kind,
        pmc_platform::TemplateSlotBindingKind::ComponentSurface
    ) {
        if !slot.accepts.iter().any(|accepts| {
            matches!(
                (accepts, &binding.kind),
                (
                    TemplateSlotAccepts::ActiveSurface,
                    pmc_platform::TemplateSlotBindingKind::ActiveSurface
                ) | (
                    TemplateSlotAccepts::Widget,
                    pmc_platform::TemplateSlotBindingKind::Widget
                ) | (
                    TemplateSlotAccepts::Navigation,
                    pmc_platform::TemplateSlotBindingKind::Navigation
                ) | (
                    TemplateSlotAccepts::Tabs,
                    pmc_platform::TemplateSlotBindingKind::Tabs
                ) | (
                    TemplateSlotAccepts::Toolbar,
                    pmc_platform::TemplateSlotBindingKind::Toolbar
                ) | (
                    TemplateSlotAccepts::Status,
                    pmc_platform::TemplateSlotBindingKind::Status
                )
            )
        }) {
            diagnostics.push(interface_diagnostic(
                "SLOT_KIND_INCOMPATIBLE",
                "error",
                path,
                format!("插槽 {} 不接受该绑定类型", slot.name),
            ));
        }
        return;
    }
    if !slot
        .accepts
        .contains(&TemplateSlotAccepts::ComponentSurface)
    {
        diagnostics.push(interface_diagnostic(
            "SLOT_KIND_INCOMPATIBLE",
            "error",
            path,
            format!("插槽 {} 不接受组件页面", slot.name),
        ));
        return;
    }
    let (Some(component_id), Some(surface_id)) = (&binding.component_id, &binding.surface_id)
    else {
        diagnostics.push(interface_diagnostic(
            "SURFACE_REFERENCE_INVALID",
            "error",
            path,
            "组件页面绑定缺少 componentId 或 surfaceId",
        ));
        return;
    };
    let Some(component) = catalog.get(component_id) else {
        diagnostics.push(interface_diagnostic(
            "SURFACE_COMPONENT_MISSING",
            "error",
            path,
            format!("组件页面的组件未安装：{component_id}"),
        ));
        return;
    };
    let Some(surface) = component
        .manifest
        .contributes
        .script_surfaces
        .iter()
        .find(|surface| surface.id == *surface_id)
    else {
        diagnostics.push(interface_diagnostic(
            "SURFACE_MISSING",
            "error",
            path,
            format!("组件 {} 未公开页面 {}", component.manifest.name, surface_id),
        ));
        return;
    };
    if !surface.placements.iter().any(|placement| {
        matches!(
            placement,
            pmc_platform::ScriptSurfacePlacement::Shell
                | pmc_platform::ScriptSurfacePlacement::Workspace
        )
    }) {
        diagnostics.push(interface_diagnostic(
            "SURFACE_PLACEMENT_INCOMPATIBLE",
            "error",
            path,
            format!("页面 {} 未声明可嵌入界面模板", surface.name),
        ));
    }
    let identity = format!("{component_id}:{surface_id}");
    if surface.instance_mode == ComponentSurfaceInstanceMode::Singleton
        && !singleton_surfaces.insert(identity)
    {
        diagnostics.push(interface_diagnostic(
            "SINGLETON_SURFACE_DUPLICATED",
            "error",
            path,
            format!("单例页面 {} 不能在同一模板中重复装配", surface.name),
        ));
    }
    if surface
        .size_hints
        .min_width
        .zip(slot.max_width)
        .is_some_and(|(min, max)| min > max)
        || surface
            .size_hints
            .min_height
            .zip(slot.max_height)
            .is_some_and(|(min, max)| min > max)
    {
        diagnostics.push(interface_diagnostic(
            "SURFACE_SIZE_INCOMPATIBLE",
            "error",
            path,
            format!(
                "页面 {} 的最小尺寸超过插槽 {} 的最大尺寸",
                surface.name, slot.name
            ),
        ));
    }
}

fn rewrite_copied_shell_template(
    root: &Path,
    original_template_id: &str,
    component_id: &str,
    copied_template_id: &str,
) -> Result<(), ComponentRuntimeError> {
    let manifest_path = root.join(COMPONENT_MANIFEST_FILE);
    let mut document: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|error| io_error("读取复制模板的 component.json 失败", error))?,
    )
    .map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentManifestInvalid,
            format!("复制模板的 component.json 无效：{error}"),
        )
    })?;
    let templates = document
        .pointer_mut("/contributes/shellTemplates")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentManifestInvalid,
                "复制模板未声明 shellTemplates",
            )
        })?;
    let position = templates
        .iter()
        .position(|template| {
            template.get("id").and_then(Value::as_str) == Some(original_template_id)
        })
        .ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentManifestInvalid,
                "复制模板未找到目标界面模板贡献",
            )
        })?;
    let mut template = templates.remove(position);
    let template_path = template
        .get("templatePath")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentManifestInvalid,
                "Shell 模板贡献缺少 templatePath",
            )
        })?;
    template["id"] = Value::String(copied_template_id.into());
    template["name"] = Value::String(format!(
        "{}（开发副本）",
        template
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("界面模板")
    ));
    templates.clear();
    templates.push(template);
    document["id"] = Value::String(component_id.into());
    document["name"] = Value::String(format!(
        "{}（开发副本）",
        document
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("界面模板")
    ));
    document["distribution"] = Value::String("local".into());
    if let Some(contributes) = document
        .get_mut("contributes")
        .and_then(Value::as_object_mut)
    {
        contributes.remove("pageTemplates");
        contributes.remove("themePresets");
    }
    let descriptor_path = resolve_data_path(root, &template_path)?;
    let mut descriptor: Value = serde_json::from_str(
        &fs::read_to_string(&descriptor_path)
            .map_err(|error| io_error("读取复制模板 descriptor 失败", error))?,
    )
    .map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentManifestInvalid,
            format!("复制模板 descriptor 无效：{error}"),
        )
    })?;
    descriptor["id"] = Value::String(copied_template_id.into());
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&document).map_err(|error| contract_error(error))?,
    )
    .map_err(|error| io_error("写入复制模板 component.json 失败", error))?;
    fs::write(
        descriptor_path,
        serde_json::to_string_pretty(&descriptor).map_err(|error| contract_error(error))?,
    )
    .map_err(|error| io_error("写入复制模板 descriptor 失败", error))?;
    let manifest = read_component_manifest(&manifest_path)?;
    validate_presentation_component(root, &manifest)?;
    Ok(())
}

fn create_builtin_interface_template_copy(
    root: &Path,
    component_id: &str,
    template_id: &str,
    name: &str,
) -> Result<(), ComponentRuntimeError> {
    let template_directory = root.join("presentation").join("shell").join("starter");
    fs::create_dir_all(&template_directory)
        .map_err(|error| io_error("创建内置模板开发副本失败", error))?;
    let manifest = json!({
        "schemaVersion": 1,
        "id": component_id,
        "name": format!("{}（开发副本）", name),
        "description": "由 Nexora 内置界面模板生成的可编辑静态起始模板。",
        "version": "0.1.0",
        "apiVersion": "1",
        "runtime": "data-pack",
        "role": "data",
        "distribution": "local",
        "uiMode": "none",
        "platforms": ["any"],
        "entry": "presentation",
        "capabilities": [],
        "contributes": {
            "shellTemplates": [{
                "id": template_id,
                "name": format!("{}（开发副本）", name),
                "version": "0.1.0",
                "templatePath": "presentation/shell/starter/template.json"
            }]
        }
    });
    let descriptor = json!({
        "schemaVersion": 1,
        "id": template_id,
        "kind": "shell",
        "version": "0.1.0",
        "semanticVersion": "1.0.0",
        "baseHtml": "presentation/shell/starter/base.html",
        "styles": "presentation/shell/starter/styles.css",
        "slots": [
            {"id": "tabs", "name": "标签", "accepts": ["tabs"], "multiplicity": "one", "layout": "single"},
            {"id": "navigation", "name": "导航", "accepts": ["navigation"], "multiplicity": "one", "layout": "single", "collapseWhenEmpty": true},
            {"id": "toolbar", "name": "项目工具", "accepts": ["toolbar"], "multiplicity": "one", "layout": "single", "collapseWhenEmpty": true},
            {"id": "primary", "name": "主内容", "accepts": ["active-surface", "component-surface"], "multiplicity": "many", "layout": "stack", "required": true},
            {"id": "status", "name": "状态", "accepts": ["status"], "multiplicity": "one", "layout": "single", "collapseWhenEmpty": true}
        ]
    });
    fs::write(
        root.join(COMPONENT_MANIFEST_FILE),
        serde_json::to_string_pretty(&manifest).map_err(|error| contract_error(error))?,
    )
    .map_err(|error| io_error("写入内置模板开发副本 component.json 失败", error))?;
    fs::write(
        template_directory.join("template.json"),
        serde_json::to_string_pretty(&descriptor).map_err(|error| contract_error(error))?,
    )
    .map_err(|error| io_error("写入内置模板开发副本 descriptor 失败", error))?;
    fs::write(
        template_directory.join("base.html"),
        "<main class=\"nexora-template-starter\"><header class=\"nexora-template-starter__tabs\"><nexora-slot name=\"tabs\"></nexora-slot></header><aside class=\"nexora-template-starter__navigation\"><nexora-slot name=\"navigation\"></nexora-slot></aside><section class=\"nexora-template-starter__workspace\"><header><nexora-slot name=\"toolbar\"></nexora-slot></header><section class=\"nexora-template-starter__primary\"><nexora-slot name=\"primary\"></nexora-slot></section></section><footer class=\"nexora-template-starter__status\"><nexora-slot name=\"status\"></nexora-slot></footer></main>",
    ).map_err(|error| io_error("写入内置模板开发副本 HTML 失败", error))?;
    fs::write(
        template_directory.join("styles.css"),
        ".nexora-template-starter { display: grid; grid-template-columns: 15rem minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr) auto; min-height: 100%; } .nexora-template-starter__tabs, .nexora-template-starter__status { grid-column: 1 / -1; } .nexora-template-starter__navigation { min-width: 0; border-right: 1px solid var(--nexora-border, #d9e0ea); } .nexora-template-starter__workspace { min-width: 0; min-height: 0; display: flex; flex-direction: column; } .nexora-template-starter__primary { min-height: 0; flex: 1; overflow: hidden; }",
    ).map_err(|error| io_error("写入内置模板开发副本 CSS 失败", error))?;
    let parsed = read_component_manifest(&root.join(COMPONENT_MANIFEST_FILE))?;
    validate_presentation_component(root, &parsed)
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
        .filter(|manifest| !state.disabled_components.contains(&manifest.id))
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
        if state.disabled_components.contains(&manifest.id) {
            continue;
        }
        validate_entry(&path, &manifest)?;
        validate_platform_compatibility(&manifest)?;
        validate_native_library_entry(&path, &manifest)?;
        validate_presentation_component(&path, &manifest)?;
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

fn extend_script_sdk_pythonpath(app_handle: &AppHandle, env: &mut HashMap<String, String>) {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        candidates.push(resource_dir.join("script-sdk"));
        candidates.push(resource_dir.join("resources").join("script-sdk"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/script-sdk"));
    let Some(sdk_root) = candidates.into_iter().find(|path| path.is_dir()) else {
        return;
    };
    let separator = if cfg!(windows) { ";" } else { ":" };
    let sdk = sdk_root.to_string_lossy();
    let existing = env.get("PYTHONPATH").cloned().unwrap_or_default();
    env.insert(
        "PYTHONPATH".into(),
        if existing.trim().is_empty() {
            sdk.into_owned()
        } else {
            format!("{sdk}{separator}{existing}")
        },
    );
    env.insert("NEXORA_SCRIPT_SDK_VERSION".into(), "1".into());
}

fn read_component_manifest_from_package(
    path: &Path,
) -> Result<ComponentManifestV1, ComponentRuntimeError> {
    let file = fs::File::open(path).map_err(|error| io_error("打开组件归档失败", error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("组件包不是有效 ZIP 归档: {error}"),
        )
    })?;
    let mut entry = archive.by_name(COMPONENT_MANIFEST_FILE).map_err(|_| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "组件包根目录缺少 component.json",
        )
    })?;
    if entry.encrypted() || entry.size() > 2 * 1024 * 1024 {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "组件包的 component.json 不安全或过大",
        ));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("读取组件包 component.json 失败", error))?;
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("component.json 不是 UTF-8: {error}"),
        )
    })?;
    parse_component_manifest(text).map_err(contract_error)
}

fn inspect_component_package(
    path: &Path,
) -> Result<ComponentPackageInspection, ComponentRuntimeError> {
    inspect_component_package_with_trust(path, &BTreeMap::new())
}

fn inspect_component_package_with_trust(
    path: &Path,
    trusted_publishers: &BTreeMap<String, StoredTrustedPublisher>,
) -> Result<ComponentPackageInspection, ComponentRuntimeError> {
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
    let mut content_entries = Vec::<(String, Vec<u8>)>::new();
    let mut manifest_bytes = None;
    let mut package_header = None;
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
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| io_error("读取组件归档内容失败", error))?;
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
        if name != "manifest.json" {
            content_entries.push((name.clone(), bytes.clone()));
        }
        if name == COMPONENT_MANIFEST_FILE {
            manifest_bytes = Some(bytes);
        } else if name == "manifest.json" {
            package_header = Some(
                parse_package_header(std::str::from_utf8(&bytes).map_err(|error| {
                    ComponentRuntimeError::new(
                        ComponentRuntimeErrorCode::ComponentPackageInvalid,
                        format!("组件包 manifest.json 不是 UTF-8: {error}"),
                    )
                })?)
                .map_err(contract_error)?,
            );
        }
    }
    let manifest_input = manifest_bytes.ok_or_else(|| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "组件包根目录缺少 component.json",
        )
    })?;
    let mut warnings = Vec::new();
    if let Some(header) = &package_header {
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
    let manifest =
        parse_component_manifest(std::str::from_utf8(&manifest_input).map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageInvalid,
                format!("component.json 不是 UTF-8: {error}"),
            )
        })?)
        .map_err(contract_error)?;
    content_entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut content_digest = blake3::Hasher::new();
    for (name, bytes) in content_entries {
        content_digest.update(name.as_bytes());
        content_digest.update(&[0]);
        content_digest.update(&bytes);
    }
    let content_digest = content_digest.finalize().to_hex().to_string();
    let trust = inspect_package_trust(
        package_header.as_ref(),
        &content_digest,
        trusted_publishers,
        manifest.runtime,
        &mut warnings,
    )?;
    Ok(ComponentPackageInspection {
        package_path: path.to_string_lossy().into_owned(),
        valid: true,
        component_id: Some(manifest.id),
        component_name: Some(manifest.name),
        component_version: Some(manifest.version),
        file_count,
        total_bytes,
        package_digest: Some(digest.finalize().to_hex().to_string()),
        content_digest: Some(content_digest),
        publisher: package_header
            .as_ref()
            .and_then(package_publisher_from_header),
        license: package_header
            .as_ref()
            .and_then(|header| header.extensions.get("license"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        trust,
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
        let mut output =
            fs::File::create(&target).map_err(|error| io_error("创建组件解压文件失败", error))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| io_error("解压组件文件失败", error))?;
    }
    Ok(())
}

fn package_publisher_from_header(
    header: &pmc_platform::PackageHeaderV1,
) -> Option<ComponentPackagePublisher> {
    let value = header.extensions.get("publisher")?.as_object()?;
    let id = value.get("id")?.as_str()?.trim();
    let public_key = value.get("publicKey")?.as_str()?.trim();
    if id.is_empty() || public_key.is_empty() {
        return None;
    }
    let display_name = value
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(id);
    Some(ComponentPackagePublisher {
        id: id.into(),
        display_name: display_name.into(),
        public_key: public_key.into(),
    })
}

fn inspect_package_trust(
    header: Option<&pmc_platform::PackageHeaderV1>,
    computed_content_digest: &str,
    trusted_publishers: &BTreeMap<String, StoredTrustedPublisher>,
    runtime: ComponentRuntime,
    warnings: &mut Vec<String>,
) -> Result<ComponentPackageTrust, ComponentRuntimeError> {
    let executable = !matches!(
        runtime,
        ComponentRuntime::DataPack | ComponentRuntime::BuiltinRust
    );
    let Some(header) = header else {
        warnings.push("未提供 PMC_PACKAGE 包头，无法验证发布者签名。".into());
        return Ok(ComponentPackageTrust {
            status: ComponentPackageTrustStatus::IntegrityOnly,
            signature_present: false,
            signature_valid: false,
            installable: !executable,
            message: if executable {
                "含可执行代码的组件包必须携带受信任发布者的 Ed25519 签名。".into()
            } else {
                "资料包按兼容模式仅做文件完整性检查。".into()
            },
        });
    };
    let declared_content_digest = header
        .extensions
        .get("contentDigest")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if declared_content_digest != Some(computed_content_digest) {
        return Ok(ComponentPackageTrust {
            status: ComponentPackageTrustStatus::InvalidSignature,
            signature_present: header.extensions.contains_key("signature"),
            signature_valid: false,
            installable: false,
            message: "组件包内容摘要与 manifest.json 不匹配。".into(),
        });
    }
    let publisher = package_publisher_from_header(header);
    let signature_value = header
        .extensions
        .get("signature")
        .and_then(Value::as_object)
        .and_then(|signature| {
            (signature.get("algorithm").and_then(Value::as_str) == Some("ed25519"))
                .then(|| signature.get("value").and_then(Value::as_str))
                .flatten()
        });
    let (Some(publisher), Some(signature_value)) = (publisher, signature_value) else {
        return Ok(ComponentPackageTrust {
            status: ComponentPackageTrustStatus::IntegrityOnly,
            signature_present: header.extensions.contains_key("signature"),
            signature_valid: false,
            installable: !executable,
            message: if executable {
                "含可执行代码的组件包缺少有效发布者或 Ed25519 签名。".into()
            } else {
                "资料包具备内容摘要，但未提供可验证发布者签名。".into()
            },
        });
    };
    let key_bytes = match base64::engine::general_purpose::STANDARD.decode(&publisher.public_key) {
        Ok(value) if value.len() == 32 => value,
        _ => {
            return Ok(ComponentPackageTrust {
                status: ComponentPackageTrustStatus::InvalidSignature,
                signature_present: true,
                signature_valid: false,
                installable: false,
                message: "发布者 Ed25519 公钥无效。".into(),
            })
        }
    };
    let signature_bytes = match base64::engine::general_purpose::STANDARD.decode(signature_value) {
        Ok(value) if value.len() == 64 => value,
        _ => {
            return Ok(ComponentPackageTrust {
                status: ComponentPackageTrustStatus::InvalidSignature,
                signature_present: true,
                signature_valid: false,
                installable: false,
                message: "组件包 Ed25519 签名格式无效。".into(),
            })
        }
    };
    let verifying_key = VerifyingKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .expect("validated Ed25519 key length"),
    )
    .map_err(|_| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageSignatureInvalid,
            "发布者 Ed25519 公钥无法解析",
        )
    })?;
    let signature = Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .expect("validated Ed25519 signature length"),
    );
    let signed = package_signature_material(header, computed_content_digest)?;
    if verifying_key.verify(&signed, &signature).is_err() {
        return Ok(ComponentPackageTrust {
            status: ComponentPackageTrustStatus::InvalidSignature,
            signature_present: true,
            signature_valid: false,
            installable: false,
            message: "组件包 Ed25519 签名校验失败。".into(),
        });
    }
    let trusted = trusted_publishers
        .get(&publisher.id)
        .is_some_and(|stored| stored.public_key == publisher.public_key);
    Ok(ComponentPackageTrust {
        status: if trusted {
            ComponentPackageTrustStatus::Trusted
        } else {
            ComponentPackageTrustStatus::SignedUntrusted
        },
        signature_present: true,
        signature_valid: true,
        installable: trusted || !executable,
        message: if trusted {
            format!("已验证并信任发布者 {}。", publisher.display_name)
        } else if executable {
            format!(
                "发布者 {} 的签名有效，但尚未获得本机信任。",
                publisher.display_name
            )
        } else {
            format!(
                "发布者 {} 的签名有效；资料包可在确认后安装。",
                publisher.display_name
            )
        },
    })
}

fn package_signature_material(
    header: &pmc_platform::PackageHeaderV1,
    content_digest: &str,
) -> Result<Vec<u8>, ComponentRuntimeError> {
    let mut unsigned_header = header.clone();
    unsigned_header.extensions.remove("signature");
    let mut material = b"nexora.component-pack.v1\0".to_vec();
    material.extend(serde_json::to_vec(&unsigned_header).map_err(protocol_error)?);
    material.push(0);
    material.extend(content_digest.as_bytes());
    Ok(material)
}

fn ensure_package_installable(
    inspection: &ComponentPackageInspection,
) -> Result<(), ComponentRuntimeError> {
    if inspection.trust.installable {
        return Ok(());
    }
    Err(ComponentRuntimeError::new(
        match inspection.trust.status {
            ComponentPackageTrustStatus::InvalidSignature => {
                ComponentRuntimeErrorCode::ComponentPackageSignatureInvalid
            }
            _ => ComponentRuntimeErrorCode::ComponentPackageUntrusted,
        },
        inspection.trust.message.clone(),
    ))
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

fn archive_file_name(manifest: &ComponentManifestV1) -> String {
    format!("{}-{}.pmc-pack", manifest.id, manifest.version)
}

fn copy_file_atomic(source: &Path, destination: &Path) -> Result<(), ComponentRuntimeError> {
    let parent = destination.parent().ok_or_else(|| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentIoError,
            "组件归档缓存路径缺少父目录",
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| io_error("创建组件归档缓存目录失败", error))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("component.pmc-pack"),
        Uuid::new_v4().simple()
    ));
    let result = (|| {
        let mut input =
            fs::File::open(source).map_err(|error| io_error("读取组件包缓存来源失败", error))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error("创建组件包缓存临时文件失败", error))?;
        std::io::copy(&mut input, &mut output)
            .map_err(|error| io_error("写入组件包缓存失败", error))?;
        output
            .sync_all()
            .map_err(|error| io_error("同步组件包缓存失败", error))?;
        replace_file(&temporary, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn replace_file(source: &Path, destination: &Path) -> Result<(), ComponentRuntimeError> {
    if destination.exists() {
        fs::remove_file(destination).map_err(|error| io_error("替换组件包缓存失败", error))?;
    }
    fs::rename(source, destination).map_err(|error| io_error("提交组件包缓存失败", error))
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

fn validate_platform_compatibility(
    manifest: &ComponentManifestV1,
) -> Result<(), ComponentRuntimeError> {
    let supported = manifest.platforms.iter().any(|target| match target {
        PlatformTarget::Any => true,
        PlatformTarget::WindowsX64 => cfg!(all(target_os = "windows", target_arch = "x86_64")),
        PlatformTarget::WindowsArm64 => cfg!(all(target_os = "windows", target_arch = "aarch64")),
    });
    if supported {
        return Ok(());
    }
    Err(ComponentRuntimeError::new(
        ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
        format!(
            "组件 {} 不支持当前 Nexora 平台（声明：{}）",
            manifest.id,
            manifest
                .platforms
                .iter()
                .map(|target| match target {
                    PlatformTarget::Any => "any",
                    PlatformTarget::WindowsX64 => "windows-x64",
                    PlatformTarget::WindowsArm64 => "windows-arm64",
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ))
}

fn validate_native_library_entry(
    root: &Path,
    manifest: &ComponentManifestV1,
) -> Result<(), ComponentRuntimeError> {
    if manifest.runtime != ComponentRuntime::NativeLibrary {
        return Ok(());
    }
    if !cfg!(target_os = "windows") {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
            "native-library 隔离宿主第一版仅支持 Windows",
        ));
    }
    let entry = resolve_entry(root, manifest)?;
    if entry.extension().and_then(|value| value.to_str()) != Some("dll") {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "native-library 入口必须是 .dll 文件",
        ));
    }
    let machine = read_pe_machine(&entry)?;
    let expected = if cfg!(target_arch = "x86_64") {
        0x8664
    } else if cfg!(target_arch = "aarch64") {
        0xaa64
    } else {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
            "当前 CPU 架构不支持 native-library 隔离宿主",
        ));
    };
    if machine != expected {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentRuntimeUnsupported,
            format!(
                "native-library DLL 架构不匹配（DLL: 0x{machine:04x}，Nexora: 0x{expected:04x}）"
            ),
        ));
    }
    Ok(())
}

fn read_pe_machine(path: &Path) -> Result<u16, ComponentRuntimeError> {
    let mut file = fs::File::open(path).map_err(|error| io_error("读取 DLL 头失败", error))?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|error| io_error("DLL 头过短", error))?;
    if &dos[..2] != b"MZ" {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "native-library 入口不是有效的 Windows PE DLL",
        ));
    }
    let offset = u32::from_le_bytes([dos[0x3c], dos[0x3d], dos[0x3e], dos[0x3f]]) as u64;
    if offset > 1024 * 1024 {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "native-library PE 头偏移异常",
        ));
    }
    use std::io::Seek;
    file.seek(std::io::SeekFrom::Start(offset))
        .map_err(|error| io_error("定位 DLL PE 头失败", error))?;
    let mut header = [0u8; 6];
    file.read_exact(&mut header)
        .map_err(|error| io_error("DLL PE 头过短", error))?;
    if &header[..4] != b"PE\0\0" {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "native-library 入口缺少有效 PE 标识",
        ));
    }
    Ok(u16::from_le_bytes([header[4], header[5]]))
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

fn load_trusted_publishers(
    path: &Path,
) -> Result<StoredComponentTrustState, ComponentRuntimeError> {
    if !path.exists() {
        return Ok(StoredComponentTrustState::default());
    }
    let input =
        fs::read_to_string(path).map_err(|error| io_error("读取组件发布者信任库失败", error))?;
    let state =
        serde_json::from_str::<StoredComponentTrustState>(&input).map_err(protocol_error)?;
    if state.schema_version != COMPONENT_TRUST_SCHEMA_VERSION {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            format!("不支持组件发布者信任库版本 {}", state.schema_version),
        ));
    }
    if state.publishers.iter().any(|publisher| {
        publisher.id.trim().is_empty()
            || publisher.display_name.trim().is_empty()
            || base64::engine::general_purpose::STANDARD
                .decode(&publisher.public_key)
                .map(|bytes| bytes.len() != 32)
                .unwrap_or(true)
    }) {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageInvalid,
            "组件发布者信任库包含无效记录",
        ));
    }
    Ok(state)
}

fn persist_trusted_publishers(
    path: &Path,
    state: &StoredComponentTrustState,
) -> Result<(), ComponentRuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("创建组件发布者信任库目录失败", error))?;
    }
    let mut publishers = state.publishers.clone();
    publishers.sort_by(|left, right| left.id.cmp(&right.id));
    let stored = StoredComponentTrustState {
        schema_version: COMPONENT_TRUST_SCHEMA_VERSION,
        publishers,
    };
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    let mut bytes = serde_json::to_vec_pretty(&stored).map_err(protocol_error)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| io_error("创建组件发布者信任库临时文件失败", error))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| io_error("写入组件发布者信任库失败", error))?;
    let backup = path.with_extension(format!("{}.bak", Uuid::new_v4().simple()));
    let result = (|| {
        if path.exists() {
            fs::rename(path, &backup)
                .map_err(|error| io_error("备份组件发布者信任库失败", error))?;
        }
        if let Err(error) = fs::rename(&temporary, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(io_error("提交组件发布者信任库失败", error));
        }
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| io_error("清理组件发布者信任库备份失败", error))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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
            candidates.push(
                parent
                    .join("resources")
                    .join("component-host")
                    .join(resource_name),
            );
            candidates.push(
                parent
                    .join("_up_")
                    .join("resources")
                    .join("component-host")
                    .join(resource_name),
            );
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

fn blendio_service_path() -> Option<PathBuf> {
    let executable = if cfg!(target_os = "windows") {
        "pmc-blendio-service.exe"
    } else {
        "pmc-blendio-service"
    };
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("NEXORA_BLENDIO_SERVICE_PATH") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            candidates.push(parent.join(executable));
            candidates.push(
                parent
                    .join("resources")
                    .join("blendio-service")
                    .join(executable),
            );
            candidates.push(
                parent
                    .join("_up_")
                    .join("resources")
                    .join("blendio-service")
                    .join(executable),
            );
            if let Some(target_root) = parent.parent() {
                candidates.push(
                    target_root
                        .join("resources")
                        .join("blendio-service")
                        .join(executable),
                );
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

async fn capture_component_stream<R>(
    stream: R,
    prefix: Option<String>,
    app_handle: Option<AppHandle>,
    operation_id: String,
    component_id: String,
) -> String
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(stream);
    let mut captured = String::new();
    let mut truncated = false;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let rendered = match &prefix {
            Some(prefix) => format!("{prefix}{}", line.trim_end_matches(['\r', '\n'])),
            None => line.trim_end_matches(['\r', '\n']).to_string(),
        };
        if let Some(app_handle) = &app_handle {
            if let Ok(value) = serde_json::from_str::<Value>(rendered.trim()) {
                if matches!(
                    value.get("type").and_then(Value::as_str),
                    Some("progress" | "log" | "heartbeat")
                ) {
                    let _ = app_handle.emit(
                        "nexora:component-operation-progress",
                        json!({
                            "operationId": operation_id,
                            "componentId": component_id,
                            "type": value.get("type").cloned().unwrap_or(Value::Null),
                            "progress": value.get("progress").cloned().unwrap_or(Value::Null),
                            "message": value.get("message").cloned().unwrap_or(Value::Null),
                            "payload": value,
                        }),
                    );
                }
            }
        }
        if captured.len().saturating_add(rendered.len() + 1) <= MAX_CAPTURED_LOG_BYTES {
            captured.push_str(&rendered);
            captured.push('\n');
        } else {
            truncated = true;
        }
    }
    if truncated {
        captured.push_str("[日志已截断]\n");
    }
    captured
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
    use crate::component_packager::{generate_signing_key, pack_component, ComponentPackRequest};
    use ed25519_dalek::{Signer, SigningKey};
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
    fn legacy_runtime_state_defaults_to_no_disabled_components() {
        let state: StoredComponentRuntimeState =
            serde_json::from_str(r#"{"schemaVersion":1,"removedBundled":["nexora.example"]}"#)
                .unwrap();
        assert!(state.disabled_components.is_empty());
        assert!(state.removed_bundled.contains("nexora.example"));
    }

    #[test]
    fn disabled_bundled_component_stays_out_of_active_catalog() {
        let manifest = parse_component_manifest(
            r#"{
              "schemaVersion": 1,
              "id": "nexora.test.disabled",
              "name": "Disabled test",
              "version": "1.0.0",
              "apiVersion": "1",
              "runtime": "builtin-rust",
              "platforms": ["any"]
            }"#,
        )
        .unwrap();
        let bundled = BTreeMap::from([(manifest.id.clone(), manifest)]);
        let mut state = StoredComponentRuntimeState::default();
        state
            .disabled_components
            .insert("nexora.test.disabled".into());
        let packages = std::env::temp_dir().join(format!("nexora-empty-{}", Uuid::new_v4()));
        let catalog = load_catalog(&packages, &bundled, &state).unwrap();
        assert!(!catalog.contains_key("nexora.test.disabled"));
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

    #[test]
    fn signed_executable_package_requires_a_trusted_publisher() {
        let path = std::env::temp_dir().join(format!("nexora-signed-{}.pmc-pack", Uuid::new_v4()));
        let manifest = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 1,
            "id": "example.signed-native",
            "name": "Signed native example",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "native-process",
            "platforms": ["windows-x64"],
            "entry": "bin/windows-x64/example.exe"
        }))
        .unwrap();
        let mut content_hasher = blake3::Hasher::new();
        content_hasher.update(b"component.json");
        content_hasher.update(&[0]);
        content_hasher.update(&manifest);
        let content_digest = content_hasher.finalize().to_hex().to_string();
        let signing_key = SigningKey::from_bytes(&[9u8; 32]);
        let public_key = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().as_bytes());
        let mut extensions = pmc_platform::ExtensionFields::new();
        extensions.insert(
            "contentDigest".into(),
            Value::String(content_digest.clone()),
        );
        extensions.insert(
            "publisher".into(),
            serde_json::json!({
                "id": "example.publisher",
                "displayName": "Example Publisher",
                "publicKey": public_key,
            }),
        );
        let mut header = pmc_platform::PackageHeaderV1 {
            magic: pmc_platform::PACKAGE_MAGIC.into(),
            schema_version: pmc_platform::PLATFORM_SCHEMA_VERSION,
            format_version: pmc_platform::PACKAGE_FORMAT_VERSION,
            kind: pmc_platform::PackageKind::ComponentPack,
            package_id: "example.signed-native".into(),
            created_at: 0,
            producer_version: "1.0.0".into(),
            payload: pmc_platform::PackagePayloadDescriptor {
                path: "component.json".into(),
                digest: pmc_platform::ContentDigest {
                    algorithm: pmc_platform::DigestAlgorithm::Blake3,
                    value: blake3::hash(&manifest).to_hex().to_string(),
                },
                size_bytes: manifest.len() as u64,
                extensions: pmc_platform::ExtensionFields::new(),
            },
            extensions,
        };
        let signature =
            signing_key.sign(&package_signature_material(&header, &content_digest).unwrap());
        header.extensions.insert(
            "signature".into(),
            serde_json::json!({
                "algorithm": "ed25519",
                "value": base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            }),
        );
        let file = fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer.start_file("component.json", options).unwrap();
        writer.write_all(&manifest).unwrap();
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(serde_json::to_vec(&header).unwrap().as_slice())
            .unwrap();
        writer.finish().unwrap();

        let unsigned = inspect_component_package(&path).unwrap();
        assert_eq!(
            unsigned.trust.status,
            ComponentPackageTrustStatus::SignedUntrusted
        );
        assert!(!unsigned.trust.installable);
        let mut trusted = BTreeMap::new();
        trusted.insert(
            "example.publisher".into(),
            StoredTrustedPublisher {
                id: "example.publisher".into(),
                display_name: "Example Publisher".into(),
                public_key: public_key.clone(),
                trusted_at: 0,
            },
        );
        let inspected = inspect_component_package_with_trust(&path, &trusted).unwrap();
        assert_eq!(inspected.trust.status, ComponentPackageTrustStatus::Trusted);
        assert!(inspected.trust.installable);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn packer_output_is_verified_then_safe_to_extract_for_python_automation() {
        let root = std::env::temp_dir().join(format!("nexora-packer-roundtrip-{}", Uuid::new_v4()));
        let source = root.join("source");
        let package = root.join("automation.pmc-pack");
        let key = root.join("publisher.json");
        let extracted = root.join("extracted");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("component.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "id": "test.packer-automation",
                "name": "Packer automation test",
                "version": "1.0.0",
                "apiVersion": "1",
                "runtime": "python-action",
                "role": "feature",
                "distribution": "marketplace",
                "uiMode": "none",
                "platforms": ["windows-x64"],
                "entry": "main.py",
                "contributes": {
                    "automationCommands": [{
                        "id": "test.packer-automation.probe",
                        "command": "probe",
                        "name": "Probe",
                        "contextRequirement": "global",
                        "executionSemantics": "pure",
                        "inputSchema": {"type": "object"},
                        "outputSchema": {"type": "object"}
                    }]
                }
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            source.join("main.py"),
            "print('{\"ok\":true,\"result\":{}}')\n",
        )
        .unwrap();

        let signing = generate_signing_key(&key).unwrap();
        let packed = pack_component(ComponentPackRequest {
            source_path: source.to_string_lossy().into_owned(),
            destination_path: package.to_string_lossy().into_owned(),
            key_path: key.to_string_lossy().into_owned(),
            publisher_id: "test.r11.publisher".into(),
            publisher_name: "R11 Test Publisher".into(),
            license: "NOASSERTION".into(),
            producer_version: "1.0.0".into(),
        })
        .unwrap();
        assert_eq!(packed.component_id, "test.packer-automation");

        let untrusted = inspect_component_package(&package).unwrap();
        assert_eq!(
            untrusted.trust.status,
            ComponentPackageTrustStatus::SignedUntrusted
        );
        assert!(untrusted.trust.signature_present);
        assert!(untrusted.trust.signature_valid);
        assert!(!untrusted.trust.installable);

        let mut trusted_publishers = BTreeMap::new();
        trusted_publishers.insert(
            "test.r11.publisher".into(),
            StoredTrustedPublisher {
                id: "test.r11.publisher".into(),
                display_name: "R11 Test Publisher".into(),
                public_key: signing.public_key,
                trusted_at: 0,
            },
        );
        let trusted = inspect_component_package_with_trust(&package, &trusted_publishers).unwrap();
        assert_eq!(trusted.trust.status, ComponentPackageTrustStatus::Trusted);
        assert!(trusted.trust.installable);

        extract_component_package(&package, &extracted).unwrap();
        let manifest = read_component_manifest(&extracted.join(COMPONENT_MANIFEST_FILE)).unwrap();
        validate_entry(&extracted, &manifest).unwrap();
        validate_platform_compatibility(&manifest).unwrap();
        validate_native_library_entry(&extracted, &manifest).unwrap();
        validate_presentation_component(&extracted, &manifest).unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
