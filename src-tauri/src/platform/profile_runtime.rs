use chrono::Utc;
use pmc_platform::{
    parse_workspace_profile, validate_profile_with_catalogs, CommandPlacement,
    ComponentDistribution, ComponentManifestV1, ComponentRole, ComponentRuntime,
    ComponentSettingsSection, ComponentUiMode, ContractResult, DataSourceScope, ExtensionFields,
    ModuleManifestV1, ProfileCommandBinding, ProfileDataSource, ProfileModuleSelection,
    ProfileShellLayout, ProfileSurface, ProfileWidget, ScriptSurfacePlacement, SurfaceKind,
    SurfaceLayoutKind, WorkspaceProfileV1, PLATFORM_SCHEMA_VERSION,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

const PROFILE_RUNTIME_SCHEMA_VERSION: u16 = 1;
const PROFILE_SWITCH_JOURNAL_SCHEMA_VERSION: u16 = 1;
const MIGRATED_PROFILE_ID: &str = "local.current-pm-center";
const BLANK_PROFILE_ID: &str = "local.blank-workspace";
const DEFAULT_PROFILE_ID: &str = "local.default-pm-center";
const MIGRATION_SOURCE: &str = "current-pm-center";
const PROJECT_MANAGER_MODULE_ID: &str = "builtin.project-manager";
#[cfg(test)]
const LAN_COLLABORATION_MODULE_ID: &str = "builtin.lan-collaboration";
#[cfg(test)]
const RENDER_CENTER_MODULE_ID: &str = "builtin.render-center";
#[cfg(test)]
const MEDIA_LIBRARY_MODULE_ID: &str = "builtin.media-library";
const SCRIPT_AUTOMATION_MODULE_ID: &str = "builtin.script-automation";
const PROJECT_HOME_PROFILE_SURFACE_ID: &str = "pm-center-project-home";
const PROJECT_HOME_CONTRIBUTION_ID: &str = "builtin.project-manager.home-surface";
#[cfg(test)]
const LAN_MAIN_PROFILE_SURFACE_ID: &str = "lan-collaboration-main";
#[cfg(test)]
const LAN_MAIN_CONTRIBUTION_ID: &str = "builtin.lan-collaboration.main-surface";
#[cfg(test)]
const EXTERNAL_RENDER_STATION_PROFILE_SURFACE_ID: &str = "external-render-station";
#[cfg(test)]
const EXTERNAL_RENDER_STATION_CONTRIBUTION_ID: &str =
    "builtin.render-center.external-station-surface";
#[cfg(test)]
const MEDIA_LIBRARY_PROFILE_SURFACE_ID: &str = "media-library";
#[cfg(test)]
const MEDIA_LIBRARY_CONTRIBUTION_ID: &str = "builtin.media-library.surface";
const PROJECT_DIRECTORY_WIDGET_ID: &str = "builtin.project-manager.project-directory-widget";
const PROJECT_QUICK_ACTIONS_WIDGET_ID: &str = "builtin.project-manager.quick-actions-widget";
const RECENT_PROJECTS_WIDGET_ID: &str = "builtin.project-manager.recent-projects-widget";
const PROJECT_CATALOG_WIDGET_ID: &str = "builtin.project-manager.project-catalog-widget";
const PROJECT_DIRECTORY_DATA_SOURCE_ID: &str =
    "builtin.project-manager.project-directory-data-source";
const PROJECT_QUICK_ACTIONS_DATA_SOURCE_ID: &str =
    "builtin.project-manager.quick-actions-data-source";
const RECENT_PROJECTS_DATA_SOURCE_ID: &str = "builtin.project-manager.recent-projects-data-source";
const PROJECT_CATALOG_DATA_SOURCE_ID: &str = "builtin.project-manager.project-catalog-data-source";
const CREATE_PROJECT_COMMAND_ID: &str = "builtin.project-manager.create-project-command";
const IMPORT_PROJECT_COMMAND_ID: &str = "builtin.project-manager.import-project-command";
const OPEN_PROJECT_COMMAND_ID: &str = "builtin.project-manager.open-project-command";
const LEGACY_HOME_COMPOSITION_MIGRATION_VERSION: u16 = 3;
const SHELL_HOME_MIGRATION_VERSION: u16 = 4;
const SETTINGS_OWNERSHIP_MIGRATION_VERSION: u16 = 1;
const BRAND_MIGRATION_VERSION: u16 = 1;
const SCRIPT_AUTOMATION_MIGRATION_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkspaceProfileRuntimeErrorCode {
    ProfileRuntimeNotInitialized,
    ProfileRuntimeStateInvalid,
    ProfileDocumentInvalid,
    ProfileIoError,
    ProfilePersistenceConflict,
    ProfileLockPoisoned,
    ProfileSwitchBlocked,
    ProfileSwitchStale,
    ProfileSwitchFailed,
    ProfileRollbackFailed,
    ProfileSwitchTransactionInvalid,
    ProfileRecoveryFailed,
    ProfileEditBlocked,
    ProfileRevisionConflict,
    ProfilePackageInvalid,
    ProfilePackageUnsafe,
    ProfilePackageUnsupported,
    ProfilePackageTooLarge,
    ProfilePackageDigestMismatch,
    ProfilePackageNameConflict,
    ProfilePackageMappingRequired,
    ProfileDeleteBlocked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileRuntimeError {
    pub code: WorkspaceProfileRuntimeErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

impl WorkspaceProfileRuntimeError {
    pub(crate) fn new(
        code: WorkspaceProfileRuntimeErrorCode,
        message: impl Into<String>,
        path: Option<&Path>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            path: path.map(|value| value.to_string_lossy().into_owned()),
            details: Vec::new(),
        }
    }

    pub fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

impl fmt::Display for WorkspaceProfileRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for WorkspaceProfileRuntimeError {}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeWorkspaceProfileRuntimeRequest {
    #[serde(default)]
    pub legacy_pinned_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileMigrationRecord {
    pub source: String,
    pub source_version: String,
    pub created_at: i64,
    pub captured_module_count: usize,
    pub captured_pinned_tool_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceProfileRuntimeState {
    schema_version: u16,
    current_profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_profile_id: Option<String>,
    created_at: i64,
    updated_at: i64,
    migration: WorkspaceProfileMigrationRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_recovery: Option<WorkspaceProfileRecoveryRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceProfileSwitchPhase {
    Prepared,
    ModulesApplied,
    ProfileCommitted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfilePendingSwitch {
    pub schema_version: u16,
    pub transaction_id: String,
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub to_profile_revision: u64,
    pub phase: WorkspaceProfileSwitchPhase,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceProfileRecoveryOutcome {
    RolledBackInterruptedSwitch,
    CompletedInterruptedSwitch,
    ReconciledRuntimeDrift,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileRecoveryRecord {
    pub outcome: WorkspaceProfileRecoveryOutcome,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    pub recovered_at: i64,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceProfileStartupRecoveryAction {
    None,
    RollBackInterruptedSwitch,
    CompleteInterruptedSwitch,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceProfileStartupRecoveryPlan {
    pub profile: WorkspaceProfileV1,
    pub pending_switch: Option<WorkspaceProfilePendingSwitch>,
    pub action: WorkspaceProfileStartupRecoveryAction,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceProfileDocumentStatus {
    Ready,
    Blocked,
    Invalid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub revision: u64,
    pub current: bool,
    pub enabled_module_count: usize,
    pub enabled_component_count: usize,
    pub effective_component_count: usize,
    pub pinned_tool_count: usize,
    pub status: WorkspaceProfileDocumentStatus,
    pub issues: Vec<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileComponentSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub runtime: ComponentRuntime,
    pub role: ComponentRole,
    pub distribution: ComponentDistribution,
    pub ui_mode: ComponentUiMode,
    pub explicit_enabled: bool,
    pub effective_enabled: bool,
    pub required_by_modules: Vec<String>,
    pub required_by_components: Vec<String>,
    pub settings_sections: Vec<ComponentSettingsSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<pmc_platform::ComponentCategory>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileRuntimeSnapshot {
    pub current_profile: WorkspaceProfileV1,
    pub profiles: Vec<WorkspaceProfileSummary>,
    pub components: Vec<WorkspaceProfileComponentSummary>,
    pub default_profile_id: String,
    pub repository_path: String,
    pub state_path: String,
    pub journal_path: String,
    pub migration: WorkspaceProfileMigrationRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_switch: Option<WorkspaceProfilePendingSwitch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_recovery: Option<WorkspaceProfileRecoveryRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewWorkspaceProfileSwitchRequest {
    pub profile_id: String,
    #[serde(default)]
    pub current_pinned_tools: Vec<String>,
    #[serde(default)]
    pub known_tool_contributions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchWorkspaceProfileRequest {
    pub profile_id: String,
    pub expected_current_profile_id: String,
    #[serde(default)]
    pub current_pinned_tools: Vec<String>,
    #[serde(default)]
    pub known_tool_contributions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceProfileSwitchIssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileSwitchIssue {
    pub code: String,
    pub severity: WorkspaceProfileSwitchIssueSeverity,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileModuleChange {
    pub module_id: String,
    pub name: String,
    pub version: String,
    pub current_state: super::ModuleState,
    pub current_desired_enabled: bool,
    pub target_enabled: bool,
    pub resource_count: usize,
    pub resource_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileSwitchPreview {
    pub current_profile_id: String,
    pub target_profile_id: String,
    pub target_profile_name: String,
    pub target_profile_revision: u64,
    pub can_switch: bool,
    pub no_changes: bool,
    pub requires_confirmation: bool,
    pub modules_to_enable: Vec<WorkspaceProfileModuleChange>,
    pub modules_to_disable: Vec<WorkspaceProfileModuleChange>,
    pub pinned_tools_before: Vec<String>,
    pub pinned_tools_after: Vec<String>,
    pub pinned_tools_added: Vec<String>,
    pub pinned_tools_removed: Vec<String>,
    pub contributions_to_close: Vec<String>,
    pub resources_to_release: usize,
    pub issues: Vec<WorkspaceProfileSwitchIssue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileSwitchResult {
    pub transaction_id: String,
    pub preview: WorkspaceProfileSwitchPreview,
    pub snapshot: WorkspaceProfileRuntimeSnapshot,
    pub switched_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceProfileRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_profile_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWorkspaceProfileRequest {
    pub profile: WorkspaceProfileV1,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileDraftValidation {
    pub valid: bool,
    pub selected_module_count: usize,
    pub explicit_component_count: usize,
    pub effective_component_count: usize,
    pub components: Vec<WorkspaceProfileComponentSummary>,
    pub issues: Vec<WorkspaceProfileSwitchIssue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileMutationResult {
    pub profile: WorkspaceProfileV1,
    pub validation: WorkspaceProfileDraftValidation,
    pub snapshot: WorkspaceProfileRuntimeSnapshot,
}

pub(crate) struct PreparedCurrentProfileUpdate {
    pub profile: WorkspaceProfileV1,
    pub validation: WorkspaceProfileDraftValidation,
    pub expected_revision: u64,
}

pub(crate) struct ComponentProfileCleanupRollback {
    backups: Vec<(PathBuf, Vec<u8>)>,
}

pub struct WorkspaceProfileRuntime {
    repository_path: PathBuf,
    state_path: PathBuf,
    journal_path: PathBuf,
    component_manifests: Mutex<Vec<ComponentManifestV1>>,
    operation_lock: Mutex<()>,
}

impl WorkspaceProfileRuntime {
    pub fn new(app_data_dir: &Path) -> Self {
        Self::new_with_components(app_data_dir, Vec::new())
    }

    pub fn new_with_components(
        app_data_dir: &Path,
        component_manifests: Vec<ComponentManifestV1>,
    ) -> Self {
        Self {
            repository_path: app_data_dir.join("profiles"),
            state_path: app_data_dir.join("profile-runtime.json"),
            journal_path: app_data_dir.join("profile-switch-journal.json"),
            component_manifests: Mutex::new(component_manifests),
            operation_lock: Mutex::new(()),
        }
    }

    fn validate_profile(
        &self,
        profile: &WorkspaceProfileV1,
        manifests: &[ModuleManifestV1],
    ) -> ContractResult<BTreeSet<String>> {
        validate_profile_with_catalogs(profile, manifests, &self.component_manifests_snapshot())
    }

    pub fn initialize_from_current_configuration(
        &self,
        manifests: &[ModuleManifestV1],
        enabled_module_ids: &BTreeSet<String>,
        legacy_pinned_tools: &[String],
    ) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        fs::create_dir_all(&self.repository_path)
            .map_err(|error| io_error("创建装配方案目录失败", &self.repository_path, error))?;
        self.ensure_blank_profile(manifests)?;
        self.ensure_default_profile(manifests)?;
        self.ensure_script_automation_default_migration(manifests)?;
        self.ensure_brand_migration(manifests)?;
        self.ensure_settings_ownership_migration(manifests)?;
        self.ensure_migrated_profile_home(manifests)?;

        if self.state_path.exists() {
            return self.snapshot_locked(manifests);
        }

        let profile_path = self.profile_path(MIGRATED_PROFILE_ID);
        let current_profile = if profile_path.exists() {
            self.read_profile(&profile_path)?
        } else {
            let profile =
                build_migrated_profile(manifests, enabled_module_ids, legacy_pinned_tools);
            self.validate_profile(&profile, manifests)
                .map_err(|error| {
                    WorkspaceProfileRuntimeError::new(
                        WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                        format!(
                            "生成的迁移装配方案无效：{}（{}）",
                            error.message, error.path
                        ),
                        Some(&profile_path),
                    )
                })?;
            write_new_json(&profile_path, &profile)?;
            profile
        };
        if current_profile.id != MIGRATED_PROFILE_ID {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfilePersistenceConflict,
                format!(
                    "迁移文件 ID 与预期不一致：{} != {MIGRATED_PROFILE_ID}",
                    current_profile.id
                ),
                Some(&profile_path),
            ));
        }
        self.validate_profile(&current_profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                    format!(
                        "迁移装配方案与当前模块不兼容：{}（{}）",
                        error.message, error.path
                    ),
                    Some(&profile_path),
                )
            })?;

        let migration = migration_record_from_profile(&current_profile);
        let now = Utc::now().timestamp_millis();
        let state = WorkspaceProfileRuntimeState {
            schema_version: PROFILE_RUNTIME_SCHEMA_VERSION,
            current_profile_id: current_profile.id.clone(),
            previous_profile_id: None,
            created_at: now,
            updated_at: now,
            migration,
            last_recovery: None,
        };
        write_new_json(&self.state_path, &state)?;
        self.snapshot_locked(manifests)
    }

    pub fn snapshot(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.snapshot_locked(manifests)
    }

    pub fn profile(
        &self,
        profile_id: &str,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileV1, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        if !is_valid_stable_id(profile_id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("装配方案 ID 无效：{profile_id}"),
                None,
            ));
        }
        let path = self.profile_path(profile_id);
        let profile = self.read_profile(&path)?;
        if profile.id != profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                "装配方案文件名与文档 ID 不一致",
                Some(&path),
            ));
        }
        self.validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileSwitchBlocked,
                    format!("目标装配方案不兼容：{}（{}）", error.message, error.path),
                    Some(&path),
                )
            })?;
        Ok(profile)
    }

    pub fn profile_document(
        &self,
        profile_id: &str,
    ) -> Result<WorkspaceProfileV1, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.read_profile_document_locked(profile_id)
    }

    pub fn component_manifest(&self, component_id: &str) -> Option<ComponentManifestV1> {
        self.component_manifests_snapshot()
            .into_iter()
            .find(|manifest| manifest.id == component_id)
    }

    pub fn component_manifests(&self) -> Vec<ComponentManifestV1> {
        self.component_manifests_snapshot()
    }

    pub fn current_effective_component_ids(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<(String, BTreeSet<String>), WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        let state = self.read_state()?;
        let profile = self.read_profile(&self.profile_path(&state.current_profile_id))?;
        let effective = self
            .validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileSwitchBlocked,
                    format!("当前装配方案不兼容：{}（{}）", error.message, error.path),
                    Some(&self.profile_path(&state.current_profile_id)),
                )
            })?;
        Ok((state.current_profile_id, effective))
    }

    pub fn replace_component_manifests(
        &self,
        manifests: Vec<ComponentManifestV1>,
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        *self.component_manifests.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "组件目录锁已损坏",
                None,
            )
        })? = manifests;
        Ok(())
    }

    pub(crate) fn remove_component_references(
        &self,
        manifest: &ComponentManifestV1,
        module_manifests: &[ModuleManifestV1],
    ) -> Result<ComponentProfileCleanupRollback, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.ensure_edit_repository_ready_locked()?;
        let component_manifests = self
            .component_manifests_snapshot()
            .into_iter()
            .filter(|candidate| candidate.id != manifest.id)
            .collect::<Vec<_>>();
        let mut changes = Vec::<(PathBuf, Vec<u8>, WorkspaceProfileV1)>::new();
        let entries = fs::read_dir(&self.repository_path)
            .map_err(|error| io_error("读取装配方案目录失败", &self.repository_path, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                io_error("读取装配方案目录项失败", &self.repository_path, error)
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let original =
                fs::read(&path).map_err(|error| io_error("读取装配方案失败", &path, error))?;
            let mut profile = self.read_profile(&path)?;
            let changed = remove_component_owned_profile_references(&mut profile, manifest);
            if changed {
                profile.revision = profile.revision.saturating_add(1);
            }
            validate_profile_with_catalogs(&profile, module_manifests, &component_manifests)
                .map_err(|error| {
                    WorkspaceProfileRuntimeError::new(
                        WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                        format!(
                            "删除组件后装配方案无法保持有效：{}（{}）",
                            error.message, error.path
                        ),
                        Some(&path),
                    )
                })?;
            if changed {
                changes.push((path, original, profile));
            }
        }

        let mut backups = Vec::new();
        for (path, original, profile) in changes {
            if let Err(mut error) = replace_json(&path, &profile) {
                if let Err(rollback_error) = restore_profile_files(&backups) {
                    error.details.push(format!(
                        "恢复已修改装配方案失败：{}",
                        rollback_error.message
                    ));
                    error.details.extend(rollback_error.details);
                }
                return Err(error);
            }
            backups.push((path, original));
        }
        Ok(ComponentProfileCleanupRollback { backups })
    }

    pub(crate) fn rollback_component_reference_removal(
        &self,
        rollback: ComponentProfileCleanupRollback,
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        restore_profile_files(&rollback.backups)
    }

    fn component_manifests_snapshot(&self) -> Vec<ComponentManifestV1> {
        self.component_manifests
            .lock()
            .expect("workspace profile component catalog mutex poisoned")
            .clone()
    }

    pub fn validate_draft(
        &self,
        profile: &WorkspaceProfileV1,
        manifests: &[ModuleManifestV1],
    ) -> WorkspaceProfileDraftValidation {
        build_draft_validation(profile, manifests, &self.component_manifests_snapshot())
    }

    /// Used by portable workspace import before embedded component packages
    /// are committed. This keeps the preview truthful without mutating the
    /// live component catalog merely to validate a package.
    pub fn validate_draft_with_component_catalog(
        &self,
        profile: &WorkspaceProfileV1,
        manifests: &[ModuleManifestV1],
        component_manifests: &[ComponentManifestV1],
    ) -> WorkspaceProfileDraftValidation {
        build_draft_validation(profile, manifests, component_manifests)
    }

    pub fn create_profile(
        &self,
        request: &CreateWorkspaceProfileRequest,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.ensure_edit_repository_ready_locked()?;
        let name = validate_profile_text(&request.name, "装配方案名称", 80)?;
        let description =
            validate_profile_text_optional(&request.description, "装配方案说明", 500)?;
        let mut profile = if let Some(source_profile_id) = request.source_profile_id.as_deref() {
            self.read_profile_document_locked(source_profile_id)?
        } else {
            build_blank_profile()
        };
        let profile_id = format!("local.profile-{}", Uuid::new_v4().simple());
        profile.id = profile_id.clone();
        profile.name = name;
        profile.description = description;
        profile.revision = 1;
        profile.extensions.remove("migration");
        profile.extensions.remove("template");
        profile.extensions.insert(
            "editorMetadata".into(),
            json!({
                "createdAt": Utc::now().timestamp_millis(),
                "sourceProfileId": request.source_profile_id,
            }),
        );
        let validation =
            build_draft_validation(&profile, manifests, &self.component_manifests_snapshot());
        if !validation.valid {
            return Err(draft_validation_error(
                "新建装配方案未通过依赖预检",
                &validation,
                None,
            ));
        }
        let path = self.profile_path(&profile_id);
        write_new_json(&path, &profile)?;
        let snapshot = self.snapshot_locked(manifests)?;
        Ok(WorkspaceProfileMutationResult {
            profile,
            validation,
            snapshot,
        })
    }

    pub fn save_profile(
        &self,
        request: &SaveWorkspaceProfileRequest,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.ensure_edit_repository_ready_locked()?;
        if !is_valid_stable_id(&request.profile.id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("装配方案 ID 无效：{}", request.profile.id),
                None,
            ));
        }
        let state = self.read_state()?;
        if state.current_profile_id == request.profile.id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileEditBlocked,
                "当前正在运行的装配方案不能直接修改，请先复制后编辑",
                Some(&self.profile_path(&request.profile.id)),
            ));
        }
        let path = self.profile_path(&request.profile.id);
        let existing = self.read_profile_document_locked(&request.profile.id)?;
        if existing.revision != request.expected_revision
            || request.profile.revision != request.expected_revision
        {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRevisionConflict,
                format!(
                    "装配方案已被其他窗口修改：预期修订 {}，当前修订 {}",
                    request.expected_revision, existing.revision
                ),
                Some(&path),
            ));
        }
        let mut profile = request.profile.clone();
        profile.name = validate_profile_text(&profile.name, "装配方案名称", 80)?;
        profile.description =
            validate_profile_text_optional(&profile.description, "装配方案说明", 500)?;
        profile.schema_version = PLATFORM_SCHEMA_VERSION;
        profile.revision = existing.revision.saturating_add(1);
        let validation =
            build_draft_validation(&profile, manifests, &self.component_manifests_snapshot());
        if !validation.valid {
            return Err(draft_validation_error(
                "装配方案草稿未通过依赖预检",
                &validation,
                Some(&path),
            ));
        }
        replace_json(&path, &profile)?;
        let snapshot = self.snapshot_locked(manifests)?;
        Ok(WorkspaceProfileMutationResult {
            profile,
            validation,
            snapshot,
        })
    }

    pub(crate) fn prepare_current_profile_update(
        &self,
        request: &SaveWorkspaceProfileRequest,
        manifests: &[ModuleManifestV1],
    ) -> Result<PreparedCurrentProfileUpdate, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.ensure_edit_repository_ready_locked()?;
        if !is_valid_stable_id(&request.profile.id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("装配方案 ID 无效：{}", request.profile.id),
                None,
            ));
        }
        let state = self.read_state()?;
        if state.current_profile_id != request.profile.id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileEditBlocked,
                "只能通过当前方案保存入口更新正在运行的装配方案",
                Some(&self.profile_path(&request.profile.id)),
            ));
        }
        let path = self.profile_path(&request.profile.id);
        let existing = self.read_profile_document_locked(&request.profile.id)?;
        if existing.revision != request.expected_revision
            || request.profile.revision != request.expected_revision
        {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRevisionConflict,
                format!(
                    "当前装配方案已被其他窗口修改：预期修订 {}，当前修订 {}",
                    request.expected_revision, existing.revision
                ),
                Some(&path),
            ));
        }

        let mut profile = request.profile.clone();
        profile.name = validate_profile_text(&profile.name, "装配方案名称", 80)?;
        profile.description =
            validate_profile_text_optional(&profile.description, "装配方案说明", 500)?;
        profile.schema_version = PLATFORM_SCHEMA_VERSION;
        profile.revision = existing.revision.saturating_add(1);
        let validation =
            build_draft_validation(&profile, manifests, &self.component_manifests_snapshot());
        if !validation.valid {
            return Err(draft_validation_error(
                "当前装配方案草稿未通过依赖预检",
                &validation,
                Some(&path),
            ));
        }

        Ok(PreparedCurrentProfileUpdate {
            profile,
            validation,
            expected_revision: existing.revision,
        })
    }

    pub(crate) fn commit_current_profile_update(
        &self,
        prepared: PreparedCurrentProfileUpdate,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        let state = self.read_state()?;
        if state.current_profile_id != prepared.profile.id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchStale,
                "保存期间当前装配方案已经切换，未写入草稿",
                Some(&self.state_path),
            ));
        }
        let path = self.profile_path(&prepared.profile.id);
        let existing = self.read_profile(&path)?;
        if existing.revision != prepared.expected_revision {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRevisionConflict,
                format!(
                    "保存期间装配方案已被修改：预期修订 {}，当前修订 {}",
                    prepared.expected_revision, existing.revision
                ),
                Some(&path),
            ));
        }
        replace_json(&path, &prepared.profile)?;
        let snapshot = self.snapshot_locked(manifests)?;
        Ok(WorkspaceProfileMutationResult {
            profile: prepared.profile,
            validation: prepared.validation,
            snapshot,
        })
    }

    pub fn import_profile_document(
        &self,
        source: &WorkspaceProfileV1,
        requested_name: &str,
        package_id: &str,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.ensure_edit_repository_ready_locked()?;
        let name = validate_profile_text(requested_name, "导入方案名称", 80)?;
        let existing = self.list_profiles_locked(manifests, "")?;
        if existing
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(&name))
        {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfilePackageNameConflict,
                format!("已经存在名为“{name}”的装配方案，请使用其他名称"),
                None,
            ));
        }

        let profile_id = format!("local.profile-{}", Uuid::new_v4().simple());
        let mut profile = source.clone();
        profile.schema_version = PLATFORM_SCHEMA_VERSION;
        profile.id = profile_id.clone();
        profile.name = name;
        profile.revision = 1;
        profile.extensions.remove("migration");
        profile.extensions.remove("template");
        profile.extensions.remove("editorMetadata");
        profile.extensions.insert(
            "editorMetadata".into(),
            json!({
                "createdAt": Utc::now().timestamp_millis(),
                "importedPackageId": package_id,
            }),
        );

        let validation =
            build_draft_validation(&profile, manifests, &self.component_manifests_snapshot());
        if !validation.valid {
            return Err(draft_validation_error(
                "导入装配方案未通过依赖预检",
                &validation,
                None,
            ));
        }
        let path = self.profile_path(&profile_id);
        write_new_json(&path, &profile)?;
        let snapshot = self.snapshot_locked(manifests)?;
        Ok(WorkspaceProfileMutationResult {
            profile,
            validation,
            snapshot,
        })
    }

    pub fn delete_profile(
        &self,
        profile_id: &str,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.ensure_edit_repository_ready_locked()?;
        let is_user_profile = profile_id.starts_with("local.profile-");
        let is_migration_profile = profile_id == MIGRATED_PROFILE_ID;
        if !is_valid_stable_id(profile_id) || (!is_user_profile && !is_migration_profile) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDeleteBlocked,
                "默认和空白恢复方案不能删除",
                None,
            ));
        }
        let state = self.read_state()?;
        if state.current_profile_id == profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDeleteBlocked,
                "当前正在运行的装配方案不能删除，请先切换到其他方案",
                Some(&self.profile_path(profile_id)),
            ));
        }
        let path = self.profile_path(profile_id);
        let profile = self.read_profile_document_locked(profile_id)?;
        if profile.id != profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfilePersistenceConflict,
                "装配方案文件名与文档 ID 不一致，拒绝删除",
                Some(&path),
            ));
        }
        let tombstone = self.repository_path.join(format!(
            ".{}.{}.delete",
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("profile.json"),
            Uuid::new_v4().simple()
        ));
        fs::rename(&path, &tombstone)
            .map_err(|error| io_error("暂存待删除装配方案失败", &path, error))?;
        let clears_previous_profile = state.previous_profile_id.as_deref() == Some(profile_id);
        if clears_previous_profile {
            let mut updated_state = state.clone();
            updated_state.previous_profile_id = None;
            updated_state.updated_at = Utc::now().timestamp_millis();
            if let Err(mut state_error) = replace_json(&self.state_path, &updated_state) {
                if let Err(rollback_error) = fs::rename(&tombstone, &path) {
                    state_error.details.push(format!(
                        "恢复方案文件失败：{} ({})",
                        rollback_error,
                        tombstone.to_string_lossy()
                    ));
                }
                return Err(state_error);
            }
        }
        if let Err(error) = fs::remove_file(&tombstone) {
            let mut mapped = io_error("删除装配方案失败", &path, error);
            let file_restored = match fs::rename(&tombstone, &path) {
                Ok(()) => true,
                Err(rollback_error) => {
                    mapped.details.push(format!(
                        "恢复方案文件失败：{} ({})",
                        rollback_error,
                        tombstone.to_string_lossy()
                    ));
                    false
                }
            };
            if clears_previous_profile && file_restored {
                if let Err(state_rollback_error) = replace_json(&self.state_path, &state) {
                    mapped.details.push(format!(
                        "恢复方案历史引用失败：{}",
                        state_rollback_error.message
                    ));
                }
            }
            return Err(mapped);
        }
        let local_bindings_path = self.local_bindings_path(profile_id);
        if local_bindings_path.exists() {
            let _ = fs::remove_file(local_bindings_path);
        }
        self.snapshot_locked(manifests)
    }

    pub(crate) fn set_current_module_enabled(
        &self,
        module_id: &str,
        version_requirement: &str,
        enabled: bool,
        removed_tool_contributions: &[&str],
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.ensure_edit_repository_ready_locked()?;
        let state = self.read_state()?;
        let path = self.profile_path(&state.current_profile_id);
        let mut profile = self.read_profile(&path)?;
        let already_enabled = profile
            .enabled_modules
            .iter()
            .any(|selection| selection.id == module_id);

        if enabled && !already_enabled {
            profile.enabled_modules.push(ProfileModuleSelection {
                id: module_id.into(),
                version_requirement: version_requirement.into(),
                extensions: ExtensionFields::new(),
            });
            profile
                .enabled_modules
                .sort_by(|left, right| left.id.cmp(&right.id));
        } else if !enabled && already_enabled {
            profile
                .enabled_modules
                .retain(|selection| selection.id != module_id);
        }

        let pinned_tool_count = profile.shell_layout.pinned_tools.len();
        if !enabled && !removed_tool_contributions.is_empty() {
            profile.shell_layout.pinned_tools.retain(|contribution_id| {
                !removed_tool_contributions.contains(&contribution_id.as_str())
            });
        }
        let pinned_tools_changed = profile.shell_layout.pinned_tools.len() != pinned_tool_count;

        if already_enabled == enabled && !pinned_tools_changed {
            return self.snapshot_locked(manifests);
        }

        profile.revision = profile.revision.saturating_add(1);
        self.validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                    format!(
                        "更新当前装配方案模块失败：{}（{}）",
                        error.message, error.path
                    ),
                    Some(&path),
                )
            })?;
        replace_json(&path, &profile)?;
        self.snapshot_locked(manifests)
    }

    pub(crate) fn begin_switch(
        &self,
        target_profile_id: &str,
        expected_current_profile_id: &str,
        target_profile_revision: u64,
    ) -> Result<WorkspaceProfilePendingSwitch, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        let state = self.read_state()?;
        if state.current_profile_id != expected_current_profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchStale,
                format!(
                    "当前装配方案已经变化：预期 {expected_current_profile_id}，实际 {}",
                    state.current_profile_id
                ),
                Some(&self.state_path),
            ));
        }
        if self.journal_path.exists() {
            let pending = self.read_pending_switch_locked()?;
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                "存在尚未完成的装配方案切换，请先完成启动恢复",
                Some(&self.journal_path),
            )
            .with_details(vec![format!(
                "transactionId={}, phase={:?}",
                pending.transaction_id, pending.phase
            )]));
        }
        if !is_valid_stable_id(target_profile_id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("目标装配方案 ID 无效：{target_profile_id}"),
                None,
            ));
        }
        let now = Utc::now().timestamp_millis();
        let pending = WorkspaceProfilePendingSwitch {
            schema_version: PROFILE_SWITCH_JOURNAL_SCHEMA_VERSION,
            transaction_id: Uuid::new_v4().to_string(),
            from_profile_id: expected_current_profile_id.to_string(),
            to_profile_id: target_profile_id.to_string(),
            to_profile_revision: target_profile_revision,
            phase: WorkspaceProfileSwitchPhase::Prepared,
            created_at: now,
            updated_at: now,
        };
        write_new_json(&self.journal_path, &pending)?;
        Ok(pending)
    }

    pub(crate) fn mark_switch_phase(
        &self,
        transaction_id: &str,
        phase: WorkspaceProfileSwitchPhase,
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        let mut pending = self.read_pending_switch_locked()?;
        if pending.transaction_id != transaction_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                "装配方案切换事务 ID 不匹配",
                Some(&self.journal_path),
            ));
        }
        if phase < pending.phase {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                "装配方案切换阶段不能回退",
                Some(&self.journal_path),
            ));
        }
        pending.phase = phase;
        pending.updated_at = Utc::now().timestamp_millis();
        replace_json(&self.journal_path, &pending)
    }

    pub(crate) fn pending_switch(
        &self,
    ) -> Result<Option<WorkspaceProfilePendingSwitch>, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        if !self.journal_path.exists() {
            return Ok(None);
        }
        self.read_pending_switch_locked().map(Some)
    }

    pub(crate) fn startup_recovery_plan(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileStartupRecoveryPlan, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        let state = self.read_state()?;
        let pending = if self.journal_path.exists() {
            Some(self.read_pending_switch_locked()?)
        } else {
            None
        };
        let action = match pending.as_ref() {
            None => WorkspaceProfileStartupRecoveryAction::None,
            Some(transaction) if state.current_profile_id == transaction.from_profile_id => {
                WorkspaceProfileStartupRecoveryAction::RollBackInterruptedSwitch
            }
            Some(transaction) if state.current_profile_id == transaction.to_profile_id => {
                WorkspaceProfileStartupRecoveryAction::CompleteInterruptedSwitch
            }
            Some(transaction) => {
                return Err(WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileRecoveryFailed,
                    "切换日志与最后提交的装配方案状态不一致，已停止自动恢复",
                    Some(&self.journal_path),
                )
                .with_details(vec![
                    format!("currentProfileId={}", state.current_profile_id),
                    format!("fromProfileId={}", transaction.from_profile_id),
                    format!("toProfileId={}", transaction.to_profile_id),
                ]));
            }
        };
        let profile_path = self.profile_path(&state.current_profile_id);
        let profile = self.read_profile(&profile_path)?;
        if profile.id != state.current_profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid,
                "恢复目标 Profile 文件名与文档 ID 不一致",
                Some(&profile_path),
            ));
        }
        self.validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileRecoveryFailed,
                    format!(
                        "恢复目标 Profile 不兼容：{}（{}）",
                        error.message, error.path
                    ),
                    Some(&profile_path),
                )
            })?;
        if let Some(transaction) = pending.as_ref() {
            if action == WorkspaceProfileStartupRecoveryAction::CompleteInterruptedSwitch
                && transaction.to_profile_revision != profile.revision
            {
                return Err(WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileRecoveryFailed,
                    "切换后目标 Profile 修订已经变化，已停止自动恢复",
                    Some(&profile_path),
                )
                .with_details(vec![
                    format!("journalRevision={}", transaction.to_profile_revision),
                    format!("profileRevision={}", profile.revision),
                ]));
            }
        }
        Ok(WorkspaceProfileStartupRecoveryPlan {
            profile,
            pending_switch: pending,
            action,
        })
    }

    pub(crate) fn complete_startup_recovery(
        &self,
        plan: &WorkspaceProfileStartupRecoveryPlan,
        runtime_drift_corrected: bool,
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        let recovery = match plan.action {
            WorkspaceProfileStartupRecoveryAction::RollBackInterruptedSwitch => {
                let transaction = plan.pending_switch.as_ref().ok_or_else(|| {
                    WorkspaceProfileRuntimeError::new(
                        WorkspaceProfileRuntimeErrorCode::ProfileRecoveryFailed,
                        "缺少待回滚的装配方案切换事务",
                        Some(&self.journal_path),
                    )
                })?;
                self.remove_journal_locked(&transaction.transaction_id)?;
                Some(WorkspaceProfileRecoveryRecord {
                    outcome: WorkspaceProfileRecoveryOutcome::RolledBackInterruptedSwitch,
                    profile_id: plan.profile.id.clone(),
                    transaction_id: Some(transaction.transaction_id.clone()),
                    recovered_at: Utc::now().timestamp_millis(),
                    message: "检测到提交前中断的 Profile 切换，已恢复最后提交的方案".into(),
                })
            }
            WorkspaceProfileStartupRecoveryAction::CompleteInterruptedSwitch => {
                let transaction = plan.pending_switch.as_ref().ok_or_else(|| {
                    WorkspaceProfileRuntimeError::new(
                        WorkspaceProfileRuntimeErrorCode::ProfileRecoveryFailed,
                        "缺少待完成的装配方案切换事务",
                        Some(&self.journal_path),
                    )
                })?;
                Some(WorkspaceProfileRecoveryRecord {
                    outcome: WorkspaceProfileRecoveryOutcome::CompletedInterruptedSwitch,
                    profile_id: plan.profile.id.clone(),
                    transaction_id: Some(transaction.transaction_id.clone()),
                    recovered_at: Utc::now().timestamp_millis(),
                    message: "检测到已提交但未完成前端落盘的 Profile 切换，已继续完成恢复".into(),
                })
            }
            WorkspaceProfileStartupRecoveryAction::None if runtime_drift_corrected => {
                Some(WorkspaceProfileRecoveryRecord {
                    outcome: WorkspaceProfileRecoveryOutcome::ReconciledRuntimeDrift,
                    profile_id: plan.profile.id.clone(),
                    transaction_id: None,
                    recovered_at: Utc::now().timestamp_millis(),
                    message: "模块期望状态与当前 Profile 不一致，已按当前方案收敛".into(),
                })
            }
            WorkspaceProfileStartupRecoveryAction::None => None,
        };
        if let Some(recovery) = recovery {
            let mut state = self.read_state()?;
            state.last_recovery = Some(recovery);
            state.updated_at = Utc::now().timestamp_millis();
            replace_json(&self.state_path, &state)?;
        }
        Ok(())
    }

    pub fn finalize_switch(
        &self,
        transaction_id: &str,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        let pending = self.read_pending_switch_locked()?;
        if pending.transaction_id != transaction_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                "装配方案切换事务 ID 不匹配",
                Some(&self.journal_path),
            ));
        }
        let state = self.read_state()?;
        if state.current_profile_id != pending.to_profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                "目标 Profile 尚未提交，不能完成切换事务",
                Some(&self.journal_path),
            ));
        }
        self.remove_journal_locked(transaction_id)?;
        self.snapshot_locked(manifests)
    }

    pub(crate) fn abort_switch(
        &self,
        transaction_id: &str,
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.remove_journal_locked(transaction_id)
    }

    pub fn preview_switch(
        &self,
        request: &PreviewWorkspaceProfileSwitchRequest,
        modules: &[super::ModuleDiagnosticSnapshot],
    ) -> Result<WorkspaceProfileSwitchPreview, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        self.preview_switch_locked(request, modules)
    }

    pub fn commit_switch(
        &self,
        target_profile_id: &str,
        expected_current_profile_id: &str,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
        let _guard = self.operation_lock.lock().map_err(|_| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileLockPoisoned,
                "装配方案运行时锁已损坏",
                None,
            )
        })?;
        if !is_valid_stable_id(target_profile_id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("目标装配方案 ID 无效：{target_profile_id}"),
                None,
            ));
        }
        let mut state = self.read_state()?;
        if state.current_profile_id != expected_current_profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchStale,
                format!(
                    "当前装配方案已经变化：预期 {expected_current_profile_id}，实际 {}",
                    state.current_profile_id
                ),
                Some(&self.state_path),
            ));
        }
        let target_path = self.profile_path(target_profile_id);
        let target = self.read_profile(&target_path)?;
        if target.id != target_profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                "目标装配方案文件名与文档 ID 不一致",
                Some(&target_path),
            ));
        }
        self.validate_profile(&target, manifests).map_err(|error| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchBlocked,
                format!("目标装配方案不兼容：{}（{}）", error.message, error.path),
                Some(&target_path),
            )
        })?;

        if state.current_profile_id != target_profile_id {
            state.previous_profile_id = Some(state.current_profile_id.clone());
            state.current_profile_id = target_profile_id.to_string();
            state.updated_at = Utc::now().timestamp_millis();
            replace_json(&self.state_path, &state)?;
        }
        self.snapshot_locked(manifests)
    }

    fn preview_switch_locked(
        &self,
        request: &PreviewWorkspaceProfileSwitchRequest,
        modules: &[super::ModuleDiagnosticSnapshot],
    ) -> Result<WorkspaceProfileSwitchPreview, WorkspaceProfileRuntimeError> {
        if !is_valid_stable_id(&request.profile_id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("目标装配方案 ID 无效：{}", request.profile_id),
                None,
            ));
        }
        let state = self.read_state()?;
        let target_path = self.profile_path(&request.profile_id);
        let target = self.read_profile(&target_path)?;
        if target.id != request.profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                "目标装配方案文件名与文档 ID 不一致",
                Some(&target_path),
            ));
        }

        let manifests = modules
            .iter()
            .filter(|module| !module.diagnostic)
            .map(|module| module.manifest.clone())
            .collect::<Vec<_>>();
        let target_module_ids = target
            .enabled_modules
            .iter()
            .map(|selection| selection.id.clone())
            .collect::<BTreeSet<_>>();
        let current_desired_ids = modules
            .iter()
            .filter(|module| !module.diagnostic && module.desired_enabled)
            .map(|module| module.manifest.id.clone())
            .collect::<BTreeSet<_>>();
        let mut issues =
            compatibility_issues(&target, &manifests, &self.component_manifests_snapshot());

        for module in modules.iter().filter(|module| !module.diagnostic) {
            if matches!(
                module.state,
                super::ModuleState::Resolving
                    | super::ModuleState::Starting
                    | super::ModuleState::Stopping
            ) {
                issues.push(switch_issue(
                    "MODULE_BUSY",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "module",
                    format!("模块 {} 正在切换状态，请稍后重试", module.manifest.name),
                    Some(module.manifest.id.clone()),
                    None,
                ));
            }
        }

        let current_profile = self.read_profile(&self.profile_path(&state.current_profile_id))?;
        let declared_current = current_profile
            .enabled_modules
            .iter()
            .map(|selection| selection.id.clone())
            .collect::<BTreeSet<_>>();
        if declared_current != current_desired_ids {
            issues.push(switch_issue(
                "RUNTIME_PROFILE_DRIFT",
                WorkspaceProfileSwitchIssueSeverity::Warning,
                "runtime",
                "当前模块期望状态与当前 Profile 存在偏差；切换时会以目标 Profile 为准统一校正",
                None,
                None,
            ));
        }

        let component_manifests = self.component_manifests_snapshot();
        let effective_component_ids =
            best_effort_effective_components(&target, &manifests, &component_manifests);
        let mut known_tools = request
            .known_tool_contributions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        known_tools.extend(component_manifests.iter().flat_map(|component| {
            component
                .contributes
                .script_surfaces
                .iter()
                .filter(|surface| surface.placements.contains(&ScriptSurfacePlacement::Shell))
                .map(|surface| surface.id.clone())
        }));
        let target_tools = manifests
            .iter()
            .filter(|manifest| target_module_ids.contains(&manifest.id))
            .flat_map(|manifest| manifest.contributes.tools.iter().cloned())
            .chain(
                known_tools
                    .iter()
                    .filter(|id| id.starts_with("core."))
                    .cloned(),
            )
            .chain(
                component_manifests
                    .iter()
                    .filter(|component| effective_component_ids.contains(&component.id))
                    .flat_map(|component| component.contributes.script_surfaces.iter())
                    .filter(|surface| surface.placements.contains(&ScriptSurfacePlacement::Shell))
                    .map(|surface| surface.id.clone()),
            )
            .collect::<BTreeSet<_>>();
        for contribution_id in &target.shell_layout.pinned_tools {
            if !known_tools.contains(contribution_id) {
                issues.push(switch_issue(
                    "PIN_IMPLEMENTATION_MISSING",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "tool",
                    format!("固定工具没有可用实现：{contribution_id}"),
                    None,
                    Some(contribution_id.clone()),
                ));
            } else if !target_tools.contains(contribution_id) {
                issues.push(switch_issue(
                    "PIN_PROVIDER_DISABLED",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "tool",
                    format!("固定工具所属模块未在目标方案启用：{contribution_id}"),
                    None,
                    Some(contribution_id.clone()),
                ));
            }
        }

        let modules_to_enable = module_changes(modules, &target_module_ids, true);
        let modules_to_disable = module_changes(modules, &target_module_ids, false);
        let resources_to_release = modules_to_disable
            .iter()
            .map(|change| change.resource_count)
            .sum();
        if resources_to_release > 0 {
            issues.push(switch_issue(
                "ACTIVE_RESOURCES_WILL_STOP",
                WorkspaceProfileSwitchIssueSeverity::Warning,
                "resource",
                format!("切换将停止并释放 {resources_to_release} 个已登记后台资源"),
                None,
                None,
            ));
        }

        let contributions_to_close = modules
            .iter()
            .filter(|module| {
                !module.diagnostic
                    && module.desired_enabled
                    && !target_module_ids.contains(&module.manifest.id)
            })
            .flat_map(|module| {
                module
                    .manifest
                    .contributes
                    .shell_tabs
                    .iter()
                    .chain(module.manifest.contributes.workspace_tabs.iter())
                    .chain(module.manifest.contributes.surfaces.iter())
                    .cloned()
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let pinned_before = dedupe(&request.current_pinned_tools);
        let pinned_after = dedupe(&target.shell_layout.pinned_tools);
        let before_set = pinned_before.iter().cloned().collect::<BTreeSet<_>>();
        let after_set = pinned_after.iter().cloned().collect::<BTreeSet<_>>();
        let pinned_tools_added = pinned_after
            .iter()
            .filter(|id| !before_set.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        let pinned_tools_removed = pinned_before
            .iter()
            .filter(|id| !after_set.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        let no_changes = modules_to_enable.is_empty()
            && modules_to_disable.is_empty()
            && pinned_before == pinned_after
            && state.current_profile_id == target.id;
        if no_changes {
            issues.push(switch_issue(
                "PROFILE_ALREADY_ACTIVE",
                WorkspaceProfileSwitchIssueSeverity::Info,
                "profile",
                "当前运行状态和快捷栏已经与目标 Profile 一致",
                None,
                None,
            ));
        }
        let can_switch = !issues
            .iter()
            .any(|issue| issue.severity == WorkspaceProfileSwitchIssueSeverity::Error);

        Ok(WorkspaceProfileSwitchPreview {
            current_profile_id: state.current_profile_id,
            target_profile_id: target.id,
            target_profile_name: target.name,
            target_profile_revision: target.revision,
            can_switch,
            no_changes,
            requires_confirmation: !no_changes,
            modules_to_enable,
            modules_to_disable,
            pinned_tools_before: pinned_before,
            pinned_tools_after: pinned_after,
            pinned_tools_added,
            pinned_tools_removed,
            contributions_to_close,
            resources_to_release,
            issues,
        })
    }

    fn ensure_blank_profile(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let path = self.profile_path(BLANK_PROFILE_ID);
        if path.exists() {
            return Ok(());
        }
        let profile = build_blank_profile();
        self.validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                    format!("空白装配方案无效：{}（{}）", error.message, error.path),
                    Some(&path),
                )
            })?;
        write_new_json(&path, &profile)
    }

    fn ensure_default_profile(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let path = self.profile_path(DEFAULT_PROFILE_ID);
        if path.exists() {
            return Ok(());
        }
        let profile = build_default_profile(manifests);
        self.validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                    format!("默认装配方案无效：{}（{}）", error.message, error.path),
                    Some(&path),
                )
            })?;
        write_new_json(&path, &profile)
    }

    fn ensure_script_automation_default_migration(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        if self.journal_path.exists() {
            return Ok(());
        }
        let manifests_by_id = manifests
            .iter()
            .map(|manifest| (manifest.id.as_str(), manifest))
            .collect::<BTreeMap<_, _>>();
        if !manifests_by_id.contains_key(SCRIPT_AUTOMATION_MODULE_ID) {
            return Ok(());
        }

        let path = self.profile_path(DEFAULT_PROFILE_ID);
        if !path.exists() {
            return Ok(());
        }
        let mut profile = self.read_profile(&path)?;
        if profile.id != DEFAULT_PROFILE_ID {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfilePersistenceConflict,
                format!(
                    "默认装配方案文件 ID 与预期不一致：{} != {DEFAULT_PROFILE_ID}",
                    profile.id
                ),
                Some(&path),
            ));
        }
        let migrated = profile
            .extensions
            .get("scriptAutomationMigration")
            .and_then(|value| value.get("version"))
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|version| version >= u64::from(SCRIPT_AUTOMATION_MIGRATION_VERSION));
        if migrated {
            return Ok(());
        }

        let mut required_ids = BTreeSet::from([SCRIPT_AUTOMATION_MODULE_ID.to_string()]);
        loop {
            let dependencies = required_ids
                .iter()
                .filter_map(|id| manifests_by_id.get(id.as_str()))
                .flat_map(|manifest| {
                    manifest
                        .requires_modules
                        .iter()
                        .map(|dependency| dependency.id.clone())
                })
                .collect::<Vec<_>>();
            let previous_len = required_ids.len();
            required_ids.extend(dependencies);
            if required_ids.len() == previous_len {
                break;
            }
        }

        let selected_ids = profile
            .enabled_modules
            .iter()
            .map(|selection| selection.id.as_str())
            .collect::<BTreeSet<_>>();
        let additions = required_ids
            .iter()
            .filter(|id| !selected_ids.contains(id.as_str()))
            .filter_map(|id| manifests_by_id.get(id.as_str()).copied())
            .map(|manifest| ProfileModuleSelection {
                id: manifest.id.clone(),
                version_requirement: format!("^{}", manifest.version),
                extensions: ExtensionFields::new(),
            })
            .collect::<Vec<_>>();
        profile.enabled_modules.extend(additions);
        profile
            .enabled_modules
            .sort_by(|left, right| left.id.cmp(&right.id));
        profile
            .enabled_modules
            .dedup_by(|left, right| left.id == right.id);
        profile.extensions.insert(
            "scriptAutomationMigration".into(),
            json!({
                "version": SCRIPT_AUTOMATION_MIGRATION_VERSION,
                "migratedAt": Utc::now().timestamp_millis(),
            }),
        );
        profile.revision = profile.revision.saturating_add(1);
        self.validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                    format!(
                        "默认装配方案脚本自动化迁移失败：{}（{}）",
                        error.message, error.path
                    ),
                    Some(&path),
                )
            })?;
        replace_json(&path, &profile)
    }

    fn ensure_migrated_profile_home(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let path = self.profile_path(MIGRATED_PROFILE_ID);
        if !path.exists() || self.journal_path.exists() {
            return Ok(());
        }
        let mut profile = self.read_profile(&path)?;
        if profile.id != MIGRATED_PROFILE_ID {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfilePersistenceConflict,
                format!(
                    "迁移文件 ID 与预期不一致：{} != {MIGRATED_PROFILE_ID}",
                    profile.id
                ),
                Some(&path),
            ));
        }

        let previous_migration_version = profile
            .extensions
            .get("shellHomeMigration")
            .and_then(|value| value.get("version"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default();
        if previous_migration_version >= u64::from(SHELL_HOME_MIGRATION_VERSION) {
            return Ok(());
        }

        // Version 3 installed the legacy project-home composition. Version 4 only
        // records that the editable shell layout is understood, so user ordering
        // and explicitly removed widgets are never restored during startup.
        if previous_migration_version < u64::from(LEGACY_HOME_COMPOSITION_MIGRATION_VERSION) {
            let has_project_home = profile.surfaces.iter().any(|surface| {
                surface.id == PROJECT_HOME_PROFILE_SURFACE_ID
                    && surface.contribution.as_deref() == Some(PROJECT_HOME_CONTRIBUTION_ID)
            });
            let project_manager_manifest = manifests
                .iter()
                .find(|manifest| manifest.id == PROJECT_MANAGER_MODULE_ID);
            let should_enable_project_manager = project_manager_manifest
                .filter(|manager| {
                    manager.requires_modules.iter().all(|dependency| {
                        profile
                            .enabled_modules
                            .iter()
                            .any(|selection| selection.id == dependency.id)
                    })
                })
                .is_some();
            let has_project_manager = profile
                .enabled_modules
                .iter()
                .any(|selection| selection.id == PROJECT_MANAGER_MODULE_ID);

            if !has_project_home {
                if profile
                    .surfaces
                    .iter()
                    .any(|surface| surface.id == PROJECT_HOME_PROFILE_SURFACE_ID)
                {
                    return Ok(());
                }
                profile.surfaces.push(project_home_surface());
            }
            if !has_project_home_composition(&profile) {
                apply_project_home_composition(&mut profile);
            }
            profile.shell_layout.home = Some(PROJECT_HOME_PROFILE_SURFACE_ID.into());

            if should_enable_project_manager && !has_project_manager {
                let manager = project_manager_manifest.expect("project manager manifest checked");
                profile.enabled_modules.push(ProfileModuleSelection {
                    id: manager.id.clone(),
                    version_requirement: format!("^{}", manager.version),
                    extensions: ExtensionFields::new(),
                });
                profile
                    .enabled_modules
                    .sort_by(|left, right| left.id.cmp(&right.id));
            }
        }
        profile.revision = profile.revision.saturating_add(1);
        profile.extensions.insert(
            "shellHomeMigration".into(),
            json!({
                "version": SHELL_HOME_MIGRATION_VERSION,
                "updatedAt": Utc::now().timestamp_millis(),
            }),
        );
        self.validate_profile(&profile, manifests)
            .map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                    format!(
                        "迁移装配方案主页升级失败：{}（{}）",
                        error.message, error.path
                    ),
                    Some(&path),
                )
            })?;
        replace_json(&path, &profile)
    }

    fn ensure_brand_migration(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        if self.journal_path.exists() {
            return Ok(());
        }
        for profile_id in [MIGRATED_PROFILE_ID, BLANK_PROFILE_ID, DEFAULT_PROFILE_ID] {
            let path = self.profile_path(profile_id);
            if !path.exists() {
                continue;
            }
            let mut profile = self.read_profile(&path)?;
            if profile.id != profile_id {
                return Err(WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfilePersistenceConflict,
                    format!(
                        "系统装配方案文件 ID 与预期不一致：{} != {profile_id}",
                        profile.id
                    ),
                    Some(&path),
                ));
            }

            let mut changed = false;
            if profile.name == "当前 PM Center 装配方案" {
                profile.name = "当前 Nexora 装配方案".into();
                changed = true;
            } else if profile.name == "PM Center 默认装配" {
                profile.name = "Nexora 默认装配".into();
                changed = true;
            }
            if profile.description.contains("PM Center") {
                profile.description = profile.description.replace("PM Center", "Nexora");
                changed = true;
            }
            if !changed {
                continue;
            }

            profile.revision = profile.revision.saturating_add(1);
            profile.extensions.insert(
                "brandMigration".into(),
                json!({
                    "version": BRAND_MIGRATION_VERSION,
                    "brand": "Nexora",
                    "migratedAt": Utc::now().timestamp_millis(),
                }),
            );
            self.validate_profile(&profile, manifests)
                .map_err(|error| {
                    WorkspaceProfileRuntimeError::new(
                        WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                        format!(
                            "系统装配方案品牌迁移失败：{}（{}）",
                            error.message, error.path
                        ),
                        Some(&path),
                    )
                })?;
            replace_json(&path, &profile)?;
        }
        Ok(())
    }

    fn ensure_settings_ownership_migration(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        if self.journal_path.exists() {
            return Ok(());
        }
        let target_ids = [
            "builtin.settings-center",
            "builtin.session-runtime",
            "builtin.desktop-integration",
            "builtin.external-tools",
        ];
        let manifests_by_id = manifests
            .iter()
            .map(|manifest| (manifest.id.as_str(), manifest))
            .collect::<BTreeMap<_, _>>();
        if !target_ids.iter().all(|id| manifests_by_id.contains_key(id)) {
            return Ok(());
        }
        let entries = fs::read_dir(&self.repository_path)
            .map_err(|error| io_error("读取装配方案目录失败", &self.repository_path, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                io_error("读取装配方案目录项失败", &self.repository_path, error)
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(mut profile) = self.read_profile(&path) else {
                continue;
            };
            if profile.id == BLANK_PROFILE_ID || profile.id == DEFAULT_PROFILE_ID {
                continue;
            }
            let migrated = profile
                .extensions
                .get("settingsOwnershipMigration")
                .and_then(|value| value.get("version"))
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|version| version >= u64::from(SETTINGS_OWNERSHIP_MIGRATION_VERSION));
            if migrated {
                continue;
            }
            let selected = profile
                .enabled_modules
                .iter()
                .map(|selection| selection.id.as_str())
                .collect::<BTreeSet<_>>();
            let additions = target_ids
                .iter()
                .filter(|id| !selected.contains(**id))
                .filter_map(|id| manifests_by_id.get(id).copied())
                .map(|manifest| ProfileModuleSelection {
                    id: manifest.id.clone(),
                    version_requirement: format!("^{}", manifest.version),
                    extensions: ExtensionFields::new(),
                })
                .collect::<Vec<_>>();
            profile.enabled_modules.extend(additions);
            profile
                .enabled_modules
                .sort_by(|left, right| left.id.cmp(&right.id));
            profile
                .enabled_modules
                .dedup_by(|left, right| left.id == right.id);
            profile.extensions.insert(
                "settingsOwnershipMigration".into(),
                json!({
                    "version": SETTINGS_OWNERSHIP_MIGRATION_VERSION,
                    "migratedAt": Utc::now().timestamp_millis(),
                }),
            );
            profile.revision = profile.revision.saturating_add(1);
            self.validate_profile(&profile, manifests)
                .map_err(|error| {
                    WorkspaceProfileRuntimeError::new(
                        WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                        format!("设置所有权迁移失败：{}（{}）", error.message, error.path),
                        Some(&path),
                    )
                })?;
            replace_json(&path, &profile)?;
        }
        Ok(())
    }

    fn snapshot_locked(
        &self,
        manifests: &[ModuleManifestV1],
    ) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
        if !self.state_path.exists() {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRuntimeNotInitialized,
                "装配方案运行时尚未初始化",
                Some(&self.state_path),
            ));
        }
        let state = self.read_state()?;
        let current_path = self.profile_path(&state.current_profile_id);
        let current_profile = self.read_profile(&current_path)?;
        if current_profile.id != state.current_profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid,
                format!(
                    "当前装配方案 ID 与运行时状态不一致：{} != {}",
                    current_profile.id, state.current_profile_id
                ),
                Some(&current_path),
            ));
        }
        let mut profiles = self.list_profiles_locked(manifests, &state.current_profile_id)?;
        profiles.sort_by(|left, right| {
            right
                .current
                .cmp(&left.current)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        let effective_components = self
            .validate_profile(&current_profile, manifests)
            .unwrap_or_default();
        let components = component_summaries(
            &current_profile,
            manifests,
            &self.component_manifests_snapshot(),
            &effective_components,
        );
        Ok(WorkspaceProfileRuntimeSnapshot {
            current_profile,
            profiles,
            components,
            default_profile_id: DEFAULT_PROFILE_ID.into(),
            repository_path: self.repository_path.to_string_lossy().into_owned(),
            state_path: self.state_path.to_string_lossy().into_owned(),
            journal_path: self.journal_path.to_string_lossy().into_owned(),
            migration: state.migration,
            pending_switch: if self.journal_path.exists() {
                Some(self.read_pending_switch_locked()?)
            } else {
                None
            },
            last_recovery: state.last_recovery,
        })
    }

    fn read_state(&self) -> Result<WorkspaceProfileRuntimeState, WorkspaceProfileRuntimeError> {
        let bytes = fs::read(&self.state_path)
            .map_err(|error| io_error("读取装配方案运行时状态失败", &self.state_path, error))?;
        let state: WorkspaceProfileRuntimeState =
            serde_json::from_slice(&bytes).map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid,
                    format!("装配方案运行时状态无法解析：{error}"),
                    Some(&self.state_path),
                )
            })?;
        if state.schema_version != PROFILE_RUNTIME_SCHEMA_VERSION {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid,
                format!(
                    "不支持装配方案运行时状态版本 {}，当前仅支持 {}",
                    state.schema_version, PROFILE_RUNTIME_SCHEMA_VERSION
                ),
                Some(&self.state_path),
            ));
        }
        if !is_valid_stable_id(&state.current_profile_id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid,
                format!("当前装配方案 ID 无效：{}", state.current_profile_id),
                Some(&self.state_path),
            ));
        }
        if state
            .previous_profile_id
            .as_deref()
            .is_some_and(|profile_id| !is_valid_stable_id(profile_id))
        {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid,
                "上一个装配方案 ID 无效",
                Some(&self.state_path),
            ));
        }
        Ok(state)
    }

    fn read_pending_switch_locked(
        &self,
    ) -> Result<WorkspaceProfilePendingSwitch, WorkspaceProfileRuntimeError> {
        let bytes = fs::read(&self.journal_path)
            .map_err(|error| io_error("读取装配方案切换日志失败", &self.journal_path, error))?;
        let pending: WorkspaceProfilePendingSwitch =
            serde_json::from_slice(&bytes).map_err(|error| {
                WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                    format!("装配方案切换日志无法解析：{error}"),
                    Some(&self.journal_path),
                )
            })?;
        if pending.schema_version != PROFILE_SWITCH_JOURNAL_SCHEMA_VERSION {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                format!(
                    "不支持装配方案切换日志版本 {}，当前仅支持 {}",
                    pending.schema_version, PROFILE_SWITCH_JOURNAL_SCHEMA_VERSION
                ),
                Some(&self.journal_path),
            ));
        }
        if pending.transaction_id.is_empty()
            || !is_valid_stable_id(&pending.from_profile_id)
            || !is_valid_stable_id(&pending.to_profile_id)
            || pending.from_profile_id == pending.to_profile_id
        {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                "装配方案切换日志字段无效",
                Some(&self.journal_path),
            ));
        }
        Ok(pending)
    }

    fn remove_journal_locked(
        &self,
        transaction_id: &str,
    ) -> Result<(), WorkspaceProfileRuntimeError> {
        let pending = self.read_pending_switch_locked()?;
        if pending.transaction_id != transaction_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
                "装配方案切换事务 ID 不匹配",
                Some(&self.journal_path),
            ));
        }
        fs::remove_file(&self.journal_path)
            .map_err(|error| io_error("清理装配方案切换日志失败", &self.journal_path, error))
    }

    fn read_profile(
        &self,
        path: &Path,
    ) -> Result<WorkspaceProfileV1, WorkspaceProfileRuntimeError> {
        let source =
            fs::read_to_string(path).map_err(|error| io_error("读取装配方案失败", path, error))?;
        parse_workspace_profile(&source).map_err(|error| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("装配方案无效：{}（{}）", error.message, error.path),
                Some(path),
            )
        })
    }

    fn read_profile_document_locked(
        &self,
        profile_id: &str,
    ) -> Result<WorkspaceProfileV1, WorkspaceProfileRuntimeError> {
        if !is_valid_stable_id(profile_id) {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("装配方案 ID 无效：{profile_id}"),
                None,
            ));
        }
        let path = self.profile_path(profile_id);
        let profile = self.read_profile(&path)?;
        if profile.id != profile_id {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                "装配方案文件名与文档 ID 不一致",
                Some(&path),
            ));
        }
        Ok(profile)
    }

    fn ensure_edit_repository_ready_locked(&self) -> Result<(), WorkspaceProfileRuntimeError> {
        if self.journal_path.exists() {
            return Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileEditBlocked,
                "装配方案切换尚未完成，暂时不能修改方案仓库",
                Some(&self.journal_path),
            ));
        }
        fs::create_dir_all(&self.repository_path)
            .map_err(|error| io_error("创建装配方案目录失败", &self.repository_path, error))
    }

    fn list_profiles_locked(
        &self,
        manifests: &[ModuleManifestV1],
        current_profile_id: &str,
    ) -> Result<Vec<WorkspaceProfileSummary>, WorkspaceProfileRuntimeError> {
        let entries = fs::read_dir(&self.repository_path)
            .map_err(|error| io_error("读取装配方案目录失败", &self.repository_path, error))?;
        let mut summaries = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                io_error("读取装配方案目录项失败", &self.repository_path, error)
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let fallback_id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("invalid-profile")
                .to_string();
            match fs::read_to_string(&path)
                .map_err(|error| io_error("读取装配方案失败", &path, error))
                .and_then(|source| {
                    parse_workspace_profile(&source).map_err(|error| {
                        WorkspaceProfileRuntimeError::new(
                            WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                            format!("{}（{}）", error.message, error.path),
                            Some(&path),
                        )
                    })
                }) {
                Ok(profile) => {
                    if profile.id != fallback_id {
                        summaries.push(WorkspaceProfileSummary {
                            id: fallback_id.clone(),
                            name: profile.name,
                            description: profile.description,
                            revision: profile.revision,
                            current: false,
                            enabled_module_count: profile.enabled_modules.len(),
                            enabled_component_count: profile.enabled_components.len(),
                            effective_component_count: 0,
                            pinned_tool_count: profile.shell_layout.pinned_tools.len(),
                            status: WorkspaceProfileDocumentStatus::Invalid,
                            issues: vec![format!(
                                "文件名与方案 ID 不一致：{fallback_id} != {}",
                                profile.id
                            )],
                            path: path.to_string_lossy().into_owned(),
                        });
                        continue;
                    }
                    let compatibility = self.validate_profile(&profile, manifests);
                    let effective_component_count = compatibility
                        .as_ref()
                        .map(BTreeSet::len)
                        .unwrap_or_default();
                    summaries.push(WorkspaceProfileSummary {
                        id: profile.id.clone(),
                        name: profile.name.clone(),
                        description: profile.description.clone(),
                        revision: profile.revision,
                        current: profile.id == current_profile_id,
                        enabled_module_count: profile.enabled_modules.len(),
                        enabled_component_count: profile.enabled_components.len(),
                        effective_component_count,
                        pinned_tool_count: profile.shell_layout.pinned_tools.len(),
                        status: if compatibility.is_ok() {
                            WorkspaceProfileDocumentStatus::Ready
                        } else {
                            WorkspaceProfileDocumentStatus::Blocked
                        },
                        issues: compatibility
                            .err()
                            .map(|error| vec![format!("{}（{}）", error.message, error.path)])
                            .unwrap_or_default(),
                        path: path.to_string_lossy().into_owned(),
                    });
                }
                Err(error) => summaries.push(WorkspaceProfileSummary {
                    id: fallback_id.clone(),
                    name: fallback_id,
                    description: String::new(),
                    revision: 0,
                    current: false,
                    enabled_module_count: 0,
                    enabled_component_count: 0,
                    effective_component_count: 0,
                    pinned_tool_count: 0,
                    status: WorkspaceProfileDocumentStatus::Invalid,
                    issues: vec![error.message],
                    path: path.to_string_lossy().into_owned(),
                }),
            }
        }
        Ok(summaries)
    }

    fn profile_path(&self, profile_id: &str) -> PathBuf {
        self.repository_path.join(format!("{profile_id}.json"))
    }

    pub(crate) fn local_bindings_path(&self, profile_id: &str) -> PathBuf {
        self.repository_path
            .join("local-bindings")
            .join(format!("{profile_id}.json"))
    }
}

