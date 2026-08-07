use super::module_manager::{ModuleManager, ModuleState};
use chrono::Utc;
use pmc_platform::{Capability, CapabilityRisk, ComponentManifestV1};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const TOKEN_TTL_MS: i64 = 60_000;
const REQUEST_TTL_MS: i64 = 10 * 60_000;
const AUDIT_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilitySubjectKind {
    Module,
    Component,
}

impl CapabilitySubjectKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Component => "component",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityOperation {
    Read,
    Write,
    Delete,
    Execute,
    Connect,
    Notify,
}

impl CapabilityOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Delete => "delete",
            Self::Execute => "execute",
            Self::Connect => "connect",
            Self::Notify => "notify",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityScopeKind {
    None,
    Project,
    Library,
    Cache,
    Receive,
    Staging,
    External,
}

impl CapabilityScopeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Project => "project",
            Self::Library => "library",
            Self::Cache => "cache",
            Self::Receive => "receive",
            Self::Staging => "staging",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityDecision {
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityRequestStatus {
    Granted,
    ApprovalRequired,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityScopeRequest {
    pub kind: CapabilityScopeKind,
    pub root_path: Option<String>,
    pub relative_path: Option<String>,
}

impl Default for CapabilityScopeRequest {
    fn default() -> Self {
        Self {
            kind: CapabilityScopeKind::None,
            root_path: None,
            relative_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCapabilityScope {
    pub kind: CapabilityScopeKind,
    pub root_path: Option<String>,
    pub relative_path: Option<String>,
    pub resolved_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityTokenRequest {
    pub subject_kind: CapabilitySubjectKind,
    pub module_id: String,
    pub component_id: Option<String>,
    pub capability: Capability,
    pub operation: CapabilityOperation,
    pub reason: String,
    #[serde(default)]
    pub scope: CapabilityScopeRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityApprovalRequest {
    pub request_id: String,
    pub subject_kind: CapabilitySubjectKind,
    pub subject_id: String,
    pub subject_name: String,
    pub subject_version: String,
    pub module_id: String,
    pub module_name: String,
    pub module_version: String,
    pub capability: Capability,
    pub risk: CapabilityRisk,
    pub operation: CapabilityOperation,
    pub reason: String,
    pub scope: ResolvedCapabilityScope,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityToken {
    pub value: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityTokenResponse {
    pub status: CapabilityRequestStatus,
    pub token: Option<CapabilityToken>,
    pub approval: Option<CapabilityApprovalRequest>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDecisionRequest {
    pub request_id: String,
    pub decision: CapabilityDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGrantRecord {
    pub id: String,
    pub subject_kind: CapabilitySubjectKind,
    pub module_id: String,
    pub module_version: String,
    pub component_id: Option<String>,
    pub component_version: Option<String>,
    pub capability: Capability,
    pub risk: CapabilityRisk,
    pub operation: CapabilityOperation,
    pub scope: ResolvedCapabilityScope,
    pub created_at: i64,
    pub updated_at: i64,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityAuditRecord {
    pub id: i64,
    pub occurred_at: i64,
    pub request_id: Option<String>,
    pub subject_kind: CapabilitySubjectKind,
    pub subject_id: String,
    pub module_id: String,
    pub capability: Capability,
    pub operation: CapabilityOperation,
    pub outcome: String,
    pub reason_code: String,
    pub reason: String,
    pub scope_kind: CapabilityScopeKind,
    pub root_path: Option<String>,
    pub relative_path: Option<String>,
    pub token_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityComponentRegistration {
    pub id: String,
    pub name: String,
    pub version: String,
    pub module_id: String,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDiagnosticScenario {
    pub id: String,
    pub name: String,
    pub description: String,
    pub request: CapabilityTokenRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGatewayOverview {
    pub database_path: String,
    pub token_ttl_ms: i64,
    pub active_token_count: usize,
    pub grants: Vec<CapabilityGrantRecord>,
    pub pending_requests: Vec<CapabilityApprovalRequest>,
    pub recent_audit: Vec<CapabilityAuditRecord>,
    pub diagnostic_scenarios: Vec<CapabilityDiagnosticScenario>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityOperationResult {
    pub success: bool,
    pub operation: CapabilityOperation,
    pub message: String,
    pub resolved_path: Option<String>,
    pub bytes_affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySecurityDiagnosticResult {
    pub success: bool,
    pub checks: usize,
    pub passed: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityErrorCode {
    CapabilityModuleNotRunning,
    CapabilityComponentNotFound,
    CapabilityComponentModuleMismatch,
    CapabilityNotDeclared,
    CapabilityOperationDenied,
    CapabilityApprovalRequired,
    CapabilityRequestNotFound,
    CapabilityRequestExpired,
    CapabilityDenied,
    CapabilityTokenInvalid,
    CapabilityTokenExpired,
    CapabilityTokenUsed,
    CapabilityTokenSubjectMismatch,
    CapabilityTokenScopeMismatch,
    CapabilityPathInvalid,
    CapabilityPathOutsideScope,
    CapabilityPersistenceError,
    CapabilityDiagnosticFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGatewayError {
    pub code: CapabilityErrorCode,
    pub message: String,
    pub details: Vec<String>,
}

impl CapabilityGatewayError {
    pub(super) fn new(code: CapabilityErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }
}

impl std::fmt::Display for CapabilityGatewayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for CapabilityGatewayError {}

#[derive(Debug, Clone)]
struct ValidatedIdentity {
    subject_id: String,
    subject_name: String,
    subject_version: String,
    module_name: String,
    module_version: String,
}

#[derive(Debug, Clone)]
struct PendingCapabilityRequest {
    approval: CapabilityApprovalRequest,
    request: CapabilityTokenRequest,
    scope_hash: String,
}

#[derive(Debug, Clone)]
struct ActiveCapabilityToken {
    token_id: String,
    request_id: String,
    request: CapabilityTokenRequest,
    scope_hash: String,
    expires_at: i64,
    used: bool,
}

pub struct CapabilityGateway {
    manager: Arc<ModuleManager>,
    components: Mutex<BTreeMap<String, CapabilityComponentRegistration>>,
    connection: Mutex<Connection>,
    database_path: PathBuf,
    pending: Mutex<HashMap<String, PendingCapabilityRequest>>,
    tokens: Mutex<HashMap<String, ActiveCapabilityToken>>,
    diagnostic_root: PathBuf,
}

impl CapabilityGateway {
    pub fn new(
        database_path: PathBuf,
        manager: Arc<ModuleManager>,
        components: Vec<CapabilityComponentRegistration>,
        diagnostic_root: PathBuf,
    ) -> Result<Arc<Self>, CapabilityGatewayError> {
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CapabilityGatewayError::new(
                    CapabilityErrorCode::CapabilityPersistenceError,
                    format!("创建权限数据库目录失败: {error}"),
                )
            })?;
        }
        prepare_diagnostic_root(&diagnostic_root)?;
        let connection = Connection::open(&database_path).map_err(persistence_error)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA foreign_keys=ON;
                 CREATE TABLE IF NOT EXISTS capability_schema_meta (
                   id INTEGER PRIMARY KEY CHECK(id = 1),
                   schema_version INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL
                 );
                 INSERT OR IGNORE INTO capability_schema_meta(id, schema_version, updated_at)
                 VALUES (1, 1, 0);
                 CREATE TABLE IF NOT EXISTS capability_grants (
                   id TEXT PRIMARY KEY,
                   subject_kind TEXT NOT NULL,
                   module_id TEXT NOT NULL,
                   module_version TEXT NOT NULL,
                   component_id TEXT NOT NULL DEFAULT '',
                   component_version TEXT NOT NULL DEFAULT '',
                   capability TEXT NOT NULL,
                   operation TEXT NOT NULL,
                   scope_kind TEXT NOT NULL,
                   scope_root TEXT NOT NULL DEFAULT '',
                   scope_relative TEXT NOT NULL DEFAULT '',
                   scope_hash TEXT NOT NULL,
                   created_at INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL,
                   UNIQUE(subject_kind, module_id, module_version, component_id, component_version, capability, operation, scope_hash)
                 );
                 CREATE TABLE IF NOT EXISTS capability_audit (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   occurred_at INTEGER NOT NULL,
                   request_id TEXT,
                   subject_kind TEXT NOT NULL,
                   subject_id TEXT NOT NULL,
                   module_id TEXT NOT NULL,
                   capability TEXT NOT NULL,
                   operation TEXT NOT NULL,
                   outcome TEXT NOT NULL,
                   reason_code TEXT NOT NULL,
                   reason TEXT NOT NULL,
                   scope_kind TEXT NOT NULL,
                   root_path TEXT,
                   relative_path TEXT,
                   token_id TEXT
                 );
                 CREATE INDEX IF NOT EXISTS idx_capability_audit_time ON capability_audit(occurred_at DESC);",
            )
            .map_err(persistence_error)?;
        let components = components
            .into_iter()
            .map(|component| (component.id.clone(), component))
            .collect();
        Ok(Arc::new(Self {
            manager,
            components: Mutex::new(components),
            connection: Mutex::new(connection),
            database_path,
            pending: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            diagnostic_root,
        }))
    }

    pub fn overview(&self) -> Result<CapabilityGatewayOverview, CapabilityGatewayError> {
        self.cleanup_expired();
        Ok(CapabilityGatewayOverview {
            database_path: self.database_path.to_string_lossy().to_string(),
            token_ttl_ms: TOKEN_TTL_MS,
            active_token_count: self
                .tokens
                .lock()
                .expect("capability token mutex poisoned")
                .values()
                .filter(|token| !token.used && token.expires_at > Utc::now().timestamp_millis())
                .count(),
            grants: self.list_grants()?,
            pending_requests: self
                .pending
                .lock()
                .expect("capability request mutex poisoned")
                .values()
                .map(|pending| pending.approval.clone())
                .collect(),
            recent_audit: self.list_audit(AUDIT_LIMIT)?,
            diagnostic_scenarios: self.diagnostic_scenarios(),
        })
    }

    pub fn sync_component_manifests(&self, manifests: &[ComponentManifestV1]) {
        let mut components = self
            .components
            .lock()
            .expect("capability component catalog mutex poisoned");
        components.retain(|_, registration| registration.module_id != "*");
        for manifest in manifests {
            components.insert(
                manifest.id.clone(),
                CapabilityComponentRegistration {
                    id: manifest.id.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    module_id: "*".into(),
                    capabilities: manifest.capabilities.clone(),
                },
            );
        }
    }

    pub fn request_token(
        &self,
        request: CapabilityTokenRequest,
    ) -> Result<CapabilityTokenResponse, CapabilityGatewayError> {
        self.cleanup_expired();
        let identity = self
            .validate_request(&request)
            .map_err(|error| self.record_error(None, &request, error))?;
        let scope = resolve_scope(&request.scope)
            .map_err(|error| self.record_error(None, &request, error))?;
        validate_scope_kind(request.capability, scope.kind)
            .map_err(|error| self.record_error(None, &request, error))?;
        let scope_hash = hash_scope(&scope);
        if self.has_matching_grant(&request, &identity, &scope_hash)? {
            self.audit(
                None,
                &request,
                "granted",
                "PERSISTED_GRANT",
                "匹配到当前版本的长期授权",
                None,
            )?;
            let token = self.issue_token(None, request.clone(), scope_hash);
            return Ok(CapabilityTokenResponse {
                status: CapabilityRequestStatus::Granted,
                token: Some(token),
                approval: None,
                message: "已根据长期授权签发一次性令牌".into(),
            });
        }

        let now = Utc::now().timestamp_millis();
        let request_id = Uuid::new_v4().to_string();
        let approval = CapabilityApprovalRequest {
            request_id: request_id.clone(),
            subject_kind: request.subject_kind,
            subject_id: identity.subject_id,
            subject_name: identity.subject_name,
            subject_version: identity.subject_version,
            module_id: request.module_id.clone(),
            module_name: identity.module_name,
            module_version: identity.module_version,
            capability: request.capability,
            risk: request.capability.risk(),
            operation: request.operation,
            reason: request.reason.clone(),
            scope,
            created_at: now,
            expires_at: now + REQUEST_TTL_MS,
        };
        self.audit(
            Some(&request_id),
            &request,
            "approval-required",
            "CAPABILITY_APPROVAL_REQUIRED",
            "首次使用或版本/范围变化，需要用户批准",
            None,
        )?;
        self.pending
            .lock()
            .expect("capability request mutex poisoned")
            .insert(
                request_id.clone(),
                PendingCapabilityRequest {
                    approval: approval.clone(),
                    request: request.clone(),
                    scope_hash,
                },
            );
        Ok(CapabilityTokenResponse {
            status: CapabilityRequestStatus::ApprovalRequired,
            token: None,
            approval: Some(approval),
            message: "需要用户批准此能力请求".into(),
        })
    }

    pub fn decide(
        &self,
        decision: CapabilityDecisionRequest,
    ) -> Result<CapabilityTokenResponse, CapabilityGatewayError> {
        self.cleanup_expired();
        let pending = self
            .pending
            .lock()
            .expect("capability request mutex poisoned")
            .remove(&decision.request_id)
            .ok_or_else(|| {
                CapabilityGatewayError::new(
                    CapabilityErrorCode::CapabilityRequestNotFound,
                    "权限请求不存在或已经处理",
                )
            })?;
        if pending.approval.expires_at <= Utc::now().timestamp_millis() {
            let error = CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityRequestExpired,
                "权限请求已过期，请重新发起",
            );
            return Err(self.record_error(Some(&decision.request_id), &pending.request, error));
        }
        self.validate_request(&pending.request).map_err(|error| {
            self.record_error(Some(&decision.request_id), &pending.request, error)
        })?;

        if decision.decision == CapabilityDecision::Deny {
            self.audit(
                Some(&decision.request_id),
                &pending.request,
                "denied",
                "USER_DENIED",
                "用户拒绝能力请求",
                None,
            )?;
            return Ok(CapabilityTokenResponse {
                status: CapabilityRequestStatus::Denied,
                token: None,
                approval: Some(pending.approval),
                message: "已拒绝，本次操作未执行".into(),
            });
        }

        if decision.decision == CapabilityDecision::AllowAlways {
            self.persist_grant_and_audit(&pending, &decision.request_id)?;
        } else {
            self.audit(
                Some(&decision.request_id),
                &pending.request,
                "granted",
                "USER_ALLOWED_ONCE",
                "用户仅批准本次操作",
                None,
            )?;
        }
        let token = self.issue_token(
            Some(decision.request_id.clone()),
            pending.request.clone(),
            pending.scope_hash,
        );
        Ok(CapabilityTokenResponse {
            status: CapabilityRequestStatus::Granted,
            token: Some(token),
            approval: Some(pending.approval),
            message: "已签发 60 秒内有效的一次性令牌".into(),
        })
    }

    pub fn revoke_grant(&self, grant_id: &str) -> Result<(), CapabilityGatewayError> {
        let mut connection = self
            .connection
            .lock()
            .expect("capability database mutex poisoned");
        let transaction = connection.transaction().map_err(persistence_error)?;
        let stored = transaction
            .query_row(
                "SELECT subject_kind, module_id, component_id, capability, operation,
                        scope_kind, scope_root, scope_relative
                 FROM capability_grants WHERE id = ?1",
                params![grant_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(persistence_error)?
            .ok_or_else(|| {
                CapabilityGatewayError::new(
                    CapabilityErrorCode::CapabilityRequestNotFound,
                    "授权记录不存在",
                )
            })?;
        let request = CapabilityTokenRequest {
            subject_kind: parse_subject_kind(&stored.0)?,
            module_id: stored.1,
            component_id: (!stored.2.is_empty()).then_some(stored.2),
            capability: Capability::from_name(&stored.3)
                .ok_or_else(|| persistence_value_error("capability", &stored.3))?,
            operation: parse_operation(&stored.4)?,
            reason: "撤销长期授权".into(),
            scope: CapabilityScopeRequest {
                kind: parse_scope_kind(&stored.5)?,
                root_path: (!stored.6.is_empty()).then_some(stored.6),
                relative_path: (!stored.7.is_empty()).then_some(stored.7),
            },
        };
        transaction
            .execute(
                "DELETE FROM capability_grants WHERE id = ?1",
                params![grant_id],
            )
            .map_err(persistence_error)?;
        insert_audit(
            &transaction,
            None,
            &request,
            "revoked",
            "USER_REVOKED",
            "用户撤销长期授权",
            None,
        )?;
        transaction.commit().map_err(persistence_error)?;
        Ok(())
    }

    pub fn consume_token(
        &self,
        token_value: &str,
        request: &CapabilityTokenRequest,
    ) -> Result<ResolvedCapabilityScope, CapabilityGatewayError> {
        self.cleanup_expired();
        self.validate_request(request)
            .map_err(|error| self.record_error(None, request, error))?;
        let scope = resolve_scope(&request.scope)
            .map_err(|error| self.record_error(None, request, error))?;
        validate_scope_kind(request.capability, scope.kind)
            .map_err(|error| self.record_error(None, request, error))?;
        let expected_scope_hash = hash_scope(&scope);
        let mut tokens = self.tokens.lock().expect("capability token mutex poisoned");
        let token = match tokens.get_mut(token_value) {
            Some(token) => token,
            None => {
                let error = CapabilityGatewayError::new(
                    CapabilityErrorCode::CapabilityTokenInvalid,
                    "能力令牌无效或不属于当前进程",
                );
                return Err(self.record_error(None, request, error));
            }
        };
        if token.used {
            let error = CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityTokenUsed,
                "能力令牌已经使用",
            );
            return Err(self.record_error(Some(&token.request_id), request, error));
        }
        if token.expires_at <= Utc::now().timestamp_millis() {
            let error = CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityTokenExpired,
                "能力令牌已过期",
            );
            return Err(self.record_error(Some(&token.request_id), request, error));
        }
        if token.request.subject_kind != request.subject_kind
            || token.request.module_id != request.module_id
            || token.request.component_id != request.component_id
            || token.request.capability != request.capability
            || token.request.operation != request.operation
        {
            let error = CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityTokenSubjectMismatch,
                "能力令牌不能跨模块、组件、能力或操作使用",
            );
            return Err(self.record_error(Some(&token.request_id), request, error));
        }
        if token.scope_hash != expected_scope_hash {
            let error = CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityTokenScopeMismatch,
                "能力令牌不能用于其他路径范围",
            );
            return Err(self.record_error(Some(&token.request_id), request, error));
        }
        token.used = true;
        let request_id = token.request_id.clone();
        let token_id = token.token_id.clone();
        drop(tokens);
        if let Err(error) = self.audit(
            Some(&request_id),
            request,
            "consumed",
            "TOKEN_CONSUMED",
            "能力令牌验证通过并已消费",
            Some(token_id),
        ) {
            if let Some(token) = self
                .tokens
                .lock()
                .expect("capability token mutex poisoned")
                .get_mut(token_value)
            {
                token.used = false;
            }
            return Err(error);
        }
        Ok(scope)
    }

    pub fn run_diagnostic_operation(
        &self,
        token: &str,
        request: CapabilityTokenRequest,
    ) -> Result<CapabilityOperationResult, CapabilityGatewayError> {
        let scope = self.consume_token(token, &request)?;
        let path = scope.resolved_path.as_ref().map(PathBuf::from);
        let (message, bytes_affected) = match request.operation {
            CapabilityOperation::Read => {
                let path = path.ok_or_else(|| diagnostic_error("读取诊断缺少目标路径"))?;
                let bytes = fs::read(&path)
                    .map_err(|error| diagnostic_error(format!("读取诊断文件失败: {error}")))?;
                (format!("读取 {} 字节", bytes.len()), bytes.len() as u64)
            }
            CapabilityOperation::Write => {
                let path = path.ok_or_else(|| diagnostic_error("写入诊断缺少目标路径"))?;
                let payload = format!(
                    "Nexora Capability Gateway diagnostic {}\n",
                    Utc::now().to_rfc3339()
                );
                fs::write(&path, payload.as_bytes())
                    .map_err(|error| diagnostic_error(format!("写入诊断文件失败: {error}")))?;
                (format!("写入 {} 字节", payload.len()), payload.len() as u64)
            }
            CapabilityOperation::Delete => {
                let path = path.ok_or_else(|| diagnostic_error("删除诊断缺少目标路径"))?;
                if path.exists() {
                    fs::remove_file(&path)
                        .map_err(|error| diagnostic_error(format!("删除诊断文件失败: {error}")))?;
                }
                ("诊断文件已删除".into(), 0)
            }
            CapabilityOperation::Execute => ("执行权限验证通过，未启动真实进程".into(), 0),
            CapabilityOperation::Connect => ("网络权限验证通过，未建立真实连接".into(), 0),
            CapabilityOperation::Notify => ("通知权限验证通过，未发送系统通知".into(), 0),
        };
        Ok(CapabilityOperationResult {
            success: true,
            operation: request.operation,
            message,
            resolved_path: scope.resolved_path,
            bytes_affected,
        })
    }

    pub fn diagnostic_scenarios(&self) -> Vec<CapabilityDiagnosticScenario> {
        let project_root = self.diagnostic_root.join("project");
        let staging_root = self.diagnostic_root.join("staging");
        vec![
            CapabilityDiagnosticScenario {
                id: "project-read".into(),
                name: "项目文件读取".into(),
                description: "普通风险，读取诊断项目中的固定样本。".into(),
                request: CapabilityTokenRequest {
                    subject_kind: CapabilitySubjectKind::Component,
                    module_id: "diagnostic.runtime-worker".into(),
                    component_id: Some("diagnostic.component-worker".into()),
                    capability: Capability::ProjectFilesRead,
                    operation: CapabilityOperation::Read,
                    reason: "读取项目文件以验证路径范围和只读授权".into(),
                    scope: CapabilityScopeRequest {
                        kind: CapabilityScopeKind::Project,
                        root_path: Some(project_root.to_string_lossy().to_string()),
                        relative_path: Some("sample.txt".into()),
                    },
                },
            },
            CapabilityDiagnosticScenario {
                id: "project-write".into(),
                name: "项目文件写入".into(),
                description: "敏感风险，只写入应用数据目录中的诊断项目。".into(),
                request: CapabilityTokenRequest {
                    subject_kind: CapabilitySubjectKind::Component,
                    module_id: "diagnostic.runtime-failing".into(),
                    component_id: Some("diagnostic.component-failing".into()),
                    capability: Capability::ProjectFilesWrite,
                    operation: CapabilityOperation::Write,
                    reason: "写入诊断结果以验证敏感权限审批".into(),
                    scope: CapabilityScopeRequest {
                        kind: CapabilityScopeKind::Project,
                        root_path: Some(project_root.to_string_lossy().to_string()),
                        relative_path: Some("result.txt".into()),
                    },
                },
            },
            CapabilityDiagnosticScenario {
                id: "external-write".into(),
                name: "外部暂存写入".into(),
                description: "关键风险，只写入受控 staging 沙箱。".into(),
                request: CapabilityTokenRequest {
                    subject_kind: CapabilitySubjectKind::Component,
                    module_id: "diagnostic.runtime-slow-stop".into(),
                    component_id: Some("diagnostic.component-slow-stop".into()),
                    capability: Capability::FilesystemExternalWrite,
                    operation: CapabilityOperation::Write,
                    reason: "写入 staging 以验证关键权限和范围绑定".into(),
                    scope: CapabilityScopeRequest {
                        kind: CapabilityScopeKind::Staging,
                        root_path: Some(staging_root.to_string_lossy().to_string()),
                        relative_path: Some("critical-result.txt".into()),
                    },
                },
            },
        ]
    }

    fn validate_request(
        &self,
        request: &CapabilityTokenRequest,
    ) -> Result<ValidatedIdentity, CapabilityGatewayError> {
        let module = self.manager.snapshot(&request.module_id).map_err(|_| {
            CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityModuleNotRunning,
                format!("模块 {} 未注册", request.module_id),
            )
        })?;
        if module.state != ModuleState::Running {
            return Err(CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityModuleNotRunning,
                format!(
                    "模块 {} 当前状态为 {:?}，不能申请能力",
                    request.module_id, module.state
                ),
            ));
        }
        if !module.manifest.capabilities.contains(&request.capability) {
            return Err(CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityNotDeclared,
                format!(
                    "模块 {} 未在 manifest 中声明 {}",
                    request.module_id,
                    request.capability.as_str()
                ),
            ));
        }
        if !capability_allows_operation(request.capability, request.operation) {
            return Err(CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityOperationDenied,
                format!(
                    "能力 {} 不允许 {} 操作",
                    request.capability.as_str(),
                    request.operation.as_str()
                ),
            ));
        }
        match request.subject_kind {
            CapabilitySubjectKind::Module => {
                if request.component_id.is_some() {
                    return Err(CapabilityGatewayError::new(
                        CapabilityErrorCode::CapabilityComponentModuleMismatch,
                        "模块主体不能携带 componentId",
                    ));
                }
                Ok(ValidatedIdentity {
                    subject_id: request.module_id.clone(),
                    subject_name: module.manifest.name.clone(),
                    subject_version: module.manifest.version.clone(),
                    module_name: module.manifest.name,
                    module_version: module.manifest.version,
                })
            }
            CapabilitySubjectKind::Component => {
                let component_id = request.component_id.as_ref().ok_or_else(|| {
                    CapabilityGatewayError::new(
                        CapabilityErrorCode::CapabilityComponentNotFound,
                        "组件主体必须提供 componentId",
                    )
                })?;
                let components = self
                    .components
                    .lock()
                    .expect("capability component catalog mutex poisoned");
                let component = components.get(component_id).ok_or_else(|| {
                    CapabilityGatewayError::new(
                        CapabilityErrorCode::CapabilityComponentNotFound,
                        format!("组件 {component_id} 未注册"),
                    )
                })?;
                if component.module_id != "*" && component.module_id != request.module_id {
                    return Err(CapabilityGatewayError::new(
                        CapabilityErrorCode::CapabilityComponentModuleMismatch,
                        format!("组件 {component_id} 不属于模块 {}", request.module_id),
                    ));
                }
                if !component.capabilities.contains(&request.capability) {
                    return Err(CapabilityGatewayError::new(
                        CapabilityErrorCode::CapabilityNotDeclared,
                        format!("组件 {component_id} 未声明 {}", request.capability.as_str()),
                    ));
                }
                Ok(ValidatedIdentity {
                    subject_id: component.id.clone(),
                    subject_name: component.name.clone(),
                    subject_version: component.version.clone(),
                    module_name: module.manifest.name,
                    module_version: module.manifest.version,
                })
            }
        }
    }

    fn issue_token(
        &self,
        request_id: Option<String>,
        request: CapabilityTokenRequest,
        scope_hash: String,
    ) -> CapabilityToken {
        let value = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let expires_at = Utc::now().timestamp_millis() + TOKEN_TTL_MS;
        let token = ActiveCapabilityToken {
            token_id: token_id(&value),
            request_id: request_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            request,
            scope_hash,
            expires_at,
            used: false,
        };
        self.tokens
            .lock()
            .expect("capability token mutex poisoned")
            .insert(value.clone(), token);
        CapabilityToken { value, expires_at }
    }

    fn has_matching_grant(
        &self,
        request: &CapabilityTokenRequest,
        identity: &ValidatedIdentity,
        scope_hash: &str,
    ) -> Result<bool, CapabilityGatewayError> {
        let component_id = request.component_id.as_deref().unwrap_or("");
        let component_version = if request.subject_kind == CapabilitySubjectKind::Component {
            identity.subject_version.as_str()
        } else {
            ""
        };
        self.connection
            .lock()
            .expect("capability database mutex poisoned")
            .query_row(
                "SELECT 1 FROM capability_grants
                 WHERE subject_kind = ?1 AND module_id = ?2 AND module_version = ?3
                   AND component_id = ?4 AND component_version = ?5
                   AND capability = ?6 AND operation = ?7 AND scope_hash = ?8
                 LIMIT 1",
                params![
                    request.subject_kind.as_str(),
                    request.module_id,
                    identity.module_version,
                    component_id,
                    component_version,
                    request.capability.as_str(),
                    request.operation.as_str(),
                    scope_hash,
                ],
                |_| Ok(true),
            )
            .optional()
            .map(|value| value.unwrap_or(false))
            .map_err(persistence_error)
    }

    fn persist_grant_and_audit(
        &self,
        pending: &PendingCapabilityRequest,
        request_id: &str,
    ) -> Result<(), CapabilityGatewayError> {
        let now = Utc::now().timestamp_millis();
        let component_id = pending.request.component_id.as_deref().unwrap_or("");
        let component_version = if pending.request.subject_kind == CapabilitySubjectKind::Component
        {
            pending.approval.subject_version.as_str()
        } else {
            ""
        };
        let mut connection = self
            .connection
            .lock()
            .expect("capability database mutex poisoned");
        let transaction = connection.transaction().map_err(persistence_error)?;
        transaction
            .execute(
                "INSERT INTO capability_grants (
                   id, subject_kind, module_id, module_version, component_id, component_version,
                   capability, operation, scope_kind, scope_root, scope_relative, scope_hash,
                   created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
                 ON CONFLICT(subject_kind, module_id, module_version, component_id, component_version, capability, operation, scope_hash)
                 DO UPDATE SET updated_at = excluded.updated_at",
                params![
                    Uuid::new_v4().to_string(),
                    pending.request.subject_kind.as_str(),
                    pending.request.module_id,
                    pending.approval.module_version,
                    component_id,
                    component_version,
                    pending.request.capability.as_str(),
                    pending.request.operation.as_str(),
                    pending.approval.scope.kind.as_str(),
                    pending.approval.scope.root_path.as_deref().unwrap_or(""),
                    pending.approval.scope.relative_path.as_deref().unwrap_or(""),
                    pending.scope_hash,
                    now,
                ],
            )
            .map_err(persistence_error)?;
        insert_audit(
            &transaction,
            Some(request_id),
            &pending.request,
            "granted",
            "USER_ALLOWED_ALWAYS",
            "用户批准并保存当前版本和范围的授权",
            None,
        )?;
        transaction.commit().map_err(persistence_error)?;
        Ok(())
    }

    fn list_grants(&self) -> Result<Vec<CapabilityGrantRecord>, CapabilityGatewayError> {
        let connection = self
            .connection
            .lock()
            .expect("capability database mutex poisoned");
        let mut statement = connection
            .prepare(
                "SELECT id, subject_kind, module_id, module_version, component_id, component_version,
                        capability, operation, scope_kind, scope_root, scope_relative, created_at, updated_at
                 FROM capability_grants ORDER BY updated_at DESC",
            )
            .map_err(persistence_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                ))
            })
            .map_err(persistence_error)?;
        let mut grants = Vec::new();
        for row in rows {
            let (
                id,
                subject_kind,
                module_id,
                module_version,
                component_id,
                component_version,
                capability,
                operation,
                scope_kind,
                scope_root,
                scope_relative,
                created_at,
                updated_at,
            ) = row.map_err(persistence_error)?;
            let subject_kind = parse_subject_kind(&subject_kind)?;
            let capability = Capability::from_name(&capability).ok_or_else(|| {
                CapabilityGatewayError::new(
                    CapabilityErrorCode::CapabilityPersistenceError,
                    "授权数据库包含未知 Capability",
                )
            })?;
            let operation = parse_operation(&operation)?;
            let scope_kind = parse_scope_kind(&scope_kind)?;
            let valid = self.grant_version_is_current(
                subject_kind,
                &module_id,
                &module_version,
                if component_id.is_empty() {
                    None
                } else {
                    Some(component_id.as_str())
                },
                if component_version.is_empty() {
                    None
                } else {
                    Some(component_version.as_str())
                },
            );
            grants.push(CapabilityGrantRecord {
                id,
                subject_kind,
                module_id,
                module_version,
                component_id: (!component_id.is_empty()).then_some(component_id),
                component_version: (!component_version.is_empty()).then_some(component_version),
                capability,
                risk: capability.risk(),
                operation,
                scope: ResolvedCapabilityScope {
                    kind: scope_kind,
                    root_path: (!scope_root.is_empty()).then_some(scope_root),
                    relative_path: (!scope_relative.is_empty()).then_some(scope_relative),
                    resolved_path: None,
                },
                created_at,
                updated_at,
                valid,
            });
        }
        Ok(grants)
    }

    fn grant_version_is_current(
        &self,
        subject_kind: CapabilitySubjectKind,
        module_id: &str,
        module_version: &str,
        component_id: Option<&str>,
        component_version: Option<&str>,
    ) -> bool {
        let Ok(module) = self.manager.snapshot(module_id) else {
            return false;
        };
        if module.manifest.version != module_version {
            return false;
        }
        if subject_kind == CapabilitySubjectKind::Component {
            let Some(component_id) = component_id else {
                return false;
            };
            let components = self
                .components
                .lock()
                .expect("capability component catalog mutex poisoned");
            let Some(component) = components.get(component_id) else {
                return false;
            };
            return component.version == component_version.unwrap_or_default();
        }
        true
    }

    fn list_audit(
        &self,
        limit: usize,
    ) -> Result<Vec<CapabilityAuditRecord>, CapabilityGatewayError> {
        let connection = self
            .connection
            .lock()
            .expect("capability database mutex poisoned");
        let mut statement = connection
            .prepare(
                "SELECT id, occurred_at, request_id, subject_kind, subject_id, module_id,
                        capability, operation, outcome, reason_code, reason, scope_kind,
                        root_path, relative_path, token_id
                 FROM capability_audit ORDER BY occurred_at DESC, id DESC LIMIT ?1",
            )
            .map_err(persistence_error)?;
        let rows = statement
            .query_map(params![limit as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                ))
            })
            .map_err(persistence_error)?;
        let mut audit = Vec::new();
        for row in rows {
            let row = row.map_err(persistence_error)?;
            audit.push(CapabilityAuditRecord {
                id: row.0,
                occurred_at: row.1,
                request_id: row.2,
                subject_kind: parse_subject_kind(&row.3)?,
                subject_id: row.4,
                module_id: row.5,
                capability: Capability::from_name(&row.6).ok_or_else(|| {
                    CapabilityGatewayError::new(
                        CapabilityErrorCode::CapabilityPersistenceError,
                        "审计数据库包含未知 Capability",
                    )
                })?,
                operation: parse_operation(&row.7)?,
                outcome: row.8,
                reason_code: row.9,
                reason: row.10,
                scope_kind: parse_scope_kind(&row.11)?,
                root_path: row.12,
                relative_path: row.13,
                token_id: row.14,
            });
        }
        Ok(audit)
    }

    fn cleanup_expired(&self) {
        let now = Utc::now().timestamp_millis();
        self.pending
            .lock()
            .expect("capability request mutex poisoned")
            .retain(|_, pending| pending.approval.expires_at + REQUEST_TTL_MS > now);
        self.tokens
            .lock()
            .expect("capability token mutex poisoned")
            .retain(|_, token| token.expires_at + REQUEST_TTL_MS > now);
    }

    fn record_error(
        &self,
        request_id: Option<&str>,
        request: &CapabilityTokenRequest,
        error: CapabilityGatewayError,
    ) -> CapabilityGatewayError {
        match self.audit(
            request_id,
            request,
            "denied",
            capability_error_code(error.code),
            &error.message,
            None,
        ) {
            Ok(()) => error,
            Err(audit_error) => audit_error,
        }
    }

    fn audit(
        &self,
        request_id: Option<&str>,
        request: &CapabilityTokenRequest,
        outcome: &str,
        reason_code: &str,
        reason: &str,
        token_id: Option<String>,
    ) -> Result<(), CapabilityGatewayError> {
        let connection = self
            .connection
            .lock()
            .expect("capability database mutex poisoned");
        insert_audit(
            &connection,
            request_id,
            request,
            outcome,
            reason_code,
            reason,
            token_id,
        )
    }
}

fn insert_audit(
    connection: &Connection,
    request_id: Option<&str>,
    request: &CapabilityTokenRequest,
    outcome: &str,
    reason_code: &str,
    reason: &str,
    token_id: Option<String>,
) -> Result<(), CapabilityGatewayError> {
    let subject_id = request
        .component_id
        .as_deref()
        .unwrap_or(request.module_id.as_str());
    connection
        .execute(
            "INSERT INTO capability_audit (
               occurred_at, request_id, subject_kind, subject_id, module_id, capability,
               operation, outcome, reason_code, reason, scope_kind, root_path, relative_path, token_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                Utc::now().timestamp_millis(),
                request_id,
                request.subject_kind.as_str(),
                subject_id,
                request.module_id,
                request.capability.as_str(),
                request.operation.as_str(),
                outcome,
                reason_code,
                reason,
                request.scope.kind.as_str(),
                request.scope.root_path,
                request.scope.relative_path,
                token_id,
            ],
        )
        .map_err(persistence_error)?;
    connection
        .execute(
            "DELETE FROM capability_audit
             WHERE id NOT IN (SELECT id FROM capability_audit ORDER BY occurred_at DESC, id DESC LIMIT 5000)",
            [],
        )
        .map_err(persistence_error)?;
    Ok(())
}

pub async fn run_security_diagnostic(
    manager: Arc<ModuleManager>,
    gateway: Arc<CapabilityGateway>,
) -> Result<CapabilitySecurityDiagnosticResult, CapabilityGatewayError> {
    let mut checks = 0;
    let mut passed = 0;
    let scenarios = gateway.diagnostic_scenarios();
    let write_request = scenarios
        .iter()
        .find(|scenario| scenario.id == "project-write")
        .ok_or_else(|| diagnostic_error("缺少写入诊断场景"))?
        .request
        .clone();

    checks += 1;
    if gateway
        .request_token(write_request.clone())
        .is_err_and(|error| error.code == CapabilityErrorCode::CapabilityModuleNotRunning)
    {
        passed += 1;
    }

    manager
        .enable_module(&write_request.module_id)
        .await
        .map_err(|error| diagnostic_error(error.to_string()))?;
    let approval = gateway.request_token(write_request.clone())?;
    let request_id = approval
        .approval
        .as_ref()
        .map(|approval| approval.request_id.clone())
        .ok_or_else(|| diagnostic_error("权限请求未进入审批状态"))?;
    let granted = gateway.decide(CapabilityDecisionRequest {
        request_id,
        decision: CapabilityDecision::AllowOnce,
    })?;
    let token = granted
        .token
        .as_ref()
        .map(|token| token.value.clone())
        .ok_or_else(|| diagnostic_error("审批后未签发令牌"))?;

    checks += 1;
    let mut cross_component = write_request.clone();
    cross_component.component_id = Some("diagnostic.component-worker".into());
    if gateway.consume_token(&token, &cross_component).is_err() {
        passed += 1;
    }

    checks += 1;
    gateway.run_diagnostic_operation(&token, write_request.clone())?;
    if gateway
        .consume_token(&token, &write_request)
        .is_err_and(|error| error.code == CapabilityErrorCode::CapabilityTokenUsed)
    {
        passed += 1;
    }

    checks += 1;
    let mut traversal = write_request.clone();
    traversal.scope.relative_path = Some("..\\outside.txt".into());
    if gateway
        .request_token(traversal)
        .is_err_and(|error| error.code == CapabilityErrorCode::CapabilityPathInvalid)
    {
        passed += 1;
    }

    checks += 1;
    let read_request = scenarios
        .iter()
        .find(|scenario| scenario.id == "project-read")
        .ok_or_else(|| diagnostic_error("缺少读取诊断场景"))?
        .request
        .clone();
    manager
        .enable_module(&read_request.module_id)
        .await
        .map_err(|error| diagnostic_error(error.to_string()))?;
    let mut illegal_write = read_request;
    illegal_write.operation = CapabilityOperation::Write;
    if gateway
        .request_token(illegal_write)
        .is_err_and(|error| error.code == CapabilityErrorCode::CapabilityOperationDenied)
    {
        passed += 1;
    }

    let _ = manager.shutdown_all().await;
    let success = checks == passed;
    Ok(CapabilitySecurityDiagnosticResult {
        success,
        checks,
        passed,
        message: if success {
            "模块状态、审批、主体绑定、单次令牌、路径穿越和只读限制检查通过".into()
        } else {
            format!("安全自检仅通过 {passed}/{checks} 项")
        },
    })
}

fn prepare_diagnostic_root(root: &Path) -> Result<(), CapabilityGatewayError> {
    for directory in ["project", "library", "cache", "receive", "staging"] {
        fs::create_dir_all(root.join(directory)).map_err(|error| {
            CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityPersistenceError,
                format!("创建权限诊断目录失败: {error}"),
            )
        })?;
    }
    let sample = root.join("project").join("sample.txt");
    if !sample.exists() {
        fs::write(&sample, b"Nexora Capability Gateway read-only sample\n").map_err(|error| {
            CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityPersistenceError,
                format!("创建权限诊断样本失败: {error}"),
            )
        })?;
    }
    Ok(())
}

