mod builtin_components;
mod builtin_modules;
mod capability_gateway;
mod component_runtime;
mod component_settings;
mod file_router;
pub(crate) mod file_operations;
mod module_manager;
mod presentation_templates;
mod profile_package;
mod profile_runtime;
#[cfg(test)]
mod reference_assemblies;
mod resource_registry;
mod script_automation;

pub use builtin_modules::{
    AUTOMATION_RUNTIME_MODULE_ID, LOCAL_WEB_CONSOLE_MODULE_ID, MEDIA_LIBRARY_MODULE_ID,
    PROJECT_RESOURCES_MODULE_ID, RENDER_CENTER_MODULE_ID, RENDER_FARM_MODULE_ID,
    SMART_CLIPBOARD_MODULE_ID,
};
pub use module_manager::{
    DisablePreview, ModuleDiagnosticSnapshot, ModuleManager, ModuleManagerError,
    ModuleRuntimeOverview, ModuleState, StopStrategy,
};
pub use profile_package::{
    ExportWorkspacePackageRequest, ExportWorkspaceProfilePackageRequest,
    ImportWorkspacePackageRequest, ImportWorkspaceProfilePackageRequest,
    ProfilePackageExportResult, ProfilePackageImportPreview, WorkspacePackageImportPreview,
    WorkspacePackageImportResult,
};
pub use profile_runtime::{
    CreateWorkspaceProfileRequest, InitializeWorkspaceProfileRuntimeRequest,
    PreviewWorkspaceProfileSwitchRequest, SaveWorkspaceProfileRequest,
    SwitchWorkspaceProfileRequest, WorkspaceProfileDraftValidation, WorkspaceProfileMutationResult,
    WorkspaceProfileRuntime, WorkspaceProfileRuntimeError, WorkspaceProfileRuntimeErrorCode,
    WorkspaceProfileRuntimeSnapshot, WorkspaceProfileSwitchPhase, WorkspaceProfileSwitchPreview,
    WorkspaceProfileSwitchResult,
};
pub use script_automation::{
    AutomationEventRequest, AutomationRun, AutomationRunFilter, AutomationRuntimeSnapshot,
    CreateScriptComponentTemplateRequest, RemoveAutomationBindingRequest,
    ResolveAutomationAttentionRequest, SaveAutomationBindingRequest,
    SaveScriptDevelopmentFileRequest, ScriptComponentValidation, ScriptDevelopmentDocument,
    ScriptDevelopmentFile, ScriptSurfaceDocument, StartAutomationRunRequest,
};

pub use capability_gateway::{
    CapabilityDecisionRequest, CapabilityGatewayError, CapabilityGatewayOverview,
    CapabilityOperationResult, CapabilitySecurityDiagnosticResult, CapabilityTokenRequest,
    CapabilityTokenResponse,
};
pub use component_runtime::{
    ComponentInvocationRequest, ComponentInvocationResult, ComponentPackageInspection,
    ComponentPackagePublisher, ComponentRuntimeError, ComponentRuntimeErrorCode,
    ComponentRuntimeManager, ComponentRuntimeOverview, InstallComponentPackageRequest,
    InstallComponentRequest,
};
pub use component_settings::{
    ComponentSettingsRequest, ComponentSettingsSnapshot, SaveComponentSettingsRequest,
};
pub use file_router::{FileRoutePlan, FileRouteRequest};
pub use file_router::{
    FileRoutingScopeContext, FileRoutingSnapshot, RemoveFileAssociationBindingRequest,
    SetFileAssociationBindingRequest,
};
pub use presentation_templates::{PresentationTemplatePreview, PresentationTemplatePreviewRequest};

use crate::component_packager::{ComponentPackRequest, ComponentPackResult, SigningKeyResult};
use builtin_components::builtin_component_manifests;
use builtin_modules::{
    automation_runtime_component, automation_runtime_module, desktop_integration_module,
    diagnostic_components, diagnostic_modules, external_tools_module, lan_collaboration_component,
    lan_collaboration_module, local_web_console_component, local_web_console_module,
    media_library_module, project_manager_module, project_resources_component,
    project_resources_module, render_center_component, render_center_module, render_farm_component,
    render_farm_module, script_automation_module, session_runtime_module, settings_center_module,
    smart_clipboard_component, smart_clipboard_module, DiagnosticControls, DIAGNOSTIC_BASE_ID,
};
use capability_gateway::{run_security_diagnostic, CapabilityGateway};
use component_settings::ComponentSettingsStore;
use file_router::FileRoutingStore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

