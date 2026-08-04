mod builtin_modules;
mod capability_gateway;
mod module_manager;
mod resource_registry;

pub use module_manager::{
    DisablePreview, ModuleDiagnosticSnapshot, ModuleManager, ModuleManagerError,
    ModuleRuntimeOverview, ModuleState, StopStrategy,
};

pub use builtin_modules::{
    AUTOMATION_RUNTIME_MODULE_ID, PROJECT_RESOURCES_MODULE_ID, SMART_CLIPBOARD_MODULE_ID,
};

pub use capability_gateway::{
    CapabilityDecisionRequest, CapabilityGatewayError, CapabilityGatewayOverview,
    CapabilityOperationResult, CapabilitySecurityDiagnosticResult, CapabilityTokenRequest,
    CapabilityTokenResponse,
};

use builtin_modules::{
    automation_runtime_component, automation_runtime_module, diagnostic_components,
    diagnostic_modules, lan_collaboration_component, lan_collaboration_module,
    project_resources_component, project_resources_module, smart_clipboard_component,
    smart_clipboard_module, DiagnosticControls, DIAGNOSTIC_BASE_ID,
};
use capability_gateway::{run_security_diagnostic, CapabilityGateway};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

pub struct PlatformRuntime {
    pub manager: Arc<ModuleManager>,
    pub gateway: Arc<CapabilityGateway>,
    controls: Arc<DiagnosticControls>,
}