fn resolve_scope(
    request: &CapabilityScopeRequest,
) -> Result<ResolvedCapabilityScope, CapabilityGatewayError> {
    if request.kind == CapabilityScopeKind::None {
        if request.root_path.is_some() || request.relative_path.is_some() {
            return Err(CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityPathInvalid,
                "无路径能力不能携带路径范围",
            ));
        }
        return Ok(ResolvedCapabilityScope {
            kind: request.kind,
            root_path: None,
            relative_path: None,
            resolved_path: None,
        });
    }
    let root = request.root_path.as_deref().ok_or_else(|| {
        CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityPathInvalid,
            "路径能力必须提供宿主授权根目录",
        )
    })?;
    let relative = request.relative_path.as_deref().ok_or_else(|| {
        CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityPathInvalid,
            "路径能力必须使用相对路径",
        )
    })?;
    let relative_path = Path::new(relative);
    if relative.trim().is_empty()
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityPathInvalid,
            "组件路径必须是根目录内且不包含 .. 的相对路径",
        ));
    }
    let root_path = PathBuf::from(root);
    if !root_path.is_absolute() {
        return Err(CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityPathInvalid,
            "宿主授权根目录必须是绝对路径",
        ));
    }
    let canonical_root = fs::canonicalize(&root_path).map_err(|error| {
        CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityPathInvalid,
            format!("授权根目录不可用: {error}"),
        )
    })?;
    let target = canonical_root.join(relative_path);
    let mut existing = target.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or_else(|| {
            CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityPathInvalid,
                "无法找到目标路径的现有父目录",
            )
        })?;
    }
    let canonical_existing = fs::canonicalize(existing).map_err(|error| {
        CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityPathInvalid,
            format!("目标路径父目录不可用: {error}"),
        )
    })?;
    if !canonical_existing.starts_with(&canonical_root) {
        return Err(CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityPathOutsideScope,
            "目标路径通过符号链接或重解析点越过授权根目录",
        ));
    }
    if target.exists() {
        let canonical_target = fs::canonicalize(&target).map_err(|error| {
            CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityPathInvalid,
                format!("目标路径不可用: {error}"),
            )
        })?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(CapabilityGatewayError::new(
                CapabilityErrorCode::CapabilityPathOutsideScope,
                "目标路径越过授权根目录",
            ));
        }
    }
    Ok(ResolvedCapabilityScope {
        kind: request.kind,
        root_path: Some(canonical_root.to_string_lossy().to_string()),
        relative_path: Some(relative.to_owned()),
        resolved_path: Some(target.to_string_lossy().to_string()),
    })
}