pub struct PlatformRuntime {
    pub manager: Arc<ModuleManager>,
    pub gateway: Arc<CapabilityGateway>,
    pub components: Arc<ComponentRuntimeManager>,
    pub profiles: Arc<WorkspaceProfileRuntime>,
    pub script_automation: Arc<script_automation::ScriptAutomationRuntime>,
    component_settings: ComponentSettingsStore,
    file_routing: FileRoutingStore,
    profile_switch_lock: tokio::sync::Mutex<()>,
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
        crate::media_library::initialize_lifecycle_control();
        crate::render_farm::initialize_lifecycle_control();
        crate::render_center::initialize_lifecycle_control();
        let components = ComponentRuntimeManager::new(
            app_data_dir,
            app_handle.clone(),
            builtin_component_manifests(),
        )
        .map_err(|error| {
            ModuleManagerError::new(
                module_manager::ModuleErrorCode::ModulePersistenceError,
                None,
                error.to_string(),
            )
        })?;
        let script_automation = script_automation::ScriptAutomationRuntime::new(
            app_data_dir,
            app_handle.clone(),
            components.clone(),
        )
        .map_err(|error| {
            ModuleManagerError::new(
                module_manager::ModuleErrorCode::ModulePersistenceError,
                None,
                error,
            )
        })?;
        let controls = Arc::new(DiagnosticControls::default());
        let mut modules = diagnostic_modules(controls.clone());
        modules.push(settings_center_module());
        modules.push(session_runtime_module());
        modules.push(desktop_integration_module());
        modules.push(external_tools_module());
        modules.push(smart_clipboard_module(app_data_dir.to_path_buf()));
        modules.push(lan_collaboration_module(
            app_data_dir.to_path_buf(),
            app_handle.clone(),
        ));
        modules.push(local_web_console_module(
            app_data_dir.to_path_buf(),
            app_handle.clone(),
        ));
        modules.push(project_resources_module(project_databases));
        modules.push(project_manager_module());
        modules.push(automation_runtime_module());
        modules.push(script_automation_module(script_automation.clone()));
        modules.push(media_library_module());
        modules.push(render_center_module());
        modules.push(render_farm_module());
        let manager = ModuleManager::new(app_data_dir.join("module-runtime.json"), modules)?;
        let project_resources_desired = manager
            .snapshot(PROJECT_RESOURCES_MODULE_ID)?
            .desired_enabled;
        crate::project_resources::set_initial_desired_enabled(project_resources_desired);
        let automation_runtime_desired = manager
            .snapshot(AUTOMATION_RUNTIME_MODULE_ID)?
            .desired_enabled;
        crate::automation_runtime::set_initial_desired_enabled(automation_runtime_desired);
        let media_library_desired = manager.snapshot(MEDIA_LIBRARY_MODULE_ID)?.desired_enabled;
        crate::media_library::set_initial_desired_enabled(media_library_desired);
        let render_center_desired = manager.snapshot(RENDER_CENTER_MODULE_ID)?.desired_enabled;
        crate::render_center::set_initial_desired_enabled(render_center_desired);
        let render_farm_desired = manager.snapshot(RENDER_FARM_MODULE_ID)?.desired_enabled;
        crate::render_farm::set_initial_desired_enabled(render_farm_desired);
        let mut capability_components = diagnostic_components();
        capability_components.push(smart_clipboard_component());
        capability_components.push(lan_collaboration_component());
        capability_components.push(local_web_console_component());
        capability_components.push(project_resources_component());
        capability_components.push(automation_runtime_component());
        capability_components.push(render_center_component());
        capability_components.push(render_farm_component());
        let gateway = CapabilityGateway::new(
            app_data_dir.join("capability-gateway.db"),
            manager.clone(),
            capability_components,
            app_data_dir.join("capability-diagnostics"),
        )
        .map_err(|error| {
            ModuleManagerError::new(
                module_manager::ModuleErrorCode::ModulePersistenceError,
                None,
                error.to_string(),
            )
        })?;
        let active_component_manifests = components.manifests();
        gateway.sync_component_manifests(&active_component_manifests);
        let profiles = Arc::new(WorkspaceProfileRuntime::new_with_components(
            app_data_dir,
            components.installed_manifests(),
        ));
        script_automation.attach_host(manager.clone(), profiles.clone(), gateway.clone());
        Ok(Self {
            manager,
            gateway,
            components,
            profiles,
            script_automation,
            component_settings: ComponentSettingsStore::new(app_data_dir),
            file_routing: FileRoutingStore::new(app_data_dir),
            profile_switch_lock: tokio::sync::Mutex::new(()),
            controls,
        })
    }
}

fn sync_component_catalog(runtime: &PlatformRuntime) -> Result<(), ComponentRuntimeError> {
    let active_manifests = runtime.components.manifests();
    let installed_manifests = runtime.components.installed_manifests();
    runtime
        .profiles
        .replace_component_manifests(installed_manifests)
        .map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentDependencyConflict,
                error.message,
            )
            .with_details(error.details)
        })?;
    runtime.gateway.sync_component_manifests(&active_manifests);
    Ok(())
}

#[tauri::command]
pub fn get_component_runtime_overview(
    runtime: State<'_, PlatformRuntime>,
) -> ComponentRuntimeOverview {
    runtime.components.overview()
}

#[tauri::command]
pub fn route_file_intent(
    mut request: FileRouteRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<FileRoutePlan, ComponentRuntimeError> {
    let module_manifests = runtime
        .manager
        .overview()
        .modules
        .into_iter()
        .map(|module| module.manifest)
        .collect::<Vec<_>>();
    let (profile_id, effective_component_ids) = runtime
        .profiles
        .current_effective_component_ids(&module_manifests)
        .map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentDependencyConflict,
                error.message,
            )
            .with_details(error.details)
        })?;
    request.profile_id = Some(profile_id);
    file_router::route_file(
        &runtime.components,
        &runtime.file_routing,
        &effective_component_ids,
        request,
    )
}

#[tauri::command]
pub fn get_file_routing_snapshot(
    context: Option<FileRoutingScopeContext>,
    runtime: State<'_, PlatformRuntime>,
) -> Result<FileRoutingSnapshot, ComponentRuntimeError> {
    runtime.file_routing.snapshot(context.unwrap_or_default())
}

