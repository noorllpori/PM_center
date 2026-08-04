use super::capability_gateway::CapabilityComponentRegistration;
use super::module_manager::{
    LifecycleFuture, ModuleContext, ModuleHealth, ModuleHealthLevel, ModuleLifecycle,
    RegisteredModule,
};
use super::resource_registry::ResourceKind;
use pmc_platform::{
    Capability, ExtensionFields, ModuleContributions, ModuleDataPolicy, ModuleDependency,
    ModuleManifestV1, ModuleScope,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const SMART_CLIPBOARD_MODULE_ID: &str = "builtin.smart-clipboard";
pub const LAN_COLLABORATION_MODULE_ID: &str = "builtin.lan-collaboration";
pub use crate::automation_runtime::AUTOMATION_RUNTIME_MODULE_ID;
pub use crate::project_resources::PROJECT_RESOURCES_MODULE_ID;
pub use crate::render_center::RENDER_CENTER_MODULE_ID;
pub const DIAGNOSTIC_BASE_ID: &str = "diagnostic.runtime-base";
pub const DIAGNOSTIC_WORKER_ID: &str = "diagnostic.runtime-worker";
pub const DIAGNOSTIC_FAILING_ID: &str = "diagnostic.runtime-failing";
pub const DIAGNOSTIC_SLOW_STOP_ID: &str = "diagnostic.runtime-slow-stop";

struct AutomationRuntimeLifecycle;

impl ModuleLifecycle for AutomationRuntimeLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            crate::automation_runtime::start_runtime();
            let mut details = BTreeMap::new();
            details.insert("taskProcesses".into(), "任务中心 Python/插件任务".into());
            details.insert("pythonProcesses".into(), "脚本、venv 与 pip 操作".into());
            details.insert("pluginProcesses".into(), "旧插件动作与依赖安装".into());
            context.resources.register(
                context.module_id,
                ResourceKind::ChildProcess,
                "任务、Python 与旧插件进程协调器",
                details,
                Box::new(|| Box::pin(crate::automation_runtime::stop_runtime())),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(crate::automation_runtime::stop_runtime())
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async {
            if !crate::automation_runtime::is_running() {
                return Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "任务、Python 与旧插件运行时当前未启用".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                });
            }
            let task_count = crate::task::active_task_count();
            let process_count = crate::automation_runtime::active_process_count();
            let message = format!(
                "任务 {} 个；{}",
                task_count,
                crate::automation_runtime::active_process_summary()
            );
            if task_count <= process_count {
                Ok(ModuleHealth::healthy(message))
            } else {
                Ok(ModuleHealth {
                    level: ModuleHealthLevel::Degraded,
                    message: format!("任务登记数高于进程登记数；{message}"),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                })
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(12)
    }
}

fn automation_runtime_capabilities() -> Vec<Capability> {
    vec![
        Capability::AppSettingsRead,
        Capability::AppSettingsWrite,
        Capability::FilesystemExternalRead,
        Capability::FilesystemExternalWrite,
        Capability::ProjectFilesRead,
        Capability::ProjectFilesWrite,
        Capability::TaskRun,
        Capability::TaskCancel,
        Capability::PythonExecute,
        Capability::PythonPackagesManage,
        Capability::ProcessSpawn,
        Capability::NetworkHttpRequest,
    ]
}

pub fn automation_runtime_module() -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: AUTOMATION_RUNTIME_MODULE_ID.into(),
            name: "任务、Python 与旧插件".into(),
            description: "管理普通任务、Python 环境、venv、pip 和旧插件动作的进程生命周期。".into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: Vec::new(),
            optional_modules: vec![ModuleDependency {
                id: PROJECT_RESOURCES_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            }],
            conflicts: Vec::new(),
            capabilities: automation_runtime_capabilities(),
            background_services: vec!["managed-process-registry".into()],
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(AutomationRuntimeLifecycle),
        diagnostic: false,
    }
}

pub fn automation_runtime_component() -> CapabilityComponentRegistration {
    CapabilityComponentRegistration {
        id: "builtin.automation-runtime.service".into(),
        name: "任务与脚本进程协调组件".into(),
        version: "1.0.0".into(),
        module_id: AUTOMATION_RUNTIME_MODULE_ID.into(),
        capabilities: automation_runtime_capabilities(),
    }
}