fn remove_component_owned_profile_references(
    profile: &mut WorkspaceProfileV1,
    manifest: &ComponentManifestV1,
) -> bool {
    let before = serde_json::to_value(&*profile).ok();
    let script_surface_ids = manifest
        .contributes
        .script_surfaces
        .iter()
        .map(|surface| surface.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut tool_ids = manifest
        .contributes
        .tool_actions
        .iter()
        .map(|tool| tool.id.as_str())
        .collect::<BTreeSet<_>>();
    tool_ids.extend(script_surface_ids.iter().copied());
    let mut widget_ids = manifest
        .contributes
        .widgets
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    widget_ids.extend(
        manifest
            .contributes
            .script_surfaces
            .iter()
            .filter(|surface| surface.placements.contains(&ScriptSurfacePlacement::Widget))
            .map(|surface| surface.id.as_str()),
    );
    let data_source_ids = manifest
        .contributes
        .data_sources
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let command_ids = manifest
        .contributes
        .automation_commands
        .iter()
        .map(|command| command.id.as_str())
        .chain(
            manifest
                .contributes
                .tool_actions
                .iter()
                .map(|tool| tool.id.as_str()),
        )
        .chain(
            manifest
                .contributes
                .workflow_nodes
                .iter()
                .map(|node| node.id.as_str()),
        )
        .collect::<BTreeSet<_>>();
    let workflow_ids = manifest
        .contributes
        .workflow_nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let shell_template_ids = manifest
        .contributes
        .shell_templates
        .iter()
        .map(|template| template.id.as_str())
        .collect::<BTreeSet<_>>();
    let page_template_ids = manifest
        .contributes
        .page_templates
        .iter()
        .map(|template| template.id.as_str())
        .collect::<BTreeSet<_>>();
    let theme_preset_ids = manifest
        .contributes
        .theme_presets
        .iter()
        .map(|preset| preset.id.as_str())
        .collect::<BTreeSet<_>>();

    profile
        .enabled_components
        .retain(|selection| selection.id != manifest.id);
    profile.component_settings.remove(&manifest.id);
    profile
        .automation_bindings
        .retain(|binding| binding.component_id != manifest.id);
    profile
        .workflow_bindings
        .retain(|binding| !workflow_ids.contains(binding.workflow.as_str()));
    profile
        .tool_aliases
        .retain(|alias| !tool_ids.contains(alias.tool.as_str()));

    let removed_surface_ids = profile
        .surfaces
        .iter()
        .filter(|surface| {
            surface
                .contribution
                .as_deref()
                .is_some_and(|id| script_surface_ids.contains(id))
        })
        .map(|surface| surface.id.clone())
        .collect::<BTreeSet<_>>();
    profile
        .surfaces
        .retain(|surface| !removed_surface_ids.contains(&surface.id));

    let removed_data_source_local_ids = profile
        .data_sources
        .iter()
        .filter(|source| data_source_ids.contains(source.source.as_str()))
        .map(|source| source.id.clone())
        .collect::<BTreeSet<_>>();
    profile
        .data_sources
        .retain(|source| !data_source_ids.contains(source.source.as_str()));

    for surface in &mut profile.surfaces {
        surface
            .widgets
            .retain(|widget| !widget_ids.contains(widget.widget.as_str()));
        for widget in &mut surface.widgets {
            if widget
                .data_source
                .as_ref()
                .is_some_and(|id| removed_data_source_local_ids.contains(id))
            {
                widget.data_source = None;
            }
        }
        if surface
            .template
            .as_ref()
            .is_some_and(|binding| presentation_id_is_owned(&page_template_ids, &binding.id))
        {
            surface.template = None;
        }
        if surface
            .theme_preset
            .as_ref()
            .is_some_and(|binding| presentation_id_is_owned(&theme_preset_ids, &binding.id))
        {
            surface.theme_preset = None;
        }
    }

    profile.command_bindings.retain(|binding| {
        !command_ids.contains(binding.command.as_str())
            && !binding
                .surface
                .as_ref()
                .is_some_and(|surface| removed_surface_ids.contains(surface))
            && !binding.target.as_ref().is_some_and(|target| {
                removed_surface_ids.contains(target) || script_surface_ids.contains(target.as_str())
            })
    });
    profile
        .shell_layout
        .pinned_tools
        .retain(|id| !tool_ids.contains(id.as_str()) && !script_surface_ids.contains(id.as_str()));
    profile
        .shell_layout
        .navigation
        .retain(|id| !removed_surface_ids.contains(id));
    if profile
        .shell_layout
        .home
        .as_ref()
        .is_some_and(|id| removed_surface_ids.contains(id))
    {
        profile.shell_layout.home = None;
    }
    if profile
        .shell_layout
        .shell_template
        .as_ref()
        .is_some_and(|binding| presentation_id_is_owned(&shell_template_ids, &binding.id))
    {
        profile.shell_layout.shell_template = None;
    }
    if profile
        .shell_layout
        .theme_preset
        .as_ref()
        .is_some_and(|binding| presentation_id_is_owned(&theme_preset_ids, &binding.id))
    {
        profile.shell_layout.theme_preset = None;
    }

    before != serde_json::to_value(&*profile).ok()
}

fn presentation_id_is_owned(ids: &BTreeSet<&str>, candidate: &str) -> bool {
    ids.contains(candidate) || ids.contains(presentation_template_id_alias(candidate))
}

fn restore_profile_files(
    backups: &[(PathBuf, Vec<u8>)],
) -> Result<(), WorkspaceProfileRuntimeError> {
    let mut failures = Vec::new();
    for (path, bytes) in backups.iter().rev() {
        if let Err(error) = replace_file_bytes(path, bytes) {
            failures.push(format!("{}: {}", path.to_string_lossy(), error.message));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileRollbackFailed,
            "部分装配方案文件未能恢复",
            None,
        )
        .with_details(failures))
    }
}