fn validate_scope_kind(
    capability: Capability,
    kind: CapabilityScopeKind,
) -> Result<(), CapabilityGatewayError> {
    let allowed = match capability {
        Capability::ProjectOpen
        | Capability::ProjectFilesRead
        | Capability::ProjectFilesWrite
        | Capability::ProjectMetadataRead
        | Capability::ProjectMetadataWrite => kind == CapabilityScopeKind::Project,
        Capability::ProjectStorageRead
        | Capability::ProjectStorageWrite
        | Capability::ProjectStorageDirect => {
            matches!(kind, CapabilityScopeKind::Project | CapabilityScopeKind::None)
        }
        Capability::CacheInspect | Capability::CacheMaintain => kind == CapabilityScopeKind::Cache,
        Capability::FilesystemExternalRead | Capability::FilesystemExternalWrite => matches!(
            kind,
            CapabilityScopeKind::Library
                | CapabilityScopeKind::Receive
                | CapabilityScopeKind::Staging
                | CapabilityScopeKind::External
        ),
        _ => kind == CapabilityScopeKind::None,
    };
    if allowed {
        Ok(())
    } else {
        Err(CapabilityGatewayError::new(
            CapabilityErrorCode::CapabilityTokenScopeMismatch,
            format!(
                "能力 {} 不允许使用 {} 路径范围",
                capability.as_str(),
                kind.as_str()
            ),
        ))
    }
}