struct RenderCenterLifecycle;

impl ModuleLifecycle for RenderCenterLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            crate::render_center::start_runtime();
            let mut details = BTreeMap::new();
            details.insert("scheduler".into(), "全局作业与 Blender 进程预算调度".into());
            details.insert("workers".into(), "常驻与逐帧 Blender Worker".into());
            details.insert("hooks".into(), "渲染前后置 PMC Python 脚本".into());
            details.insert("packaging".into(), "FFmpeg 序列帧视频打包".into());
            context.resources.register(
                context.module_id,
                ResourceKind::ChildProcess,
                "渲染调度、Worker 与打包进程协调器",
                details,
                Box::new(|| Box::pin(crate::render_center::stop_runtime())),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(crate::render_center::stop_runtime())
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async {
            if !crate::render_center::is_running() {
                return Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "渲染中心当前未启用".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                });
            }
            let jobs = crate::render_center::active_job_count();
            let processes = crate::render_center::active_process_count();
            let packages = crate::render_center::active_package_count();
            let message = format!(
                "活动作业 {jobs} 个，视频打包 {packages} 个；{}",
                crate::render_center::active_process_summary()
            );
            if jobs == 0 && processes == 0 && packages == 0 {
                Ok(ModuleHealth::healthy(
                    "渲染中心待命，当前没有活动作业或子进程",
                ))
            } else if jobs > 0 && processes == 0 {
                Ok(ModuleHealth {
                    level: ModuleHealthLevel::Degraded,
                    message: format!("作业已登记但尚无 Blender 进程；{message}"),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                })
            } else {
                Ok(ModuleHealth::healthy(message))
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(20)
    }
}

fn render_center_capabilities() -> Vec<Capability> {
    vec![
        Capability::AppSettingsRead,
        Capability::AppSettingsWrite,
        Capability::ProjectFilesRead,
        Capability::ProjectFilesWrite,
        Capability::FilesystemExternalRead,
        Capability::FilesystemExternalWrite,
        Capability::RenderInspect,
        Capability::RenderQueueRead,
        Capability::RenderQueueWrite,
        Capability::RenderWorkerExecute,
        Capability::RenderResultCommit,
        Capability::PythonExecute,
        Capability::ProcessSpawn,
    ]
}

pub fn render_center_module() -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: RENDER_CENTER_MODULE_ID.into(),
            name: "渲染中心".into(),
            description: "管理渲染队列、Blender Worker、渲染脚本、性能采样和视频打包生命周期。"
                .into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: vec![ModuleDependency {
                id: PROJECT_RESOURCES_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            }],
            optional_modules: Vec::new(),
            conflicts: Vec::new(),
            capabilities: render_center_capabilities(),
            background_services: vec![
                "render-scheduler".into(),
                "blender-worker-registry".into(),
                "render-performance-sampling".into(),
                "render-video-packaging".into(),
            ],
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(RenderCenterLifecycle),
        diagnostic: false,
    }
}

pub fn render_center_component() -> CapabilityComponentRegistration {
    CapabilityComponentRegistration {
        id: "builtin.render-center.service".into(),
        name: "渲染调度与 Worker 协调组件".into(),
        version: "1.0.0".into(),
        module_id: RENDER_CENTER_MODULE_ID.into(),
        capabilities: render_center_capabilities(),
    }
}

struct ProjectResourcesLifecycle {
    databases: crate::project_resources::ProjectDatabaseState,
}

impl ProjectResourcesLifecycle {
    fn new(databases: crate::project_resources::ProjectDatabaseState) -> Self {
        Self { databases }
    }
}

