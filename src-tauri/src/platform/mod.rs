mod builtin_modules;
mod module_manager;
mod resource_registry;

pub use module_manager::{
    DisablePreview, ModuleDiagnosticSnapshot, ModuleManager, ModuleManagerError,
    ModuleRuntimeOverview, StopStrategy,
};

use builtin_modules::{diagnostic_modules, DiagnosticControls, DIAGNOSTIC_BASE_ID};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

pub struct PlatformRuntime {
    pub manager: Arc<ModuleManager>,
    controls: Arc<DiagnosticControls>,
}

impl PlatformRuntime {
    pub fn initialize(app_data_dir: &Path) -> Result<Self, ModuleManagerError> {
        let controls = Arc::new(DiagnosticControls::default());
        let manager = ModuleManager::new(
            app_data_dir.join("module-runtime.json"),
            diagnostic_modules(controls.clone()),
        )?;
        Ok(Self { manager, controls })
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