fn build_migrated_profile(
    manifests: &[ModuleManifestV1],
    enabled_module_ids: &BTreeSet<String>,
    legacy_pinned_tools: &[String],
) -> WorkspaceProfileV1 {
    let mut enabled_modules = manifests
        .iter()
        .filter(|manifest| enabled_module_ids.contains(&manifest.id))
        .map(|manifest| ProfileModuleSelection {
            id: manifest.id.clone(),
            version_requirement: format!("^{}", manifest.version),
            extensions: ExtensionFields::new(),
        })
        .collect::<Vec<_>>();
    enabled_modules.sort_by(|left, right| left.id.cmp(&right.id));

    let mut seen_tools = BTreeSet::new();
    let pinned_tools = legacy_pinned_tools
        .iter()
        .filter(|tool_id| seen_tools.insert((*tool_id).clone()))
        .cloned()
        .collect::<Vec<_>>();
    let now = Utc::now().timestamp_millis();
    let migration = WorkspaceProfileMigrationRecord {
        source: MIGRATION_SOURCE.into(),
        source_version: env!("CARGO_PKG_VERSION").into(),
        created_at: now,
        captured_module_count: enabled_modules.len(),
        captured_pinned_tool_count: pinned_tools.len(),
    };
    let mut extensions = ExtensionFields::new();
    extensions.insert("migration".into(), json!(migration));
    extensions.insert(
        "shellHomeMigration".into(),
        json!({
            "version": SHELL_HOME_MIGRATION_VERSION,
            "updatedAt": now,
        }),
    );

    WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: MIGRATED_PROFILE_ID.into(),
        name: "当前 Nexora 装配方案".into(),
        description:
            "由升级前的模块启用状态和功能栏 Pin 自动生成；首轮迁移不会改变当前界面或后台服务。"
                .into(),
        revision: 1,
        enabled_modules,
        enabled_components: Vec::new(),
        module_settings: BTreeMap::new(),
        component_settings: BTreeMap::new(),
        tool_aliases: Vec::new(),
        path_variables: Vec::new(),
        shell_layout: ProfileShellLayout {
            home: Some(PROJECT_HOME_PROFILE_SURFACE_ID.into()),
            pinned_tools,
            ..ProfileShellLayout::default()
        },
        surfaces: vec![project_home_surface()],
        data_sources: project_home_data_sources(),
        command_bindings: project_home_command_bindings(),
        workflow_bindings: Vec::new(),
        automation_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    }
}