impl ModuleLifecycle for ProjectResourcesLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            crate::project_resources::set_module_enabled(true);

            let mut database_details = BTreeMap::new();
            database_details.insert("dataDbRegistry".into(), "按已打开项目复用连接".into());
            database_details.insert("treeCacheRegistry".into(), "按已打开项目复用连接".into());
            database_details.insert("watcher".into(), "仅监听当前活动项目".into());
            let databases = self.databases.clone();
            context.resources.register(
                context.module_id.clone(),
                ResourceKind::Database,
                "项目数据库、目录索引与 watcher 协调器",
                database_details,
                Box::new(move || {
                    Box::pin(async move {
                        crate::project_resources::release_all_handles(&databases).await
                    })
                }),
            );

            let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
            let repair_task = tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(2));
                loop {
                    tokio::select! {
                        _ = &mut stop_rx => break,
                        _ = interval.tick() => {
                            let Some(project_path) = crate::watcher::get_active_project_path() else {
                                continue;
                            };
                            if crate::cache_manager::is_project_under_maintenance(&project_path) {
                                continue;
                            }
                            let _ = crate::tree_cache::process_project_dirty_dirs(&project_path, 25);
                        }
                    }
                }
            });
            let mut task_details = BTreeMap::new();
            task_details.insert("interval".into(), "2s".into());
            task_details.insert("maxDirectoriesPerTick".into(), "25".into());
            context.resources.register(
                context.module_id,
                ResourceKind::TokioTask,
                "活动项目脏目录修复任务",
                task_details,
                Box::new(move || {
                    Box::pin(async move {
                        let _ = stop_tx.send(());
                        repair_task
                            .await
                            .map_err(|error| format!("等待脏目录修复任务退出失败: {error}"))?;
                        Ok(())
                    })
                }),
            );

            for error in crate::project_resources::restore_registered_handles(&self.databases).await
            {
                eprintln!("[project-resources] {error}");
            }
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async {
            crate::project_resources::set_module_enabled(false);
            crate::cache_manager::cancel_all_cache_maintenance()?;
            Ok(())
        })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async move {
            if !crate::project_resources::is_module_enabled() {
                return Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "项目资源模块当前未启用".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                });
            }

            let snapshot = crate::project_resources::health_snapshot(&self.databases).await;
            if snapshot.registered_projects == 0 {
                return Ok(ModuleHealth::healthy(
                    "等待项目打开，当前没有 SQLite 或 watcher 句柄",
                ));
            }

            let database_ready = snapshot.database_count == snapshot.registered_projects;
            let cache_ready = snapshot.tree_cache_count == snapshot.registered_projects;
            let watcher_ready = snapshot.active_project.is_none()
                || (snapshot.watcher_running && snapshot.watcher_worker_running);
            let message = format!(
                "项目 {} 个，data.db {} 个，TreeCache {} 个，watcher={}，缓存维护 {} 个",
                snapshot.registered_projects,
                snapshot.database_count,
                snapshot.tree_cache_count,
                if watcher_ready { "正常" } else { "缺失" },
                snapshot.active_maintenance
            );
            if database_ready && cache_ready && watcher_ready {
                Ok(ModuleHealth::healthy(message))
            } else {
                Ok(ModuleHealth {
                    level: ModuleHealthLevel::Degraded,
                    message,
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                })
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(15)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

fn project_resource_capabilities() -> Vec<Capability> {
    vec![
        Capability::ProjectOpen,
        Capability::ProjectFilesRead,
        Capability::ProjectFilesWrite,
        Capability::ProjectMetadataRead,
        Capability::ProjectMetadataWrite,
    ]
}

pub fn project_resources_module(
    databases: crate::project_resources::ProjectDatabaseState,
) -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: PROJECT_RESOURCES_MODULE_ID.into(),
            name: "项目资源".into(),
            description: "管理项目数据库、目录索引、文件监听和项目关闭后的资源释放。".into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Project,
            builtin: true,
            requires_modules: Vec::new(),
            optional_modules: Vec::new(),
            conflicts: Vec::new(),
            capabilities: project_resource_capabilities(),
            background_services: vec![
                "project-database-registry".into(),
                "tree-cache-registry".into(),
                "active-project-watcher".into(),
                "dirty-directory-repair".into(),
            ],
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(ProjectResourcesLifecycle::new(databases)),
        diagnostic: false,
    }
}

pub fn project_resources_component() -> CapabilityComponentRegistration {
    CapabilityComponentRegistration {
        id: "builtin.project-resources.service".into(),
        name: "项目资源协调组件".into(),
        version: "1.0.0".into(),
        module_id: PROJECT_RESOURCES_MODULE_ID.into(),
        capabilities: project_resource_capabilities(),
    }
}

struct LanCollaborationLifecycle {
    app_data_dir: PathBuf,
    app_handle: tauri::AppHandle,
}

impl LanCollaborationLifecycle {
    fn new(app_data_dir: PathBuf, app_handle: tauri::AppHandle) -> Self {
        Self {
            app_data_dir,
            app_handle,
        }
    }
}