fn capability_allows_operation(capability: Capability, operation: CapabilityOperation) -> bool {
    use Capability::*;
    use CapabilityOperation::*;
    match capability {
        AppProfileRead
        | AppSettingsRead
        | ClipboardRead
        | FilesystemExternalRead
        | ProjectOpen
        | ProjectFilesRead
        | ProjectMetadataRead
        | ProjectStorageRead
        | CacheInspect
        | RenderInspect
        | RenderQueueRead => matches!(operation, Read),
        AppProfileWrite | AppSettingsWrite | ClipboardWrite | ProjectFilesWrite
        | ProjectMetadataWrite | ProjectStorageWrite | RenderQueueWrite | RenderResultCommit => {
            matches!(operation, Write | Delete)
        }
        ProjectStorageDirect => matches!(operation, Read),
        FilesystemExternalWrite | CacheMaintain => matches!(operation, Write | Delete),
        FilesystemDialogOpen | TaskRun | TaskCancel | PythonExecute | PythonPackagesManage
        | ProcessSpawn | RenderWorkerExecute => matches!(operation, Execute),
        NetworkHttpRequest | NetworkLanDiscover | NetworkLanMessage | NetworkLanTransfer
        | NetworkServerConnect => matches!(operation, Connect),
        NotificationSend => matches!(operation, Notify),
    }
}

