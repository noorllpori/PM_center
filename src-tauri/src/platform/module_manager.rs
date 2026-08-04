use super::resource_registry::{ResourceCleanupReport, ResourceDiagnostic, ResourceRegistry};
use chrono::Utc;
use pmc_platform::{validate_module_graph, ModuleManifestV1, ValidateContract};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

pub type LifecycleFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

#[derive(Clone)]
pub struct ModuleContext {
    pub module_id: String,
    pub resources: Arc<ResourceRegistry>,
}

pub trait ModuleLifecycle: Send + Sync {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()>;
    fn stop<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()>;
    fn health<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth>;

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

pub struct RegisteredModule {
    pub manifest: ModuleManifestV1,
    pub lifecycle: Arc<dyn ModuleLifecycle>,
    pub diagnostic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleState {
    Disabled,
    Resolving,
    Starting,
    Running,
    Stopping,
    Blocked,
    Error,
    RestartRequired,
}

impl ModuleState {
    fn is_active(self) -> bool {
        matches!(
            self,
            Self::Resolving
                | Self::Starting
                | Self::Running
                | Self::Stopping
                | Self::RestartRequired
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleHealthLevel {
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleHealth {
    pub level: ModuleHealthLevel,
    pub message: String,
    pub checked_at: Option<i64>,
}

impl ModuleHealth {
    pub fn unknown() -> Self {
        Self {
            level: ModuleHealthLevel::Unknown,
            message: "尚未检查".into(),
            checked_at: None,
        }
    }

    pub fn healthy(message: impl Into<String>) -> Self {
        Self {
            level: ModuleHealthLevel::Healthy,
            message: message.into(),
            checked_at: Some(Utc::now().timestamp_millis()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModuleErrorCode {
    ModuleNotFound,
    ModuleAlreadyRegistered,
    ModuleDependencyCycle,
    ModuleMissingDependency,
    ModuleDependencyVersion,
    ModuleConflict,
    ModuleBusy,
    ModuleStartTimeout,
    ModuleStartFailed,
    ModuleHealthCheckFailed,
    ModuleStopBlocked,
    ModuleStopTimeout,
    ModuleStopFailed,
    ModuleForcedStop,
    ModuleResourceCleanupFailed,
    ModulePersistenceError,
    ModuleDiagnosticActionUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleManagerError {
    pub code: ModuleErrorCode,
    pub module_id: Option<String>,
    pub message: String,
    pub details: Vec<String>,
}

impl ModuleManagerError {
    pub fn new(code: ModuleErrorCode, module_id: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            code,
            module_id: module_id.map(str::to_owned),
            message: message.into(),
            details: Vec::new(),
        }
    }

    fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

impl Display for ModuleManagerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ModuleManagerError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleErrorRecord {
    pub code: ModuleErrorCode,
    pub message: String,
    pub details: Vec<String>,
    pub occurred_at: i64,
}

impl From<&ModuleManagerError> for ModuleErrorRecord {
    fn from(error: &ModuleManagerError) -> Self {
        Self {
            code: error.code,
            message: error.message.clone(),
            details: error.details.clone(),
            occurred_at: Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopStrategy {
    Graceful,
    Cascade,
    Force,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyDiagnostic {
    pub id: String,
    pub required: bool,
    pub version_requirement: String,
    pub installed: bool,
    pub compatible: bool,
    pub state: Option<ModuleState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDiagnosticSnapshot {
    pub manifest: ModuleManifestV1,
    pub state: ModuleState,
    pub desired_enabled: bool,
    pub health: ModuleHealth,
    pub dependencies: Vec<DependencyDiagnostic>,
    pub dependents: Vec<String>,
    pub resources: Vec<ResourceDiagnostic>,
    pub last_error: Option<ModuleErrorRecord>,
    pub started_at: Option<i64>,
    pub updated_at: i64,
    pub diagnostic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRuntimeOverview {
    pub modules: Vec<ModuleDiagnosticSnapshot>,
    pub resource_count: usize,
    pub persistence_path: String,
    pub previous_shutdown_clean: bool,
    pub startup_notice: Option<ModuleErrorRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisablePreview {
    pub module_id: String,
    pub can_disable_gracefully: bool,
    pub running_dependents: Vec<String>,
    pub resources: Vec<ResourceDiagnostic>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ProfileModuleRuntimeSnapshot {
    desired_enabled: BTreeSet<String>,
    running: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileModuleTransition {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
    pub restored_after_failure: bool,
}

#[derive(Debug, Clone)]
struct ModuleRuntimeRecord {
    state: ModuleState,
    desired_enabled: bool,
    health: ModuleHealth,
    last_error: Option<ModuleErrorRecord>,
    started_at: Option<i64>,
    updated_at: i64,
}

impl ModuleRuntimeRecord {
    fn disabled() -> Self {
        Self {
            state: ModuleState::Disabled,
            desired_enabled: false,
            health: ModuleHealth::unknown(),
            last_error: None,
            started_at: None,
            updated_at: Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedModuleState {
    #[serde(default)]
    desired_enabled: bool,
    #[serde(default = "default_disabled_state")]
    last_state: ModuleState,
    #[serde(default)]
    last_error: Option<ModuleErrorRecord>,
}

fn default_disabled_state() -> ModuleState {
    ModuleState::Disabled
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModuleRuntimePersistence {
    schema_version: u16,
    last_clean_shutdown: bool,
    updated_at: i64,
    #[serde(default)]
    modules: BTreeMap<String, PersistedModuleState>,
}

impl Default for ModuleRuntimePersistence {
    fn default() -> Self {
        Self {
            schema_version: 1,
            last_clean_shutdown: true,
            updated_at: Utc::now().timestamp_millis(),
            modules: BTreeMap::new(),
        }
    }
}

pub struct ModuleManager {
    modules: BTreeMap<String, RegisteredModule>,
    runtime: BTreeMap<String, StdMutex<ModuleRuntimeRecord>>,
    resources: Arc<ResourceRegistry>,
    persistence_path: PathBuf,
    persistence_lock: StdMutex<()>,
    operation_lock: Mutex<()>,
    previous_shutdown_clean: bool,
    startup_notice: Option<ModuleErrorRecord>,
}

impl ModuleManager {
    pub fn new(
        persistence_path: PathBuf,
        registered_modules: Vec<RegisteredModule>,
    ) -> Result<Arc<Self>, ModuleManagerError> {
        let manifests = registered_modules
            .iter()
            .map(|module| module.manifest.clone())
            .collect::<Vec<_>>();
        let installed_ids = manifests
            .iter()
            .map(|manifest| manifest.id.clone())
            .collect::<BTreeSet<_>>();
        if installed_ids.len() != manifests.len() {
            return Err(ModuleManagerError::new(
                ModuleErrorCode::ModuleAlreadyRegistered,
                None,
                "存在重复的模块 ID",
            ));
        }
        for manifest in &manifests {
            manifest.validate_contract().map_err(|error| {
                ModuleManagerError::new(
                    ModuleErrorCode::ModuleDependencyVersion,
                    Some(&manifest.id),
                    error.message,
                )
                .with_details(vec![format!("path={}", error.path)])
            })?;
        }

        // Missing or incompatible dependencies are recoverable installation states. Validate the
        // installed topology here, then expose affected modules as blocked below.
        let mut installed_topology = manifests.clone();
        for manifest in &mut installed_topology {
            manifest
                .requires_modules
                .retain(|dependency| installed_ids.contains(&dependency.id));
            for dependency in &mut manifest.requires_modules {
                dependency.version_requirement = "*".into();
            }
        }
        validate_module_graph(&installed_topology).map_err(|error| {
            let code = match error.code.as_str() {
                "DEPENDENCY_CYCLE" => ModuleErrorCode::ModuleDependencyCycle,
                "MISSING_DEPENDENCY" => ModuleErrorCode::ModuleMissingDependency,
                "MODULE_CONFLICT" => ModuleErrorCode::ModuleConflict,
                _ => ModuleErrorCode::ModuleDependencyVersion,
            };
            ModuleManagerError::new(code, None, error.message)
        })?;

        let mut modules = BTreeMap::new();
        for module in registered_modules {
            let id = module.manifest.id.clone();
            if modules.insert(id.clone(), module).is_some() {
                return Err(ModuleManagerError::new(
                    ModuleErrorCode::ModuleAlreadyRegistered,
                    Some(&id),
                    format!("模块 {id} 已注册"),
                ));
            }
        }

        let (persistence, startup_notice) = load_persistence(&persistence_path);
        let previous_shutdown_clean = persistence.last_clean_shutdown;
        let mut runtime = BTreeMap::new();
        for id in modules.keys() {
            let mut record = ModuleRuntimeRecord::disabled();
            record.desired_enabled = modules
                .get(id)
                .and_then(|module| module.manifest.extensions.get("defaultEnabled"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if let Some(persisted) = persistence.modules.get(id) {
                record.desired_enabled = persisted.desired_enabled;
                record.last_error = persisted.last_error.clone();
                if !previous_shutdown_clean && persisted.last_state != ModuleState::Disabled {
                    record.last_error = Some(ModuleErrorRecord {
                        code: ModuleErrorCode::ModuleForcedStop,
                        message: "检测到上次运行未完成清理，已从禁用状态恢复并等待重新启动".into(),
                        details: vec![format!("previousState={:?}", persisted.last_state)],
                        occurred_at: Utc::now().timestamp_millis(),
                    });
                }
            }
            runtime.insert(id.clone(), StdMutex::new(record));
        }

        let manager = Arc::new(Self {
            modules,
            runtime,
            resources: Arc::new(ResourceRegistry::default()),
            persistence_path,
            persistence_lock: StdMutex::new(()),
            operation_lock: Mutex::new(()),
            previous_shutdown_clean,
            startup_notice,
        });
        for id in manager.modules.keys() {
            if let Some(error) = manager.direct_dependency_error(id) {
                manager.update_record(id, |record| {
                    record.state = ModuleState::Blocked;
                    record.health = ModuleHealth {
                        level: ModuleHealthLevel::Unhealthy,
                        message: error.message.clone(),
                        checked_at: Some(Utc::now().timestamp_millis()),
                    };
                    record.last_error = Some(ModuleErrorRecord::from(&error));
                })?;
            }
        }
        manager.persist(false)?;
        Ok(manager)
    }

    pub fn resources(&self) -> Arc<ResourceRegistry> {
        self.resources.clone()
    }

    pub fn overview(&self) -> ModuleRuntimeOverview {
        ModuleRuntimeOverview {
            modules: self.list_modules(),
            resource_count: self.resources.total_count(),
            persistence_path: self.persistence_path.to_string_lossy().to_string(),
            previous_shutdown_clean: self.previous_shutdown_clean,
            startup_notice: self.startup_notice.clone(),
        }
    }

    pub fn list_modules(&self) -> Vec<ModuleDiagnosticSnapshot> {
        self.modules
            .keys()
            .filter_map(|id| self.snapshot(id).ok())
            .collect()
    }

    pub fn snapshot(
        &self,
        module_id: &str,
    ) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
        let module = self.module(module_id)?;
        let record = self.runtime_record(module_id)?;
        let dependencies = module
            .manifest
            .requires_modules
            .iter()
            .map(|dependency| self.dependency_diagnostic(dependency, true))
            .chain(
                module
                    .manifest
                    .optional_modules
                    .iter()
                    .map(|dependency| self.dependency_diagnostic(dependency, false)),
            )
            .collect();
        Ok(ModuleDiagnosticSnapshot {
            manifest: module.manifest.clone(),
            state: record.state,
            desired_enabled: record.desired_enabled,
            health: record.health.clone(),
            dependencies,
            dependents: self.direct_dependents(module_id),
            resources: self.resources.diagnostics_for(module_id),
            last_error: record.last_error.clone(),
            started_at: record.started_at,
            updated_at: record.updated_at,
            diagnostic: module.diagnostic,
        })
    }

    pub fn preview_disable(&self, module_id: &str) -> Result<DisablePreview, ModuleManagerError> {
        self.module(module_id)?;
        let running_dependents = self.running_dependents(module_id);
        let can_disable_gracefully = running_dependents.is_empty();
        Ok(DisablePreview {
            module_id: module_id.to_owned(),
            can_disable_gracefully,
            running_dependents: running_dependents.clone(),
            resources: self.resources.diagnostics_for(module_id),
            message: if can_disable_gracefully {
                "可以安全停用；模块数据不会被删除".into()
            } else {
                format!(
                    "有 {} 个运行中模块依赖它，需要级联停用或取消操作",
                    running_dependents.len()
                )
            },
        })
    }

    pub async fn restore_desired_modules(&self) -> Vec<ModuleManagerError> {
        let desired = self
            .runtime
            .iter()
            .filter_map(|(id, record)| {
                record
                    .lock()
                    .expect("module runtime mutex poisoned")
                    .desired_enabled
                    .then(|| id.clone())
            })
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for id in desired {
            if let Err(error) = self.enable_module(&id).await {
                errors.push(error);
            }
        }
        errors
    }

    pub(crate) fn capture_profile_module_state(
        &self,
    ) -> Result<ProfileModuleRuntimeSnapshot, ModuleManagerError> {
        let mut desired_enabled = BTreeSet::new();
        let mut running = BTreeSet::new();
        for (id, module) in &self.modules {
            if module.diagnostic {
                continue;
            }
            let record = self.runtime_record(id)?;
            if record.desired_enabled {
                desired_enabled.insert(id.clone());
            }
            if record.state == ModuleState::Running {
                running.insert(id.clone());
            }
        }
        Ok(ProfileModuleRuntimeSnapshot {
            desired_enabled,
            running,
        })
    }

    pub(crate) async fn apply_profile_module_set(
        &self,
        target: &BTreeSet<String>,
    ) -> Result<ProfileModuleTransition, ModuleManagerError> {
        let _guard = self.operation_lock.lock().await;
        self.validate_profile_target(target)?;
        let previous = self.capture_profile_module_state()?;
        match self.apply_profile_module_set_locked(target).await {
            Ok(mut transition) => {
                self.persist(false)?;
                transition.restored_after_failure = false;
                Ok(transition)
            }
            Err(mut error) => {
                match self.restore_profile_module_state_locked(&previous).await {
                    Ok(()) => {
                        error.details.push("profileRollback=restored".to_string());
                    }
                    Err(rollback_error) => {
                        error.details.push(format!(
                            "profileRollbackFailed={:?}:{}",
                            rollback_error.code, rollback_error.message
                        ));
                        error.details.extend(
                            rollback_error
                                .details
                                .into_iter()
                                .map(|detail| format!("rollback:{detail}")),
                        );
                    }
                }
                let _ = self.persist(false);
                Err(error)
            }
        }
    }

    pub(crate) async fn restore_profile_module_state(
        &self,
        snapshot: &ProfileModuleRuntimeSnapshot,
    ) -> Result<(), ModuleManagerError> {
        let _guard = self.operation_lock.lock().await;
        self.restore_profile_module_state_locked(snapshot).await?;
        self.persist(false)
    }

    fn validate_profile_target(&self, target: &BTreeSet<String>) -> Result<(), ModuleManagerError> {
        for id in target {
            let module = self.module(id)?;
            if module.diagnostic {
                return Err(ModuleManagerError::new(
                    ModuleErrorCode::ModuleNotFound,
                    Some(id),
                    "诊断模块不能写入普通装配方案",
                ));
            }
            for dependency in &module.manifest.requires_modules {
                if !target.contains(&dependency.id) {
                    return Err(ModuleManagerError::new(
                        ModuleErrorCode::ModuleMissingDependency,
                        Some(id),
                        format!("装配方案缺少模块依赖 {}", dependency.id),
                    ));
                }
            }
            for conflict in &module.manifest.conflicts {
                if target.contains(conflict) {
                    return Err(ModuleManagerError::new(
                        ModuleErrorCode::ModuleConflict,
                        Some(id),
                        format!("装配方案同时启用了冲突模块 {id} 与 {conflict}"),
                    ));
                }
            }
        }
        Ok(())
    }

    async fn apply_profile_module_set_locked(
        &self,
        target: &BTreeSet<String>,
    ) -> Result<ProfileModuleTransition, ModuleManagerError> {
        let previous = self.capture_profile_module_state()?;
        let active_to_stop = self
            .modules
            .iter()
            .filter(|(_, module)| !module.diagnostic)
            .filter_map(|(id, _)| {
                self.runtime_record(id)
                    .ok()
                    .filter(|record| record.state.is_active() || record.state == ModuleState::Error)
                    .filter(|_| !target.contains(id))
                    .map(|_| id.clone())
            })
            .collect::<BTreeSet<_>>();
        let stop_order = self.stop_order(&active_to_stop)?;
        let mut disabled = Vec::new();
        for id in stop_order {
            self.stop_one(&id, false, true).await?;
            disabled.push(id);
        }

        let topological = self.topological_order()?;
        let mut enabled = Vec::new();
        for id in topological {
            if !target.contains(&id) || self.module(&id)?.diagnostic {
                continue;
            }
            if self.runtime_record(&id)?.state != ModuleState::Running {
                self.enable_locked(&id, false).await?;
                enabled.push(id);
            }
        }

        for (id, module) in &self.modules {
            if module.diagnostic {
                continue;
            }
            self.update_record(id, |record| record.desired_enabled = target.contains(id))?;
        }

        disabled.retain(|id| previous.running.contains(id));
        Ok(ProfileModuleTransition {
            enabled,
            disabled,
            restored_after_failure: false,
        })
    }

    async fn restore_profile_module_state_locked(
        &self,
        snapshot: &ProfileModuleRuntimeSnapshot,
    ) -> Result<(), ModuleManagerError> {
        let active_to_stop = self
            .modules
            .iter()
            .filter(|(_, module)| !module.diagnostic)
            .filter_map(|(id, _)| {
                self.runtime_record(id)
                    .ok()
                    .filter(|record| record.state.is_active() || record.state == ModuleState::Error)
                    .filter(|_| !snapshot.running.contains(id))
                    .map(|_| id.clone())
            })
            .collect::<BTreeSet<_>>();
        for id in self.stop_order(&active_to_stop)? {
            self.stop_one(&id, true, true).await?;
        }

        for id in self.topological_order()? {
            if self.module(&id)?.diagnostic || !snapshot.running.contains(&id) {
                continue;
            }
            if self.runtime_record(&id)?.state != ModuleState::Running {
                self.enable_locked(&id, false).await?;
            }
        }

        for (id, module) in &self.modules {
            if module.diagnostic {
                continue;
            }
            self.update_record(id, |record| {
                record.desired_enabled = snapshot.desired_enabled.contains(id)
            })?;
        }
        Ok(())
    }

    pub async fn enable_module(
        &self,
        module_id: &str,
    ) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
        let _guard = self.operation_lock.lock().await;
        self.enable_locked(module_id, true).await?;
        self.persist(false)?;
        self.snapshot(module_id)
    }

    async fn enable_locked(
        &self,
        module_id: &str,
        mark_desired: bool,
    ) -> Result<(), ModuleManagerError> {
        self.module(module_id)?;
        if mark_desired {
            self.update_record(module_id, |record| record.desired_enabled = true)?;
        }
        if self.runtime_record(module_id)?.state == ModuleState::Running {
            return Ok(());
        }

        let order = match self.resolve_enable_order(module_id) {
            Ok(order) => order,
            Err(error) => {
                self.update_record(module_id, |record| {
                    record.state = ModuleState::Blocked;
                    record.health = ModuleHealth {
                        level: ModuleHealthLevel::Unhealthy,
                        message: error.message.clone(),
                        checked_at: Some(Utc::now().timestamp_millis()),
                    };
                    record.last_error = Some(ModuleErrorRecord::from(&error));
                })?;
                self.persist(false)?;
                return Err(error);
            }
        };
        self.validate_conflicts(&order)?;
        let mut started = Vec::new();
        for id in order {
            if self.runtime_record(&id)?.state == ModuleState::Running {
                continue;
            }
            if let Err(error) = self.start_one(&id).await {
                self.update_record(&id, |record| {
                    record.state = ModuleState::Error;
                    record.health = ModuleHealth {
                        level: ModuleHealthLevel::Unhealthy,
                        message: error.message.clone(),
                        checked_at: Some(Utc::now().timestamp_millis()),
                    };
                    record.last_error = Some(ModuleErrorRecord::from(&error));
                })?;
                self.rollback_started(&started).await;
                self.persist(false)?;
                return Err(error);
            }
            started.push(id);
        }
        Ok(())
    }

    async fn start_one(&self, module_id: &str) -> Result<(), ModuleManagerError> {
        let module = self.module(module_id)?;
        self.transition(module_id, ModuleState::Resolving)?;
        self.transition(module_id, ModuleState::Starting)?;
        let context = self.context(module_id);
        match tokio::time::timeout(
            module.lifecycle.start_timeout(),
            module.lifecycle.start(context.clone()),
        )
        .await
        {
            Err(_) => {
                let _ = self
                    .resources
                    .cleanup_module(module_id, Duration::from_secs(2))
                    .await;
                return Err(ModuleManagerError::new(
                    ModuleErrorCode::ModuleStartTimeout,
                    Some(module_id),
                    format!("模块 {module_id} 启动超时"),
                ));
            }
            Ok(Err(message)) => {
                let _ = self
                    .resources
                    .cleanup_module(module_id, Duration::from_secs(2))
                    .await;
                return Err(ModuleManagerError::new(
                    ModuleErrorCode::ModuleStartFailed,
                    Some(module_id),
                    message,
                ));
            }
            Ok(Ok(())) => {}
        }

        let health =
            match tokio::time::timeout(Duration::from_secs(5), module.lifecycle.health(context))
                .await
            {
                Ok(Ok(health)) if health.level != ModuleHealthLevel::Unhealthy => health,
                Ok(Ok(health)) => {
                    self.stop_after_failed_start(module_id).await;
                    return Err(ModuleManagerError::new(
                        ModuleErrorCode::ModuleHealthCheckFailed,
                        Some(module_id),
                        health.message,
                    ));
                }
                Ok(Err(message)) => {
                    self.stop_after_failed_start(module_id).await;
                    return Err(ModuleManagerError::new(
                        ModuleErrorCode::ModuleHealthCheckFailed,
                        Some(module_id),
                        message,
                    ));
                }
                Err(_) => {
                    self.stop_after_failed_start(module_id).await;
                    return Err(ModuleManagerError::new(
                        ModuleErrorCode::ModuleHealthCheckFailed,
                        Some(module_id),
                        "健康检查超时",
                    ));
                }
            };

        self.update_record(module_id, |record| {
            record.state = ModuleState::Running;
            record.health = health;
            record.last_error = None;
            record.started_at = Some(Utc::now().timestamp_millis());
        })?;
        self.persist(false)?;
        Ok(())
    }

    async fn stop_after_failed_start(&self, module_id: &str) {
        if let Ok(module) = self.module(module_id) {
            let _ = tokio::time::timeout(
                module.lifecycle.stop_timeout(),
                module.lifecycle.stop(self.context(module_id)),
            )
            .await;
        }
        let _ = self
            .resources
            .cleanup_module(module_id, Duration::from_secs(2))
            .await;
    }

    async fn rollback_started(&self, started: &[String]) {
        for id in started.iter().rev() {
            let _ = self.stop_one(id, true, false).await;
        }
    }

    pub async fn disable_module(
        &self,
        module_id: &str,
        strategy: StopStrategy,
    ) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
        let _guard = self.operation_lock.lock().await;
        self.disable_locked(module_id, strategy, false).await?;
        self.persist(false)?;
        self.snapshot(module_id)
    }

    async fn disable_locked(
        &self,
        module_id: &str,
        strategy: StopStrategy,
        preserve_desired: bool,
    ) -> Result<(), ModuleManagerError> {
        self.module(module_id)?;
        let running_dependents = self.running_dependents(module_id);
        if strategy == StopStrategy::Graceful && !running_dependents.is_empty() {
            let error = ModuleManagerError::new(
                ModuleErrorCode::ModuleStopBlocked,
                Some(module_id),
                format!("模块 {module_id} 仍被运行中的模块依赖"),
            )
            .with_details(running_dependents);
            self.update_record(module_id, |record| {
                record.state = ModuleState::Blocked;
                record.last_error = Some(ModuleErrorRecord::from(&error));
            })?;
            self.persist(false)?;
            return Err(error);
        }

        let mut stop_set = self.transitive_running_dependents(module_id);
        stop_set.insert(module_id.to_owned());
        let stop_order = self.stop_order(&stop_set)?;
        let forced = strategy == StopStrategy::Force;
        for id in &stop_order {
            if !preserve_desired {
                self.update_record(id, |record| record.desired_enabled = false)?;
            }
            let state = self.runtime_record(id)?.state;
            if state == ModuleState::Disabled {
                continue;
            }
            if let Err(error) = self.stop_one(id, forced, preserve_desired).await {
                if !forced {
                    self.persist(false)?;
                    return Err(error);
                }
            }
        }

        if !preserve_desired {
            self.stop_orphan_dependencies(module_id, forced).await;
        }
        Ok(())
    }

    async fn stop_one(
        &self,
        module_id: &str,
        forced: bool,
        preserve_desired: bool,
    ) -> Result<(), ModuleManagerError> {
        let module = self.module(module_id)?;
        self.transition(module_id, ModuleState::Stopping)?;
        let stop_result = tokio::time::timeout(
            module.lifecycle.stop_timeout(),
            module.lifecycle.stop(self.context(module_id)),
        )
        .await;
        let cleanup = self
            .resources
            .cleanup_module(module_id, module.lifecycle.stop_timeout())
            .await;

        let lifecycle_error = match stop_result {
            Err(_) => Some(ModuleManagerError::new(
                if forced {
                    ModuleErrorCode::ModuleForcedStop
                } else {
                    ModuleErrorCode::ModuleStopTimeout
                },
                Some(module_id),
                if forced {
                    format!("模块 {module_id} 停止超时，已强制释放登记资源")
                } else {
                    format!("模块 {module_id} 停止超时")
                },
            )),
            Ok(Err(message)) => Some(ModuleManagerError::new(
                if forced {
                    ModuleErrorCode::ModuleForcedStop
                } else {
                    ModuleErrorCode::ModuleStopFailed
                },
                Some(module_id),
                message,
            )),
            Ok(Ok(())) => cleanup_error(module_id, &cleanup, forced),
        };

        if let Some(error) = lifecycle_error {
            self.update_record(module_id, |record| {
                record.state = if forced {
                    ModuleState::Disabled
                } else {
                    ModuleState::Error
                };
                record.health = ModuleHealth::unknown();
                record.started_at = None;
                record.last_error = Some(ModuleErrorRecord::from(&error));
                if !preserve_desired && forced {
                    record.desired_enabled = false;
                }
            })?;
            self.persist(false)?;
            if forced {
                return Ok(());
            }
            return Err(error);
        }

        self.update_record(module_id, |record| {
            record.state = ModuleState::Disabled;
            record.health = ModuleHealth::unknown();
            record.started_at = None;
            record.last_error = None;
            if !preserve_desired {
                record.desired_enabled = false;
            }
        })?;
        self.persist(false)?;
        Ok(())
    }

    async fn stop_orphan_dependencies(&self, module_id: &str, forced: bool) {
        let mut candidates = self.collect_required_dependencies(module_id);
        let mut changed = true;
        while changed {
            changed = false;
            let ids = candidates.iter().cloned().collect::<Vec<_>>();
            for id in ids {
                let Ok(record) = self.runtime_record(&id) else {
                    continue;
                };
                if record.state != ModuleState::Running || record.desired_enabled {
                    continue;
                }
                if self.running_dependents(&id).is_empty() {
                    let _ = self.stop_one(&id, forced, false).await;
                    candidates.extend(self.collect_required_dependencies(&id));
                    changed = true;
                }
            }
        }
    }

    pub async fn restart_module(
        &self,
        module_id: &str,
    ) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
        let _guard = self.operation_lock.lock().await;
        self.module(module_id)?;
        let mut affected = self.transitive_running_dependents(module_id);
        affected.insert(module_id.to_owned());
        let desired = affected
            .iter()
            .filter_map(|id| {
                self.runtime_record(id)
                    .ok()
                    .and_then(|record| record.desired_enabled.then(|| id.clone()))
            })
            .collect::<BTreeSet<_>>();

        self.disable_locked(module_id, StopStrategy::Cascade, true)
            .await?;
        let topo = self.topological_order()?;
        for id in topo {
            if desired.contains(&id) {
                self.enable_locked(&id, true).await?;
            }
        }
        if self.runtime_record(module_id)?.state != ModuleState::Running {
            self.enable_locked(module_id, desired.contains(module_id))
                .await?;
        }
        self.persist(false)?;
        self.snapshot(module_id)
    }

    pub async fn run_health_checks(
        &self,
        module_id: Option<&str>,
    ) -> Result<Vec<ModuleDiagnosticSnapshot>, ModuleManagerError> {
        let _guard = self.operation_lock.lock().await;
        let ids = match module_id {
            Some(id) => {
                self.module(id)?;
                vec![id.to_owned()]
            }
            None => self.modules.keys().cloned().collect(),
        };
        let mut snapshots = Vec::new();
        for id in ids {
            if self.runtime_record(&id)?.state == ModuleState::Running {
                let module = self.module(&id)?;
                let result = tokio::time::timeout(
                    Duration::from_secs(5),
                    module.lifecycle.health(self.context(&id)),
                )
                .await;
                let (health, error) = match result {
                    Ok(Ok(health)) => (health, None),
                    Ok(Err(message)) => {
                        let error = ModuleManagerError::new(
                            ModuleErrorCode::ModuleHealthCheckFailed,
                            Some(&id),
                            message,
                        );
                        (
                            ModuleHealth {
                                level: ModuleHealthLevel::Unhealthy,
                                message: error.message.clone(),
                                checked_at: Some(Utc::now().timestamp_millis()),
                            },
                            Some(error),
                        )
                    }
                    Err(_) => {
                        let error = ModuleManagerError::new(
                            ModuleErrorCode::ModuleHealthCheckFailed,
                            Some(&id),
                            "健康检查超时",
                        );
                        (
                            ModuleHealth {
                                level: ModuleHealthLevel::Unhealthy,
                                message: error.message.clone(),
                                checked_at: Some(Utc::now().timestamp_millis()),
                            },
                            Some(error),
                        )
                    }
                };
                self.update_record(&id, |record| {
                    record.health = health;
                    if let Some(error) = &error {
                        record.last_error = Some(ModuleErrorRecord::from(error));
                    }
                })?;
            }
            snapshots.push(self.snapshot(&id)?);
        }
        self.persist(false)?;
        Ok(snapshots)
    }

    pub async fn shutdown_all(&self) -> Vec<ModuleManagerError> {
        let _guard = self.operation_lock.lock().await;
        let order = self
            .topological_order()
            .unwrap_or_else(|_| self.modules.keys().cloned().collect());
        let mut errors = Vec::new();
        for id in order.into_iter().rev() {
            let state = self
                .runtime_record(&id)
                .map(|record| record.state)
                .unwrap_or(ModuleState::Disabled);
            if state.is_active() || state == ModuleState::Error || self.resources.count_for(&id) > 0
            {
                if let Err(error) = self.stop_one(&id, true, true).await {
                    errors.push(error);
                }
            }
        }
        let cleanup = self.resources.cleanup_all(Duration::from_secs(2)).await;
        if !cleanup.failures.is_empty() || !cleanup.timed_out.is_empty() {
            errors.push(
                ModuleManagerError::new(
                    ModuleErrorCode::ModuleResourceCleanupFailed,
                    None,
                    "应用退出时仍有资源未能正常释放",
                )
                .with_details(cleanup_details(&cleanup)),
            );
        }
        if let Err(error) = self.persist(true) {
            errors.push(error);
        }
        errors
    }

    fn context(&self, module_id: &str) -> ModuleContext {
        ModuleContext {
            module_id: module_id.to_owned(),
            resources: self.resources.clone(),
        }
    }

    fn module(&self, module_id: &str) -> Result<&RegisteredModule, ModuleManagerError> {
        self.modules.get(module_id).ok_or_else(|| {
            ModuleManagerError::new(
                ModuleErrorCode::ModuleNotFound,
                Some(module_id),
                format!("未注册模块 {module_id}"),
            )
        })
    }

    fn runtime_record(&self, module_id: &str) -> Result<ModuleRuntimeRecord, ModuleManagerError> {
        self.runtime
            .get(module_id)
            .ok_or_else(|| {
                ModuleManagerError::new(
                    ModuleErrorCode::ModuleNotFound,
                    Some(module_id),
                    format!("未注册模块 {module_id}"),
                )
            })?
            .lock()
            .map(|record| record.clone())
            .map_err(|_| {
                ModuleManagerError::new(
                    ModuleErrorCode::ModuleBusy,
                    Some(module_id),
                    "模块状态锁不可用",
                )
            })
    }

    fn update_record(
        &self,
        module_id: &str,
        update: impl FnOnce(&mut ModuleRuntimeRecord),
    ) -> Result<(), ModuleManagerError> {
        let runtime = self.runtime.get(module_id).ok_or_else(|| {
            ModuleManagerError::new(
                ModuleErrorCode::ModuleNotFound,
                Some(module_id),
                format!("未注册模块 {module_id}"),
            )
        })?;
        let mut record = runtime.lock().map_err(|_| {
            ModuleManagerError::new(
                ModuleErrorCode::ModuleBusy,
                Some(module_id),
                "模块状态锁不可用",
            )
        })?;
        update(&mut record);
        record.updated_at = Utc::now().timestamp_millis();
        Ok(())
    }

    fn transition(&self, module_id: &str, state: ModuleState) -> Result<(), ModuleManagerError> {
        self.update_record(module_id, |record| record.state = state)?;
        self.persist(false)
    }

    fn dependency_diagnostic(
        &self,
        dependency: &pmc_platform::ModuleDependency,
        required: bool,
    ) -> DependencyDiagnostic {
        let installed = self.modules.get(&dependency.id);
        let compatible = installed.is_some_and(|module| {
            VersionReq::parse(&dependency.version_requirement)
                .ok()
                .zip(Version::parse(&module.manifest.version).ok())
                .is_some_and(|(requirement, version)| requirement.matches(&version))
        });
        DependencyDiagnostic {
            id: dependency.id.clone(),
            required,
            version_requirement: dependency.version_requirement.clone(),
            installed: installed.is_some(),
            compatible,
            state: installed.and_then(|_| {
                self.runtime_record(&dependency.id)
                    .ok()
                    .map(|record| record.state)
            }),
        }
    }

    fn direct_dependency_error(&self, module_id: &str) -> Option<ModuleManagerError> {
        let module = self.modules.get(module_id)?;
        for dependency in &module.manifest.requires_modules {
            let Some(installed) = self.modules.get(&dependency.id) else {
                return Some(ModuleManagerError::new(
                    ModuleErrorCode::ModuleMissingDependency,
                    Some(module_id),
                    format!("模块 {module_id} 缺少必需模块 {}", dependency.id),
                ));
            };
            let requirement = VersionReq::parse(&dependency.version_requirement).ok()?;
            let version = Version::parse(&installed.manifest.version).ok()?;
            if !requirement.matches(&version) {
                return Some(
                    ModuleManagerError::new(
                        ModuleErrorCode::ModuleDependencyVersion,
                        Some(module_id),
                        format!(
                            "模块 {} 版本 {} 不满足 {} 的要求 {}",
                            dependency.id,
                            installed.manifest.version,
                            module_id,
                            dependency.version_requirement
                        ),
                    )
                    .with_details(vec![
                        format!("dependency={}", dependency.id),
                        format!("installedVersion={}", installed.manifest.version),
                        format!("requiredVersion={}", dependency.version_requirement),
                    ]),
                );
            }
        }
        None
    }

    fn direct_dependents(&self, module_id: &str) -> Vec<String> {
        self.modules
            .iter()
            .filter(|(_, module)| {
                module
                    .manifest
                    .requires_modules
                    .iter()
                    .any(|dependency| dependency.id == module_id)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn running_dependents(&self, module_id: &str) -> Vec<String> {
        self.direct_dependents(module_id)
            .into_iter()
            .filter(|id| {
                self.runtime_record(id)
                    .is_ok_and(|record| record.state.is_active())
            })
            .collect()
    }

    fn transitive_running_dependents(&self, module_id: &str) -> BTreeSet<String> {
        let mut result = BTreeSet::new();
        let mut queue = vec![module_id.to_owned()];
        while let Some(current) = queue.pop() {
            for dependent in self.running_dependents(&current) {
                if result.insert(dependent.clone()) {
                    queue.push(dependent);
                }
            }
        }
        result
    }

    fn collect_required_dependencies(&self, module_id: &str) -> BTreeSet<String> {
        let mut result = BTreeSet::new();
        let mut queue = vec![module_id.to_owned()];
        while let Some(current) = queue.pop() {
            let Some(module) = self.modules.get(&current) else {
                continue;
            };
            for dependency in &module.manifest.requires_modules {
                if result.insert(dependency.id.clone()) {
                    queue.push(dependency.id.clone());
                }
            }
        }
        result
    }

    fn resolve_enable_order(&self, module_id: &str) -> Result<Vec<String>, ModuleManagerError> {
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        self.visit_dependencies(module_id, &mut visiting, &mut visited, &mut order)?;
        Ok(order)
    }

    fn visit_dependencies(
        &self,
        module_id: &str,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), ModuleManagerError> {
        if visited.contains(module_id) {
            return Ok(());
        }
        if !visiting.insert(module_id.to_owned()) {
            return Err(ModuleManagerError::new(
                ModuleErrorCode::ModuleDependencyCycle,
                Some(module_id),
                format!("模块依赖形成循环，回到 {module_id}"),
            ));
        }
        let module = self.module(module_id)?;
        for dependency in &module.manifest.requires_modules {
            let Some(installed) = self.modules.get(&dependency.id) else {
                return Err(ModuleManagerError::new(
                    ModuleErrorCode::ModuleMissingDependency,
                    Some(module_id),
                    format!("模块 {module_id} 缺少必需模块 {}", dependency.id),
                ));
            };
            let requirement = VersionReq::parse(&dependency.version_requirement).map_err(|_| {
                ModuleManagerError::new(
                    ModuleErrorCode::ModuleDependencyVersion,
                    Some(module_id),
                    format!("模块 {module_id} 的依赖版本要求无效"),
                )
            })?;
            let version = Version::parse(&installed.manifest.version).map_err(|_| {
                ModuleManagerError::new(
                    ModuleErrorCode::ModuleDependencyVersion,
                    Some(module_id),
                    format!("模块 {} 的版本无效", dependency.id),
                )
            })?;
            if !requirement.matches(&version) {
                return Err(ModuleManagerError::new(
                    ModuleErrorCode::ModuleDependencyVersion,
                    Some(module_id),
                    format!(
                        "模块 {} 版本 {} 不满足 {} 的要求 {}",
                        dependency.id,
                        installed.manifest.version,
                        module_id,
                        dependency.version_requirement
                    ),
                ));
            }
            self.visit_dependencies(&dependency.id, visiting, visited, order)?;
        }
        visiting.remove(module_id);
        visited.insert(module_id.to_owned());
        order.push(module_id.to_owned());
        Ok(())
    }

    fn topological_order(&self) -> Result<Vec<String>, ModuleManagerError> {
        let mut order = Vec::new();
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        for id in self.modules.keys() {
            self.visit_installed_dependencies(id, &mut visiting, &mut visited, &mut order)?;
        }
        Ok(order)
    }

    fn visit_installed_dependencies(
        &self,
        module_id: &str,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), ModuleManagerError> {
        if visited.contains(module_id) {
            return Ok(());
        }
        if !visiting.insert(module_id.to_owned()) {
            return Err(ModuleManagerError::new(
                ModuleErrorCode::ModuleDependencyCycle,
                Some(module_id),
                format!("模块依赖形成循环，回到 {module_id}"),
            ));
        }
        let module = self.module(module_id)?;
        for dependency in &module.manifest.requires_modules {
            if self.modules.contains_key(&dependency.id) {
                self.visit_installed_dependencies(&dependency.id, visiting, visited, order)?;
            }
        }
        visiting.remove(module_id);
        visited.insert(module_id.to_owned());
        order.push(module_id.to_owned());
        Ok(())
    }

    fn stop_order(&self, stop_set: &BTreeSet<String>) -> Result<Vec<String>, ModuleManagerError> {
        let mut order = self.topological_order()?;
        order.retain(|id| stop_set.contains(id));
        order.reverse();
        Ok(order)
    }

    fn validate_conflicts(&self, start_order: &[String]) -> Result<(), ModuleManagerError> {
        let start_set = start_order.iter().cloned().collect::<BTreeSet<_>>();
        for id in start_order {
            let module = self.module(id)?;
            for (other_id, other) in &self.modules {
                if id == other_id {
                    continue;
                }
                let conflicts = module.manifest.conflicts.contains(other_id)
                    || other.manifest.conflicts.contains(id);
                if !conflicts {
                    continue;
                }
                let other_active = start_set.contains(other_id)
                    || self
                        .runtime_record(other_id)
                        .is_ok_and(|record| record.state.is_active());
                if other_active {
                    return Err(ModuleManagerError::new(
                        ModuleErrorCode::ModuleConflict,
                        Some(id),
                        format!("模块 {id} 与 {other_id} 冲突"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn persist(&self, clean_shutdown: bool) -> Result<(), ModuleManagerError> {
        let _guard = self.persistence_lock.lock().map_err(|_| {
            ModuleManagerError::new(
                ModuleErrorCode::ModulePersistenceError,
                None,
                "模块状态持久化锁不可用",
            )
        })?;
        if let Some(parent) = self.persistence_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ModuleManagerError::new(
                    ModuleErrorCode::ModulePersistenceError,
                    None,
                    format!("创建模块状态目录失败: {error}"),
                )
            })?;
        }
        let modules = self
            .runtime
            .iter()
            .map(|(id, record)| {
                let record = record.lock().expect("module runtime mutex poisoned");
                (
                    id.clone(),
                    PersistedModuleState {
                        desired_enabled: record.desired_enabled,
                        last_state: record.state,
                        last_error: record.last_error.clone(),
                    },
                )
            })
            .collect();
        let persistence = ModuleRuntimePersistence {
            schema_version: 1,
            last_clean_shutdown: clean_shutdown,
            updated_at: Utc::now().timestamp_millis(),
            modules,
        };
        let bytes = serde_json::to_vec_pretty(&persistence).map_err(|error| {
            ModuleManagerError::new(
                ModuleErrorCode::ModulePersistenceError,
                None,
                format!("序列化模块状态失败: {error}"),
            )
        })?;
        let temporary = self.persistence_path.with_extension("json.tmp");
        fs::write(&temporary, bytes).map_err(|error| {
            ModuleManagerError::new(
                ModuleErrorCode::ModulePersistenceError,
                None,
                format!("写入模块状态失败: {error}"),
            )
        })?;
        if self.persistence_path.exists() {
            fs::remove_file(&self.persistence_path).map_err(|error| {
                ModuleManagerError::new(
                    ModuleErrorCode::ModulePersistenceError,
                    None,
                    format!("替换模块状态失败: {error}"),
                )
            })?;
        }
        fs::rename(&temporary, &self.persistence_path).map_err(|error| {
            ModuleManagerError::new(
                ModuleErrorCode::ModulePersistenceError,
                None,
                format!("提交模块状态失败: {error}"),
            )
        })
    }
}

fn cleanup_error(
    module_id: &str,
    cleanup: &ResourceCleanupReport,
    forced: bool,
) -> Option<ModuleManagerError> {
    if cleanup.failures.is_empty() && cleanup.timed_out.is_empty() {
        return None;
    }
    Some(
        ModuleManagerError::new(
            if forced {
                ModuleErrorCode::ModuleForcedStop
            } else {
                ModuleErrorCode::ModuleResourceCleanupFailed
            },
            Some(module_id),
            format!("模块 {module_id} 有资源未能正常释放"),
        )
        .with_details(cleanup_details(cleanup)),
    )
}

fn cleanup_details(cleanup: &ResourceCleanupReport) -> Vec<String> {
    cleanup
        .failures
        .iter()
        .map(|failure| format!("{}: {}", failure.resource_id, failure.message))
        .chain(
            cleanup
                .timed_out
                .iter()
                .map(|resource_id| format!("{resource_id}: cleanup timeout")),
        )
        .collect()
}

fn load_persistence(path: &Path) -> (ModuleRuntimePersistence, Option<ModuleErrorRecord>) {
    if !path.exists() {
        return (ModuleRuntimePersistence::default(), None);
    }
    match fs::read(path)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            serde_json::from_slice::<ModuleRuntimePersistence>(&bytes)
                .map_err(|error| error.to_string())
        }) {
        Ok(persistence) if persistence.schema_version == 1 => (persistence, None),
        Ok(persistence) => (
            ModuleRuntimePersistence::default(),
            Some(ModuleErrorRecord {
                code: ModuleErrorCode::ModulePersistenceError,
                message: format!(
                    "不支持模块状态版本 {}，已使用默认状态",
                    persistence.schema_version
                ),
                details: vec![path.to_string_lossy().to_string()],
                occurred_at: Utc::now().timestamp_millis(),
            }),
        ),
        Err(message) => (
            ModuleRuntimePersistence::default(),
            Some(ModuleErrorRecord {
                code: ModuleErrorCode::ModulePersistenceError,
                message: "模块状态文件无法读取，已保留原文件并使用默认状态".into(),
                details: vec![message, path.to_string_lossy().to_string()],
                occurred_at: Utc::now().timestamp_millis(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::resource_registry::ResourceKind;
    use pmc_platform::{
        ExtensionFields, ModuleContributions, ModuleDataPolicy, ModuleDependency, ModuleScope,
    };
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use uuid::Uuid;

    struct RecordingLifecycle {
        id: String,
        log: Arc<StdMutex<Vec<String>>>,
        active: Arc<AtomicUsize>,
        fail_start: AtomicBool,
        fail_health: AtomicBool,
        stop_delay: Duration,
        stop_timeout: Duration,
    }

    impl RecordingLifecycle {
        fn new(id: &str, log: Arc<StdMutex<Vec<String>>>, active: Arc<AtomicUsize>) -> Self {
            Self {
                id: id.into(),
                log,
                active,
                fail_start: AtomicBool::new(false),
                fail_health: AtomicBool::new(false),
                stop_delay: Duration::ZERO,
                stop_timeout: Duration::from_secs(1),
            }
        }

        fn failing(mut self) -> Self {
            self.fail_start = AtomicBool::new(true);
            self
        }

        fn slow_stop(mut self) -> Self {
            self.stop_delay = Duration::from_millis(100);
            self.stop_timeout = Duration::from_millis(10);
            self
        }

        fn unhealthy(mut self) -> Self {
            self.fail_health = AtomicBool::new(true);
            self
        }
    }

    impl ModuleLifecycle for RecordingLifecycle {
        fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
            Box::pin(async move {
                self.log.lock().unwrap().push(format!("start:{}", self.id));
                if self.fail_start.load(Ordering::SeqCst) {
                    return Err(format!("{} failed", self.id));
                }
                self.active.fetch_add(1, Ordering::SeqCst);
                let id = self.id.clone();
                let log = self.log.clone();
                let active = self.active.clone();
                context.resources.register(
                    context.module_id,
                    ResourceKind::Other,
                    format!("{} resource", self.id),
                    BTreeMap::new(),
                    Box::new(move || {
                        Box::pin(async move {
                            log.lock().unwrap().push(format!("cleanup:{id}"));
                            active.fetch_sub(1, Ordering::SeqCst);
                            Ok(())
                        })
                    }),
                );
                Ok(())
            })
        }

        fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
            Box::pin(async move {
                self.log.lock().unwrap().push(format!("stop:{}", self.id));
                if !self.stop_delay.is_zero() {
                    tokio::time::sleep(self.stop_delay).await;
                }
                Ok(())
            })
        }

        fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
            Box::pin(async move {
                if self.fail_health.load(Ordering::SeqCst) {
                    Ok(ModuleHealth {
                        level: ModuleHealthLevel::Unhealthy,
                        message: format!("{} unhealthy", self.id),
                        checked_at: Some(Utc::now().timestamp_millis()),
                    })
                } else {
                    Ok(ModuleHealth::healthy("ok"))
                }
            })
        }

        fn stop_timeout(&self) -> Duration {
            self.stop_timeout
        }
    }

    fn manifest(id: &str, required: &[&str]) -> ModuleManifestV1 {
        ModuleManifestV1 {
            schema_version: 1,
            id: id.into(),
            name: id.into(),
            description: String::new(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: required
                .iter()
                .map(|id| ModuleDependency {
                    id: (*id).into(),
                    version_requirement: "*".into(),
                })
                .collect(),
            optional_modules: Vec::new(),
            conflicts: Vec::new(),
            capabilities: Vec::new(),
            background_services: Vec::new(),
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions: ExtensionFields::new(),
        }
    }

    fn module(id: &str, required: &[&str], lifecycle: RecordingLifecycle) -> RegisteredModule {
        RegisteredModule {
            manifest: manifest(id, required),
            lifecycle: Arc::new(lifecycle),
            diagnostic: true,
        }
    }

    fn formal_module(
        id: &str,
        required: &[&str],
        lifecycle: RecordingLifecycle,
    ) -> RegisteredModule {
        RegisteredModule {
            manifest: manifest(id, required),
            lifecycle: Arc::new(lifecycle),
            diagnostic: false,
        }
    }

    fn state_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pm-center-module-manager-{name}-{}.json",
            Uuid::new_v4()
        ))
    }

    #[tokio::test]
    async fn profile_module_switch_restores_previous_runtime_after_start_failure() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("profile-rollback");
        let manager = ModuleManager::new(
            path.clone(),
            vec![
                formal_module(
                    "test.alpha",
                    &[],
                    RecordingLifecycle::new("alpha", log.clone(), active.clone()),
                ),
                formal_module(
                    "test.beta",
                    &[],
                    RecordingLifecycle::new("beta", log.clone(), active.clone()).failing(),
                ),
            ],
        )
        .unwrap();
        manager.enable_module("test.alpha").await.unwrap();

        let error = manager
            .apply_profile_module_set(&BTreeSet::from(["test.beta".to_string()]))
            .await
            .unwrap_err();
        assert!(error
            .details
            .iter()
            .any(|detail| detail == "profileRollback=restored"));
        let alpha = manager.snapshot("test.alpha").unwrap();
        let beta = manager.snapshot("test.beta").unwrap();
        assert_eq!(alpha.state, ModuleState::Running);
        assert!(alpha.desired_enabled);
        assert_ne!(beta.state, ModuleState::Running);
        assert!(!beta.desired_enabled);
        assert_eq!(active.load(Ordering::SeqCst), 1);
        assert!(log
            .lock()
            .unwrap()
            .iter()
            .any(|entry| entry == "start:alpha"));
        let _ = manager.shutdown_all().await;
        let _ = fs::remove_file(path);
    }

    #[test]
    fn new_modules_can_opt_into_default_enabled_state() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("default-enabled");
        let mut registered = module(
            "test.default-enabled",
            &[],
            RecordingLifecycle::new("default-enabled", log, active),
        );
        registered
            .manifest
            .extensions
            .insert("defaultEnabled".into(), serde_json::Value::Bool(true));

        let manager = ModuleManager::new(path.clone(), vec![registered]).unwrap();
        assert!(
            manager
                .snapshot("test.default-enabled")
                .unwrap()
                .desired_enabled
        );
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn persisted_disabled_choice_overrides_module_default() {
        let path = state_path("persisted-disabled");
        let create_module = || {
            let mut registered = module(
                "test.persisted-disabled",
                &[],
                RecordingLifecycle::new(
                    "persisted-disabled",
                    Arc::new(StdMutex::new(Vec::new())),
                    Arc::new(AtomicUsize::new(0)),
                ),
            );
            registered
                .manifest
                .extensions
                .insert("defaultEnabled".into(), serde_json::Value::Bool(true));
            registered
        };

        let manager = ModuleManager::new(path.clone(), vec![create_module()]).unwrap();
        assert!(
            manager
                .snapshot("test.persisted-disabled")
                .unwrap()
                .desired_enabled
        );
        manager
            .disable_module("test.persisted-disabled", StopStrategy::Graceful)
            .await
            .unwrap();
        drop(manager);

        let restored = ModuleManager::new(path.clone(), vec![create_module()]).unwrap();
        assert!(
            !restored
                .snapshot("test.persisted-disabled")
                .unwrap()
                .desired_enabled
        );
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn starts_dependencies_and_stops_dependents_in_reverse_order() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("order");
        let manager = ModuleManager::new(
            path.clone(),
            vec![
                module(
                    "test.base",
                    &[],
                    RecordingLifecycle::new("base", log.clone(), active.clone()),
                ),
                module(
                    "test.worker",
                    &["test.base"],
                    RecordingLifecycle::new("worker", log.clone(), active.clone()),
                ),
                module(
                    "test.leaf",
                    &["test.worker"],
                    RecordingLifecycle::new("leaf", log.clone(), active.clone()),
                ),
            ],
        )
        .unwrap();

        manager.enable_module("test.leaf").await.unwrap();
        assert_eq!(active.load(Ordering::SeqCst), 3);
        manager
            .disable_module("test.base", StopStrategy::Cascade)
            .await
            .unwrap();

        let lifecycle_log = log
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.starts_with("start:") || entry.starts_with("stop:"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            lifecycle_log,
            vec![
                "start:base",
                "start:worker",
                "start:leaf",
                "stop:leaf",
                "stop:worker",
                "stop:base"
            ]
        );
        assert_eq!(manager.resources.total_count(), 0);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_dependency_cycles_before_runtime_start() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let error = ModuleManager::new(
            state_path("cycle"),
            vec![
                module(
                    "test.alpha",
                    &["test.beta"],
                    RecordingLifecycle::new("alpha", log.clone(), active.clone()),
                ),
                module(
                    "test.beta",
                    &["test.alpha"],
                    RecordingLifecycle::new("beta", log, active),
                ),
            ],
        )
        .err()
        .expect("cycle should fail");
        assert_eq!(error.code, ModuleErrorCode::ModuleDependencyCycle);
    }

    #[tokio::test]
    async fn startup_failure_rolls_back_started_dependencies() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("rollback");
        let manager = ModuleManager::new(
            path.clone(),
            vec![
                module(
                    "test.base",
                    &[],
                    RecordingLifecycle::new("base", log.clone(), active.clone()),
                ),
                module(
                    "test.failing",
                    &["test.base"],
                    RecordingLifecycle::new("failing", log.clone(), active.clone()).failing(),
                ),
            ],
        )
        .unwrap();

        let error = manager.enable_module("test.failing").await.unwrap_err();
        assert_eq!(error.code, ModuleErrorCode::ModuleStartFailed);
        assert_eq!(
            manager.snapshot("test.base").unwrap().state,
            ModuleState::Disabled
        );
        assert_eq!(manager.resources.total_count(), 0);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert!(log.lock().unwrap().contains(&"stop:base".into()));
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn health_failure_stops_lifecycle_before_releasing_resources() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("health-stop");
        let manager = ModuleManager::new(
            path.clone(),
            vec![module(
                "test.unhealthy",
                &[],
                RecordingLifecycle::new("unhealthy", log.clone(), active.clone()).unhealthy(),
            )],
        )
        .unwrap();

        let error = manager.enable_module("test.unhealthy").await.unwrap_err();
        assert_eq!(error.code, ModuleErrorCode::ModuleHealthCheckFailed);
        assert_eq!(
            &*log.lock().unwrap(),
            &["start:unhealthy", "stop:unhealthy", "cleanup:unhealthy"]
        );
        assert_eq!(manager.resources.total_count(), 0);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn missing_required_dependency_blocks_only_the_affected_module() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("missing-required");
        let manager = ModuleManager::new(
            path.clone(),
            vec![
                module(
                    "test.blocked",
                    &["test.not-installed"],
                    RecordingLifecycle::new("blocked", log.clone(), active.clone()),
                ),
                module(
                    "test.healthy",
                    &[],
                    RecordingLifecycle::new("healthy", log, active.clone()),
                ),
            ],
        )
        .unwrap();

        let snapshot = manager.snapshot("test.blocked").unwrap();
        assert_eq!(snapshot.state, ModuleState::Blocked);
        assert_eq!(
            snapshot.last_error.unwrap().code,
            ModuleErrorCode::ModuleMissingDependency
        );
        let error = manager.enable_module("test.blocked").await.unwrap_err();
        assert_eq!(error.code, ModuleErrorCode::ModuleMissingDependency);
        assert_eq!(
            manager.snapshot("test.blocked").unwrap().state,
            ModuleState::Blocked
        );
        manager.enable_module("test.healthy").await.unwrap();
        manager
            .disable_module("test.healthy", StopStrategy::Graceful)
            .await
            .unwrap();
        assert_eq!(active.load(Ordering::SeqCst), 0);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn incompatible_required_dependency_version_blocks_only_the_consumer() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("incompatible-required");
        let base = module(
            "test.base",
            &[],
            RecordingLifecycle::new("base", log.clone(), active.clone()),
        );
        let mut consumer = module(
            "test.consumer",
            &["test.base"],
            RecordingLifecycle::new("consumer", log, active),
        );
        consumer.manifest.requires_modules[0].version_requirement = "^2.0".into();
        let manager = ModuleManager::new(path.clone(), vec![base, consumer]).unwrap();

        assert_eq!(
            manager.snapshot("test.base").unwrap().state,
            ModuleState::Disabled
        );
        let consumer = manager.snapshot("test.consumer").unwrap();
        assert_eq!(consumer.state, ModuleState::Blocked);
        assert_eq!(
            consumer.last_error.unwrap().code,
            ModuleErrorCode::ModuleDependencyVersion
        );
        let error = manager.enable_module("test.consumer").await.unwrap_err();
        assert_eq!(error.code, ModuleErrorCode::ModuleDependencyVersion);
        assert_eq!(manager.resources.total_count(), 0);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn one_hundred_cycles_leave_no_registered_or_live_resources() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("cycles");
        let manager = ModuleManager::new(
            path.clone(),
            vec![module(
                "test.cycle",
                &[],
                RecordingLifecycle::new("cycle", log, active.clone()),
            )],
        )
        .unwrap();

        for _ in 0..100 {
            manager.enable_module("test.cycle").await.unwrap();
            manager
                .disable_module("test.cycle", StopStrategy::Graceful)
                .await
                .unwrap();
            assert_eq!(manager.resources.total_count(), 0);
            assert_eq!(active.load(Ordering::SeqCst), 0);
        }
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn stop_timeout_is_explainable_and_force_releases_resources() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("timeout");
        let manager = ModuleManager::new(
            path.clone(),
            vec![module(
                "test.slow",
                &[],
                RecordingLifecycle::new("slow", log, active.clone()).slow_stop(),
            )],
        )
        .unwrap();
        manager.enable_module("test.slow").await.unwrap();

        let error = manager
            .disable_module("test.slow", StopStrategy::Graceful)
            .await
            .unwrap_err();
        assert_eq!(error.code, ModuleErrorCode::ModuleStopTimeout);
        assert_eq!(
            manager.snapshot("test.slow").unwrap().state,
            ModuleState::Error
        );
        assert_eq!(manager.resources.total_count(), 0);
        assert_eq!(active.load(Ordering::SeqCst), 0);

        manager
            .disable_module("test.slow", StopStrategy::Force)
            .await
            .unwrap();
        let snapshot = manager.snapshot("test.slow").unwrap();
        assert_eq!(snapshot.state, ModuleState::Disabled);
        assert_eq!(
            snapshot.last_error.unwrap().code,
            ModuleErrorCode::ModuleForcedStop
        );
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn unclean_persistence_recovers_to_disabled_with_diagnostic() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("recovery");
        let manager = ModuleManager::new(
            path.clone(),
            vec![module(
                "test.recovery",
                &[],
                RecordingLifecycle::new("recovery", log.clone(), active.clone()),
            )],
        )
        .unwrap();
        manager.enable_module("test.recovery").await.unwrap();
        drop(manager);
        tokio::task::yield_now().await;

        let recovered = ModuleManager::new(
            path.clone(),
            vec![module(
                "test.recovery",
                &[],
                RecordingLifecycle::new("recovery", log, active),
            )],
        )
        .unwrap();
        let snapshot = recovered.snapshot("test.recovery").unwrap();
        assert_eq!(snapshot.state, ModuleState::Disabled);
        assert!(snapshot.desired_enabled);
        assert_eq!(
            snapshot.last_error.unwrap().code,
            ModuleErrorCode::ModuleForcedStop
        );
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn conflict_and_optional_dependency_are_reported() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let path = state_path("conflict");
        let mut first = manifest("test.first", &[]);
        first.conflicts.push("test.second".into());
        first.optional_modules.push(ModuleDependency {
            id: "test.optional-missing".into(),
            version_requirement: "^1".into(),
        });
        let manager = ModuleManager::new(
            path.clone(),
            vec![
                RegisteredModule {
                    manifest: first,
                    lifecycle: Arc::new(RecordingLifecycle::new(
                        "first",
                        log.clone(),
                        active.clone(),
                    )),
                    diagnostic: true,
                },
                module(
                    "test.second",
                    &[],
                    RecordingLifecycle::new("second", log, active),
                ),
            ],
        )
        .unwrap();
        manager.enable_module("test.second").await.unwrap();
        let error = manager.enable_module("test.first").await.unwrap_err();
        assert_eq!(error.code, ModuleErrorCode::ModuleConflict);
        let optional = &manager.snapshot("test.first").unwrap().dependencies[0];
        assert!(!optional.required);
        assert!(!optional.installed);
        let _ = manager.shutdown_all().await;
        let _ = fs::remove_file(path);
    }
}