impl ModuleLifecycle for LanCollaborationLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            crate::p2p::start_managed_service(self.app_handle.clone(), self.app_data_dir.clone())
                .await?;

            let mut details = BTreeMap::new();
            details.insert("udpDiscovery".into(), "0.0.0.0:31523".into());
            details.insert("tcpMessages".into(), "0.0.0.0:31524".into());
            details.insert("serverConnection".into(), "按全局设置连接".into());
            details.insert("activeTransfers".into(), "停用时统一取消".into());
            let app_handle = self.app_handle.clone();
            context.resources.register(
                context.module_id,
                ResourceKind::NetworkListener,
                "局域网协同网络服务",
                details,
                Box::new(move || {
                    Box::pin(async move { crate::p2p::stop_managed_service(app_handle).await })
                }),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async {
            let health = crate::p2p::managed_service_health();
            if !crate::p2p::is_managed_service_enabled() || !health.initialized {
                return Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "局域网数据库或设备身份尚未初始化".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                });
            }
            if health.udp_bound && health.tcp_bound {
                return Ok(ModuleHealth::healthy(format!(
                    "UDP/TCP 监听正常，活动传输 {} 个，服务器连接{}",
                    health.active_transfers,
                    if health.server_connected {
                        "正常"
                    } else {
                        "未启用或未连接"
                    }
                )));
            }
            Ok(ModuleHealth {
                level: ModuleHealthLevel::Degraded,
                message: health.last_error.unwrap_or_else(|| {
                    format!(
                        "局域网监听不完整：UDP={}，TCP={}",
                        health.udp_bound, health.tcp_bound
                    )
                }),
                checked_at: Some(chrono::Utc::now().timestamp_millis()),
            })
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(15)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

fn lan_capabilities() -> Vec<Capability> {
    vec![
        Capability::AppProfileRead,
        Capability::AppProfileWrite,
        Capability::AppSettingsRead,
        Capability::AppSettingsWrite,
        Capability::NotificationSend,
        Capability::FilesystemExternalRead,
        Capability::FilesystemExternalWrite,
        Capability::NetworkHttpRequest,
        Capability::NetworkLanDiscover,
        Capability::NetworkLanMessage,
        Capability::NetworkLanTransfer,
        Capability::NetworkServerConnect,
    ]
}

pub fn lan_collaboration_module(
    app_data_dir: PathBuf,
    app_handle: tauri::AppHandle,
) -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: LAN_COLLABORATION_MODULE_ID.into(),
            name: "局域网协同".into(),
            description: "管理联系人发现、局域网消息、文件传输和可选服务器连接。".into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: Vec::new(),
            optional_modules: Vec::new(),
            conflicts: Vec::new(),
            capabilities: lan_capabilities(),
            background_services: vec![
                "udp-discovery".into(),
                "tcp-message-server".into(),
                "server-client".into(),
                "transfer-supervisor".into(),
            ],
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(LanCollaborationLifecycle::new(app_data_dir, app_handle)),
        diagnostic: false,
    }
}

pub fn lan_collaboration_component() -> CapabilityComponentRegistration {
    CapabilityComponentRegistration {
        id: "builtin.lan-collaboration.service".into(),
        name: "局域网协同服务组件".into(),
        version: "1.0.0".into(),
        module_id: LAN_COLLABORATION_MODULE_ID.into(),
        capabilities: lan_capabilities(),
    }
}

struct SmartClipboardLifecycle {
    app_data_dir: PathBuf,
}

impl SmartClipboardLifecycle {
    fn new(app_data_dir: PathBuf) -> Self {
        Self { app_data_dir }
    }
}