fn hash_scope(scope: &ResolvedCapabilityScope) -> String {
    let material = format!(
        "{}\n{}\n{}",
        scope.kind.as_str(),
        scope.root_path.as_deref().unwrap_or(""),
        scope.relative_path.as_deref().unwrap_or("")
    );
    blake3::hash(material.as_bytes()).to_hex().to_string()
}

fn token_id(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex()[..16].to_string()
}

fn capability_error_code(code: CapabilityErrorCode) -> &'static str {
    match code {
        CapabilityErrorCode::CapabilityModuleNotRunning => "CAPABILITY_MODULE_NOT_RUNNING",
        CapabilityErrorCode::CapabilityComponentNotFound => "CAPABILITY_COMPONENT_NOT_FOUND",
        CapabilityErrorCode::CapabilityComponentModuleMismatch => {
            "CAPABILITY_COMPONENT_MODULE_MISMATCH"
        }
        CapabilityErrorCode::CapabilityNotDeclared => "CAPABILITY_NOT_DECLARED",
        CapabilityErrorCode::CapabilityOperationDenied => "CAPABILITY_OPERATION_DENIED",
        CapabilityErrorCode::CapabilityApprovalRequired => "CAPABILITY_APPROVAL_REQUIRED",
        CapabilityErrorCode::CapabilityRequestNotFound => "CAPABILITY_REQUEST_NOT_FOUND",
        CapabilityErrorCode::CapabilityRequestExpired => "CAPABILITY_REQUEST_EXPIRED",
        CapabilityErrorCode::CapabilityDenied => "CAPABILITY_DENIED",
        CapabilityErrorCode::CapabilityTokenInvalid => "CAPABILITY_TOKEN_INVALID",
        CapabilityErrorCode::CapabilityTokenExpired => "CAPABILITY_TOKEN_EXPIRED",
        CapabilityErrorCode::CapabilityTokenUsed => "CAPABILITY_TOKEN_USED",
        CapabilityErrorCode::CapabilityTokenSubjectMismatch => "CAPABILITY_TOKEN_SUBJECT_MISMATCH",
        CapabilityErrorCode::CapabilityTokenScopeMismatch => "CAPABILITY_TOKEN_SCOPE_MISMATCH",
        CapabilityErrorCode::CapabilityPathInvalid => "CAPABILITY_PATH_INVALID",
        CapabilityErrorCode::CapabilityPathOutsideScope => "CAPABILITY_PATH_OUTSIDE_SCOPE",
        CapabilityErrorCode::CapabilityPersistenceError => "CAPABILITY_PERSISTENCE_ERROR",
        CapabilityErrorCode::CapabilityDiagnosticFailed => "CAPABILITY_DIAGNOSTIC_FAILED",
    }
}

