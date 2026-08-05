use chrono::Utc;
use pmc_platform::{
    parse_workspace_profile, validate_profile_with_modules, ExtensionFields, ModuleManifestV1,
    ProfileModuleSelection, ProfileShellLayout, WorkspaceProfileV1, PLATFORM_SCHEMA_VERSION,
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
const MIGRATION_SOURCE: &str = "current-pm-center";

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
    pub pinned_tool_count: usize,
    pub status: WorkspaceProfileDocumentStatus,
    pub issues: Vec<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileRuntimeSnapshot {
    pub current_profile: WorkspaceProfileV1,
    pub profiles: Vec<WorkspaceProfileSummary>,
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

pub struct WorkspaceProfileRuntime {
    repository_path: PathBuf,
    state_path: PathBuf,
    journal_path: PathBuf,
    operation_lock: Mutex<()>,
}

impl WorkspaceProfileRuntime {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            repository_path: app_data_dir.join("profiles"),
            state_path: app_data_dir.join("profile-runtime.json"),
            journal_path: app_data_dir.join("profile-switch-journal.json"),
            operation_lock: Mutex::new(()),
        }
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

        if self.state_path.exists() {
            return self.snapshot_locked(manifests);
        }

        let profile_path = self.profile_path(MIGRATED_PROFILE_ID);
        let current_profile = if profile_path.exists() {
            self.read_profile(&profile_path)?
        } else {
            let profile =
                build_migrated_profile(manifests, enabled_module_ids, legacy_pinned_tools);
            validate_profile_with_modules(&profile, manifests).map_err(|error| {
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
        validate_profile_with_modules(&current_profile, manifests).map_err(|error| {
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
        validate_profile_with_modules(&profile, manifests).map_err(|error| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchBlocked,
                format!("目标装配方案不兼容：{}（{}）", error.message, error.path),
                Some(&path),
            )
        })?;
        Ok(profile)
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
        validate_profile_with_modules(&profile, manifests).map_err(|error| {
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
        validate_profile_with_modules(&target, manifests).map_err(|error| {
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
        let mut issues = compatibility_issues(&target, &manifests);

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

        let known_tools = request
            .known_tool_contributions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
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
        validate_profile_with_modules(&profile, manifests).map_err(|error| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileDocumentInvalid,
                format!("空白装配方案无效：{}（{}）", error.message, error.path),
                Some(&path),
            )
        })?;
        write_new_json(&path, &profile)
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
        Ok(WorkspaceProfileRuntimeSnapshot {
            current_profile,
            profiles,
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
                    let compatibility = validate_profile_with_modules(&profile, manifests);
                    summaries.push(WorkspaceProfileSummary {
                        id: profile.id.clone(),
                        name: profile.name.clone(),
                        description: profile.description.clone(),
                        revision: profile.revision,
                        current: profile.id == current_profile_id,
                        enabled_module_count: profile.enabled_modules.len(),
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

    WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: MIGRATED_PROFILE_ID.into(),
        name: "当前 PM Center 装配方案".into(),
        description:
            "由升级前的模块启用状态和功能栏 Pin 自动生成；首轮迁移不会改变当前界面或后台服务。"
                .into(),
        revision: 1,
        enabled_modules,
        module_settings: BTreeMap::new(),
        shell_layout: ProfileShellLayout {
            pinned_tools,
            ..ProfileShellLayout::default()
        },
        surfaces: Vec::new(),
        data_sources: Vec::new(),
        command_bindings: Vec::new(),
        workflow_bindings: Vec::new(),
        variables: BTreeMap::new(),
        extensions,
    }
}

fn build_blank_profile() -> WorkspaceProfileV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("template".into(), json!({ "kind": "blank", "version": 1 }));
    WorkspaceProfileV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: BLANK_PROFILE_ID.into(),
        name: "空白装配空间".into(),
        description: "不启用非核心模块，仅保留设置入口；用于从最小状态开始组装自己的 PM Center。"
            .into(),
        revision: 1,
        enabled_modules: Vec::new(),
        module_settings: BTreeMap::new(),
        shell_layout: ProfileShellLayout {
            pinned_tools: vec!["core.settings.tool".into()],
            ..ProfileShellLayout::default()
        },
        surfaces: Vec::new(),
        data_sources: Vec::new(),
        command_bindings: Vec::new(),
        workflow_bindings: Vec::new(),
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

fn compatibility_issues(
    profile: &WorkspaceProfileV1,
    manifests: &[ModuleManifestV1],
) -> Vec<WorkspaceProfileSwitchIssue> {
    let installed = manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();
    let enabled = profile
        .enabled_modules
        .iter()
        .map(|selection| selection.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut issues = Vec::new();

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

    if let Err(error) = validate_profile_with_modules(profile, manifests) {
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
    use pmc_platform::{ModuleContributions, ModuleDataPolicy, ModuleScope};

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
            conflicts: Vec::new(),
            capabilities: Vec::new(),
            background_services: Vec::new(),
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions: ExtensionFields::new(),
        }
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
        assert_eq!(snapshot.profiles.len(), 3);
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
        assert_eq!(blank.shell_layout.pinned_tools, vec!["core.settings.tool"]);
        assert!(snapshot
            .profiles
            .iter()
            .any(|profile| profile.id == BLANK_PROFILE_ID));
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