fn project_home_surface() -> ProfileSurface {
    ProfileSurface {
        id: PROJECT_HOME_PROFILE_SURFACE_ID.into(),
        title: Some("项目主页".into()),
        kind: SurfaceKind::Dashboard,
        layout: SurfaceLayoutKind::ContributionDefined,
        contribution: Some(PROJECT_HOME_CONTRIBUTION_ID.into()),
        template: None,
        theme_preset: None,
        widgets: vec![
            project_home_widget(
                "project-directory",
                PROJECT_DIRECTORY_WIDGET_ID,
                "project-directory-state",
                "sidebar",
                0,
            ),
            project_home_widget(
                "quick-actions",
                PROJECT_QUICK_ACTIONS_WIDGET_ID,
                "project-quick-actions-state",
                "sidebar",
                1,
            ),
            project_home_widget(
                "recent-projects",
                RECENT_PROJECTS_WIDGET_ID,
                "recent-projects-state",
                "content",
                0,
            ),
            project_home_widget(
                "project-catalog",
                PROJECT_CATALOG_WIDGET_ID,
                "project-catalog-state",
                "content",
                1,
            ),
        ],
        settings: BTreeMap::new(),
        extensions: ExtensionFields::new(),
    }
}

fn project_home_widget(
    id: &str,
    widget: &str,
    data_source: &str,
    region: &str,
    order: i32,
) -> ProfileWidget {
    ProfileWidget {
        id: id.into(),
        widget: widget.into(),
        data_source: Some(data_source.into()),
        region: Some(region.into()),
        order,
        grid: None,
        settings: BTreeMap::new(),
        visible_when: None,
        extensions: ExtensionFields::new(),
    }
}

fn project_home_data_sources() -> Vec<ProfileDataSource> {
    vec![
        project_home_data_source(
            "project-directory-state",
            PROJECT_DIRECTORY_DATA_SOURCE_ID,
            DataSourceScope::Global,
        ),
        project_home_data_source(
            "project-quick-actions-state",
            PROJECT_QUICK_ACTIONS_DATA_SOURCE_ID,
            DataSourceScope::Surface,
        ),
        project_home_data_source(
            "recent-projects-state",
            RECENT_PROJECTS_DATA_SOURCE_ID,
            DataSourceScope::Global,
        ),
        project_home_data_source(
            "project-catalog-state",
            PROJECT_CATALOG_DATA_SOURCE_ID,
            DataSourceScope::Surface,
        ),
    ]
}

fn project_home_data_source(id: &str, source: &str, scope: DataSourceScope) -> ProfileDataSource {
    ProfileDataSource {
        id: id.into(),
        source: source.into(),
        scope,
        settings: BTreeMap::new(),
        extensions: ExtensionFields::new(),
    }
}

fn project_home_command_bindings() -> Vec<ProfileCommandBinding> {
    vec![
        project_home_command_binding("create-project", CREATE_PROJECT_COMMAND_ID, 0),
        project_home_command_binding("import-project", IMPORT_PROJECT_COMMAND_ID, 1),
        project_home_command_binding("open-project", OPEN_PROJECT_COMMAND_ID, 2),
    ]
}

fn project_home_command_binding(id: &str, command: &str, order: i32) -> ProfileCommandBinding {
    ProfileCommandBinding {
        id: id.into(),
        command: command.into(),
        placement: CommandPlacement::SurfaceAction,
        surface: Some(PROJECT_HOME_PROFILE_SURFACE_ID.into()),
        target: None,
        shortcut: None,
        order,
        settings: BTreeMap::new(),
        extensions: ExtensionFields::new(),
    }
}

fn has_project_home_composition(profile: &WorkspaceProfileV1) -> bool {
    let required_widgets = [
        PROJECT_DIRECTORY_WIDGET_ID,
        PROJECT_QUICK_ACTIONS_WIDGET_ID,
        RECENT_PROJECTS_WIDGET_ID,
        PROJECT_CATALOG_WIDGET_ID,
    ];
    let required_sources = [
        PROJECT_DIRECTORY_DATA_SOURCE_ID,
        PROJECT_QUICK_ACTIONS_DATA_SOURCE_ID,
        RECENT_PROJECTS_DATA_SOURCE_ID,
        PROJECT_CATALOG_DATA_SOURCE_ID,
    ];
    let required_commands = [
        CREATE_PROJECT_COMMAND_ID,
        IMPORT_PROJECT_COMMAND_ID,
        OPEN_PROJECT_COMMAND_ID,
    ];
    profile
        .surfaces
        .iter()
        .find(|surface| surface.id == PROJECT_HOME_PROFILE_SURFACE_ID)
        .is_some_and(|surface| {
            required_widgets.iter().all(|required| {
                surface
                    .widgets
                    .iter()
                    .any(|widget| widget.widget == *required)
            })
        })
        && required_sources.iter().all(|required| {
            profile
                .data_sources
                .iter()
                .any(|source| source.source == *required)
        })
        && required_commands.iter().all(|required| {
            profile
                .command_bindings
                .iter()
                .any(|binding| binding.command == *required)
        })
}

fn apply_project_home_composition(profile: &mut WorkspaceProfileV1) {
    if let Some(surface) = profile
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == PROJECT_HOME_PROFILE_SURFACE_ID)
    {
        surface.widgets = project_home_surface().widgets;
    }

    let data_source_ids = project_home_data_sources()
        .into_iter()
        .map(|source| source.id)
        .collect::<BTreeSet<_>>();
    profile
        .data_sources
        .retain(|source| !data_source_ids.contains(&source.id));
    profile.data_sources.extend(project_home_data_sources());

    let command_binding_ids = project_home_command_bindings()
        .into_iter()
        .map(|binding| binding.id)
        .collect::<BTreeSet<_>>();
    profile
        .command_bindings
        .retain(|binding| !command_binding_ids.contains(&binding.id));
    profile
        .command_bindings
        .extend(project_home_command_bindings());
}

fn build_blank_profile() -> WorkspaceProfileV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("template".into(), json!({ "kind": "blank", "version": 1 }));
    WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: BLANK_PROFILE_ID.into(),
        name: "空白装配空间".into(),
        description:
            "不启用普通模块，仅保留不可停用的恢复入口；用于从最小状态开始组装自己的 Nexora。".into(),
        revision: 1,
        enabled_modules: Vec::new(),
        enabled_components: Vec::new(),
        module_settings: BTreeMap::new(),
        component_settings: BTreeMap::new(),
        tool_aliases: Vec::new(),
        path_variables: Vec::new(),
        shell_layout: ProfileShellLayout::default(),
        surfaces: Vec::new(),
        data_sources: Vec::new(),
        command_bindings: Vec::new(),
        workflow_bindings: Vec::new(),
        automation_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    }
}

#[cfg(test)]
pub(crate) fn build_project_manager_reference_profile(
    manifests: &[ModuleManifestV1],
) -> Result<WorkspaceProfileV1, String> {
    let enabled_modules = reference_module_selections(manifests, PROJECT_MANAGER_MODULE_ID)?;
    let mut extensions = ExtensionFields::new();
    extensions.insert(
        "referenceAssembly".into(),
        json!({ "id": "reference.project-manager", "version": 1 }),
    );
    Ok(WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: "reference.project-manager".into(),
        name: "项目管理器参考装配".into(),
        description: "R12-A 最小项目管理器装配；局域网、渲染和剪贴板按需添加，不是固定模式。"
            .into(),
        revision: 1,
        enabled_modules,
        enabled_components: Vec::new(),
        module_settings: BTreeMap::new(),
        component_settings: BTreeMap::new(),
        tool_aliases: Vec::new(),
        path_variables: Vec::new(),
        shell_layout: ProfileShellLayout {
            home: Some(PROJECT_HOME_PROFILE_SURFACE_ID.into()),
            ..ProfileShellLayout::default()
        },
        surfaces: vec![project_home_surface()],
        data_sources: project_home_data_sources(),
        command_bindings: project_home_command_bindings(),
        workflow_bindings: Vec::new(),
        automation_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    })
}