fn parse_subject_kind(value: &str) -> Result<CapabilitySubjectKind, CapabilityGatewayError> {
    match value {
        "module" => Ok(CapabilitySubjectKind::Module),
        "component" => Ok(CapabilitySubjectKind::Component),
        _ => Err(persistence_value_error("subject kind", value)),
    }
}

fn parse_operation(value: &str) -> Result<CapabilityOperation, CapabilityGatewayError> {
    match value {
        "read" => Ok(CapabilityOperation::Read),
        "write" => Ok(CapabilityOperation::Write),
        "delete" => Ok(CapabilityOperation::Delete),
        "execute" => Ok(CapabilityOperation::Execute),
        "connect" => Ok(CapabilityOperation::Connect),
        "notify" => Ok(CapabilityOperation::Notify),
        _ => Err(persistence_value_error("operation", value)),
    }
}

fn parse_scope_kind(value: &str) -> Result<CapabilityScopeKind, CapabilityGatewayError> {
    match value {
        "none" => Ok(CapabilityScopeKind::None),
        "project" => Ok(CapabilityScopeKind::Project),
        "library" => Ok(CapabilityScopeKind::Library),
        "cache" => Ok(CapabilityScopeKind::Cache),
        "receive" => Ok(CapabilityScopeKind::Receive),
        "staging" => Ok(CapabilityScopeKind::Staging),
        "external" => Ok(CapabilityScopeKind::External),
        _ => Err(persistence_value_error("scope kind", value)),
    }
}