impl ModuleLifecycle for SmartClipboardLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            let app_data_dir = self.app_data_dir.clone();
            tokio::task::spawn_blocking(move || crate::smart_clipboard::initialize(&app_data_dir))
                .await
                .map_err(|error| format!("等待智能剪贴板监听线程启动失败: {error}"))??;

            let mut details = BTreeMap::new();
            details.insert(
                "clipboardListener".into(),
                "AddClipboardFormatListener".into(),
            );
            details.insert("nativeWindow".into(), "Win32/GDI".into());
            details.insert("globalHotkey".into(), "Ctrl+`".into());
            context.resources.register(
                context.module_id,
                ResourceKind::NativeThread,
                "智能剪贴板原生监听线程",
                details,
                Box::new(|| {
                    Box::pin(async move {
                        tokio::task::spawn_blocking(crate::smart_clipboard::shutdown)
                            .await
                            .map_err(|error| format!("等待智能剪贴板监听线程清理失败: {error}"))?
                    })
                }),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async {
            if crate::smart_clipboard::is_running() {
                Ok(ModuleHealth::healthy(
                    "剪贴板监听、原生窗口与全局快捷键线程运行正常",
                ))
            } else {
                Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "智能剪贴板监听线程或原生窗口已经退出".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                })
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

pub fn smart_clipboard_module(app_data_dir: PathBuf) -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(cfg!(windows)));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: SMART_CLIPBOARD_MODULE_ID.into(),
            name: "智能剪贴板".into(),
            description: "记录软件级剪贴板历史，并提供原生检索窗口和 Ctrl+` 快捷键。".into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: Vec::new(),
            optional_modules: Vec::new(),
            conflicts: Vec::new(),
            capabilities: vec![Capability::ClipboardRead, Capability::ClipboardWrite],
            background_services: vec![
                "clipboard-listener".into(),
                "native-window".into(),
                "global-hotkey".into(),
            ],
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(SmartClipboardLifecycle::new(app_data_dir)),
        diagnostic: false,
    }
}

pub fn smart_clipboard_component() -> CapabilityComponentRegistration {
    CapabilityComponentRegistration {
        id: "builtin.smart-clipboard.native".into(),
        name: "智能剪贴板原生组件".into(),
        version: "1.0.0".into(),
        module_id: SMART_CLIPBOARD_MODULE_ID.into(),
        capabilities: vec![Capability::ClipboardRead, Capability::ClipboardWrite],
    }
}

#[derive(Default)]
pub struct DiagnosticControls {
    fail_start: AtomicBool,
    fail_health: AtomicBool,
    slow_stop: AtomicBool,
    active_tasks: AtomicUsize,
}

impl DiagnosticControls {
    pub fn configure(&self, module_id: &str, point: &str, enabled: bool) -> Result<(), String> {
        match (module_id, point) {
            (DIAGNOSTIC_FAILING_ID, "start") => {
                self.fail_start.store(enabled, Ordering::SeqCst);
                Ok(())
            }
            (DIAGNOSTIC_FAILING_ID, "health") => {
                self.fail_health.store(enabled, Ordering::SeqCst);
                Ok(())
            }
            (DIAGNOSTIC_SLOW_STOP_ID, "stop-timeout") => {
                self.slow_stop.store(enabled, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(format!("模块 {module_id} 不支持故障点 {point}")),
        }
    }

    pub fn active_tasks(&self) -> usize {
        self.active_tasks.load(Ordering::SeqCst)
    }

    pub fn failure_injections(&self) -> BTreeMap<String, bool> {
        BTreeMap::from([
            (
                format!("{DIAGNOSTIC_FAILING_ID}:start"),
                self.fail_start.load(Ordering::SeqCst),
            ),
            (
                format!("{DIAGNOSTIC_FAILING_ID}:health"),
                self.fail_health.load(Ordering::SeqCst),
            ),
            (
                format!("{DIAGNOSTIC_SLOW_STOP_ID}:stop-timeout"),
                self.slow_stop.load(Ordering::SeqCst),
            ),
        ])
    }
}

#[derive(Clone, Copy)]
enum DiagnosticKind {
    Base,
    Worker,
    Failing,
    SlowStop,
}

struct DiagnosticLifecycle {
    kind: DiagnosticKind,
    controls: Arc<DiagnosticControls>,
}

impl DiagnosticLifecycle {
    fn new(kind: DiagnosticKind, controls: Arc<DiagnosticControls>) -> Self {
        Self { kind, controls }
    }

    fn label(&self) -> &'static str {
        match self.kind {
            DiagnosticKind::Base => "基础心跳任务",
            DiagnosticKind::Worker => "依赖工作任务",
            DiagnosticKind::Failing => "故障注入任务",
            DiagnosticKind::SlowStop => "慢停止任务",
        }
    }
}

impl ModuleLifecycle for DiagnosticLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            if matches!(self.kind, DiagnosticKind::Failing)
                && self.controls.fail_start.load(Ordering::SeqCst)
            {
                return Err("已注入启动失败，用于验证事务回滚".into());
            }