#[cfg(test)]
pub(crate) fn build_lan_communications_reference_profile(
    manifests: &[ModuleManifestV1],
) -> Result<WorkspaceProfileV1, String> {
    let enabled_modules = reference_module_selections(manifests, LAN_COLLABORATION_MODULE_ID)?;
    let mut extensions = ExtensionFields::new();
    extensions.insert(
        "referenceAssembly".into(),
        json!({ "id": "reference.lan-communications", "version": 1 }),
    );
    Ok(WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: "reference.lan-communications".into(),
        name: "局域网通信端参考装配".into(),
        description: "R12-C 独立局域网通信端；联系人、消息和传输不依赖项目管理器。".into(),
        revision: 1,
        enabled_modules,
        enabled_components: Vec::new(),
        module_settings: BTreeMap::new(),
        component_settings: BTreeMap::new(),
        tool_aliases: Vec::new(),
        path_variables: Vec::new(),
        shell_layout: ProfileShellLayout {
            home: Some(LAN_MAIN_PROFILE_SURFACE_ID.into()),
            ..ProfileShellLayout::default()
        },
        surfaces: vec![lan_main_surface()],
        data_sources: Vec::new(),
        command_bindings: Vec::new(),
        workflow_bindings: Vec::new(),
        automation_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    })
}

#[cfg(test)]
pub(crate) fn build_external_blender_renderer_reference_profile(
    manifests: &[ModuleManifestV1],
) -> Result<WorkspaceProfileV1, String> {
    let enabled_modules = reference_module_selections(manifests, RENDER_CENTER_MODULE_ID)?;
    let mut extensions = ExtensionFields::new();
    extensions.insert(
        "referenceAssembly".into(),
        json!({ "id": "reference.external-blender-renderer", "version": 1 }),
    );
    Ok(WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: "reference.external-blender-renderer".into(),
        name: "Blender 外部渲染器参考装配".into(),
        description: "R12-D 独立 Blender 渲染器；队列、Worker 和交付结果不依赖项目管理器。".into(),
        revision: 1,
        enabled_modules,
        enabled_components: Vec::new(),
        module_settings: BTreeMap::new(),
        component_settings: BTreeMap::new(),
        tool_aliases: Vec::new(),
        path_variables: Vec::new(),
        shell_layout: ProfileShellLayout {
            home: Some(EXTERNAL_RENDER_STATION_PROFILE_SURFACE_ID.into()),
            ..ProfileShellLayout::default()
        },
        surfaces: vec![external_render_station_surface()],
        data_sources: Vec::new(),
        command_bindings: Vec::new(),
        workflow_bindings: Vec::new(),
        automation_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    })
}

#[cfg(test)]
pub(crate) fn build_media_library_reference_profile(
    manifests: &[ModuleManifestV1],
) -> Result<WorkspaceProfileV1, String> {
    let enabled_modules = reference_module_selections(manifests, MEDIA_LIBRARY_MODULE_ID)?;
    let mut extensions = ExtensionFields::new();
    extensions.insert(
        "referenceAssembly".into(),
        json!({ "id": "reference.media-library", "version": 1 }),
    );
    Ok(WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: "reference.media-library".into(),
        name: "媒体资料库参考装配".into(),
        description: "R12-B 独立媒体资料库；索引、注释和归档均保存到用户选择的资料库目录。".into(),
        revision: 1,
        enabled_modules,
        enabled_components: Vec::new(),
        module_settings: BTreeMap::new(),
        component_settings: BTreeMap::new(),
        tool_aliases: Vec::new(),
        path_variables: Vec::new(),
        shell_layout: ProfileShellLayout {
            home: Some(MEDIA_LIBRARY_PROFILE_SURFACE_ID.into()),
            ..ProfileShellLayout::default()
        },
        surfaces: vec![media_library_surface()],
        data_sources: Vec::new(),
        command_bindings: Vec::new(),
        workflow_bindings: Vec::new(),
        automation_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    })
}

#[cfg(test)]
fn reference_module_selections(
    manifests: &[ModuleManifestV1],
    root_module_id: &str,
) -> Result<Vec<ProfileModuleSelection>, String> {
    let manifest_by_id = manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();
    let mut required_ids = BTreeSet::from([root_module_id.to_string()]);
    let mut pending = vec![root_module_id];
    while let Some(module_id) = pending.pop() {
        let manifest = manifest_by_id
            .get(module_id)
            .ok_or_else(|| format!("参考装配缺少模块：{module_id}"))?;
        for dependency in &manifest.requires_modules {
            if required_ids.insert(dependency.id.clone()) {
                pending.push(&dependency.id);
            }
        }
    }
    let mut enabled_modules = required_ids
        .iter()
        .map(|module_id| {
            manifest_by_id
                .get(module_id.as_str())
                .map(|manifest| ProfileModuleSelection {
                    id: manifest.id.clone(),
                    version_requirement: format!("^{}", manifest.version),
                    extensions: ExtensionFields::new(),
                })
                .ok_or_else(|| format!("参考装配缺少模块：{module_id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    enabled_modules.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(enabled_modules)
}

#[cfg(test)]
fn lan_main_surface() -> ProfileSurface {
    ProfileSurface {
        id: LAN_MAIN_PROFILE_SURFACE_ID.into(),
        title: Some("局域网主面板".into()),
        kind: SurfaceKind::ShellPage,
        layout: SurfaceLayoutKind::ContributionDefined,
        contribution: Some(LAN_MAIN_CONTRIBUTION_ID.into()),
        template: None,
        theme_preset: None,
        widgets: Vec::new(),
        settings: BTreeMap::new(),
        extensions: ExtensionFields::new(),
    }
}

#[cfg(test)]
fn external_render_station_surface() -> ProfileSurface {
    ProfileSurface {
        id: EXTERNAL_RENDER_STATION_PROFILE_SURFACE_ID.into(),
        title: Some("外部 Blender 渲染器".into()),
        kind: SurfaceKind::ShellPage,
        layout: SurfaceLayoutKind::ContributionDefined,
        contribution: Some(EXTERNAL_RENDER_STATION_CONTRIBUTION_ID.into()),
        template: None,
        theme_preset: None,
        widgets: Vec::new(),
        settings: BTreeMap::new(),
        extensions: ExtensionFields::new(),
    }
}

#[cfg(test)]
fn media_library_surface() -> ProfileSurface {
    ProfileSurface {
        id: MEDIA_LIBRARY_PROFILE_SURFACE_ID.into(),
        title: Some("媒体资料库".into()),
        kind: SurfaceKind::ShellPage,
        layout: SurfaceLayoutKind::ContributionDefined,
        contribution: Some(MEDIA_LIBRARY_CONTRIBUTION_ID.into()),
        template: None,
        theme_preset: None,
        widgets: Vec::new(),
        settings: BTreeMap::new(),
        extensions: ExtensionFields::new(),
    }
}

pub(crate) fn build_default_profile(manifests: &[ModuleManifestV1]) -> WorkspaceProfileV1 {
    let manifest_by_id = manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();
    let mut enabled_ids = manifests
        .iter()
        .filter(|manifest| {
            manifest
                .extensions
                .get("defaultEnabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .map(|manifest| manifest.id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let dependencies = enabled_ids
            .iter()
            .filter_map(|id| manifest_by_id.get(id.as_str()))
            .flat_map(|manifest| {
                manifest
                    .requires_modules
                    .iter()
                    .map(|dependency| dependency.id.clone())
            })
            .collect::<Vec<_>>();
        let previous_len = enabled_ids.len();
        enabled_ids.extend(dependencies);
        if enabled_ids.len() == previous_len {
            break;
        }
    }

    let mut enabled_modules = enabled_ids
        .iter()
        .filter_map(|id| manifest_by_id.get(id.as_str()))
        .map(|manifest| ProfileModuleSelection {
            id: manifest.id.clone(),
            version_requirement: format!("^{}", manifest.version),
            extensions: ExtensionFields::new(),
        })
        .collect::<Vec<_>>();
    enabled_modules.sort_by(|left, right| left.id.cmp(&right.id));

    let project_manager_enabled = enabled_ids.contains(PROJECT_MANAGER_MODULE_ID);
    let mut pinned_tools = Vec::new();
    for (module_id, tool_id) in [
        ("builtin.render-center", "builtin.render-center.tool"),
        (
            "builtin.project-resources",
            "builtin.project-resources.cache-tool",
        ),
        (
            "builtin.lan-collaboration",
            "builtin.lan-collaboration.main-tool",
        ),
    ] {
        if enabled_ids.contains(module_id) {
            pinned_tools.push(tool_id.into());
        }
    }
    let mut extensions = ExtensionFields::new();
    extensions.insert(
        "template".into(),
        json!({ "kind": "pm-center-default", "version": 1 }),
    );
    WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: DEFAULT_PROFILE_ID.into(),
        name: "Nexora 默认装配".into(),
        description: "按当前版本默认启用模块生成；用于独立恢复入口，不覆盖用户自定义方案。".into(),
        revision: 1,
        enabled_modules,
        enabled_components: Vec::new(),
        module_settings: BTreeMap::new(),
        component_settings: BTreeMap::new(),
        tool_aliases: Vec::new(),
        path_variables: Vec::new(),
        shell_layout: ProfileShellLayout {
            home: project_manager_enabled.then(|| PROJECT_HOME_PROFILE_SURFACE_ID.into()),
            pinned_tools,
            ..ProfileShellLayout::default()
        },
        surfaces: project_manager_enabled
            .then(project_home_surface)
            .into_iter()
            .collect(),
        data_sources: if project_manager_enabled {
            project_home_data_sources()
        } else {
            Vec::new()
        },
        command_bindings: if project_manager_enabled {
            project_home_command_bindings()
        } else {
            Vec::new()
        },
        workflow_bindings: Vec::new(),
        automation_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    }
}

fn module_changes(
    modules: &[super::ModuleDiagnosticSnapshot],
    target_module_ids: &BTreeSet<String>,
    enabling: bool,
) -> Vec<WorkspaceProfileModuleChange> {
    let mut changes = modules
        .iter()
        .filter(|module| !module.diagnostic)
        .filter(|module| {
            let target_enabled = target_module_ids.contains(&module.manifest.id);
            let active = matches!(
                module.state,
                super::ModuleState::Resolving
                    | super::ModuleState::Starting
                    | super::ModuleState::Running
                    | super::ModuleState::Stopping
                    | super::ModuleState::RestartRequired
                    | super::ModuleState::Error
            );
            if enabling {
                target_enabled && !module.desired_enabled
            } else {
                !target_enabled && (module.desired_enabled || active)
            }
        })
        .map(|module| WorkspaceProfileModuleChange {
            module_id: module.manifest.id.clone(),
            name: module.manifest.name.clone(),
            version: module.manifest.version.clone(),
            current_state: module.state,
            current_desired_enabled: module.desired_enabled,
            target_enabled: target_module_ids.contains(&module.manifest.id),
            resource_count: module.resources.len(),
            resource_labels: module
                .resources
                .iter()
                .map(|resource| resource.label.clone())
                .collect(),
        })
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| left.name.cmp(&right.name));
    changes
}

fn component_summaries(
    profile: &WorkspaceProfileV1,
    manifests: &[ModuleManifestV1],
    component_manifests: &[ComponentManifestV1],
    effective: &BTreeSet<String>,
) -> Vec<WorkspaceProfileComponentSummary> {
    let explicit = profile
        .enabled_components
        .iter()
        .map(|selection| selection.id.as_str())
        .collect::<BTreeSet<_>>();
    let enabled_modules = profile
        .enabled_modules
        .iter()
        .map(|selection| selection.id.as_str())
        .collect::<BTreeSet<_>>();

    let mut required_by_modules = BTreeMap::<&str, BTreeSet<String>>::new();
    for manifest in manifests
        .iter()
        .filter(|manifest| enabled_modules.contains(manifest.id.as_str()))
    {
        for dependency in manifest
            .requires_components
            .iter()
            .chain(manifest.optional_components.iter())
        {
            if effective.contains(&dependency.id) {
                required_by_modules
                    .entry(dependency.id.as_str())
                    .or_default()
                    .insert(manifest.id.clone());
            }
        }
    }

    let mut required_by_components = BTreeMap::<&str, BTreeSet<String>>::new();
    for manifest in component_manifests
        .iter()
        .filter(|manifest| effective.contains(&manifest.id))
    {
        for dependency in manifest
            .requires_components
            .iter()
            .chain(manifest.optional_components.iter())
        {
            if effective.contains(&dependency.id) {
                required_by_components
                    .entry(dependency.id.as_str())
                    .or_default()
                    .insert(manifest.id.clone());
            }
        }
    }

    let mut summaries = component_manifests
        .iter()
        .map(|manifest| WorkspaceProfileComponentSummary {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            description: manifest.description.clone(),
            version: manifest.version.clone(),
            runtime: manifest.runtime,
            role: manifest.role,
            distribution: manifest.distribution,
            ui_mode: manifest.ui_mode,
            explicit_enabled: explicit.contains(manifest.id.as_str()),
            effective_enabled: effective.contains(&manifest.id),
            required_by_modules: required_by_modules
                .remove(manifest.id.as_str())
                .unwrap_or_default()
                .into_iter()
                .collect(),
            required_by_components: required_by_components
                .remove(manifest.id.as_str())
                .unwrap_or_default()
                .into_iter()
                .collect(),
            settings_sections: manifest.contributes.settings_sections.clone(),
            category: manifest.category,
            tags: manifest.tags.clone(),
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .effective_enabled
            .cmp(&left.effective_enabled)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    summaries
}

fn build_draft_validation(
    profile: &WorkspaceProfileV1,
    manifests: &[ModuleManifestV1],
    component_manifests: &[ComponentManifestV1],
) -> WorkspaceProfileDraftValidation {
    let exact = validate_profile_with_catalogs(profile, manifests, component_manifests);
    let effective = exact.as_ref().cloned().unwrap_or_else(|_| {
        best_effort_effective_components(profile, manifests, component_manifests)
    });
    let issues = compatibility_issues(profile, manifests, component_manifests);
    WorkspaceProfileDraftValidation {
        valid: exact.is_ok()
            && !issues
                .iter()
                .any(|issue| issue.severity == WorkspaceProfileSwitchIssueSeverity::Error),
        selected_module_count: profile.enabled_modules.len(),
        explicit_component_count: profile.enabled_components.len(),
        effective_component_count: effective.len(),
        components: component_summaries(profile, manifests, component_manifests, &effective),
        issues,
    }
}

fn best_effort_effective_components(
    profile: &WorkspaceProfileV1,
    manifests: &[ModuleManifestV1],
    component_manifests: &[ComponentManifestV1],
) -> BTreeSet<String> {
    let modules = manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();
    let components = component_manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();
    let mut effective = BTreeSet::new();
    let mut roots = profile
        .enabled_components
        .iter()
        .map(|selection| selection.id.clone())
        .collect::<Vec<_>>();
    for selection in &profile.enabled_modules {
        if let Some(module) = modules.get(selection.id.as_str()) {
            roots.extend(
                module
                    .requires_components
                    .iter()
                    .chain(module.optional_components.iter())
                    .map(|dependency| dependency.id.clone()),
            );
        }
    }
    for root in roots {
        add_best_effort_component_closure(&root, &components, &mut effective);
    }
    effective
}

fn add_best_effort_component_closure(
    component_id: &str,
    components: &BTreeMap<&str, &ComponentManifestV1>,
    effective: &mut BTreeSet<String>,
) {
    if !components.contains_key(component_id) || !effective.insert(component_id.to_string()) {
        return;
    }
    if let Some(component) = components.get(component_id) {
        for dependency in component
            .requires_components
            .iter()
            .chain(component.optional_components.iter())
        {
            add_best_effort_component_closure(&dependency.id, components, effective);
        }
    }
}

fn validate_profile_text(
    value: &str,
    label: &str,
    max_chars: usize,
) -> Result<String, WorkspaceProfileRuntimeError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
            format!("{label}不能为空"),
            None,
        ));
    }
    if value.chars().count() > max_chars {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
            format!("{label}不能超过 {max_chars} 个字符"),
            None,
        ));
    }
    Ok(value.to_string())
}

fn validate_profile_text_optional(
    value: &str,
    label: &str,
    max_chars: usize,
) -> Result<String, WorkspaceProfileRuntimeError> {
    let value = value.trim();
    if value.chars().count() > max_chars {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
            format!("{label}不能超过 {max_chars} 个字符"),
            None,
        ));
    }
    Ok(value.to_string())
}

fn draft_validation_error(
    message: &str,
    validation: &WorkspaceProfileDraftValidation,
    path: Option<&Path>,
) -> WorkspaceProfileRuntimeError {
    WorkspaceProfileRuntimeError::new(
        WorkspaceProfileRuntimeErrorCode::ProfileEditBlocked,
        message,
        path,
    )
    .with_details(
        validation
            .issues
            .iter()
            .filter(|issue| issue.severity == WorkspaceProfileSwitchIssueSeverity::Error)
            .map(|issue| format!("{}: {}", issue.code, issue.message))
            .collect(),
    )
}