fn persistence_value_error(kind: &str, value: &str) -> CapabilityGatewayError {
    CapabilityGatewayError::new(
        CapabilityErrorCode::CapabilityPersistenceError,
        format!("权限数据库包含未知 {kind}: {value}"),
    )
}

fn persistence_error(error: rusqlite::Error) -> CapabilityGatewayError {
    CapabilityGatewayError::new(
        CapabilityErrorCode::CapabilityPersistenceError,
        format!("权限数据库操作失败: {error}"),
    )
}

fn diagnostic_error(message: impl Into<String>) -> CapabilityGatewayError {
    CapabilityGatewayError::new(CapabilityErrorCode::CapabilityDiagnosticFailed, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::builtin_modules::{diagnostic_components, diagnostic_modules};
    use crate::platform::module_manager::StopStrategy;
    use crate::platform::DiagnosticControls;
    use std::sync::Arc;

    fn temporary_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pm-center-capability-{name}-{}", Uuid::new_v4()))
    }

    fn runtime(name: &str) -> (PathBuf, Arc<ModuleManager>, Arc<CapabilityGateway>) {
        let root = temporary_root(name);
        fs::create_dir_all(&root).unwrap();
        let controls = Arc::new(DiagnosticControls::default());
        let manager = ModuleManager::new(
            root.join("module-runtime.json"),
            diagnostic_modules(controls),
        )
        .unwrap();
        let gateway = CapabilityGateway::new(
            root.join("capability-gateway.db"),
            manager.clone(),
            diagnostic_components(),
            root.join("diagnostics"),
        )
        .unwrap();
        (root, manager, gateway)
    }

    #[tokio::test]
    async fn disabled_module_and_undeclared_capability_are_denied() {
        let (root, manager, gateway) = runtime("state-declaration");
        let request = gateway.diagnostic_scenarios()[1].request.clone();
        let error = gateway.request_token(request.clone()).unwrap_err();
        assert_eq!(error.code, CapabilityErrorCode::CapabilityModuleNotRunning);

        manager.enable_module(&request.module_id).await.unwrap();
        let mut undeclared = request;
        undeclared.capability = Capability::ProcessSpawn;
        undeclared.operation = CapabilityOperation::Execute;
        undeclared.scope = CapabilityScopeRequest::default();
        let error = gateway.request_token(undeclared).unwrap_err();
        assert_eq!(error.code, CapabilityErrorCode::CapabilityNotDeclared);
        let _ = manager.shutdown_all().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn one_time_token_is_bound_to_subject_scope_and_single_use() {
        let (root, manager, gateway) = runtime("token-binding");
        let request = gateway.diagnostic_scenarios()[1].request.clone();
        manager.enable_module(&request.module_id).await.unwrap();
        let approval = gateway.request_token(request.clone()).unwrap();
        let granted = gateway
            .decide(CapabilityDecisionRequest {
                request_id: approval.approval.unwrap().request_id,
                decision: CapabilityDecision::AllowOnce,
            })
            .unwrap();
        let token = granted.token.unwrap().value;

        let mut different_scope = request.clone();
        different_scope.scope.relative_path = Some("other.txt".into());
        assert_eq!(
            gateway
                .consume_token(&token, &different_scope)
                .unwrap_err()
                .code,
            CapabilityErrorCode::CapabilityTokenScopeMismatch
        );
        gateway
            .run_diagnostic_operation(&token, request.clone())
            .unwrap();
        assert_eq!(
            gateway.consume_token(&token, &request).unwrap_err().code,
            CapabilityErrorCode::CapabilityTokenUsed
        );

        let approval = gateway.request_token(request.clone()).unwrap();
        let expired = gateway
            .decide(CapabilityDecisionRequest {
                request_id: approval.approval.unwrap().request_id,
                decision: CapabilityDecision::AllowOnce,
            })
            .unwrap()
            .token
            .unwrap()
            .value;
        gateway
            .tokens
            .lock()
            .unwrap()
            .get_mut(&expired)
            .unwrap()
            .expires_at = Utc::now().timestamp_millis() - 1;
        assert_eq!(
            gateway.consume_token(&expired, &request).unwrap_err().code,
            CapabilityErrorCode::CapabilityTokenExpired
        );
        let _ = manager.shutdown_all().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn token_cannot_be_forged_or_used_by_a_sibling_component() {
        let root = temporary_root("cross-component");
        fs::create_dir_all(&root).unwrap();
        let controls = Arc::new(DiagnosticControls::default());
        let manager = ModuleManager::new(
            root.join("module-runtime.json"),
            diagnostic_modules(controls),
        )
        .unwrap();
        let mut components = diagnostic_components();
        components.push(CapabilityComponentRegistration {
            id: "diagnostic.component-failing-sibling".into(),
            name: "同模块写入诊断组件".into(),
            version: "1.0.0".into(),
            module_id: "diagnostic.runtime-failing".into(),
            capabilities: vec![Capability::ProjectFilesWrite],
        });
        let gateway = CapabilityGateway::new(
            root.join("capability-gateway.db"),
            manager.clone(),
            components,
            root.join("diagnostics"),
        )
        .unwrap();
        let request = gateway.diagnostic_scenarios()[1].request.clone();
        manager.enable_module(&request.module_id).await.unwrap();
        assert_eq!(
            gateway
                .consume_token("forged-token", &request)
                .unwrap_err()
                .code,
            CapabilityErrorCode::CapabilityTokenInvalid
        );

        let approval = gateway.request_token(request.clone()).unwrap();
        let token = gateway
            .decide(CapabilityDecisionRequest {
                request_id: approval.approval.unwrap().request_id,
                decision: CapabilityDecision::AllowOnce,
            })
            .unwrap()
            .token
            .unwrap()
            .value;
        let mut sibling_request = request;
        sibling_request.component_id = Some("diagnostic.component-failing-sibling".into());
        assert_eq!(
            gateway
                .consume_token(&token, &sibling_request)
                .unwrap_err()
                .code,
            CapabilityErrorCode::CapabilityTokenSubjectMismatch
        );
        let _ = manager.shutdown_all().await;
        drop(gateway);
        drop(manager);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn user_denial_does_not_create_the_target_file() {
        let (root, manager, gateway) = runtime("user-denial");
        let request = gateway.diagnostic_scenarios()[1].request.clone();
        manager.enable_module(&request.module_id).await.unwrap();
        let target = PathBuf::from(request.scope.root_path.as_ref().unwrap())
            .join(request.scope.relative_path.as_ref().unwrap());
        assert!(!target.exists());
        let approval = gateway.request_token(request).unwrap();
        let response = gateway
            .decide(CapabilityDecisionRequest {
                request_id: approval.approval.unwrap().request_id,
                decision: CapabilityDecision::Deny,
            })
            .unwrap();
        assert_eq!(response.status, CapabilityRequestStatus::Denied);
        assert!(!target.exists());
        assert!(gateway
            .overview()
            .unwrap()
            .recent_audit
            .iter()
            .any(|record| record.reason_code == "USER_DENIED"));
        let _ = manager.shutdown_all().await;
        drop(gateway);
        drop(manager);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn traversal_and_absolute_component_paths_are_rejected_without_mutation() {
        let (root, manager, gateway) = runtime("path-boundary");
        let request = gateway.diagnostic_scenarios()[1].request.clone();
        manager.enable_module(&request.module_id).await.unwrap();
        let outside = root.join("outside.txt");
        fs::write(&outside, b"protected").unwrap();

        for relative in ["..\\outside.txt", "C:\\Windows\\system.ini"] {
            let mut invalid = request.clone();
            invalid.scope.relative_path = Some(relative.into());
            assert_eq!(
                gateway.request_token(invalid).unwrap_err().code,
                CapabilityErrorCode::CapabilityPathInvalid
            );
        }
        assert_eq!(fs::read(&outside).unwrap(), b"protected");
        let _ = manager.shutdown_all().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn read_capability_cannot_write_or_delete() {
        let (root, manager, gateway) = runtime("read-only");
        let request = gateway.diagnostic_scenarios()[0].request.clone();
        manager.enable_module(&request.module_id).await.unwrap();
        for operation in [CapabilityOperation::Write, CapabilityOperation::Delete] {
            let mut invalid = request.clone();
            invalid.operation = operation;
            assert_eq!(
                gateway.request_token(invalid).unwrap_err().code,
                CapabilityErrorCode::CapabilityOperationDenied
            );
        }
        let _ = manager.shutdown_all().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn persisted_grant_is_reused_and_audited() {
        let (root, manager, gateway) = runtime("persistent-grant");
        let request = gateway.diagnostic_scenarios()[1].request.clone();
        manager.enable_module(&request.module_id).await.unwrap();
        let approval = gateway.request_token(request.clone()).unwrap();
        gateway
            .decide(CapabilityDecisionRequest {
                request_id: approval.approval.unwrap().request_id,
                decision: CapabilityDecision::AllowAlways,
            })
            .unwrap();

        let reused = gateway.request_token(request).unwrap();
        assert_eq!(reused.status, CapabilityRequestStatus::Granted);
        let overview = gateway.overview().unwrap();
        assert_eq!(overview.grants.len(), 1);
        assert!(overview.grants[0].valid);
        assert!(!overview.recent_audit.is_empty());
        let _ = manager.shutdown_all().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn component_version_change_requires_new_approval() {
        let (root, manager, gateway) = runtime("version-change");
        let request = gateway.diagnostic_scenarios()[1].request.clone();
        manager.enable_module(&request.module_id).await.unwrap();
        let approval = gateway.request_token(request.clone()).unwrap();
        gateway
            .decide(CapabilityDecisionRequest {
                request_id: approval.approval.unwrap().request_id,
                decision: CapabilityDecision::AllowAlways,
            })
            .unwrap();
        drop(gateway);

        let mut components = diagnostic_components();
        components
            .iter_mut()
            .find(|component| component.id == "diagnostic.component-failing")
            .unwrap()
            .version = "2.0.0".into();
        let upgraded = CapabilityGateway::new(
            root.join("capability-gateway.db"),
            manager.clone(),
            components,
            root.join("diagnostics"),
        )
        .unwrap();
        assert_eq!(
            upgraded.request_token(request).unwrap().status,
            CapabilityRequestStatus::ApprovalRequired
        );
        assert!(!upgraded.overview().unwrap().grants[0].valid);
        let _ = manager.shutdown_all().await;
        drop(upgraded);
        drop(manager);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn security_diagnostic_covers_core_denial_paths() {
        let (root, manager, gateway) = runtime("self-test");
        let result = run_security_diagnostic(manager.clone(), gateway)
            .await
            .unwrap();
        assert!(result.success, "{}", result.message);
        assert_eq!(result.checks, result.passed);
        let _ = manager
            .disable_module("diagnostic.runtime-base", StopStrategy::Force)
            .await;
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn symlink_escape_is_rejected_when_symlink_creation_is_available() {
        use std::os::windows::fs::symlink_dir;

        let root = temporary_root("symlink");
        let allowed = root.join("allowed");
        let outside = root.join("outside");
        fs::create_dir_all(&allowed).unwrap();
        fs::create_dir_all(&outside).unwrap();
        if symlink_dir(&outside, allowed.join("escape")).is_err() {
            let _ = fs::remove_dir_all(root);
            return;
        }
        let error = resolve_scope(&CapabilityScopeRequest {
            kind: CapabilityScopeKind::Project,
            root_path: Some(allowed.to_string_lossy().to_string()),
            relative_path: Some("escape\\payload.txt".into()),
        })
        .unwrap_err();
        assert_eq!(error.code, CapabilityErrorCode::CapabilityPathOutsideScope);
        let _ = fs::remove_dir_all(root);
    }
}