            let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
            let active_tasks = self.controls.clone();
            active_tasks.active_tasks.fetch_add(1, Ordering::SeqCst);
            let task = tokio::spawn(async move {
                let _ = stop_rx.await;
                active_tasks.active_tasks.fetch_sub(1, Ordering::SeqCst);
            });
            let mut details = BTreeMap::new();
            details.insert("runtime".into(), "tokio".into());
            details.insert("purpose".into(), "R2 lifecycle verification".into());
            context.resources.register(
                context.module_id,
                ResourceKind::TokioTask,
                self.label(),
                details,
                Box::new(move || {
                    Box::pin(async move {
                        let _ = stop_tx.send(());
                        task.await
                            .map_err(|error| format!("等待诊断任务退出失败: {error}"))?;
                        Ok(())
                    })
                }),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            if matches!(self.kind, DiagnosticKind::SlowStop)
                && self.controls.slow_stop.load(Ordering::SeqCst)
            {
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
            Ok(())
        })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async move {
            if matches!(self.kind, DiagnosticKind::Failing)
                && self.controls.fail_health.load(Ordering::SeqCst)
            {
                return Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "已注入健康检查失败".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                });
            }
            Ok(ModuleHealth::healthy("诊断资源运行正常"))
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(2)
    }

    fn stop_timeout(&self) -> Duration {
        if matches!(self.kind, DiagnosticKind::SlowStop) {
            Duration::from_millis(150)
        } else {
            Duration::from_secs(2)
        }
    }
}

pub fn diagnostic_modules(controls: Arc<DiagnosticControls>) -> Vec<RegisteredModule> {
    vec![
        diagnostic_module(
            DIAGNOSTIC_BASE_ID,
            "运行时基础模块",
            "用于验证基础启停和资源释放。",
            DiagnosticKind::Base,
            &[],
            &[],
            &[Capability::AppSettingsRead],
            controls.clone(),
        ),
        diagnostic_module(
            DIAGNOSTIC_WORKER_ID,
            "依赖工作模块",
            "依赖基础模块，用于验证拓扑启动和反向停止。",
            DiagnosticKind::Worker,
            &[(DIAGNOSTIC_BASE_ID, "*")],
            &[(DIAGNOSTIC_SLOW_STOP_ID, "*")],
            &[Capability::ProjectFilesRead],
            controls.clone(),
        ),
        diagnostic_module(
            DIAGNOSTIC_FAILING_ID,
            "故障注入模块",
            "可注入启动或健康检查失败，用于验证回滚。",
            DiagnosticKind::Failing,
            &[(DIAGNOSTIC_BASE_ID, "*")],
            &[],
            &[Capability::ProjectFilesWrite],
            controls.clone(),
        ),
        diagnostic_module(
            DIAGNOSTIC_SLOW_STOP_ID,
            "停止超时模块",
            "可注入停止超时，用于验证强制资源释放。",
            DiagnosticKind::SlowStop,
            &[],
            &[],
            &[Capability::FilesystemExternalWrite],
            controls,
        ),
    ]
}

fn diagnostic_module(
    id: &str,
    name: &str,
    description: &str,
    kind: DiagnosticKind,
    required: &[(&str, &str)],
    optional: &[(&str, &str)],
    capabilities: &[Capability],
    controls: Arc<DiagnosticControls>,
) -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("diagnostic".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: id.into(),
            name: name.into(),
            description: description.into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: required
                .iter()
                .map(|(id, requirement)| ModuleDependency {
                    id: (*id).into(),
                    version_requirement: (*requirement).into(),
                })
                .collect(),
            optional_modules: optional
                .iter()
                .map(|(id, requirement)| ModuleDependency {
                    id: (*id).into(),
                    version_requirement: (*requirement).into(),
                })
                .collect(),
            conflicts: Vec::new(),
            capabilities: capabilities.to_vec(),
            background_services: vec!["diagnostic-heartbeat".into()],
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(DiagnosticLifecycle::new(kind, controls)),
        diagnostic: true,
    }
}