fn compatibility_issues(
    profile: &WorkspaceProfileV1,
    manifests: &[ModuleManifestV1],
    component_manifests: &[ComponentManifestV1],
) -> Vec<WorkspaceProfileSwitchIssue> {
    let installed = manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();
    let installed_components = component_manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();
    let enabled = profile
        .enabled_modules
        .iter()
        .map(|selection| selection.id.as_str())
        .collect::<BTreeSet<_>>();
    let effective_components =
        best_effort_effective_components(profile, manifests, component_manifests);
    let component_surface_owners = component_manifests
        .iter()
        .flat_map(|component| {
            component
                .contributes
                .script_surfaces
                .iter()
                .map(move |surface| (surface.id.as_str(), (component, surface)))
        })
        .collect::<BTreeMap<_, _>>();
    let contribution_owners = ModuleContributionOwners::from_manifests(manifests);
    let mut issues = Vec::new();

    validate_presentation_bindings(profile, manifests, component_manifests, &mut issues);

    for selection in &profile.enabled_components {
        let Some(component) = installed_components.get(selection.id.as_str()) else {
            issues.push(switch_issue(
                "COMPONENT_NOT_INSTALLED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "component",
                format!("目标方案显式选择了未安装组件：{}", selection.id),
                None,
                None,
            ));
            continue;
        };
        if !version_requirement_matches(&selection.version_requirement, &component.version) {
            issues.push(switch_issue(
                "COMPONENT_VERSION_INCOMPATIBLE",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "component",
                format!(
                    "组件 {} 当前版本 {} 不满足 {}",
                    selection.id, component.version, selection.version_requirement
                ),
                None,
                None,
            ));
        }
    }

    for selection in &profile.enabled_modules {
        let Some(manifest) = installed.get(selection.id.as_str()) else {
            issues.push(switch_issue(
                "MODULE_NOT_INSTALLED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "module",
                format!("目标方案需要未安装模块：{}", selection.id),
                Some(selection.id.clone()),
                None,
            ));
            continue;
        };
        let compatible = VersionReq::parse(&selection.version_requirement)
            .ok()
            .zip(Version::parse(&manifest.version).ok())
            .is_some_and(|(requirement, version)| requirement.matches(&version));
        if !compatible {
            issues.push(switch_issue(
                "MODULE_VERSION_INCOMPATIBLE",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "module",
                format!(
                    "模块 {} 当前版本 {} 不满足 {}",
                    selection.id, manifest.version, selection.version_requirement
                ),
                Some(selection.id.clone()),
                None,
            ));
        }
        for dependency in &manifest.requires_modules {
            if !enabled.contains(dependency.id.as_str()) {
                issues.push(switch_issue(
                    "MODULE_DEPENDENCY_NOT_SELECTED",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "dependency",
                    format!("模块 {} 需要同时启用 {}", manifest.name, dependency.id),
                    Some(manifest.id.clone()),
                    None,
                ));
            }
        }
        for (required, dependency) in manifest
            .requires_components
            .iter()
            .map(|dependency| (true, dependency))
            .chain(
                manifest
                    .optional_components
                    .iter()
                    .map(|dependency| (false, dependency)),
            )
        {
            let Some(component) = installed_components.get(dependency.id.as_str()) else {
                if required {
                    issues.push(switch_issue(
                        "MODULE_COMPONENT_NOT_INSTALLED",
                        WorkspaceProfileSwitchIssueSeverity::Error,
                        "component",
                        format!("模块 {} 需要未安装组件 {}", manifest.name, dependency.id),
                        Some(manifest.id.clone()),
                        None,
                    ));
                }
                continue;
            };
            if !version_requirement_matches(&dependency.version_requirement, &component.version) {
                issues.push(switch_issue(
                    "MODULE_COMPONENT_VERSION_INCOMPATIBLE",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "component",
                    format!(
                        "模块 {} 需要组件 {} {}，当前安装 {}",
                        manifest.name,
                        dependency.id,
                        dependency.version_requirement,
                        component.version
                    ),
                    Some(manifest.id.clone()),
                    None,
                ));
            }
        }
        for conflict in &manifest.conflicts {
            if enabled.contains(conflict.as_str()) {
                issues.push(switch_issue(
                    "MODULE_CONFLICT",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "dependency",
                    format!("目标方案同时启用了冲突模块 {} 与 {conflict}", manifest.id),
                    Some(manifest.id.clone()),
                    None,
                ));
            }
        }
    }

    for surface in &profile.surfaces {
        let Some(contribution_id) = surface.contribution.as_deref() else {
            continue;
        };
        if let Some(module_id) =
            disabled_contribution_provider(&contribution_owners.surfaces, contribution_id, &enabled)
        {
            issues.push(switch_issue(
                "SURFACE_PROVIDER_DISABLED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "surface",
                format!("页面 {} 由未启用模块 {module_id} 提供", surface.id),
                Some(module_id.into()),
                Some(contribution_id.into()),
            ));
        }

        for widget in &surface.widgets {
            if let Some(module_id) = disabled_contribution_provider(
                &contribution_owners.widgets,
                &widget.widget,
                &enabled,
            ) {
                issues.push(switch_issue(
                    "WIDGET_PROVIDER_DISABLED",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "widget",
                    format!("组件 {} 由未启用模块 {module_id} 提供", widget.id),
                    Some(module_id.into()),
                    Some(widget.widget.clone()),
                ));
            }
        }
    }

    for source in &profile.data_sources {
        if let Some(module_id) = disabled_contribution_provider(
            &contribution_owners.data_sources,
            &source.source,
            &enabled,
        ) {
            issues.push(switch_issue(
                "DATA_SOURCE_PROVIDER_DISABLED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "data-source",
                format!("数据源 {} 由未启用模块 {module_id} 提供", source.id),
                Some(module_id.into()),
                Some(source.source.clone()),
            ));
        }
    }

    for binding in &profile.command_bindings {
        if let Some(module_id) = disabled_contribution_provider(
            &contribution_owners.commands,
            &binding.command,
            &enabled,
        ) {
            issues.push(switch_issue(
                "COMMAND_PROVIDER_DISABLED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "command",
                format!("命令 {} 由未启用模块 {module_id} 提供", binding.id),
                Some(module_id.into()),
                Some(binding.command.clone()),
            ));
        }
    }

    for contribution_id in &profile.shell_layout.pinned_tools {
        if let Some(module_id) =
            disabled_contribution_provider(&contribution_owners.tools, contribution_id, &enabled)
        {
            issues.push(switch_issue(
                "PIN_PROVIDER_DISABLED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "pin",
                format!("快捷栏功能 {contribution_id} 由未启用模块 {module_id} 提供"),
                Some(module_id.into()),
                Some(contribution_id.clone()),
            ));
        } else if let Some((component, surface)) =
            component_surface_owners.get(contribution_id.as_str())
        {
            if !surface.placements.contains(&ScriptSurfacePlacement::Shell) {
                issues.push(switch_issue(
                    "PIN_PLACEMENT_UNSUPPORTED",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "pin",
                    format!(
                        "组件页面 {} 未声明 shell 放置，不能固定到快捷栏",
                        surface.name
                    ),
                    None,
                    Some(contribution_id.clone()),
                ));
            } else if !effective_components.contains(&component.id) {
                issues.push(switch_issue(
                    "PIN_COMPONENT_PROVIDER_DISABLED",
                    WorkspaceProfileSwitchIssueSeverity::Error,
                    "pin",
                    format!(
                        "快捷栏组件页面 {} 的组件 {} 未在当前方案中启用",
                        surface.name, component.name
                    ),
                    None,
                    Some(contribution_id.clone()),
                ));
            }
        }
    }

    for surface_id in &profile.shell_layout.navigation {
        let Some(surface) = profile
            .surfaces
            .iter()
            .find(|surface| surface.id == *surface_id)
        else {
            continue;
        };
        let provider_has_shell_entry = surface.contribution.as_deref().is_some_and(|id| {
            contribution_owners
                .surfaces
                .get(id)
                .is_some_and(|providers| {
                    providers.iter().any(|provider| {
                        enabled.contains(provider.as_str())
                            && contribution_owners
                                .modules_with_shell_tabs
                                .contains(provider)
                    })
                })
        });
        let is_home_navigation = profile.shell_layout.home.as_deref() == Some(surface_id.as_str());
        if !is_home_navigation
            && (!matches!(surface.kind, SurfaceKind::ShellPage) || !provider_has_shell_entry)
        {
            issues.push(switch_issue(
                "NAVIGATION_ENTRY_UNSUPPORTED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "navigation",
                format!("导航页面 {surface_id} 没有可用的 Shell 标签入口"),
                surface
                    .contribution
                    .as_deref()
                    .and_then(|id| contribution_owners.surfaces.get(id))
                    .and_then(|providers| providers.first())
                    .cloned(),
                surface.contribution.clone(),
            ));
        }
    }

    if let Err(error) = validate_profile_with_catalogs(profile, manifests, component_manifests) {
        if issues.is_empty() {
            issues.push(switch_issue(
                "PROFILE_CONTRACT_BLOCKED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "profile",
                format!("{}（{}）", error.message, error.path),
                None,
                None,
            ));
        }
    }
    issues
}

fn validate_presentation_bindings(
    profile: &WorkspaceProfileV1,
    module_manifests: &[ModuleManifestV1],
    component_manifests: &[ComponentManifestV1],
    issues: &mut Vec<WorkspaceProfileSwitchIssue>,
) {
    let effective_components =
        best_effort_effective_components(profile, module_manifests, component_manifests);
    let mut bindings = Vec::new();
    if let Some(binding) = profile.shell_layout.shell_template.as_ref() {
        bindings.push(("Shell", binding));
    }
    if let Some(binding) = profile.shell_layout.theme_preset.as_ref() {
        bindings.push(("主题", binding));
    }
    for surface in &profile.surfaces {
        if let Some(binding) = surface.template.as_ref() {
            bindings.push(("页面", binding));
        }
        if let Some(binding) = surface.theme_preset.as_ref() {
            bindings.push(("主题", binding));
        }
    }
    for (kind, binding) in bindings {
        let resolved_template_id = presentation_template_id_alias(&binding.id);
        let owner = component_manifests.iter().find(|component| match kind {
            "Shell" => component
                .contributes
                .shell_templates
                .iter()
                .any(|template| template.id == resolved_template_id),
            "页面" => component
                .contributes
                .page_templates
                .iter()
                .any(|template| template.id == resolved_template_id),
            _ => component
                .contributes
                .theme_presets
                .iter()
                .any(|template| template.id == resolved_template_id),
        });
        let Some(owner) = owner else {
            issues.push(switch_issue(
                "PRESENTATION_TEMPLATE_MISSING",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "presentation",
                format!("{kind}模板 {} 未安装或类型不匹配", binding.id),
                None,
                Some(binding.id.clone()),
            ));
            continue;
        };
        let template_version =
            presentation_template_version(owner, kind, resolved_template_id).unwrap_or_default();
        if !version_requirement_matches(&binding.version_requirement, template_version) {
            issues.push(switch_issue(
                "PRESENTATION_TEMPLATE_VERSION_INCOMPATIBLE",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "presentation",
                format!(
                    "{kind}模板 {} 版本 {} 不满足 {}",
                    binding.id, template_version, binding.version_requirement
                ),
                None,
                Some(binding.id.clone()),
            ));
        }
        if owner.id != "nexora.presentation.templates" && !effective_components.contains(&owner.id)
        {
            issues.push(switch_issue(
                "PRESENTATION_TEMPLATE_PROVIDER_NOT_ENABLED",
                WorkspaceProfileSwitchIssueSeverity::Error,
                "presentation",
                format!(
                    "{kind}模板 {} 的组件 {} 未在该装配方案中启用",
                    binding.id, owner.name
                ),
                None,
                Some(binding.id.clone()),
            ));
        }
    }
}

fn presentation_template_id_alias(id: &str) -> &str {
    match id {
        "builtin.shell.top-bar" => "nexora.shell.top-bar",
        "builtin.shell.side-bar" => "nexora.shell.side-bar",
        "builtin.shell.compact" => "nexora.shell.minimal",
        _ => id,
    }
}

fn presentation_template_version<'a>(
    component: &'a ComponentManifestV1,
    kind: &str,
    template_id: &str,
) -> Option<&'a str> {
    match kind {
        "Shell" => component
            .contributes
            .shell_templates
            .iter()
            .find(|template| template.id == template_id)
            .map(|template| template.version.as_str()),
        "页面" => component
            .contributes
            .page_templates
            .iter()
            .find(|template| template.id == template_id)
            .map(|template| template.version.as_str()),
        _ => component
            .contributes
            .theme_presets
            .iter()
            .find(|template| template.id == template_id)
            .map(|template| template.version.as_str()),
    }
}

#[derive(Default)]
struct ModuleContributionOwners {
    surfaces: BTreeMap<String, Vec<String>>,
    widgets: BTreeMap<String, Vec<String>>,
    data_sources: BTreeMap<String, Vec<String>>,
    commands: BTreeMap<String, Vec<String>>,
    tools: BTreeMap<String, Vec<String>>,
    modules_with_shell_tabs: BTreeSet<String>,
}

impl ModuleContributionOwners {
    fn from_manifests(manifests: &[ModuleManifestV1]) -> Self {
        let mut owners = Self::default();
        for manifest in manifests {
            record_contribution_owners(
                &mut owners.surfaces,
                &manifest.contributes.surfaces,
                &manifest.id,
            );
            record_contribution_owners(
                &mut owners.widgets,
                &manifest.contributes.widgets,
                &manifest.id,
            );
            record_contribution_owners(
                &mut owners.data_sources,
                &manifest.contributes.data_sources,
                &manifest.id,
            );
            record_contribution_owners(
                &mut owners.commands,
                &manifest.contributes.commands,
                &manifest.id,
            );
            record_contribution_owners(
                &mut owners.tools,
                &manifest.contributes.tools,
                &manifest.id,
            );
            if !manifest.contributes.shell_tabs.is_empty() {
                owners.modules_with_shell_tabs.insert(manifest.id.clone());
            }
        }
        owners
    }
}

fn record_contribution_owners(
    owners: &mut BTreeMap<String, Vec<String>>,
    contribution_ids: &[String],
    module_id: &str,
) {
    for contribution_id in contribution_ids {
        owners
            .entry(contribution_id.clone())
            .or_default()
            .push(module_id.into());
    }
}

fn disabled_contribution_provider<'a>(
    owners: &'a BTreeMap<String, Vec<String>>,
    contribution_id: &str,
    enabled: &BTreeSet<&str>,
) -> Option<&'a str> {
    let providers = owners.get(contribution_id)?;
    if providers
        .iter()
        .any(|provider| enabled.contains(provider.as_str()))
    {
        return None;
    }
    providers.first().map(String::as_str)
}

fn version_requirement_matches(requirement: &str, version: &str) -> bool {
    VersionReq::parse(requirement)
        .ok()
        .zip(Version::parse(version).ok())
        .is_some_and(|(requirement, version)| requirement.matches(&version))
}

fn switch_issue(
    code: &str,
    severity: WorkspaceProfileSwitchIssueSeverity,
    category: &str,
    message: impl Into<String>,
    module_id: Option<String>,
    contribution_id: Option<String>,
) -> WorkspaceProfileSwitchIssue {
    WorkspaceProfileSwitchIssue {
        code: code.into(),
        severity,
        category: category.into(),
        message: message.into(),
        module_id,
        contribution_id,
    }
}

fn dedupe(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .filter(|value| seen.insert((*value).clone()))
        .cloned()
        .collect()
}

fn migration_record_from_profile(profile: &WorkspaceProfileV1) -> WorkspaceProfileMigrationRecord {
    profile
        .extensions
        .get("migration")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_else(|| WorkspaceProfileMigrationRecord {
            source: MIGRATION_SOURCE.into(),
            source_version: env!("CARGO_PKG_VERSION").into(),
            created_at: Utc::now().timestamp_millis(),
            captured_module_count: profile.enabled_modules.len(),
            captured_pinned_tool_count: profile.shell_layout.pinned_tools.len(),
        })
}

fn is_valid_stable_id(value: &str) -> bool {
    (2..=128).contains(&value.len())
        && value.contains('.')
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && !segment.ends_with('-')
        })
}