#[tauri::command]
pub fn set_file_association_binding(
    request: SetFileAssociationBindingRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<(), ComponentRuntimeError> {
    runtime.file_routing.set(request.binding)
}

#[tauri::command]
pub fn remove_file_association_binding(
    request: RemoveFileAssociationBindingRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<(), ComponentRuntimeError> {
    runtime.file_routing.remove(request)
}

#[tauri::command]
pub fn get_file_route_trace(
    route_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<FileRoutePlan, ComponentRuntimeError> {
    runtime.file_routing.route_trace(&route_id).ok_or_else(|| {
        ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentOperationNotFound,
            format!("文件路由追踪不存在或已过期: {route_id}"),
        )
    })
}

#[tauri::command]
pub fn inspect_component_package(
    package_path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ComponentPackageInspection, ComponentRuntimeError> {
    runtime.components.inspect_package(&package_path)
}

#[tauri::command]
pub fn trust_component_package_publisher(
    package_path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ComponentPackagePublisher, ComponentRuntimeError> {
    runtime.components.trust_package_publisher(&package_path)
}

#[tauri::command]
pub fn get_presentation_template_preview(
    request: PresentationTemplatePreviewRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<PresentationTemplatePreview, ComponentRuntimeError> {
    runtime.components.presentation_template_preview(&request)
}

#[tauri::command]
pub async fn install_component_from_directory(
    request: InstallComponentRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    let validation = runtime
        .script_automation
        .validate_development_component(&request.source_path);
    if validation.manifest.as_ref().is_some_and(|manifest| {
        !manifest.contributes.automation_commands.is_empty()
            || !manifest.contributes.script_surfaces.is_empty()
    }) && !runtime
        .script_automation
        .is_trusted_directory(&request.source_path)
    {
        return Err(ComponentRuntimeError::new(
            ComponentRuntimeErrorCode::ComponentPackageUntrusted,
            "脚本组件目录尚未在开发模式中显式信任",
        )
        .with_details(validation.warnings));
    }
    let manifest = runtime.components.install_from_directory(&request).await?;
    sync_component_catalog(&runtime)?;
    Ok(manifest)
}

#[tauri::command]
pub async fn install_component_from_package(
    request: InstallComponentPackageRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    let manifest = runtime.components.install_from_package(&request).await?;
    sync_component_catalog(&runtime)?;
    Ok(manifest)
}

#[tauri::command]
pub async fn disable_component(
    component_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    let _profile_guard = runtime.profile_switch_lock.lock().await;
    runtime
        .script_automation
        .prepare_component_uninstall(&component_id)
        .await
        .map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentDependencyConflict,
                error,
            )
        })?;
    let manifest = runtime.components.disable(&component_id).await?;
    sync_component_catalog(&runtime)?;
    Ok(manifest)
}

#[tauri::command]
pub async fn enable_component(
    component_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    let _profile_guard = runtime.profile_switch_lock.lock().await;
    let manifest = runtime.components.enable(&component_id).await?;
    sync_component_catalog(&runtime)?;
    Ok(manifest)
}

#[tauri::command]
pub async fn delete_component(
    component_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    let _profile_guard = runtime.profile_switch_lock.lock().await;
    runtime
        .script_automation
        .prepare_component_uninstall(&component_id)
        .await
        .map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentDependencyConflict,
                error,
            )
        })?;
    let manifest = runtime
        .components
        .stored_manifest(&component_id)
        .ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentNotInstalled,
                format!("组件 {component_id} 未安装"),
            )
        })?;
    let module_manifests = formal_module_manifests(&runtime);
    let profile_rollback = runtime
        .profiles
        .remove_component_references(&manifest, &module_manifests)
        .map_err(|error| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentDependencyConflict,
                error.message,
            )
            .with_details(error.details)
        })?;
    let handler_ids = manifest
        .contributes
        .file_handlers
        .iter()
        .map(|handler| handler.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let routing_rollback = match runtime.file_routing.remove_component_handlers(&handler_ids) {
        Ok(rollback) => rollback,
        Err(mut error) => {
            if let Err(rollback_error) = runtime
                .profiles
                .rollback_component_reference_removal(profile_rollback)
            {
                error
                    .details
                    .push(format!("恢复装配方案失败：{}", rollback_error.message));
                error.details.extend(rollback_error.details);
            }
            return Err(error);
        }
    };
    if let Err(mut error) = runtime.components.delete(&component_id).await {
        if let Err(rollback_error) = runtime
            .file_routing
            .rollback_component_handler_removal(routing_rollback)
        {
            error
                .details
                .push(format!("恢复文件打开绑定失败：{}", rollback_error.message));
            error.details.extend(rollback_error.details);
        }
        if let Err(rollback_error) = runtime
            .profiles
            .rollback_component_reference_removal(profile_rollback)
        {
            error
                .details
                .push(format!("恢复装配方案失败：{}", rollback_error.message));
            error.details.extend(rollback_error.details);
        }
        return Err(error);
    }
    sync_component_catalog(&runtime)?;
    Ok(manifest)
}

#[tauri::command]
pub async fn uninstall_component(
    component_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    if runtime.components.is_bundled(&component_id) {
        disable_component(component_id, runtime).await
    } else {
        delete_component(component_id, runtime).await
    }
}

#[tauri::command]
pub async fn reinstall_bundled_component(
    component_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    enable_component(component_id, runtime).await
}