pub fn diagnostic_components() -> Vec<CapabilityComponentRegistration> {
    vec![
        CapabilityComponentRegistration {
            id: "diagnostic.component-base".into(),
            name: "基础设置读取组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_BASE_ID.into(),
            capabilities: vec![Capability::AppSettingsRead],
        },
        CapabilityComponentRegistration {
            id: "diagnostic.component-worker".into(),
            name: "项目只读诊断组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_WORKER_ID.into(),
            capabilities: vec![Capability::ProjectFilesRead],
        },
        CapabilityComponentRegistration {
            id: "diagnostic.component-failing".into(),
            name: "项目写入诊断组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_FAILING_ID.into(),
            capabilities: vec![Capability::ProjectFilesWrite],
        },
        CapabilityComponentRegistration {
            id: "diagnostic.component-slow-stop".into(),
            name: "外部暂存诊断组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_SLOW_STOP_ID.into(),
            capabilities: vec![Capability::FilesystemExternalWrite],
        },
    ]
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::platform::{ModuleManager, ModuleState, StopStrategy};
    use std::fs;
    use uuid::Uuid;

    #[tokio::test(flavor = "multi_thread")]
    async fn smart_clipboard_lifecycle_releases_and_restores_native_resources() {
        let root = std::env::temp_dir().join(format!(
            "pm-center-smart-clipboard-lifecycle-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let state_path = root.join("module-runtime.json");
        let manager =
            ModuleManager::new(state_path, vec![smart_clipboard_module(root.clone())]).unwrap();

        assert!(
            manager
                .snapshot(SMART_CLIPBOARD_MODULE_ID)
                .unwrap()
                .desired_enabled
        );
        assert!(crate::smart_clipboard::ensure_module_running(&manager).is_err());

        assert!(manager.restore_desired_modules().await.is_empty());
        let running = manager.snapshot(SMART_CLIPBOARD_MODULE_ID).unwrap();
        assert_eq!(running.state, ModuleState::Running);
        assert_eq!(running.resources.len(), 1);
        assert!(crate::smart_clipboard::ensure_module_running(&manager).is_ok());
        assert!(root
            .join("smart_clipboard")
            .join("clipboard_history.db")
            .exists());

        manager
            .disable_module(SMART_CLIPBOARD_MODULE_ID, StopStrategy::Graceful)
            .await
            .unwrap();
        assert!(!crate::smart_clipboard::is_running());
        assert_eq!(manager.resources().total_count(), 0);
        assert!(crate::smart_clipboard::ensure_module_running(&manager).is_err());

        manager
            .enable_module(SMART_CLIPBOARD_MODULE_ID)
            .await
            .unwrap();
        assert!(crate::smart_clipboard::is_running());
        assert_eq!(manager.resources().total_count(), 1);

        assert!(manager.shutdown_all().await.is_empty());
        assert!(!crate::smart_clipboard::is_running());
        assert_eq!(manager.resources().total_count(), 0);
        assert!(root
            .join("smart_clipboard")
            .join("clipboard_history.db")
            .exists());
        drop(manager);
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod project_resource_tests {
    use super::*;

    #[test]
    fn project_resource_manifest_declares_project_scope_and_default_enablement() {
        let module = project_resources_module(crate::project_resources::new_database_state());
        assert_eq!(module.manifest.id, PROJECT_RESOURCES_MODULE_ID);
        assert_eq!(module.manifest.scope, ModuleScope::Project);
        assert_eq!(
            module.manifest.extensions.get("defaultEnabled"),
            Some(&Value::Bool(true))
        );
        assert!(module
            .manifest
            .capabilities
            .contains(&Capability::ProjectMetadataWrite));
        assert!(module
            .manifest
            .background_services
            .contains(&"active-project-watcher".to_string()));
    }

    #[test]
    fn render_center_manifest_owns_worker_and_commit_capabilities() {
        let module = render_center_module();
        assert_eq!(module.manifest.id, RENDER_CENTER_MODULE_ID);
        assert_eq!(module.manifest.scope, ModuleScope::Global);
        assert_eq!(
            module.manifest.extensions.get("defaultEnabled"),
            Some(&Value::Bool(true))
        );
        assert!(module
            .manifest
            .requires_modules
            .iter()
            .any(|dependency| dependency.id == PROJECT_RESOURCES_MODULE_ID));
        assert!(module
            .manifest
            .capabilities
            .contains(&Capability::RenderWorkerExecute));
        assert!(module
            .manifest
            .capabilities
            .contains(&Capability::RenderResultCommit));
    }
}