fn write_new_json<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), WorkspaceProfileRuntimeError> {
    if path.exists() {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfilePersistenceConflict,
            "装配方案文件已经存在，拒绝覆盖",
            Some(path),
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileIoError,
            "装配方案路径缺少父目录",
            Some(path),
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| io_error("创建装配方案父目录失败", parent, error))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("profile.json");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
            format!("序列化装配方案失败：{error}"),
            Some(path),
        )
    })?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error("创建装配方案临时文件失败", &temporary, error))?;
        file.write_all(&bytes)
            .map_err(|error| io_error("写入装配方案临时文件失败", &temporary, error))?;
        file.write_all(b"\n")
            .map_err(|error| io_error("完成装配方案临时文件失败", &temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error("同步装配方案临时文件失败", &temporary, error))?;
        fs::rename(&temporary, path).map_err(|error| io_error("提交装配方案文件失败", path, error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn replace_json<T: Serialize>(path: &Path, value: &T) -> Result<(), WorkspaceProfileRuntimeError> {
    let parent = path.parent().ok_or_else(|| {
        WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileIoError,
            "装配方案状态路径缺少父目录",
            Some(path),
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| io_error("创建装配方案状态目录失败", parent, error))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("profile-runtime.json");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let backup = parent.join(format!(".{file_name}.{}.bak", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
            format!("序列化装配方案状态失败：{error}"),
            Some(path),
        )
    })?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error("创建装配方案状态临时文件失败", &temporary, error))?;
        file.write_all(&bytes)
            .map_err(|error| io_error("写入装配方案状态临时文件失败", &temporary, error))?;
        file.write_all(b"\n")
            .map_err(|error| io_error("完成装配方案状态临时文件失败", &temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error("同步装配方案状态临时文件失败", &temporary, error))?;
        if path.exists() {
            fs::rename(path, &backup)
                .map_err(|error| io_error("备份旧装配方案状态失败", path, error))?;
        }
        if let Err(error) = fs::rename(&temporary, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(io_error("提交装配方案状态失败", path, error));
        }
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| io_error("清理旧装配方案状态备份失败", &backup, error))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn replace_file_bytes(path: &Path, bytes: &[u8]) -> Result<(), WorkspaceProfileRuntimeError> {
    let parent = path.parent().ok_or_else(|| {
        WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileIoError,
            "装配方案路径缺少父目录",
            Some(path),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| io_error("创建装配方案目录失败", parent, error))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("profile.json");
    let temporary = parent.join(format!(".{file_name}.{}.restore.tmp", Uuid::new_v4()));
    let backup = parent.join(format!(".{file_name}.{}.restore.bak", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error("创建装配方案恢复文件失败", &temporary, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error("写入装配方案恢复文件失败", &temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error("同步装配方案恢复文件失败", &temporary, error))?;
        if path.exists() {
            fs::rename(path, &backup)
                .map_err(|error| io_error("暂存待恢复装配方案失败", path, error))?;
        }
        if let Err(error) = fs::rename(&temporary, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(io_error("提交装配方案恢复失败", path, error));
        }
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| io_error("清理装配方案恢复备份失败", &backup, error))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn io_error(context: &str, path: &Path, error: std::io::Error) -> WorkspaceProfileRuntimeError {
    WorkspaceProfileRuntimeError::new(
        WorkspaceProfileRuntimeErrorCode::ProfileIoError,
        format!("{context}：{error}"),
        Some(path),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmc_platform::{
        Capability, ComponentContributions, ComponentDependency, ComponentDistribution,
        ComponentResourceLimits, ComponentRole, ComponentRuntime, ComponentUiMode,
        ModuleContributions, ModuleDataPolicy, ModuleDependency, ModuleScope, PlatformTarget,
        ScriptSurfaceContribution,
    };

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pm-center-profile-{label}-{}", Uuid::new_v4()))
    }

    fn manifest(id: &str, version: &str) -> ModuleManifestV1 {
        ModuleManifestV1 {
            schema_version: PLATFORM_SCHEMA_VERSION,
            id: id.into(),
            name: id.into(),
            description: String::new(),
            version: version.into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: Vec::new(),
            optional_modules: Vec::new(),
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: Vec::new(),
            background_services: Vec::new(),
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions: ExtensionFields::new(),
        }
    }

    fn component_manifest(id: &str, version: &str) -> ComponentManifestV1 {
        ComponentManifestV1 {
            schema_version: PLATFORM_SCHEMA_VERSION,
            id: id.into(),
            name: id.into(),
            description: String::new(),
            version: version.into(),
            api_version: "1".into(),
            category: None,
            tags: Vec::new(),
            runtime: ComponentRuntime::NativeProcess,
            role: ComponentRole::Service,
            distribution: ComponentDistribution::Bundled,
            ui_mode: ComponentUiMode::Hosted,
            platforms: vec![PlatformTarget::WindowsX64],
            entry: Some("bin/component.exe".into()),
            capabilities: vec![Capability::ProjectFilesRead],
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            contributes: ComponentContributions::default(),
            resources: ComponentResourceLimits::default(),
            publisher: None,
            extensions: ExtensionFields::new(),
        }
    }

    fn layout_manifest() -> ModuleManifestV1 {
        let mut manifest = manifest("test.layout-provider", "1.0.0");
        manifest.contributes.shell_tabs = vec!["test.layout-provider.shell-tab".into()];
        manifest.contributes.tools = vec!["test.layout-provider.tool".into()];
        manifest.contributes.surfaces = vec!["test.layout-provider.surface".into()];
        manifest.contributes.widgets = vec!["test.layout-provider.widget".into()];
        manifest.contributes.data_sources = vec!["test.layout-provider.data-source".into()];
        manifest.contributes.commands = vec!["test.layout-provider.command".into()];
        manifest
    }

    fn profile_with_layout(provider_enabled: bool) -> WorkspaceProfileV1 {
        let mut profile = build_blank_profile();
        if provider_enabled {
            profile.enabled_modules.push(ProfileModuleSelection {
                id: "test.layout-provider".into(),
                version_requirement: "^1.0".into(),
                extensions: ExtensionFields::new(),
            });
        }
        profile.shell_layout.navigation = vec!["layout-page".into()];
        profile.shell_layout.pinned_tools = vec!["test.layout-provider.tool".into()];
        profile.data_sources.push(ProfileDataSource {
            id: "layout-source".into(),
            source: "test.layout-provider.data-source".into(),
            scope: DataSourceScope::Profile,
            settings: BTreeMap::new(),
            extensions: ExtensionFields::new(),
        });
        profile.surfaces.push(ProfileSurface {
            id: "layout-page".into(),
            title: Some("布局页面".into()),
            kind: SurfaceKind::ShellPage,
            layout: SurfaceLayoutKind::ContributionDefined,
            contribution: Some("test.layout-provider.surface".into()),
            template: None,
            theme_preset: None,
            widgets: vec![ProfileWidget {
                id: "layout-widget".into(),
                widget: "test.layout-provider.widget".into(),
                data_source: Some("layout-source".into()),
                region: Some("content".into()),
                order: 0,
                grid: None,
                settings: BTreeMap::new(),
                visible_when: None,
                extensions: ExtensionFields::new(),
            }],
            settings: BTreeMap::new(),
            extensions: ExtensionFields::new(),
        });
        profile.command_bindings.push(ProfileCommandBinding {
            id: "layout-command".into(),
            command: "test.layout-provider.command".into(),
            placement: CommandPlacement::SurfaceAction,
            surface: Some("layout-page".into()),
            target: None,
            shortcut: None,
            order: 0,
            settings: BTreeMap::new(),
            extensions: ExtensionFields::new(),
        });
        profile
    }

    #[test]
    fn draft_validation_rejects_layout_owned_by_a_disabled_module() {
        let provider = layout_manifest();
        let validation = build_draft_validation(&profile_with_layout(false), &[provider], &[]);
        let codes = validation
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect::<BTreeSet<_>>();

        assert!(!validation.valid);
        assert!(codes.contains("SURFACE_PROVIDER_DISABLED"));
        assert!(codes.contains("WIDGET_PROVIDER_DISABLED"));
        assert!(codes.contains("DATA_SOURCE_PROVIDER_DISABLED"));
        assert!(codes.contains("COMMAND_PROVIDER_DISABLED"));
        assert!(codes.contains("PIN_PROVIDER_DISABLED"));
        assert!(codes.contains("NAVIGATION_ENTRY_UNSUPPORTED"));
    }

    #[test]
    fn draft_validation_accepts_enabled_layout_with_a_shell_entry() {
        let provider = layout_manifest();
        let validation = build_draft_validation(&profile_with_layout(true), &[provider], &[]);

        assert!(
            validation.valid,
            "unexpected issues: {:?}",
            validation.issues
        );
    }

    #[test]
    fn draft_validation_tracks_component_script_surface_pins() {
        let mut component = component_manifest("test.music-player", "1.0.0");
        component.contributes.script_surfaces = vec![ScriptSurfaceContribution {
            id: "test.music-player.surface".into(),
            name: "音乐播放器".into(),
            entry: "ui/index.html".into(),
            placements: vec![
                ScriptSurfacePlacement::Shell,
                ScriptSurfacePlacement::Dialog,
            ],
            allowed_commands: Vec::new(),
            extensions: ExtensionFields::new(),
        }];
        let mut profile = build_blank_profile();
        profile
            .enabled_components
            .push(pmc_platform::ProfileComponentSelection {
                id: component.id.clone(),
                version_requirement: "^1.0".into(),
                extensions: ExtensionFields::new(),
            });
        profile.shell_layout.pinned_tools = vec!["test.music-player.surface".into()];

        let enabled = build_draft_validation(&profile, &[], &[component.clone()]);
        assert!(enabled.valid, "unexpected issues: {:?}", enabled.issues);

        profile.enabled_components.clear();
        let disabled = build_draft_validation(&profile, &[], &[component]);
        assert!(disabled
            .issues
            .iter()
            .any(|issue| issue.code == "PIN_COMPONENT_PROVIDER_DISABLED"));
    }

    #[test]
    fn component_removal_cleans_every_profile_and_can_roll_back() {
        let root = test_root("component-removal-cleanup");
        let mut component = component_manifest("test.music-player", "1.0.0");
        component.contributes.script_surfaces = vec![ScriptSurfaceContribution {
            id: "test.music-player.surface".into(),
            name: "音乐播放器".into(),
            entry: "ui/index.html".into(),
            placements: vec![
                ScriptSurfacePlacement::Shell,
                ScriptSurfacePlacement::Workspace,
            ],
            allowed_commands: Vec::new(),
            extensions: ExtensionFields::new(),
        }];
        let runtime = WorkspaceProfileRuntime::new_with_components(&root, vec![component.clone()]);
        let snapshot = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let mut current = snapshot.current_profile;
        current
            .enabled_components
            .push(pmc_platform::ProfileComponentSelection {
                id: component.id.clone(),
                version_requirement: "^1.0".into(),
                extensions: ExtensionFields::new(),
            });
        current
            .component_settings
            .insert(component.id.clone(), json!({ "volume": 75 }));
        current.shell_layout.pinned_tools = vec!["test.music-player.surface".into()];
        current.shell_layout.home = Some("music-page".into());
        current.shell_layout.navigation = vec!["music-page".into()];
        current.surfaces.push(ProfileSurface {
            id: "music-page".into(),
            title: Some("音乐播放器".into()),
            kind: SurfaceKind::ShellPage,
            layout: SurfaceLayoutKind::ContributionDefined,
            contribution: Some("test.music-player.surface".into()),
            template: None,
            theme_preset: None,
            widgets: Vec::new(),
            settings: BTreeMap::new(),
            extensions: ExtensionFields::new(),
        });
        let current_path = runtime.profile_path(&current.id);
        replace_json(&current_path, &current).unwrap();

        let mut second = current.clone();
        second.id = "local.music-copy".into();
        second.name = "Music copy".into();
        let second_path = runtime.profile_path(&second.id);
        write_new_json(&second_path, &second).unwrap();
        let current_before = fs::read(&current_path).unwrap();
        let second_before = fs::read(&second_path).unwrap();

        let rollback = runtime
            .remove_component_references(&component, &[])
            .unwrap();
        for profile_id in [&current.id, &second.id] {
            let cleaned = runtime.profile_document(profile_id).unwrap();
            assert!(cleaned
                .enabled_components
                .iter()
                .all(|selection| selection.id != component.id));
            assert!(!cleaned.component_settings.contains_key(&component.id));
            assert!(!cleaned
                .shell_layout
                .pinned_tools
                .contains(&"test.music-player.surface".to_string()));
            assert!(cleaned.shell_layout.home.is_none());
            assert!(cleaned.shell_layout.navigation.is_empty());
            assert!(cleaned.surfaces.iter().all(
                |surface| surface.contribution.as_deref() != Some("test.music-player.surface")
            ));
        }

        runtime
            .rollback_component_reference_removal(rollback)
            .unwrap();
        assert_eq!(fs::read(&current_path).unwrap(), current_before);
        assert_eq!(fs::read(&second_path).unwrap(), second_before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn component_removal_rejects_required_module_dependency_without_writing() {
        let root = test_root("component-removal-dependency");
        let component = component_manifest("test.reader", "1.0.0");
        let runtime = WorkspaceProfileRuntime::new_with_components(&root, vec![component.clone()]);
        let mut viewer = manifest("test.viewer", "1.0.0");
        viewer.requires_components.push(ComponentDependency {
            id: component.id.clone(),
            version_requirement: "^1.0".into(),
        });
        let snapshot = runtime
            .initialize_from_current_configuration(
                &[viewer.clone()],
                &BTreeSet::from([viewer.id.clone()]),
                &[],
            )
            .unwrap();
        let path = runtime.profile_path(&snapshot.current_profile.id);
        let before = fs::read(&path).unwrap();

        assert!(runtime
            .remove_component_references(&component, &[viewer])
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn draft_validation_rejects_navigation_without_a_shell_entry() {
        let mut provider = layout_manifest();
        provider.contributes.shell_tabs.clear();
        let validation = build_draft_validation(&profile_with_layout(true), &[provider], &[]);

        assert!(validation
            .issues
            .iter()
            .any(|issue| issue.code == "NAVIGATION_ENTRY_UNSUPPORTED"));
    }

    #[test]
    fn draft_validation_accepts_home_surface_in_navigation_without_a_shell_entry() {
        let mut provider = layout_manifest();
        provider.contributes.shell_tabs.clear();
        let mut profile = profile_with_layout(true);
        profile.shell_layout.home = Some("layout-page".into());
        profile.surfaces[0].kind = SurfaceKind::Dashboard;

        let validation = build_draft_validation(&profile, &[provider], &[]);

        assert!(
            validation.valid,
            "unexpected issues: {:?}",
            validation.issues
        );
    }

    #[test]
    fn snapshot_reports_components_enabled_through_module_dependencies() {
        let root = test_root("component-summary");
        let component = component_manifest("test.reader", "1.2.0");
        let runtime = WorkspaceProfileRuntime::new_with_components(&root, vec![component]);
        let mut viewer = manifest("test.viewer", "1.0.0");
        viewer.requires_components.push(ComponentDependency {
            id: "test.reader".into(),
            version_requirement: "^1.0".into(),
        });
        let snapshot = runtime
            .initialize_from_current_configuration(
                &[viewer],
                &BTreeSet::from(["test.viewer".to_string()]),
                &[],
            )
            .unwrap();

        assert_eq!(snapshot.components.len(), 1);
        assert!(snapshot.components[0].effective_enabled);
        assert!(!snapshot.components[0].explicit_enabled);
        assert_eq!(
            snapshot.components[0].required_by_modules,
            vec!["test.viewer".to_string()]
        );
        assert_eq!(snapshot.profiles[0].effective_component_count, 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn migration_captures_modules_and_pins_without_changing_on_second_initialize() {
        let root = test_root("idempotent");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![
            manifest("builtin.alpha", "1.0.0"),
            manifest("builtin.beta", "1.2.0"),
        ];
        let enabled = BTreeSet::from(["builtin.alpha".to_string()]);
        let first = runtime
            .initialize_from_current_configuration(
                &manifests,
                &enabled,
                &["builtin.alpha.tool".into(), "builtin.alpha.tool".into()],
            )
            .unwrap();
        assert_eq!(first.current_profile.enabled_modules.len(), 1);
        assert_eq!(first.current_profile.shell_layout.pinned_tools.len(), 1);
        assert_eq!(
            first.current_profile.shell_layout.home.as_deref(),
            Some(PROJECT_HOME_PROFILE_SURFACE_ID)
        );
        assert_eq!(first.current_profile.surfaces.len(), 1);
        assert_eq!(
            first.current_profile.surfaces[0].contribution.as_deref(),
            Some(PROJECT_HOME_CONTRIBUTION_ID)
        );

        let second = runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.beta".to_string()]),
                &["builtin.beta.tool".into()],
            )
            .unwrap();
        assert_eq!(
            second.current_profile.enabled_modules[0].id,
            "builtin.alpha"
        );
        assert_eq!(
            second.current_profile.shell_layout.pinned_tools,
            vec!["builtin.alpha.tool"]
        );
        assert_eq!(
            second.current_profile.revision,
            first.current_profile.revision
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_profile_module_toggle_persists_and_removes_owned_pin() {
        let root = test_root("current-module-toggle");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.local-web-console", "1.0.0")];
        let initial = runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::new(),
                &["builtin.local-web-console.tool".into()],
            )
            .unwrap();

        let enabled = runtime
            .set_current_module_enabled(
                "builtin.local-web-console",
                "^1.0",
                true,
                &["builtin.local-web-console.tool"],
                &manifests,
            )
            .unwrap();
        assert!(enabled
            .current_profile
            .enabled_modules
            .iter()
            .any(|selection| selection.id == "builtin.local-web-console"));
        assert_eq!(
            enabled.current_profile.revision,
            initial.current_profile.revision + 1
        );

        let disabled = runtime
            .set_current_module_enabled(
                "builtin.local-web-console",
                "^1.0",
                false,
                &["builtin.local-web-console.tool"],
                &manifests,
            )
            .unwrap();
        assert!(disabled.current_profile.enabled_modules.is_empty());
        assert!(disabled
            .current_profile
            .shell_layout
            .pinned_tools
            .is_empty());
        assert_eq!(
            disabled.current_profile.revision,
            enabled.current_profile.revision + 1
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_migrated_profile_receives_project_home_once() {
        let root = test_root("home-migration");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();
        let mut legacy = build_migrated_profile(&[], &BTreeSet::new(), &[]);
        legacy.shell_layout.home = None;
        legacy.surfaces.clear();
        legacy.data_sources.clear();
        legacy.command_bindings.clear();
        legacy.extensions.remove("shellHomeMigration");
        legacy.revision = 7;
        fs::write(
            profiles.join(format!("{MIGRATED_PROFILE_ID}.json")),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let runtime = WorkspaceProfileRuntime::new(&root);
        let first = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        assert_eq!(first.current_profile.revision, 8);
        assert_eq!(
            first.current_profile.shell_layout.home.as_deref(),
            Some(PROJECT_HOME_PROFILE_SURFACE_ID)
        );
        assert_eq!(first.current_profile.surfaces.len(), 1);
        assert_eq!(
            first.current_profile.surfaces[0].contribution.as_deref(),
            Some(PROJECT_HOME_CONTRIBUTION_ID)
        );
        assert_eq!(first.current_profile.surfaces[0].widgets.len(), 4);
        assert_eq!(first.current_profile.data_sources.len(), 4);
        assert_eq!(first.current_profile.command_bindings.len(), 3);

        let second = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        assert_eq!(second.current_profile.revision, 8);
        assert_eq!(second.current_profile.surfaces.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_migrated_profile_receives_project_manager_once() {
        let root = test_root("project-manager-migration");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();

        let resources = manifest("builtin.project-resources", "1.0.0");
        let mut manager = manifest(PROJECT_MANAGER_MODULE_ID, "1.0.0");
        manager
            .requires_modules
            .push(pmc_platform::ModuleDependency {
                id: resources.id.clone(),
                version_requirement: "^1.0".into(),
            });
        let manifests = vec![resources, manager];
        let mut legacy = build_migrated_profile(
            &manifests,
            &BTreeSet::from(["builtin.project-resources".to_string()]),
            &[],
        );
        legacy.extensions.remove("shellHomeMigration");
        legacy.revision = 5;
        fs::write(
            profiles.join(format!("{MIGRATED_PROFILE_ID}.json")),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let runtime = WorkspaceProfileRuntime::new(&root);
        let first = runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        assert_eq!(first.current_profile.revision, 6);
        assert!(first
            .current_profile
            .enabled_modules
            .iter()
            .any(|selection| selection.id == PROJECT_MANAGER_MODULE_ID));

        let second = runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        assert_eq!(second.current_profile.revision, 6);
        assert_eq!(
            second
                .current_profile
                .enabled_modules
                .iter()
                .filter(|selection| selection.id == PROJECT_MANAGER_MODULE_ID)
                .count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_runtime_state_is_reported_without_overwrite() {
        let root = test_root("corrupt-state");
        fs::create_dir_all(&root).unwrap();
        let state_path = root.join("profile-runtime.json");
        fs::write(&state_path, b"not-json").unwrap();
        let runtime = WorkspaceProfileRuntime::new(&root);
        let error = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid
        ));
        assert_eq!(fs::read(&state_path).unwrap(), b"not-json");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_non_current_profile_is_visible_in_diagnostics() {
        let root = test_root("invalid-profile");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.alpha", "1.0.0")];
        let snapshot = runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.alpha".to_string()]),
                &[],
            )
            .unwrap();
        fs::write(
            Path::new(&snapshot.repository_path).join("broken.json"),
            b"{broken",
        )
        .unwrap();
        let snapshot = runtime.snapshot(&manifests).unwrap();
        assert!(snapshot
            .profiles
            .iter()
            .any(|profile| profile.id == BLANK_PROFILE_ID));
        assert!(snapshot
            .profiles
            .iter()
            .any(|profile| profile.id == DEFAULT_PROFILE_ID));
        assert!(snapshot
            .profiles
            .iter()
            .any(|profile| matches!(profile.status, WorkspaceProfileDocumentStatus::Invalid)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_current_profile_id_is_rejected_before_path_resolution() {
        let root = test_root("unsafe-id");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("profile-runtime.json"),
            br#"{
              "schemaVersion": 1,
              "currentProfileId": "../outside",
              "createdAt": 1,
              "updatedAt": 1,
              "migration": {
                "source": "current-pm-center",
                "sourceVersion": "2.8.4",
                "createdAt": 1,
                "capturedModuleCount": 0,
                "capturedPinnedToolCount": 0
              }
            }"#,
        )
        .unwrap();
        let runtime = WorkspaceProfileRuntime::new(&root);
        let error = runtime.snapshot(&[]).unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileRuntimeStateInvalid
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_does_not_adopt_a_different_profile_from_the_reserved_path() {
        let root = test_root("reserved-path");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();
        let mut profile = build_migrated_profile(&[], &BTreeSet::new(), &[]);
        profile.id = "local.other-profile".into();
        fs::write(
            profiles.join(format!("{MIGRATED_PROFILE_ID}.json")),
            serde_json::to_vec_pretty(&profile).unwrap(),
        )
        .unwrap();
        let runtime = WorkspaceProfileRuntime::new(&root);
        let error = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePersistenceConflict
        ));
        assert!(!root.join("profile-runtime.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initialization_creates_a_plain_blank_profile_without_modules() {
        let root = test_root("blank");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.alpha", "1.0.0")];
        let snapshot = runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.alpha".to_string()]),
                &[],
            )
            .unwrap();
        let blank = runtime.profile(BLANK_PROFILE_ID, &manifests).unwrap();
        assert!(blank.enabled_modules.is_empty());
        assert!(blank.shell_layout.pinned_tools.is_empty());
        assert!(blank.shell_layout.home.is_none());
        assert!(blank.surfaces.is_empty());
        assert!(snapshot
            .profiles
            .iter()
            .any(|profile| profile.id == BLANK_PROFILE_ID));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_profile_contains_default_modules_and_required_dependencies() {
        let root = test_root("default-profile-dependencies");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let dependency = manifest("builtin.dependency", "1.2.0");
        let mut feature = manifest("builtin.default-feature", "2.3.0");
        feature
            .extensions
            .insert("defaultEnabled".into(), json!(true));
        feature.requires_modules.push(ModuleDependency {
            id: dependency.id.clone(),
            version_requirement: "^1.0".into(),
        });
        let manifests = vec![dependency, feature];

        let snapshot = runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        let default_profile = runtime.profile(DEFAULT_PROFILE_ID, &manifests).unwrap();
        let enabled_ids = default_profile
            .enabled_modules
            .iter()
            .map(|selection| selection.id.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(snapshot.default_profile_id, DEFAULT_PROFILE_ID);
        assert_eq!(
            enabled_ids,
            BTreeSet::from(["builtin.default-feature", "builtin.dependency"])
        );
        assert_eq!(default_profile.revision, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creating_default_profile_does_not_overwrite_a_custom_profile() {
        let root = test_root("default-profile-preserves-custom");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();
        let mut custom = build_blank_profile();
        custom.id = "local.custom-workspace".into();
        custom.name = "自定义装配".into();
        custom.revision = 7;
        let custom_path = profiles.join("local.custom-workspace.json");
        fs::write(&custom_path, serde_json::to_vec_pretty(&custom).unwrap()).unwrap();
        let original = fs::read(&custom_path).unwrap();

        let mut alpha = manifest("builtin.alpha", "1.0.0");
        alpha
            .extensions
            .insert("defaultEnabled".into(), json!(true));
        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[alpha], &BTreeSet::new(), &[])
            .unwrap();

        assert_eq!(fs::read(&custom_path).unwrap(), original);
        assert!(runtime.profile_path(DEFAULT_PROFILE_ID).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn script_automation_migration_updates_only_the_reserved_default_profile_once() {
        let root = test_root("script-automation-default-migration");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();

        let automation_runtime = manifest("builtin.automation-runtime", "1.0.0");
        let mut legacy_default = build_default_profile(&[automation_runtime.clone()]);
        legacy_default.revision = 6;
        fs::write(
            profiles.join(format!("{DEFAULT_PROFILE_ID}.json")),
            serde_json::to_vec_pretty(&legacy_default).unwrap(),
        )
        .unwrap();

        let mut custom = build_blank_profile();
        custom.id = "local.custom-before-r11".into();
        custom.name = "R11 前自定义装配".into();
        let custom_path = profiles.join("local.custom-before-r11.json");
        let custom_bytes = serde_json::to_vec_pretty(&custom).unwrap();
        fs::write(&custom_path, &custom_bytes).unwrap();

        let mut script_automation = manifest(SCRIPT_AUTOMATION_MODULE_ID, "1.0.0");
        script_automation.requires_modules.push(ModuleDependency {
            id: automation_runtime.id.clone(),
            version_requirement: "^1.0".into(),
        });
        script_automation
            .extensions
            .insert("defaultEnabled".into(), json!(true));
        let manifests = vec![automation_runtime, script_automation];
        let runtime = WorkspaceProfileRuntime::new(&root);

        runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        let migrated = runtime.profile(DEFAULT_PROFILE_ID, &manifests).unwrap();
        assert_eq!(migrated.revision, 7);
        assert!(migrated
            .enabled_modules
            .iter()
            .any(|selection| selection.id == SCRIPT_AUTOMATION_MODULE_ID));
        assert_eq!(
            migrated.extensions["scriptAutomationMigration"]["version"],
            json!(SCRIPT_AUTOMATION_MIGRATION_VERSION)
        );
        assert_eq!(fs::read(&custom_path).unwrap(), custom_bytes);

        runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        let second = runtime.profile(DEFAULT_PROFILE_ID, &manifests).unwrap();
        assert_eq!(second.revision, migrated.revision);

        let mut user_disabled = second;
        user_disabled
            .enabled_modules
            .retain(|selection| selection.id != SCRIPT_AUTOMATION_MODULE_ID);
        user_disabled.revision = user_disabled.revision.saturating_add(1);
        replace_json(&runtime.profile_path(DEFAULT_PROFILE_ID), &user_disabled).unwrap();
        runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        let after_user_disable = runtime.profile(DEFAULT_PROFILE_ID, &manifests).unwrap();
        assert!(!after_user_disable
            .enabled_modules
            .iter()
            .any(|selection| selection.id == SCRIPT_AUTOMATION_MODULE_ID));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_layout_migration_is_idempotent_and_preserves_user_widget_changes() {
        let root = test_root("shell-layout-migration");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();
        let mut profile = build_migrated_profile(&[], &BTreeSet::new(), &[]);
        profile.extensions.insert(
            "shellHomeMigration".into(),
            json!({ "version": LEGACY_HOME_COMPOSITION_MIGRATION_VERSION }),
        );
        let surface = profile
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == PROJECT_HOME_PROFILE_SURFACE_ID)
            .unwrap();
        surface
            .widgets
            .retain(|widget| widget.id == "recent-projects");
        surface
            .settings
            .insert("widgetsConfigured".into(), json!(true));
        let expected_widgets = surface.widgets.clone();
        fs::write(
            profiles.join(format!("{MIGRATED_PROFILE_ID}.json")),
            serde_json::to_vec_pretty(&profile).unwrap(),
        )
        .unwrap();

        let runtime = WorkspaceProfileRuntime::new(&root);
        let first = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let first_revision = first.current_profile.revision;
        let migrated_surface = first
            .current_profile
            .surfaces
            .iter()
            .find(|surface| surface.id == PROJECT_HOME_PROFILE_SURFACE_ID)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&migrated_surface.widgets).unwrap(),
            serde_json::to_value(&expected_widgets).unwrap()
        );
        assert_eq!(
            first.current_profile.extensions["shellHomeMigration"]["version"],
            json!(SHELL_HOME_MIGRATION_VERSION)
        );

        let second = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        assert_eq!(second.current_profile.revision, first_revision);
        assert_eq!(
            serde_json::to_value(
                &second
                    .current_profile
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == PROJECT_HOME_PROFILE_SURFACE_ID)
                    .unwrap()
                    .widgets
            )
            .unwrap(),
            serde_json::to_value(&expected_widgets).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_layout_migration_does_not_modify_custom_profiles() {
        let root = test_root("shell-layout-custom-profile");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();
        let mut custom = build_blank_profile();
        custom.id = "local.custom-layout".into();
        custom.name = "自定义布局".into();
        custom.revision = 9;
        custom.shell_layout.navigation_kind = pmc_platform::ShellNavigationKind::SideBar;
        let custom_path = profiles.join("local.custom-layout.json");
        let custom_bytes = serde_json::to_vec_pretty(&custom).unwrap();
        fs::write(&custom_path, &custom_bytes).unwrap();

        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();

        assert_eq!(fs::read(custom_path).unwrap(), custom_bytes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_ownership_migration_is_idempotent_for_existing_profiles() {
        let root = test_root("settings-ownership-migration");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();
        let target_ids = [
            "builtin.settings-center",
            "builtin.session-runtime",
            "builtin.desktop-integration",
            "builtin.external-tools",
        ];
        let manifests = target_ids
            .iter()
            .map(|id| manifest(id, "1.0.0"))
            .collect::<Vec<_>>();
        let profile = build_migrated_profile(&manifests, &BTreeSet::new(), &[]);
        fs::write(
            profiles.join(format!("{MIGRATED_PROFILE_ID}.json")),
            serde_json::to_vec_pretty(&profile).unwrap(),
        )
        .unwrap();
        let runtime = WorkspaceProfileRuntime::new(&root);

        let first = runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        let first_revision = first.current_profile.revision;
        for target_id in target_ids {
            assert!(first
                .current_profile
                .enabled_modules
                .iter()
                .any(|selection| selection.id == target_id));
        }
        assert_eq!(
            first.current_profile.extensions["settingsOwnershipMigration"]["version"],
            json!(SETTINGS_OWNERSHIP_MIGRATION_VERSION)
        );

        let second = runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        assert_eq!(second.current_profile.revision, first_revision);
        assert_eq!(
            second.current_profile.enabled_modules,
            first.current_profile.enabled_modules
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_ownership_migration_preserves_invalid_profiles_for_diagnostics() {
        let root = test_root("settings-migration-invalid-profile");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();
        fs::write(profiles.join("broken.json"), b"{broken").unwrap();
        let manifests = [
            "builtin.settings-center",
            "builtin.session-runtime",
            "builtin.desktop-integration",
            "builtin.external-tools",
        ]
        .iter()
        .map(|id| manifest(id, "1.0.0"))
        .collect::<Vec<_>>();
        let runtime = WorkspaceProfileRuntime::new(&root);

        let snapshot = runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();

        assert_eq!(fs::read(profiles.join("broken.json")).unwrap(), b"{broken");
        assert!(snapshot
            .profiles
            .iter()
            .any(|profile| matches!(profile.status, WorkspaceProfileDocumentStatus::Invalid)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn brand_migration_updates_reserved_profiles_without_touching_custom_profiles() {
        let root = test_root("brand-migration");
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).unwrap();

        let mut migrated = build_migrated_profile(&[], &BTreeSet::new(), &[]);
        migrated.name = "当前 PM Center 装配方案".into();
        migrated.description = "PM Center 旧版系统方案".into();
        fs::write(
            profiles.join(format!("{MIGRATED_PROFILE_ID}.json")),
            serde_json::to_vec_pretty(&migrated).unwrap(),
        )
        .unwrap();

        let mut custom = build_blank_profile();
        custom.id = "local.custom-brand".into();
        custom.name = "PM Center 私人方案".into();
        custom.description = "用户保留的旧名称".into();
        let custom_path = profiles.join("local.custom-brand.json");
        let custom_bytes = serde_json::to_vec_pretty(&custom).unwrap();
        fs::write(&custom_path, &custom_bytes).unwrap();

        let runtime = WorkspaceProfileRuntime::new(&root);
        let snapshot = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();

        assert_eq!(snapshot.current_profile.name, "当前 Nexora 装配方案");
        assert_eq!(snapshot.current_profile.description, "Nexora 旧版系统方案");
        assert_eq!(
            snapshot.current_profile.extensions["brandMigration"]["version"],
            json!(BRAND_MIGRATION_VERSION)
        );
        assert_eq!(fs::read(custom_path).unwrap(), custom_bytes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copied_profile_gets_a_new_id_without_modifying_the_source() {
        let root = test_root("copy-profile");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.alpha", "1.0.0")];
        let snapshot = runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.alpha".to_string()]),
                &["builtin.alpha.tool".into()],
            )
            .unwrap();
        let source_id = snapshot.current_profile.id.clone();
        let source_path = runtime.profile_path(&source_id);
        let source_before = fs::read(&source_path).unwrap();

        let result = runtime
            .create_profile(
                &CreateWorkspaceProfileRequest {
                    name: "Alpha Copy".into(),
                    description: "editable copy".into(),
                    source_profile_id: Some(source_id.clone()),
                },
                &manifests,
            )
            .unwrap();

        assert_ne!(result.profile.id, source_id);
        assert!(result.profile.id.starts_with("local.profile-"));
        assert_eq!(result.profile.name, "Alpha Copy");
        assert_eq!(result.profile.revision, 1);
        assert_eq!(fs::read(&source_path).unwrap(), source_before);
        assert_eq!(
            serde_json::to_value(runtime.profile_document(&source_id).unwrap()).unwrap(),
            serde_json::to_value(snapshot.current_profile).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn custom_profile_can_be_deleted_without_changing_the_current_profile() {
        let root = test_root("delete-custom-profile");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let snapshot = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let current_profile_id = snapshot.current_profile.id.clone();
        let current_profile_bytes = fs::read(runtime.profile_path(&current_profile_id)).unwrap();
        let created = runtime
            .create_profile(
                &CreateWorkspaceProfileRequest {
                    name: "Disposable".into(),
                    description: "delete test".into(),
                    source_profile_id: None,
                },
                &[],
            )
            .unwrap();
        let deleted_profile_id = created.profile.id.clone();
        let deleted_profile_path = runtime.profile_path(&deleted_profile_id);
        assert!(deleted_profile_path.exists());

        let after = runtime.delete_profile(&deleted_profile_id, &[]).unwrap();

        assert!(!deleted_profile_path.exists());
        assert!(!after
            .profiles
            .iter()
            .any(|profile| profile.id == deleted_profile_id));
        assert_eq!(after.current_profile.id, current_profile_id);
        assert_eq!(
            fs::read(runtime.profile_path(&current_profile_id)).unwrap(),
            current_profile_bytes
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_custom_profile_cannot_be_deleted() {
        let root = test_root("delete-current-profile");
        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let created = runtime
            .create_profile(
                &CreateWorkspaceProfileRequest {
                    name: "Current Custom".into(),
                    description: String::new(),
                    source_profile_id: None,
                },
                &[],
            )
            .unwrap();
        let profile_id = created.profile.id.clone();
        let profile_path = runtime.profile_path(&profile_id);
        let profile_bytes = fs::read(&profile_path).unwrap();
        let mut state = runtime.read_state().unwrap();
        state.current_profile_id = profile_id.clone();
        replace_json(&runtime.state_path, &state).unwrap();
        let snapshot_before = runtime.snapshot(&[]).unwrap();

        let error = runtime.delete_profile(&profile_id, &[]).unwrap_err();

        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileDeleteBlocked
        ));
        assert_eq!(fs::read(profile_path).unwrap(), profile_bytes);
        let snapshot_after = runtime.snapshot(&[]).unwrap();
        assert_eq!(snapshot_after.current_profile.id, profile_id);
        assert_eq!(
            snapshot_after.profiles.len(),
            snapshot_before.profiles.len()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reserved_profiles_cannot_be_deleted() {
        let root = test_root("delete-reserved-profiles");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let snapshot_before = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let state_before = fs::read(&runtime.state_path).unwrap();

        for profile_id in [DEFAULT_PROFILE_ID, BLANK_PROFILE_ID] {
            let path = runtime.profile_path(profile_id);
            let bytes_before = fs::read(&path).unwrap();
            let error = runtime.delete_profile(profile_id, &[]).unwrap_err();
            assert!(matches!(
                error.code,
                WorkspaceProfileRuntimeErrorCode::ProfileDeleteBlocked
            ));
            assert_eq!(fs::read(path).unwrap(), bytes_before);
        }

        let snapshot_after = runtime.snapshot(&[]).unwrap();
        assert_eq!(fs::read(&runtime.state_path).unwrap(), state_before);
        assert_eq!(
            snapshot_after.current_profile.id,
            snapshot_before.current_profile.id
        );
        assert_eq!(
            snapshot_after.profiles.len(),
            snapshot_before.profiles.len()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn non_current_migration_profile_can_be_deleted_and_clears_history_reference() {
        let root = test_root("delete-migration-profile");
        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let migration_path = runtime.profile_path(MIGRATED_PROFILE_ID);
        let mut state = runtime.read_state().unwrap();
        state.previous_profile_id = Some(MIGRATED_PROFILE_ID.into());
        state.current_profile_id = DEFAULT_PROFILE_ID.into();
        replace_json(&runtime.state_path, &state).unwrap();

        let snapshot = runtime.delete_profile(MIGRATED_PROFILE_ID, &[]).unwrap();

        assert!(!migration_path.exists());
        assert_eq!(snapshot.current_profile.id, DEFAULT_PROFILE_ID);
        assert!(!snapshot
            .profiles
            .iter()
            .any(|profile| profile.id == MIGRATED_PROFILE_ID));
        assert_eq!(runtime.read_state().unwrap().previous_profile_id, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saving_a_profile_increments_its_revision() {
        let root = test_root("save-revision");
        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let created = runtime
            .create_profile(
                &CreateWorkspaceProfileRequest {
                    name: "Editable".into(),
                    description: String::new(),
                    source_profile_id: None,
                },
                &[],
            )
            .unwrap();
        let mut profile = created.profile;
        profile.name = "Edited".into();

        let saved = runtime
            .save_profile(
                &SaveWorkspaceProfileRequest {
                    profile,
                    expected_revision: 1,
                },
                &[],
            )
            .unwrap();

        assert_eq!(saved.profile.revision, 2);
        assert_eq!(saved.profile.name, "Edited");
        assert_eq!(
            runtime
                .profile_document(&saved.profile.id)
                .unwrap()
                .revision,
            2
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_profile_revision_is_rejected() {
        let root = test_root("stale-revision");
        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let created = runtime
            .create_profile(
                &CreateWorkspaceProfileRequest {
                    name: "Editable".into(),
                    description: String::new(),
                    source_profile_id: None,
                },
                &[],
            )
            .unwrap();
        let stale = created.profile.clone();
        runtime
            .save_profile(
                &SaveWorkspaceProfileRequest {
                    profile: created.profile,
                    expected_revision: 1,
                },
                &[],
            )
            .unwrap();

        let error = runtime
            .save_profile(
                &SaveWorkspaceProfileRequest {
                    profile: stale,
                    expected_revision: 1,
                },
                &[],
            )
            .unwrap_err();

        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileRevisionConflict
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_profile_cannot_be_edited_directly() {
        let root = test_root("current-edit");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let snapshot = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();

        let error = runtime
            .save_profile(
                &SaveWorkspaceProfileRequest {
                    profile: snapshot.current_profile,
                    expected_revision: 1,
                },
                &[],
            )
            .unwrap_err();

        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileEditBlocked
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_profile_update_is_deferred_until_commit() {
        let root = test_root("current-update-commit");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let snapshot = runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let original = snapshot.current_profile;
        let original_path = runtime.profile_path(&original.id);
        let original_bytes = fs::read(&original_path).unwrap();
        let mut edited = original.clone();
        edited.name = "Updated Current Profile".into();

        let prepared = runtime
            .prepare_current_profile_update(
                &SaveWorkspaceProfileRequest {
                    profile: edited,
                    expected_revision: original.revision,
                },
                &[],
            )
            .unwrap();

        assert_eq!(fs::read(&original_path).unwrap(), original_bytes);
        let saved = runtime
            .commit_current_profile_update(prepared, &[])
            .unwrap();
        assert_eq!(saved.profile.id, original.id);
        assert_eq!(saved.profile.name, "Updated Current Profile");
        assert_eq!(saved.profile.revision, original.revision + 1);
        assert_eq!(
            saved.snapshot.current_profile.name,
            "Updated Current Profile"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_dependency_save_keeps_the_existing_file_unchanged() {
        let root = test_root("invalid-save");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let mut alpha = manifest("builtin.alpha", "1.0.0");
        alpha.requires_modules.push(ModuleDependency {
            id: "builtin.beta".into(),
            version_requirement: "^1.0".into(),
        });
        let manifests = vec![alpha, manifest("builtin.beta", "1.0.0")];
        runtime
            .initialize_from_current_configuration(&manifests, &BTreeSet::new(), &[])
            .unwrap();
        let created = runtime
            .create_profile(
                &CreateWorkspaceProfileRequest {
                    name: "Editable".into(),
                    description: String::new(),
                    source_profile_id: None,
                },
                &manifests,
            )
            .unwrap();
        let path = runtime.profile_path(&created.profile.id);
        let before = fs::read(&path).unwrap();
        let mut invalid = created.profile;
        invalid.enabled_modules.push(ProfileModuleSelection {
            id: "builtin.alpha".into(),
            version_requirement: "^1.0".into(),
            extensions: ExtensionFields::new(),
        });

        let error = runtime
            .save_profile(
                &SaveWorkspaceProfileRequest {
                    profile: invalid,
                    expected_revision: 1,
                },
                &manifests,
            )
            .unwrap_err();

        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileEditBlocked
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_reports_changes_without_modifying_runtime_state() {
        let root = test_root("preview");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.alpha", "1.0.0")];
        let snapshot = runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.alpha".to_string()]),
                &["builtin.alpha.tool".into()],
            )
            .unwrap();
        let state_before = fs::read(&snapshot.state_path).unwrap();
        let modules = vec![super::super::ModuleDiagnosticSnapshot {
            manifest: manifests[0].clone(),
            state: super::super::ModuleState::Running,
            desired_enabled: true,
            health: super::super::module_manager::ModuleHealth::healthy("ok"),
            dependencies: Vec::new(),
            dependents: Vec::new(),
            resources: Vec::new(),
            last_error: None,
            started_at: Some(1),
            updated_at: 1,
            diagnostic: false,
        }];
        let preview = runtime
            .preview_switch(
                &PreviewWorkspaceProfileSwitchRequest {
                    profile_id: BLANK_PROFILE_ID.into(),
                    current_pinned_tools: vec!["builtin.alpha.tool".into()],
                    known_tool_contributions: vec![
                        "builtin.alpha.tool".into(),
                        "core.settings.tool".into(),
                    ],
                },
                &modules,
            )
            .unwrap();
        assert!(preview.can_switch);
        assert_eq!(preview.modules_to_disable.len(), 1);
        assert_eq!(fs::read(&snapshot.state_path).unwrap(), state_before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_switch_before_profile_commit_rolls_back_to_committed_profile() {
        let root = test_root("recover-before-commit");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.alpha", "1.0.0")];
        runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.alpha".to_string()]),
                &[],
            )
            .unwrap();
        let transaction = runtime
            .begin_switch(BLANK_PROFILE_ID, MIGRATED_PROFILE_ID, 1)
            .unwrap();
        runtime
            .mark_switch_phase(
                &transaction.transaction_id,
                WorkspaceProfileSwitchPhase::ModulesApplied,
            )
            .unwrap();

        let plan = runtime.startup_recovery_plan(&manifests).unwrap();
        assert_eq!(
            plan.action,
            WorkspaceProfileStartupRecoveryAction::RollBackInterruptedSwitch
        );
        assert_eq!(plan.profile.id, MIGRATED_PROFILE_ID);
        runtime.complete_startup_recovery(&plan, false).unwrap();

        let snapshot = runtime.snapshot(&manifests).unwrap();
        assert!(snapshot.pending_switch.is_none());
        assert_eq!(
            snapshot.last_recovery.unwrap().outcome,
            WorkspaceProfileRecoveryOutcome::RolledBackInterruptedSwitch
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_switch_after_profile_commit_completes_frontend_finalization() {
        let root = test_root("recover-after-commit");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.alpha", "1.0.0")];
        runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.alpha".to_string()]),
                &[],
            )
            .unwrap();
        let transaction = runtime
            .begin_switch(BLANK_PROFILE_ID, MIGRATED_PROFILE_ID, 1)
            .unwrap();
        runtime
            .mark_switch_phase(
                &transaction.transaction_id,
                WorkspaceProfileSwitchPhase::ModulesApplied,
            )
            .unwrap();
        runtime
            .commit_switch(BLANK_PROFILE_ID, MIGRATED_PROFILE_ID, &manifests)
            .unwrap();

        let plan = runtime.startup_recovery_plan(&manifests).unwrap();
        assert_eq!(
            plan.action,
            WorkspaceProfileStartupRecoveryAction::CompleteInterruptedSwitch
        );
        assert_eq!(plan.profile.id, BLANK_PROFILE_ID);
        runtime.complete_startup_recovery(&plan, false).unwrap();
        assert!(runtime
            .snapshot(&manifests)
            .unwrap()
            .pending_switch
            .is_some());

        let snapshot = runtime
            .finalize_switch(&transaction.transaction_id, &manifests)
            .unwrap();
        assert!(snapshot.pending_switch.is_none());
        assert_eq!(
            snapshot.last_recovery.unwrap().outcome,
            WorkspaceProfileRecoveryOutcome::CompletedInterruptedSwitch
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_drift_reconciliation_is_recorded_without_switch_journal() {
        let root = test_root("runtime-drift");
        let runtime = WorkspaceProfileRuntime::new(&root);
        let manifests = vec![manifest("builtin.alpha", "1.0.0")];
        runtime
            .initialize_from_current_configuration(
                &manifests,
                &BTreeSet::from(["builtin.alpha".to_string()]),
                &[],
            )
            .unwrap();
        let plan = runtime.startup_recovery_plan(&manifests).unwrap();
        runtime.complete_startup_recovery(&plan, true).unwrap();

        let recovery = runtime.snapshot(&manifests).unwrap().last_recovery.unwrap();
        assert_eq!(
            recovery.outcome,
            WorkspaceProfileRecoveryOutcome::ReconciledRuntimeDrift
        );
        assert_eq!(recovery.profile_id, MIGRATED_PROFILE_ID);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_switch_journal_is_reported_without_overwrite() {
        let root = test_root("corrupt-journal");
        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        fs::write(&runtime.journal_path, b"not-json").unwrap();

        let error = runtime.startup_recovery_plan(&[]).unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid
        ));
        assert_eq!(fs::read(&runtime.journal_path).unwrap(), b"not-json");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_switch_journal_never_changes_the_committed_profile() {
        let root = test_root("stale-journal");
        let runtime = WorkspaceProfileRuntime::new(&root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        let transaction = runtime
            .begin_switch(BLANK_PROFILE_ID, MIGRATED_PROFILE_ID, 1)
            .unwrap();
        let mut third_profile = build_blank_profile();
        third_profile.id = "local.third-profile".into();
        third_profile.name = "Third".into();
        write_new_json(&runtime.profile_path(&third_profile.id), &third_profile).unwrap();
        let mut state = runtime.read_state().unwrap();
        state.current_profile_id = third_profile.id.clone();
        replace_json(&runtime.state_path, &state).unwrap();

        let error = runtime.startup_recovery_plan(&[]).unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfileRecoveryFailed
        ));
        assert_eq!(
            runtime.read_state().unwrap().current_profile_id,
            third_profile.id
        );
        assert_eq!(
            runtime.pending_switch().unwrap().unwrap().transaction_id,
            transaction.transaction_id
        );
        fs::remove_dir_all(root).unwrap();
    }
}