#[tauri::command]
pub async fn invoke_component_command(
    request: ComponentInvocationRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ComponentInvocationResult, ComponentRuntimeError> {
    let manifest = runtime
        .components
        .manifest(&request.component_id)
        .ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentNotInstalled,
                format!("组件 {} 未安装", request.component_id),
            )
        })?;
    if !manifest.capabilities.is_empty() {
        let capability_request = request.capability_request.as_ref().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentCapabilityRequired,
                "该组件命令需要先通过 CapabilityGateway 获取一次性令牌",
            )
        })?;
        if capability_request.module_id != request.module_id
            || capability_request.component_id.as_deref() != Some(request.component_id.as_str())
        {
            return Err(ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentCapabilityRequired,
                "组件调用与能力令牌的模块或组件主体不一致",
            ));
        }
        let token = request.capability_token.as_deref().ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentCapabilityRequired,
                "组件命令缺少一次性能力令牌",
            )
        })?;
        runtime
            .gateway
            .consume_token(token, capability_request)
            .map_err(|error| {
                ComponentRuntimeError::new(
                    ComponentRuntimeErrorCode::ComponentCapabilityRequired,
                    error.message,
                )
            })?;
    }
    runtime.components.invoke(request).await
}

#[tauri::command]
pub fn cancel_component_operation(
    operation_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<(), ComponentRuntimeError> {
    runtime.components.cancel(&operation_id)
}

#[tauri::command]
pub fn validate_script_component(
    path: String,
    runtime: State<'_, PlatformRuntime>,
) -> ScriptComponentValidation {
    runtime
        .script_automation
        .validate_development_component(&path)
}

#[tauri::command]
pub fn get_development_component_snapshot(
    runtime: State<'_, PlatformRuntime>,
) -> Vec<script_automation::DevelopmentComponentSnapshot> {
    runtime.script_automation.development_component_snapshot()
}

#[tauri::command]
pub fn trust_script_development_directory(
    path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<Vec<String>, String> {
    runtime.script_automation.trust_development_directory(&path)
}

#[tauri::command]
pub fn untrust_script_development_directory(
    path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<Vec<String>, String> {
    runtime
        .script_automation
        .untrust_development_directory(&path)
}

#[tauri::command]
pub async fn reload_script_component(
    component_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::ComponentManifestV1, ComponentRuntimeError> {
    runtime
        .script_automation
        .ensure_development_reload_safe(&component_id)
        .map_err(|message| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentOperationConflict,
                message,
            )
        })?;
    let source = runtime
        .script_automation
        .trusted_directories()
        .into_iter()
        .find(|path| {
            runtime
                .script_automation
                .validate_development_component(path)
                .manifest
                .is_some_and(|manifest| manifest.id == component_id)
        })
        .ok_or_else(|| {
            ComponentRuntimeError::new(
                ComponentRuntimeErrorCode::ComponentPackageUntrusted,
                "没有找到该组件对应的受信任开发目录",
            )
        })?;
    let manifest = runtime
        .components
        .install_from_directory(&InstallComponentRequest {
            source_path: source,
        })
        .await?;
    sync_component_catalog(&runtime)?;
    Ok(manifest)
}

#[tauri::command]
pub async fn reload_development_components(
    only_dirty: bool,
    runtime: State<'_, PlatformRuntime>,
) -> Result<script_automation::DevelopmentReloadResult, String> {
    let snapshots = runtime.script_automation.development_component_snapshot();
    let mut result = script_automation::DevelopmentReloadResult {
        reloaded: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
    };
    for snapshot in snapshots {
        let label = snapshot
            .component_id
            .clone()
            .unwrap_or_else(|| snapshot.source_path.clone());
        if !snapshot.valid || !snapshot.trusted {
            result.errors.push(format!(
                "{label}: {}",
                if snapshot.errors.is_empty() {
                    "开发目录无效或未受信任".into()
                } else {
                    snapshot.errors.join("；")
                }
            ));
            continue;
        }
        if only_dirty && !snapshot.dirty {
            result.skipped.push(label);
            continue;
        }
        let Some(component_id) = snapshot.component_id else {
            result
                .errors
                .push(format!("{}: 缺少组件 ID", snapshot.source_path));
            continue;
        };
        if let Err(error) = runtime
            .script_automation
            .ensure_development_reload_safe(&component_id)
        {
            result.errors.push(format!("{component_id}: {error}"));
            continue;
        }
        match runtime
            .components
            .install_from_directory(&InstallComponentRequest {
                source_path: snapshot.source_path,
            })
            .await
        {
            Ok(_) => result.reloaded.push(component_id),
            Err(error) => result
                .errors
                .push(format!("{component_id}: {}", error.message)),
        }
    }
    if !result.reloaded.is_empty() {
        sync_component_catalog(&runtime).map_err(|error| error.message)?;
    }
    Ok(result)
}

#[tauri::command]
pub fn create_script_component_template(
    request: CreateScriptComponentTemplateRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<String, String> {
    runtime.script_automation.create_template(request)
}

#[tauri::command]
pub fn open_script_development_directory_in_vscode(
    path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<(), String> {
    runtime
        .script_automation
        .open_development_directory_in_vscode(&path)
}

#[tauri::command]
pub fn generate_script_signing_key(path: String) -> Result<SigningKeyResult, String> {
    crate::component_packager::generate_signing_key(std::path::Path::new(&path))
}

#[tauri::command]
pub fn package_script_component(
    request: ComponentPackRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ComponentPackResult, String> {
    if !runtime
        .script_automation
        .is_trusted_directory(&request.source_path)
    {
        return Err("只有已信任的脚本开发目录可以生成可执行组件包".into());
    }
    let validation = runtime
        .script_automation
        .validate_development_component(&request.source_path);
    if !validation.valid {
        return Err(format!("组件合同无效：{}", validation.errors.join("；")));
    }
    crate::component_packager::pack_component(request)
}

#[tauri::command]
pub fn list_script_development_files(
    path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<Vec<ScriptDevelopmentFile>, String> {
    runtime.script_automation.list_development_files(&path)
}

#[tauri::command]
pub fn read_script_development_file(
    path: String,
    relative_path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ScriptDevelopmentDocument, String> {
    runtime
        .script_automation
        .read_development_file(&path, &relative_path)
}

#[tauri::command]
pub fn save_script_development_file(
    request: SaveScriptDevelopmentFileRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ScriptDevelopmentDocument, String> {
    runtime.script_automation.save_development_file(request)
}

#[tauri::command]
pub fn get_script_surface_document(
    component_id: String,
    surface_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ScriptSurfaceDocument, String> {
    runtime
        .script_automation
        .script_surface_document(&component_id, &surface_id)
}

#[tauri::command]
pub fn get_automation_runtime_snapshot(
    runtime: State<'_, PlatformRuntime>,
) -> Result<AutomationRuntimeSnapshot, String> {
    runtime.script_automation.snapshot()
}

#[tauri::command]
pub fn list_automation_runs(
    filter: Option<AutomationRunFilter>,
    runtime: State<'_, PlatformRuntime>,
) -> Result<Vec<AutomationRun>, String> {
    runtime
        .script_automation
        .list_runs(filter.unwrap_or_default())
}

#[tauri::command]
pub fn get_automation_run(
    run_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<AutomationRun, String> {
    runtime.script_automation.get_run(&run_id)
}

#[tauri::command]
pub fn start_automation_run(
    request: StartAutomationRunRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<AutomationRun, String> {
    runtime.script_automation.start_run(request)
}

#[tauri::command]
pub fn cancel_automation_run(
    run_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<AutomationRun, String> {
    runtime.script_automation.cancel_run(&run_id)
}

#[tauri::command]
pub fn retry_automation_run(
    run_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<AutomationRun, String> {
    runtime.script_automation.retry_run(&run_id)
}

#[tauri::command]
pub fn resolve_automation_attention(
    request: ResolveAutomationAttentionRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<AutomationRun, String> {
    runtime.script_automation.resolve_attention(request)
}

#[tauri::command]
pub fn emit_automation_event(
    request: AutomationEventRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<usize, String> {
    runtime.script_automation.emit_event(request)
}

#[tauri::command]
pub fn list_automation_bindings(
    profile_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<Vec<pmc_platform::ProfileAutomationBinding>, WorkspaceProfileRuntimeError> {
    Ok(runtime
        .profiles
        .profile_document(&profile_id)?
        .automation_bindings)
}

#[tauri::command]
pub async fn save_automation_binding(
    request: SaveAutomationBindingRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    let mut profile = runtime.profiles.profile_document(&request.profile_id)?;
    if let Some(existing) = profile
        .automation_bindings
        .iter_mut()
        .find(|binding| binding.id == request.binding.id)
    {
        *existing = request.binding;
    } else {
        profile.automation_bindings.push(request.binding);
    }
    let save_request = SaveWorkspaceProfileRequest {
        profile,
        expected_revision: request.expected_revision,
    };
    let current_id = runtime.profiles.snapshot(&manifests)?.current_profile.id;
    if current_id == request.profile_id {
        let prepared = runtime
            .profiles
            .prepare_current_profile_update(&save_request, &manifests)?;
        runtime
            .profiles
            .commit_current_profile_update(prepared, &manifests)
    } else {
        runtime.profiles.save_profile(&save_request, &manifests)
    }
}

#[tauri::command]
pub async fn remove_automation_binding(
    request: RemoveAutomationBindingRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    let mut profile = runtime.profiles.profile_document(&request.profile_id)?;
    profile
        .automation_bindings
        .retain(|binding| binding.id != request.binding_id);
    let save_request = SaveWorkspaceProfileRequest {
        profile,
        expected_revision: request.expected_revision,
    };
    let current_id = runtime.profiles.snapshot(&manifests)?.current_profile.id;
    if current_id == request.profile_id {
        let prepared = runtime
            .profiles
            .prepare_current_profile_update(&save_request, &manifests)?;
        runtime
            .profiles
            .commit_current_profile_update(prepared, &manifests)
    } else {
        runtime.profiles.save_profile(&save_request, &manifests)
    }
}

#[tauri::command]
pub fn get_component_settings(
    request: ComponentSettingsRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ComponentSettingsSnapshot, String> {
    let manifest = runtime
        .profiles
        .component_manifest(&request.component_id)
        .ok_or_else(|| format!("组件未安装: {}", request.component_id))?;
    runtime.component_settings.get(&manifest, &request)
}

#[tauri::command]
pub fn save_component_settings(
    request: SaveComponentSettingsRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ComponentSettingsSnapshot, String> {
    let manifest = runtime
        .profiles
        .component_manifest(&request.target.component_id)
        .ok_or_else(|| format!("组件未安装: {}", request.target.component_id))?;
    runtime.component_settings.save(&manifest, &request)
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
pub async fn initialize_workspace_profile_runtime(
    request: InitializeWorkspaceProfileRuntimeRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let overview = runtime.manager.overview();
    let manifests = overview
        .modules
        .iter()
        .filter(|module| !module.diagnostic)
        .map(|module| module.manifest.clone())
        .collect::<Vec<_>>();
    let enabled_module_ids = overview
        .modules
        .iter()
        .filter(|module| !module.diagnostic && module.desired_enabled)
        .map(|module| module.manifest.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    runtime.profiles.initialize_from_current_configuration(
        &manifests,
        &enabled_module_ids,
        &request.legacy_pinned_tools,
    )?;
    let recovery_plan = runtime.profiles.startup_recovery_plan(&manifests)?;
    let target_module_ids = profile_module_ids(&recovery_plan.profile);
    let runtime_drift_corrected = !runtime
        .manager
        .profile_module_set_matches(&target_module_ids)
        .map_err(profile_switch_module_error)?;
    runtime
        .manager
        .apply_profile_module_set(&target_module_ids)
        .await
        .map_err(profile_switch_module_error)?;
    runtime
        .profiles
        .complete_startup_recovery(&recovery_plan, runtime_drift_corrected)?;
    let snapshot = runtime.profiles.snapshot(&manifests)?;
    if let Err(error) = runtime.script_automation.emit_app_started_once() {
        eprintln!("[script-automation] app.started dispatch failed: {error}");
    }
    Ok(snapshot)
}

#[tauri::command]
pub fn get_workspace_profile_runtime(
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
    let manifests = runtime
        .manager
        .overview()
        .modules
        .into_iter()
        .filter(|module| !module.diagnostic)
        .map(|module| module.manifest)
        .collect::<Vec<_>>();
    runtime.profiles.snapshot(&manifests)
}

#[tauri::command]
pub fn get_workspace_profile_document(
    profile_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<pmc_platform::WorkspaceProfileV1, WorkspaceProfileRuntimeError> {
    runtime.profiles.profile_document(&profile_id)
}

#[tauri::command]
pub fn validate_workspace_profile_draft(
    profile: pmc_platform::WorkspaceProfileV1,
    runtime: State<'_, PlatformRuntime>,
) -> WorkspaceProfileDraftValidation {
    let manifests = formal_module_manifests(&runtime);
    runtime.profiles.validate_draft(&profile, &manifests)
}

#[tauri::command]
pub async fn create_workspace_profile(
    request: CreateWorkspaceProfileRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    runtime.profiles.create_profile(&request, &manifests)
}

#[tauri::command]
pub async fn save_workspace_profile(
    request: SaveWorkspaceProfileRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    runtime.profiles.save_profile(&request, &manifests)
}

#[tauri::command]
pub async fn apply_current_workspace_profile(
    request: SaveWorkspaceProfileRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    let prepared = runtime
        .profiles
        .prepare_current_profile_update(&request, &manifests)?;
    let target_module_ids = profile_module_ids(&prepared.profile);
    let previous_modules = runtime
        .manager
        .capture_profile_module_state()
        .map_err(profile_switch_module_error)?;
    runtime
        .manager
        .apply_profile_module_set(&target_module_ids)
        .await
        .map_err(profile_switch_module_error)?;

    match runtime
        .profiles
        .commit_current_profile_update(prepared, &manifests)
    {
        Ok(result) => Ok(result),
        Err(mut commit_error) => {
            if let Err(rollback_error) = runtime
                .manager
                .restore_profile_module_state(&previous_modules)
                .await
            {
                commit_error
                    .details
                    .push(format!("模块状态回滚失败：{}", rollback_error.message));
            }
            Err(commit_error)
        }
    }
}

#[tauri::command]
pub fn export_workspace_profile_package(
    request: ExportWorkspaceProfilePackageRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ProfilePackageExportResult, WorkspaceProfileRuntimeError> {
    let profile = runtime.profiles.profile_document(&request.profile_id)?;
    profile_package::export_profile_package(&profile, &request.destination_path)
}

#[tauri::command]
pub fn inspect_workspace_profile_package(
    package_path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ProfilePackageImportPreview, WorkspaceProfileRuntimeError> {
    let manifests = formal_module_manifests(&runtime);
    profile_package::inspect_profile_package(&package_path, &runtime.profiles, &manifests)
}

#[tauri::command]
pub async fn import_workspace_profile_package(
    request: ImportWorkspaceProfilePackageRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    profile_package::import_profile_package(&request, &runtime.profiles, &manifests)
}

#[tauri::command]
pub fn export_workspace_package(
    request: ExportWorkspacePackageRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ProfilePackageExportResult, WorkspaceProfileRuntimeError> {
    let profile = runtime.profiles.profile_document(&request.profile_id)?;
    profile_package::export_workspace_package_with_components(
        &profile,
        &request,
        &runtime.components,
    )
}

#[tauri::command]
pub fn inspect_workspace_package(
    package_path: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspacePackageImportPreview, WorkspaceProfileRuntimeError> {
    let manifests = formal_module_manifests(&runtime);
    profile_package::inspect_workspace_package_with_components(
        &package_path,
        &runtime.profiles,
        &manifests,
        &runtime.components,
    )
}

#[tauri::command]
pub async fn import_workspace_package(
    request: ImportWorkspacePackageRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspacePackageImportResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    let imported = profile_package::import_workspace_package_with_components(
        &request,
        &runtime.profiles,
        &manifests,
        &runtime.components,
    )
    .await?;
    sync_component_catalog(&runtime).map_err(|error| {
        WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("装配空间已导入，但同步组件目录失败：{}", error.message),
            None,
        )
        .with_details(error.details)
    })?;
    Ok(imported)
}

#[tauri::command]
pub async fn delete_workspace_profile(
    profile_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = formal_module_manifests(&runtime);
    runtime.profiles.delete_profile(&profile_id, &manifests)
}

#[tauri::command]
pub fn preview_workspace_profile_switch(
    request: PreviewWorkspaceProfileSwitchRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileSwitchPreview, WorkspaceProfileRuntimeError> {
    runtime
        .profiles
        .preview_switch(&request, &runtime.manager.overview().modules)
}

#[tauri::command]
pub async fn switch_workspace_profile(
    request: SwitchWorkspaceProfileRequest,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileSwitchResult, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let preview_request = PreviewWorkspaceProfileSwitchRequest {
        profile_id: request.profile_id.clone(),
        current_pinned_tools: request.current_pinned_tools.clone(),
        known_tool_contributions: request.known_tool_contributions.clone(),
    };
    let overview = runtime.manager.overview();
    let preview = runtime
        .profiles
        .preview_switch(&preview_request, &overview.modules)?;
    if preview.current_profile_id != request.expected_current_profile_id {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileSwitchStale,
            format!(
                "装配方案预览已经过期：预期 {}，实际 {}",
                request.expected_current_profile_id, preview.current_profile_id
            ),
            None,
        ));
    }
    if !preview.can_switch {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileSwitchBlocked,
            "目标装配方案存在阻塞项，未修改任何状态",
            None,
        )
        .with_details(
            preview
                .issues
                .iter()
                .filter(|issue| {
                    issue.severity == profile_runtime::WorkspaceProfileSwitchIssueSeverity::Error
                })
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect(),
        ));
    }
    let _automation_trigger_guard = runtime
        .script_automation
        .prepare_profile_switch()
        .await
        .map_err(|message| {
            WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileSwitchBlocked,
                message,
                None,
            )
        })?;

    let manifests = overview
        .modules
        .iter()
        .filter(|module| !module.diagnostic)
        .map(|module| module.manifest.clone())
        .collect::<Vec<_>>();
    let target = runtime.profiles.profile(&request.profile_id, &manifests)?;
    let target_module_ids = profile_module_ids(&target);
    let previous_modules = runtime
        .manager
        .capture_profile_module_state()
        .map_err(profile_switch_module_error)?;
    let transaction = runtime.profiles.begin_switch(
        &request.profile_id,
        &request.expected_current_profile_id,
        target.revision,
    )?;
    if let Err(error) = runtime
        .manager
        .apply_profile_module_set(&target_module_ids)
        .await
    {
        let mut mapped = profile_switch_module_error(error);
        if let Err(abort_error) = runtime.profiles.abort_switch(&transaction.transaction_id) {
            mapped
                .details
                .push(format!("journalCleanup={}", abort_error.message));
        }
        return Err(mapped);
    }
    if let Err(phase_error) = runtime.profiles.mark_switch_phase(
        &transaction.transaction_id,
        WorkspaceProfileSwitchPhase::ModulesApplied,
    ) {
        let rollback_result = runtime
            .manager
            .restore_profile_module_state(&previous_modules)
            .await;
        let _ = runtime.profiles.abort_switch(&transaction.transaction_id);
        return match rollback_result {
            Ok(()) => Err(phase_error.with_details(vec!["模块状态已恢复到切换前状态".into()])),
            Err(rollback_error) => Err(WorkspaceProfileRuntimeError::new(
                WorkspaceProfileRuntimeErrorCode::ProfileRollbackFailed,
                "切换日志更新失败，模块回滚也未完整完成",
                None,
            )
            .with_details(vec![
                format!("journal={}", phase_error.message),
                format!("rollback={}", rollback_error.message),
            ])),
        };
    }

    let snapshot = match runtime.profiles.commit_switch(
        &request.profile_id,
        &request.expected_current_profile_id,
        &manifests,
    ) {
        Ok(snapshot) => snapshot,
        Err(commit_error) => {
            let result = match runtime
                .manager
                .restore_profile_module_state(&previous_modules)
                .await
            {
                Ok(()) => Err(commit_error.with_details(vec!["模块状态已恢复到切换前状态".into()])),
                Err(rollback_error) => Err(WorkspaceProfileRuntimeError::new(
                    WorkspaceProfileRuntimeErrorCode::ProfileRollbackFailed,
                    "装配方案状态提交失败，模块回滚也未完整完成",
                    None,
                )
                .with_details(vec![
                    format!("commit={}", commit_error.message),
                    format!("rollback={}", rollback_error.message),
                ])),
            };
            let _ = runtime.profiles.abort_switch(&transaction.transaction_id);
            return result;
        }
    };

    if let Err(error) = runtime.profiles.mark_switch_phase(
        &transaction.transaction_id,
        WorkspaceProfileSwitchPhase::ProfileCommitted,
    ) {
        eprintln!(
            "[platform] Profile 已提交，但切换日志阶段更新失败，将在下次启动恢复: {}",
            error.message
        );
    }

    Ok(WorkspaceProfileSwitchResult {
        transaction_id: transaction.transaction_id,
        preview,
        snapshot,
        switched_at: chrono::Utc::now().timestamp_millis(),
    })
}

#[tauri::command]
pub async fn finalize_workspace_profile_switch(
    transaction_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let manifests = runtime
        .manager
        .overview()
        .modules
        .into_iter()
        .filter(|module| !module.diagnostic)
        .map(|module| module.manifest)
        .collect::<Vec<_>>();
    runtime
        .profiles
        .finalize_switch(&transaction_id, &manifests)
}

#[tauri::command]
pub async fn rollback_workspace_profile_switch(
    transaction_id: String,
    runtime: State<'_, PlatformRuntime>,
) -> Result<WorkspaceProfileRuntimeSnapshot, WorkspaceProfileRuntimeError> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let pending = runtime.profiles.pending_switch()?.ok_or_else(|| {
        WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
            "没有可回滚的装配方案切换事务",
            None,
        )
    })?;
    if pending.transaction_id != transaction_id {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
            "装配方案切换事务 ID 不匹配",
            None,
        ));
    }
    let overview = runtime.manager.overview();
    let manifests = overview
        .modules
        .iter()
        .filter(|module| !module.diagnostic)
        .map(|module| module.manifest.clone())
        .collect::<Vec<_>>();
    let snapshot = runtime.profiles.snapshot(&manifests)?;
    if snapshot.current_profile.id != pending.to_profile_id {
        return Err(WorkspaceProfileRuntimeError::new(
            WorkspaceProfileRuntimeErrorCode::ProfileSwitchTransactionInvalid,
            "当前 Profile 与待回滚事务目标不一致",
            None,
        ));
    }
    let previous_profile = runtime
        .profiles
        .profile(&pending.from_profile_id, &manifests)?;
    let previous_modules = runtime
        .manager
        .capture_profile_module_state()
        .map_err(profile_switch_module_error)?;
    runtime
        .manager
        .apply_profile_module_set(&profile_module_ids(&previous_profile))
        .await
        .map_err(profile_switch_module_error)?;
    let rolled_back =
        match runtime.profiles.commit_switch(
            &pending.from_profile_id,
            &pending.to_profile_id,
            &manifests,
        ) {
            Ok(snapshot) => snapshot,
            Err(commit_error) => {
                return match runtime
                    .manager
                    .restore_profile_module_state(&previous_modules)
                    .await
                {
                    Ok(()) => Err(commit_error
                        .with_details(vec!["模块状态已恢复到回滚前的目标 Profile".into()])),
                    Err(rollback_error) => Err(WorkspaceProfileRuntimeError::new(
                        WorkspaceProfileRuntimeErrorCode::ProfileRollbackFailed,
                        "Profile 回滚提交失败，模块恢复也未完整完成",
                        None,
                    )
                    .with_details(vec![
                        format!("commit={}", commit_error.message),
                        format!("moduleRestore={}", rollback_error.message),
                    ])),
                };
            }
        };
    match runtime.profiles.abort_switch(&transaction_id) {
        Ok(()) => runtime.profiles.snapshot(&manifests),
        Err(error) => {
            eprintln!(
                "[platform] Profile 已回滚，但切换日志未清理，将在下次启动恢复: {}",
                error.message
            );
            Ok(rolled_back)
        }
    }
}

fn profile_module_ids(
    profile: &pmc_platform::WorkspaceProfileV1,
) -> std::collections::BTreeSet<String> {
    profile
        .enabled_modules
        .iter()
        .map(|selection| selection.id.clone())
        .collect()
}

fn formal_module_manifests(runtime: &PlatformRuntime) -> Vec<pmc_platform::ModuleManifestV1> {
    runtime
        .manager
        .overview()
        .modules
        .into_iter()
        .filter(|module| !module.diagnostic)
        .map(|module| module.manifest)
        .collect()
}

fn profile_switch_module_error(error: ModuleManagerError) -> WorkspaceProfileRuntimeError {
    WorkspaceProfileRuntimeError::new(
        WorkspaceProfileRuntimeErrorCode::ProfileSwitchFailed,
        format!("模块切换失败：{}", error.message),
        None,
    )
    .with_details(
        std::iter::once(format!("code={:?}", error.code))
            .chain(error.module_id.map(|id| format!("moduleId={id}")))
            .chain(error.details)
            .collect(),
    )
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
pub async fn set_local_web_console_enabled(
    enabled: bool,
    runtime: State<'_, PlatformRuntime>,
) -> Result<ModuleDiagnosticSnapshot, String> {
    let _guard = runtime.profile_switch_lock.lock().await;
    let previous = runtime
        .manager
        .snapshot(LOCAL_WEB_CONSOLE_MODULE_ID)
        .map_err(|error| error.to_string())?;

    let transition = if enabled {
        runtime
            .manager
            .enable_module(LOCAL_WEB_CONSOLE_MODULE_ID)
            .await
    } else {
        runtime
            .manager
            .disable_module(LOCAL_WEB_CONSOLE_MODULE_ID, StopStrategy::Graceful)
            .await
    };
    transition.map_err(|error| error.to_string())?;

    let manifests = formal_module_manifests(&runtime);
    if let Err(error) = runtime.profiles.set_current_module_enabled(
        LOCAL_WEB_CONSOLE_MODULE_ID,
        "^1.0",
        enabled,
        &[crate::local_web_console::LOCAL_WEB_CONSOLE_TOOL_ID],
        &manifests,
    ) {
        let rollback = if previous.desired_enabled {
            runtime
                .manager
                .enable_module(LOCAL_WEB_CONSOLE_MODULE_ID)
                .await
        } else {
            runtime
                .manager
                .disable_module(LOCAL_WEB_CONSOLE_MODULE_ID, StopStrategy::Force)
                .await
        };
        let rollback_note = rollback
            .err()
            .map(|rollback_error| format!("；运行时回滚失败：{}", rollback_error.message))
            .unwrap_or_default();
        return Err(format!("{}{}", error.message, rollback_note));
    }

    runtime
        .manager
        .snapshot(LOCAL_WEB_CONSOLE_MODULE_ID)
        .map_err(|error| error.to_string())
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