impl PlatformRuntime {
    pub fn initialize(
        app_data_dir: &Path,
        app_handle: tauri::AppHandle,
        project_databases: crate::project_resources::ProjectDatabaseState,
    ) -> Result<Self, ModuleManagerError> {
        crate::project_resources::initialize_lifecycle_control();
        crate::automation_runtime::initialize_lifecycle_control();
        let controls = Arc::new(DiagnosticControls::default());
        let mut modules = diagnostic_modules(controls.clone());
        modules.push(smart_clipboard_module(app_data_dir.to_path_buf()));
        modules.push(lan_collaboration_module(
            app_data_dir.to_path_buf(),
            app_handle,
        ));
        modules.push(project_resources_module(project_databases));
        modules.push(automation_runtime_module());
        let manager = ModuleManager::new(app_data_dir.join("module-runtime.json"), modules)?;
        let project_resources_desired = manager
            .snapshot(PROJECT_RESOURCES_MODULE_ID)?
            .desired_enabled;
        crate::project_resources::set_initial_desired_enabled(project_resources_desired);
        let automation_runtime_desired = manager
            .snapshot(AUTOMATION_RUNTIME_MODULE_ID)?
            .desired_enabled;
        crate::automation_runtime::set_initial_desired_enabled(automation_runtime_desired);
        let mut components = diagnostic_components();
        components.push(smart_clipboard_component());
        components.push(lan_collaboration_component());
        components.push(project_resources_component());
        components.push(automation_runtime_component());
        let gateway = CapabilityGateway::new(
            app_data_dir.join("capability-gateway.db"),
            manager.clone(),
            components,
            app_data_dir.join("capability-diagnostics"),
        )
        .map_err(|error| {
            ModuleManagerError::new(
                module_manager::ModuleErrorCode::ModulePersistenceError,
                None,
                error.to_string(),
            )
        })?;
        Ok(Self {
            manager,
            gateway,
            controls,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleFailureInjectionRequest {
    pub module_id: String,
    pub point: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleFailureInjectionResult {
    pub module_id: String,
    pub point: String,
    pub enabled: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformDiagnosticAction {
    CycleLeakTest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformDiagnosticResult {
    pub action: String,
    pub success: bool,
    pub iterations: usize,
    pub leaked_resources: usize,
    pub active_tasks: usize,
    pub message: String,
}

#[tauri::command]
pub fn list_platform_modules(runtime: State<'_, PlatformRuntime>) -> ModuleRuntimeOverview {
    runtime.manager.overview()
}

#[tauri::command]
pub fn get_platform_module(
    module_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
    runtime.manager.snapshot(&module_id)
}

#[tauri::command]
pub fn preview_disable_platform_module(
    module_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<DisablePreview, ModuleManagerError> {
    runtime.manager.preview_disable(&module_id)
}

#[tauri::command]
pub async fn enable_platform_module(
    module_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
    runtime.manager.enable_module(&module_id).await
}

#[tauri::command]
pub async fn disable_platform_module(
    module_id: String,
    strategy: StopStrategy,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
    runtime.manager.disable_module(&module_id, strategy).await
}

#[tauri::command]
pub async fn restart_platform_module(
    module_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ModuleDiagnosticSnapshot, ModuleManagerError> {
    runtime.manager.restart_module(&module_id).await
}

#[tauri::command]
pub async fn run_platform_module_health_check(
    module_id: Option<String>,
    runtime: State<'_, PlatformRuntime>,
) -> Result<Vec<ModuleDiagnosticSnapshot>, ModuleManagerError> {
    runtime
        .manager
        .run_health_checks(module_id.as_deref())
        .await
}

#[tauri::command]
pub fn configure_platform_module_failure(
    request: ModuleFailureInjectionRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ModuleFailureInjectionResult, ModuleManagerError> {
    runtime
        .controls
        .configure(&request.module_id, &request.point, request.enabled)
        .map_err(|message| {
            ModuleManagerError::new(
                module_manager::ModuleErrorCode::ModuleDiagnosticActionUnknown,
                Some(&request.module_id),
                message,
            )
        })?;
    Ok(ModuleFailureInjectionResult {
        module_id: request.module_id,
        point: request.point,
        enabled: request.enabled,
        message: if request.enabled {
            "故障注入已启用".into()
        } else {
            "故障注入已关闭".into()
        },
    })
}

#[tauri::command]
pub fn get_platform_module_failure_injections(
    runtime: State<'_, PlatformRuntime>,
) -> BTreeMap<String, bool> {
    runtime.controls.failure_injections()
}

#[tauri::command]
pub async fn run_platform_module_diagnostic(
    action: PlatformDiagnosticAction,
) -> Result<PlatformDiagnosticResult, ModuleManagerError> {
    match action {
        PlatformDiagnosticAction::CycleLeakTest => run_cycle_leak_test().await,
    }
}

#[tauri::command]
pub fn get_capability_gateway_overview(
    runtime: State<'_, PlatformRuntime>,
) -> Result<CapabilityGatewayOverview, CapabilityGatewayError> {
    runtime.gateway.overview()
}

#[tauri::command]
pub fn request_platform_capability(
    request: CapabilityTokenRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<CapabilityTokenResponse, CapabilityGatewayError> {
    runtime.gateway.request_token(request)
}

#[tauri::command]
pub fn decide_platform_capability(
    request: CapabilityDecisionRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<CapabilityTokenResponse, CapabilityGatewayError> {
    runtime.gateway.decide(request)
}

#[tauri::command]
pub fn revoke_platform_capability_grant(
    grant_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<(), CapabilityGatewayError> {
    runtime.gateway.revoke_grant(&grant_id)
}

#[tauri::command]
pub fn run_platform_capability_operation(
    token: String,
    request: CapabilityTokenRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<CapabilityOperationResult, CapabilityGatewayError> {
    runtime.gateway.run_diagnostic_operation(&token, request)
}

#[tauri::command]
pub async fn run_platform_capability_diagnostic(
) -> Result<CapabilitySecurityDiagnosticResult, CapabilityGatewayError> {
    let root = std::env::temp_dir().join(format!(
        "pm-center-capability-diagnostic-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).map_err(|error| {
        CapabilityGatewayError::new(
            capability_gateway::CapabilityErrorCode::CapabilityDiagnosticFailed,
            format!("创建权限安全自检目录失败: {error}"),
        )
    })?;
    let controls = Arc::new(DiagnosticControls::default());
    let manager = ModuleManager::new(
        root.join("module-runtime.json"),
        diagnostic_modules(controls),
    )
    .map_err(|error| {
        CapabilityGatewayError::new(
            capability_gateway::CapabilityErrorCode::CapabilityDiagnosticFailed,
            error.to_string(),
        )
    })?;
    let gateway = CapabilityGateway::new(
        root.join("capability-gateway.db"),
        manager.clone(),
        diagnostic_components(),
        root.join("diagnostics"),
    )?;
    let result = run_security_diagnostic(manager.clone(), gateway.clone()).await;
    let _ = manager.shutdown_all().await;
    drop(gateway);
    drop(manager);
    let _ = std::fs::remove_dir_all(root);
    result
}

async fn run_cycle_leak_test() -> Result<PlatformDiagnosticResult, ModuleManagerError> {
    let controls = Arc::new(DiagnosticControls::default());
    let state_path =
        std::env::temp_dir().join(format!("pm-center-module-cycle-{}.json", Uuid::new_v4()));
    let manager = ModuleManager::new(state_path.clone(), diagnostic_modules(controls.clone()))?;
    let mut max_leaked = 0;
    for _ in 0..100 {
        manager.enable_module(DIAGNOSTIC_BASE_ID).await?;
        manager
            .disable_module(DIAGNOSTIC_BASE_ID, StopStrategy::Graceful)
            .await?;
        max_leaked = max_leaked.max(manager.resources().total_count());
    }
    let active_tasks = controls.active_tasks();
    let leaked_resources = manager.resources().total_count();
    let _ = manager.shutdown_all().await;
    let _ = std::fs::remove_file(state_path);
    let success = leaked_resources == 0 && active_tasks == 0 && max_leaked == 0;
    Ok(PlatformDiagnosticResult {
        action: "cycleLeakTest".into(),
        success,
        iterations: 100,
        leaked_resources,
        active_tasks,
        message: if success {
            "连续启停 100 次完成，未发现登记资源或诊断任务泄漏".into()
        } else {
            format!(
                "检测到残留：resources={leaked_resources}, tasks={active_tasks}, max={max_leaked}"
            )
        },
    })
}
