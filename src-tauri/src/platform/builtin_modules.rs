use super::builtin_components::PM_BLENDIO_COMPONENT_ID;
use super::capability_gateway::CapabilityComponentRegistration;
use super::module_manager::{
    LifecycleFuture, ModuleContext, ModuleHealth, ModuleHealthLevel, ModuleLifecycle,
    RegisteredModule,
};
use super::resource_registry::ResourceKind;
use super::script_automation::{
    all_script_capabilities, ScriptAutomationRuntime, SCRIPT_AUTOMATION_SURFACE_ID,
    SCRIPT_AUTOMATION_TOOL_ID,
};
use pmc_platform::{
    Capability, ComponentDependency, ExtensionFields, ModuleContributions, ModuleDataPolicy,
    ModuleDependency, ModuleManifestV1, ModuleScope,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const SMART_CLIPBOARD_MODULE_ID: &str = "builtin.smart-clipboard";
pub const LAN_COLLABORATION_MODULE_ID: &str = "builtin.lan-collaboration";
pub const PROJECT_MANAGER_MODULE_ID: &str = "builtin.project-manager";
pub const SETTINGS_CENTER_MODULE_ID: &str = "builtin.settings-center";
pub const SESSION_RUNTIME_MODULE_ID: &str = "builtin.session-runtime";
pub const DESKTOP_INTEGRATION_MODULE_ID: &str = "builtin.desktop-integration";
pub const EXTERNAL_TOOLS_MODULE_ID: &str = "builtin.external-tools";
pub use super::script_automation::SCRIPT_AUTOMATION_MODULE_ID;
pub use crate::automation_runtime::AUTOMATION_RUNTIME_MODULE_ID;
pub use crate::local_web_console::LOCAL_WEB_CONSOLE_MODULE_ID;
pub use crate::media_library::MEDIA_LIBRARY_MODULE_ID;
pub use crate::project_resources::PROJECT_RESOURCES_MODULE_ID;
pub use crate::render_center::RENDER_CENTER_MODULE_ID;
pub use crate::render_farm::RENDER_FARM_MODULE_ID;
pub const AUTOMATION_PYTHON_TOOL_ID: &str = "builtin.automation-runtime.python-tool";
pub const AUTOMATION_TASK_TOOL_ID: &str = "builtin.automation-runtime.task-tool";
pub const AUTOMATION_PYTHON_SURFACE_ID: &str = "builtin.automation-runtime.python-surface";
pub const AUTOMATION_TASK_SURFACE_ID: &str = "builtin.automation-runtime.task-surface";
pub const AUTOMATION_SETTINGS_SECTION_ID: &str = "builtin.automation-runtime.settings-section";
pub const AUTOMATION_PROJECT_SETTINGS_SECTION_ID: &str =
    "builtin.automation-runtime.project-settings-section";
pub const AUTOMATION_PLUGIN_CONTEXT_COMMANDS_ID: &str =
    "builtin.automation-runtime.plugin-context-commands";
pub const RENDER_TOOL_ID: &str = "builtin.render-center.tool";
pub const EXTERNAL_RENDER_STATION_TOOL_ID: &str = "builtin.render-center.external-station-tool";
pub const RENDER_WORKSPACE_TAB_ID: &str = "builtin.render-center.workspace-tab";
pub const RENDER_SURFACE_ID: &str = "builtin.render-center.surface";
pub const EXTERNAL_RENDER_STATION_SURFACE_ID: &str =
    "builtin.render-center.external-station-surface";
pub const EXTERNAL_RENDER_STATION_SHELL_TAB_ID: &str =
    "builtin.render-center.external-station-shell-tab";
pub const MEDIA_LIBRARY_SURFACE_ID: &str = "builtin.media-library.surface";
pub const MEDIA_LIBRARY_SHELL_TAB_ID: &str = "builtin.media-library.shell-tab";
pub const MEDIA_LIBRARY_TOOL_ID: &str = "builtin.media-library.tool";
pub const PROJECT_CACHE_TOOL_ID: &str = "builtin.project-resources.cache-tool";
pub const PROJECT_MDT_TOOL_ID: &str = "builtin.project-resources.mdt-tool";
pub const PROJECT_CACHE_WORKSPACE_TAB_ID: &str = "builtin.project-resources.cache-workspace-tab";
pub const PROJECT_CACHE_SURFACE_ID: &str = "builtin.project-resources.cache-surface";
pub const PROJECT_MDT_SURFACE_ID: &str = "builtin.project-resources.mdt-surface";
pub const PROJECT_COLLECTION_CONTEXT_COMMANDS_ID: &str =
    "builtin.project-resources.collection-context-commands";
pub const PROJECT_GLOBAL_EXCLUSIONS_SETTINGS_SECTION_ID: &str =
    "builtin.project-resources.global-exclusions-settings-section";
pub const PROJECT_RULES_SETTINGS_SECTION_ID: &str =
    "builtin.project-resources.project-rules-settings-section";
pub const PROJECT_MANAGER_SHELL_TAB_ID: &str = "builtin.project-manager.project-shell-tab";
pub const PROJECT_MANAGER_HOME_SURFACE_ID: &str = "builtin.project-manager.home-surface";
pub const PROJECT_MANAGER_WORKSPACE_SURFACE_ID: &str =
    "builtin.project-manager.project-workspace-surface";
pub const PROJECT_MANAGER_DIRECTORY_WIDGET_ID: &str =
    "builtin.project-manager.project-directory-widget";
pub const PROJECT_MANAGER_QUICK_ACTIONS_WIDGET_ID: &str =
    "builtin.project-manager.quick-actions-widget";
pub const PROJECT_MANAGER_RECENT_PROJECTS_WIDGET_ID: &str =
    "builtin.project-manager.recent-projects-widget";
pub const PROJECT_MANAGER_PROJECT_CATALOG_WIDGET_ID: &str =
    "builtin.project-manager.project-catalog-widget";
pub const PROJECT_MANAGER_DIRECTORY_DATA_SOURCE_ID: &str =
    "builtin.project-manager.project-directory-data-source";
pub const PROJECT_MANAGER_QUICK_ACTIONS_DATA_SOURCE_ID: &str =
    "builtin.project-manager.quick-actions-data-source";
pub const PROJECT_MANAGER_RECENT_PROJECTS_DATA_SOURCE_ID: &str =
    "builtin.project-manager.recent-projects-data-source";
pub const PROJECT_MANAGER_PROJECT_CATALOG_DATA_SOURCE_ID: &str =
    "builtin.project-manager.project-catalog-data-source";
pub const PROJECT_MANAGER_SELECT_ROOT_COMMAND_ID: &str =
    "builtin.project-manager.select-project-root-command";
pub const PROJECT_MANAGER_CLEAR_ROOT_COMMAND_ID: &str =
    "builtin.project-manager.clear-project-root-command";
pub const PROJECT_MANAGER_CREATE_PROJECT_COMMAND_ID: &str =
    "builtin.project-manager.create-project-command";
pub const PROJECT_MANAGER_IMPORT_PROJECT_COMMAND_ID: &str =
    "builtin.project-manager.import-project-command";
pub const PROJECT_MANAGER_OPEN_PROJECT_COMMAND_ID: &str =
    "builtin.project-manager.open-project-command";
pub const PROJECT_MANAGER_IGNORE_PROJECT_COMMAND_ID: &str =
    "builtin.project-manager.ignore-project-command";
pub const PROJECT_MANAGER_RESTORE_IGNORED_PROJECT_COMMAND_ID: &str =
    "builtin.project-manager.restore-ignored-project-command";
pub const PROJECT_MANAGER_SHOW_IGNORED_PROJECTS_COMMAND_ID: &str =
    "builtin.project-manager.show-ignored-projects-command";
pub const PROJECT_MANAGER_REMOVE_RECENT_PROJECT_COMMAND_ID: &str =
    "builtin.project-manager.remove-recent-project-command";
pub const PROJECT_MANAGER_HISTORY_SETTINGS_SECTION_ID: &str =
    "builtin.project-manager.history-settings-section";
pub const LAN_MAIN_TOOL_ID: &str = "builtin.lan-collaboration.main-tool";
pub const LAN_PROJECT_TOOL_ID: &str = "builtin.lan-collaboration.project-tool";
pub const LAN_SHELL_TAB_ID: &str = "builtin.lan-collaboration.shell-tab";
pub const LAN_PROJECT_WORKSPACE_TAB_ID: &str = "builtin.lan-collaboration.project-workspace-tab";
pub const LAN_MAIN_SURFACE_ID: &str = "builtin.lan-collaboration.main-surface";
pub const LAN_PROJECT_SURFACE_ID: &str = "builtin.lan-collaboration.project-surface";
pub const SMART_CLIPBOARD_TOOL_ID: &str = "builtin.smart-clipboard.tool";
pub const SMART_CLIPBOARD_SURFACE_ID: &str = "builtin.smart-clipboard.native-surface";
pub use crate::local_web_console::LOCAL_WEB_CONSOLE_TOOL_ID;
pub const LOCAL_WEB_CONSOLE_SETTINGS_SECTION_ID: &str =
    "builtin.local-web-console.settings-section";
pub const SETTINGS_CENTER_TOOL_ID: &str = "core.settings.tool";
pub const SETTINGS_CENTER_ABOUT_SECTION_ID: &str = "core.settings.about-section";
pub const SETTINGS_CENTER_COMPONENTS_GLOBAL_SECTION_ID: &str =
    "builtin.settings-center.component-settings-global-section";
pub const SETTINGS_CENTER_COMPONENTS_PROJECT_SECTION_ID: &str =
    "builtin.settings-center.component-settings-project-section";
pub const SESSION_SETTINGS_SECTION_ID: &str = "builtin.session-runtime.settings-section";
pub const DESKTOP_SETTINGS_SECTION_ID: &str = "builtin.desktop-integration.settings-section";
pub const EXTERNAL_TOOLS_SETTINGS_SECTION_ID: &str = "builtin.external-tools.settings-section";
pub const BLENDER_FILE_PARSER_TOOL_ID: &str = "core.blender-file-parser.tool";
pub const DIAGNOSTIC_BASE_ID: &str = "diagnostic.runtime-base";
pub const DIAGNOSTIC_WORKER_ID: &str = "diagnostic.runtime-worker";
pub const DIAGNOSTIC_FAILING_ID: &str = "diagnostic.runtime-failing";
pub const DIAGNOSTIC_SLOW_STOP_ID: &str = "diagnostic.runtime-slow-stop";
pub const DIAGNOSTIC_CONTRIBUTION_SAMPLE_ID: &str = "diagnostic.contribution-sample";
pub const DIAGNOSTIC_CONTRIBUTION_TOOL_ID: &str = "diagnostic.contribution-sample.tool";
pub const DIAGNOSTIC_CONTRIBUTION_WORKSPACE_TAB_ID: &str =
    "diagnostic.contribution-sample.workspace-tab";
pub const DIAGNOSTIC_CONTRIBUTION_SURFACE_ID: &str = "diagnostic.contribution-sample.surface";
pub const DIAGNOSTIC_CONTRIBUTION_WIDGET_ID: &str =
    "diagnostic.contribution-sample.registry-widget";
pub const DIAGNOSTIC_CONTRIBUTION_DATA_SOURCE_ID: &str =
    "diagnostic.contribution-sample.registry-data-source";
pub const DIAGNOSTIC_CONTRIBUTION_WORKFLOW_NODE_ID: &str =
    "diagnostic.contribution-sample.echo-node";

struct AutomationRuntimeLifecycle;

struct ContributionHostLifecycle {
    health_message: &'static str,
}

impl ContributionHostLifecycle {
    const fn new(health_message: &'static str) -> Self {
        Self { health_message }
    }
}

impl ModuleLifecycle for ContributionHostLifecycle {
    fn start<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async move { Ok(ModuleHealth::healthy(self.health_message)) })
    }
}

fn contribution_host_module(
    id: &str,
    name: &str,
    description: &str,
    capabilities: Vec<Capability>,
    contributes: ModuleContributions,
    health_message: &'static str,
) -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
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
            requires_modules: Vec::new(),
            optional_modules: Vec::new(),
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities,
            background_services: Vec::new(),
            contributes,
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(ContributionHostLifecycle::new(health_message)),
        diagnostic: false,
    }
}

pub fn settings_center_module() -> RegisteredModule {
    contribution_host_module(
        SETTINGS_CENTER_MODULE_ID,
        "设置中心",
        "提供普通设置窗口、设置导航和组件 Schema 设置宿主；恢复设置由独立内核提供。",
        vec![
            Capability::AppProfileRead,
            Capability::AppSettingsRead,
            Capability::AppSettingsWrite,
        ],
        ModuleContributions {
            tools: vec![SETTINGS_CENTER_TOOL_ID.into()],
            settings_sections: vec![
                SETTINGS_CENTER_COMPONENTS_GLOBAL_SECTION_ID.into(),
                SETTINGS_CENTER_COMPONENTS_PROJECT_SECTION_ID.into(),
                SETTINGS_CENTER_ABOUT_SECTION_ID.into(),
            ],
            ..ModuleContributions::default()
        },
        "普通设置窗口和 Schema 设置宿主可用",
    )
}

pub fn session_runtime_module() -> RegisteredModule {
    contribution_host_module(
        SESSION_RUNTIME_MODULE_ID,
        "会话运行时",
        "管理会话恢复和标签关闭确认等软件级会话偏好。",
        vec![Capability::AppSettingsRead, Capability::AppSettingsWrite],
        ModuleContributions {
            settings_sections: vec![SESSION_SETTINGS_SECTION_ID.into()],
            ..ModuleContributions::default()
        },
        "会话恢复与标签确认设置可用",
    )
}

pub fn desktop_integration_module() -> RegisteredModule {
    contribution_host_module(
        DESKTOP_INTEGRATION_MODULE_ID,
        "桌面集成",
        "管理系统登录自启动等桌面环境集成功能。",
        vec![Capability::AppSettingsRead, Capability::AppSettingsWrite],
        ModuleContributions {
            settings_sections: vec![DESKTOP_SETTINGS_SECTION_ID.into()],
            ..ModuleContributions::default()
        },
        "桌面集成设置可用",
    )
}

pub fn external_tools_module() -> RegisteredModule {
    contribution_host_module(
        EXTERNAL_TOOLS_MODULE_ID,
        "外部工具",
        "管理 Blender、FFmpeg、FFprobe 路径、版本检测和 Blender 文件解析入口。",
        vec![
            Capability::AppSettingsRead,
            Capability::AppSettingsWrite,
            Capability::FilesystemExternalRead,
        ],
        ModuleContributions {
            tools: vec![BLENDER_FILE_PARSER_TOOL_ID.into()],
            settings_sections: vec![EXTERNAL_TOOLS_SETTINGS_SECTION_ID.into()],
            ..ModuleContributions::default()
        },
        "Blender、FFmpeg 和 FFprobe 设置可用",
    )
}

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
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: automation_runtime_capabilities(),
            background_services: vec!["managed-process-registry".into()],
            contributes: ModuleContributions {
                tools: vec![
                    AUTOMATION_PYTHON_TOOL_ID.into(),
                    AUTOMATION_TASK_TOOL_ID.into(),
                ],
                surfaces: vec![
                    AUTOMATION_PYTHON_SURFACE_ID.into(),
                    AUTOMATION_TASK_SURFACE_ID.into(),
                ],
                settings_sections: vec![
                    AUTOMATION_SETTINGS_SECTION_ID.into(),
                    AUTOMATION_PROJECT_SETTINGS_SECTION_ID.into(),
                ],
                context_commands: vec![AUTOMATION_PLUGIN_CONTEXT_COMMANDS_ID.into()],
                ..ModuleContributions::default()
            },
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

struct ScriptAutomationLifecycle {
    runtime: Arc<ScriptAutomationRuntime>,
}

impl ModuleLifecycle for ScriptAutomationLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            self.runtime.start()?;
            let runtime = self.runtime.clone();
            let mut details = BTreeMap::new();
            details.insert("database".into(), "automation_runtime.db (WAL)".into());
            details.insert("triggers".into(), "手动、事件、五段式 cron".into());
            details.insert(
                "security".into(),
                "受信任 Python + Capability Gateway".into(),
            );
            context.resources.register(
                context.module_id,
                ResourceKind::TokioTask,
                "脚本自动化调度器与事件订阅",
                details,
                Box::new(move || Box::pin(async move { runtime.stop().await })),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(self.runtime.stop())
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async move {
            if self.runtime.is_running() {
                Ok(ModuleHealth::healthy(self.runtime.health_message()))
            } else {
                Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "脚本自动化运行时未启动".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                })
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
}

pub fn script_automation_module(runtime: Arc<ScriptAutomationRuntime>) -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: SCRIPT_AUTOMATION_MODULE_ID.into(),
            name: "脚本自动化".into(),
            description: "运行可安装 Python 脚本组件、事件触发和定时自动化。".into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: vec![ModuleDependency {
                id: AUTOMATION_RUNTIME_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            }],
            optional_modules: vec![ModuleDependency {
                id: PROJECT_RESOURCES_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            }],
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: all_script_capabilities(),
            background_services: vec![
                "automation-scheduler".into(),
                "automation-event-forwarder".into(),
                "automation-run-recovery".into(),
            ],
            contributes: ModuleContributions {
                tools: vec![SCRIPT_AUTOMATION_TOOL_ID.into()],
                surfaces: vec![SCRIPT_AUTOMATION_SURFACE_ID.into()],
                ..ModuleContributions::default()
            },
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(ScriptAutomationLifecycle { runtime }),
        diagnostic: false,
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
            details.insert("hooks".into(), "渲染前后置 Nexora Python 脚本".into());
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

pub(crate) fn render_center_manifest() -> ModuleManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    ModuleManifestV1 {
        schema_version: 1,
        id: RENDER_CENTER_MODULE_ID.into(),
        name: "渲染中心".into(),
        description: "管理渲染队列、Blender Worker、渲染脚本、性能采样和视频打包生命周期。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        scope: ModuleScope::Global,
        builtin: true,
        // The scheduler owns its own SQLite namespace.  Project resources
        // remain an optional UI integration, not a runtime prerequisite.
        requires_modules: vec![ModuleDependency {
            id: EXTERNAL_TOOLS_MODULE_ID.into(),
            version_requirement: "^1.0".into(),
        }],
        optional_modules: Vec::new(),
        requires_components: vec![ComponentDependency {
            id: PM_BLENDIO_COMPONENT_ID.into(),
            version_requirement: "^1.0".into(),
        }],
        optional_components: Vec::new(),
        conflicts: Vec::new(),
        capabilities: render_center_capabilities(),
        background_services: vec![
            "render-scheduler".into(),
            "blender-worker-registry".into(),
            "render-performance-sampling".into(),
            "render-video-packaging".into(),
        ],
        contributes: ModuleContributions {
            shell_tabs: vec![EXTERNAL_RENDER_STATION_SHELL_TAB_ID.into()],
            workspace_tabs: vec![RENDER_WORKSPACE_TAB_ID.into()],
            tools: vec![
                RENDER_TOOL_ID.into(),
                EXTERNAL_RENDER_STATION_TOOL_ID.into(),
            ],
            surfaces: vec![
                RENDER_SURFACE_ID.into(),
                EXTERNAL_RENDER_STATION_SURFACE_ID.into(),
            ],
            ..ModuleContributions::default()
        },
        data_policy: ModuleDataPolicy::default(),
        extensions,
    }
}

pub fn render_center_module() -> RegisteredModule {
    RegisteredModule {
        manifest: render_center_manifest(),
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

/// R13 begins as an opt-in foundation.  It deliberately declares no user
/// surface yet: pairing, encrypted transport and remote job UI arrive in
/// separate slices once the contracts below have real network adapters.
struct RenderFarmLifecycle;

impl ModuleLifecycle for RenderFarmLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            crate::render_farm::start_runtime();
            let mut details = BTreeMap::new();
            details.insert("database".into(), "render_farm.db (WAL)".into());
            details.insert(
                "identity".into(),
                "Windows Credential Manager Ed25519 key".into(),
            );
            details.insert(
                "scope".into(),
                "配对、加密传输合同、能力报告、渲染包和控制端帧租约".into(),
            );
            context.resources.register(
                context.module_id,
                ResourceKind::Database,
                "渲染农场身份与控制端租约存储",
                details,
                Box::new(|| Box::pin(crate::render_farm::stop_runtime())),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(crate::render_farm::stop_runtime())
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async {
            if crate::render_farm::is_running() {
                Ok(ModuleHealth::healthy(
                    "可信设备、加密传输合同、渲染包与控制端租约基础层可用；网络传输适配器尚未启用",
                ))
            } else {
                Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "渲染农场模块当前未启用".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                })
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

fn render_farm_capabilities() -> Vec<Capability> {
    vec![
        Capability::AppSettingsRead,
        Capability::AppSettingsWrite,
        Capability::FilesystemExternalRead,
        Capability::FilesystemExternalWrite,
        Capability::NetworkLanDiscover,
        Capability::NetworkLanTransfer,
        Capability::RenderQueueRead,
        Capability::RenderQueueWrite,
        Capability::RenderResultCommit,
    ]
}

pub(crate) fn render_farm_manifest() -> ModuleManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(false));
    ModuleManifestV1 {
        schema_version: 1,
        id: RENDER_FARM_MODULE_ID.into(),
        name: "Blender 渲染农场".into(),
        description: "提供受信任设备配对、节点能力预检、不可变渲染包与控制端帧租约；默认停用。"
            .into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        scope: ModuleScope::Global,
        builtin: true,
        requires_modules: vec![
            ModuleDependency {
                id: LAN_COLLABORATION_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            },
            ModuleDependency {
                id: RENDER_CENTER_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            },
        ],
        optional_modules: Vec::new(),
        requires_components: vec![ComponentDependency {
            id: PM_BLENDIO_COMPONENT_ID.into(),
            version_requirement: "^1.0".into(),
        }],
        optional_components: Vec::new(),
        conflicts: Vec::new(),
        capabilities: render_farm_capabilities(),
        background_services: Vec::new(),
        contributes: ModuleContributions::default(),
        data_policy: ModuleDataPolicy::default(),
        extensions,
    }
}

pub fn render_farm_module() -> RegisteredModule {
    RegisteredModule {
        manifest: render_farm_manifest(),
        lifecycle: Arc::new(RenderFarmLifecycle),
        diagnostic: false,
    }
}

pub fn render_farm_component() -> CapabilityComponentRegistration {
    CapabilityComponentRegistration {
        id: "builtin.render-farm.service".into(),
        name: "渲染农场可信节点与租约组件".into(),
        version: "1.0.0".into(),
        module_id: RENDER_FARM_MODULE_ID.into(),
        capabilities: render_farm_capabilities(),
    }
}

struct MediaLibraryLifecycle;

impl ModuleLifecycle for MediaLibraryLifecycle {
    fn start<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            crate::media_library::start_runtime();
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(crate::media_library::stop_runtime())
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async move {
            if crate::media_library::is_running() {
                Ok(ModuleHealth::healthy(
                    "媒体资料库待命；资料库数据库按用户选择的目录独立保存",
                ))
            } else {
                Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "媒体资料库当前未启用".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                })
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

fn media_library_capabilities() -> Vec<Capability> {
    vec![
        Capability::FilesystemExternalRead,
        Capability::FilesystemExternalWrite,
        Capability::FilesystemDialogOpen,
    ]
}

pub(crate) fn media_library_manifest() -> ModuleManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(false));
    ModuleManifestV1 {
        schema_version: 1,
        id: MEDIA_LIBRARY_MODULE_ID.into(),
        name: "媒体资料库".into(),
        description:
            "收集、归类和归档图片、视频、音频与参考文件；每个资料库保存独立媒体目录和索引。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        scope: ModuleScope::Global,
        builtin: true,
        requires_modules: Vec::new(),
        optional_modules: Vec::new(),
        requires_components: Vec::new(),
        optional_components: Vec::new(),
        conflicts: Vec::new(),
        capabilities: media_library_capabilities(),
        background_services: Vec::new(),
        contributes: ModuleContributions {
            shell_tabs: vec![MEDIA_LIBRARY_SHELL_TAB_ID.into()],
            tools: vec![MEDIA_LIBRARY_TOOL_ID.into()],
            surfaces: vec![MEDIA_LIBRARY_SURFACE_ID.into()],
            ..ModuleContributions::default()
        },
        data_policy: ModuleDataPolicy::default(),
        extensions,
    }
}

pub fn media_library_module() -> RegisteredModule {
    RegisteredModule {
        manifest: media_library_manifest(),
        lifecycle: Arc::new(MediaLibraryLifecycle),
        diagnostic: false,
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

pub(crate) fn project_resources_manifest() -> ModuleManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    ModuleManifestV1 {
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
        requires_components: Vec::new(),
        optional_components: vec![ComponentDependency {
            id: PM_BLENDIO_COMPONENT_ID.into(),
            version_requirement: "^1.0".into(),
        }],
        conflicts: Vec::new(),
        capabilities: project_resource_capabilities(),
        background_services: vec![
            "project-database-registry".into(),
            "tree-cache-registry".into(),
            "active-project-watcher".into(),
            "dirty-directory-repair".into(),
        ],
        contributes: ModuleContributions {
            workspace_tabs: vec![PROJECT_CACHE_WORKSPACE_TAB_ID.into()],
            tools: vec![PROJECT_CACHE_TOOL_ID.into(), PROJECT_MDT_TOOL_ID.into()],
            surfaces: vec![
                PROJECT_CACHE_SURFACE_ID.into(),
                PROJECT_MDT_SURFACE_ID.into(),
            ],
            settings_sections: vec![
                PROJECT_GLOBAL_EXCLUSIONS_SETTINGS_SECTION_ID.into(),
                PROJECT_RULES_SETTINGS_SECTION_ID.into(),
            ],
            context_commands: vec![PROJECT_COLLECTION_CONTEXT_COMMANDS_ID.into()],
            ..ModuleContributions::default()
        },
        data_policy: ModuleDataPolicy::default(),
        extensions,
    }
}

pub fn project_resources_module(
    databases: crate::project_resources::ProjectDatabaseState,
) -> RegisteredModule {
    RegisteredModule {
        manifest: project_resources_manifest(),
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

struct ProjectManagerLifecycle;

impl ModuleLifecycle for ProjectManagerLifecycle {
    fn start<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async {
            Ok(ModuleHealth::healthy(
                "项目主页、项目标签和文件工作区入口可用",
            ))
        })
    }
}

fn project_manager_capabilities() -> Vec<Capability> {
    vec![
        Capability::ProjectOpen,
        Capability::ProjectFilesRead,
        Capability::ProjectFilesWrite,
        Capability::ProjectMetadataRead,
        Capability::ProjectMetadataWrite,
    ]
}

pub fn project_manager_module() -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: PROJECT_MANAGER_MODULE_ID.into(),
            name: "项目管理器".into(),
            description: "提供项目主页、项目目录、最近项目、项目标签和文件工作区。".into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: vec![ModuleDependency {
                id: PROJECT_RESOURCES_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            }],
            optional_modules: Vec::new(),
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: project_manager_capabilities(),
            background_services: Vec::new(),
            contributes: ModuleContributions {
                shell_tabs: vec![PROJECT_MANAGER_SHELL_TAB_ID.into()],
                surfaces: vec![
                    PROJECT_MANAGER_HOME_SURFACE_ID.into(),
                    PROJECT_MANAGER_WORKSPACE_SURFACE_ID.into(),
                ],
                widgets: vec![
                    PROJECT_MANAGER_DIRECTORY_WIDGET_ID.into(),
                    PROJECT_MANAGER_QUICK_ACTIONS_WIDGET_ID.into(),
                    PROJECT_MANAGER_RECENT_PROJECTS_WIDGET_ID.into(),
                    PROJECT_MANAGER_PROJECT_CATALOG_WIDGET_ID.into(),
                ],
                data_sources: vec![
                    PROJECT_MANAGER_DIRECTORY_DATA_SOURCE_ID.into(),
                    PROJECT_MANAGER_QUICK_ACTIONS_DATA_SOURCE_ID.into(),
                    PROJECT_MANAGER_RECENT_PROJECTS_DATA_SOURCE_ID.into(),
                    PROJECT_MANAGER_PROJECT_CATALOG_DATA_SOURCE_ID.into(),
                ],
                commands: vec![
                    PROJECT_MANAGER_SELECT_ROOT_COMMAND_ID.into(),
                    PROJECT_MANAGER_CLEAR_ROOT_COMMAND_ID.into(),
                    PROJECT_MANAGER_CREATE_PROJECT_COMMAND_ID.into(),
                    PROJECT_MANAGER_IMPORT_PROJECT_COMMAND_ID.into(),
                    PROJECT_MANAGER_OPEN_PROJECT_COMMAND_ID.into(),
                    PROJECT_MANAGER_IGNORE_PROJECT_COMMAND_ID.into(),
                    PROJECT_MANAGER_RESTORE_IGNORED_PROJECT_COMMAND_ID.into(),
                    PROJECT_MANAGER_SHOW_IGNORED_PROJECTS_COMMAND_ID.into(),
                    PROJECT_MANAGER_REMOVE_RECENT_PROJECT_COMMAND_ID.into(),
                ],
                settings_sections: vec![PROJECT_MANAGER_HISTORY_SETTINGS_SECTION_ID.into()],
                ..ModuleContributions::default()
            },
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(ProjectManagerLifecycle),
        diagnostic: false,
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

pub(crate) fn lan_collaboration_manifest() -> ModuleManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(true));
    ModuleManifestV1 {
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
        requires_components: Vec::new(),
        optional_components: Vec::new(),
        conflicts: Vec::new(),
        capabilities: lan_capabilities(),
        background_services: vec![
            "udp-discovery".into(),
            "tcp-message-server".into(),
            "server-client".into(),
            "transfer-supervisor".into(),
        ],
        contributes: ModuleContributions {
            shell_tabs: vec![LAN_SHELL_TAB_ID.into()],
            workspace_tabs: vec![LAN_PROJECT_WORKSPACE_TAB_ID.into()],
            tools: vec![LAN_MAIN_TOOL_ID.into(), LAN_PROJECT_TOOL_ID.into()],
            surfaces: vec![LAN_MAIN_SURFACE_ID.into(), LAN_PROJECT_SURFACE_ID.into()],
            ..ModuleContributions::default()
        },
        data_policy: ModuleDataPolicy::default(),
        extensions,
    }
}

pub fn lan_collaboration_module(
    app_data_dir: PathBuf,
    app_handle: tauri::AppHandle,
) -> RegisteredModule {
    RegisteredModule {
        manifest: lan_collaboration_manifest(),
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
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: vec![Capability::ClipboardRead, Capability::ClipboardWrite],
            background_services: vec![
                "clipboard-listener".into(),
                "native-window".into(),
                "global-hotkey".into(),
            ],
            contributes: ModuleContributions {
                tools: vec![SMART_CLIPBOARD_TOOL_ID.into()],
                surfaces: vec![SMART_CLIPBOARD_SURFACE_ID.into()],
                ..ModuleContributions::default()
            },
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

struct LocalWebConsoleLifecycle {
    app_data_dir: PathBuf,
    app_handle: tauri::AppHandle,
}

impl LocalWebConsoleLifecycle {
    fn new(app_data_dir: PathBuf, app_handle: tauri::AppHandle) -> Self {
        Self {
            app_data_dir,
            app_handle,
        }
    }
}

impl ModuleLifecycle for LocalWebConsoleLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            let server = crate::local_web_console::start_server(
                self.app_handle.clone(),
                self.app_data_dir.clone(),
            )
            .await?;
            let mut details = BTreeMap::new();
            details.insert(
                "address".into(),
                crate::local_web_console::runtime_address()
                    .unwrap_or_else(|| "http://127.0.0.1".into()),
            );
            details.insert("exposure".into(), "仅本机回环地址".into());
            details.insert("authentication".into(), "持久访问令牌".into());
            context.resources.register(
                context.module_id,
                ResourceKind::NetworkListener,
                "本机网页控制台 HTTP 服务",
                details,
                Box::new(move || Box::pin(server.shutdown())),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async {
            match crate::local_web_console::runtime_address() {
                Some(address) => Ok(ModuleHealth::healthy(format!(
                    "本机网页控制台正在监听 {address}"
                ))),
                None => Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "本机网页控制台监听服务未运行".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                }),
            }
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn stop_timeout(&self) -> Duration {
        Duration::from_secs(8)
    }
}

fn local_web_console_capabilities() -> Vec<Capability> {
    vec![
        Capability::AppProfileRead,
        Capability::AppSettingsRead,
        Capability::AppSettingsWrite,
        Capability::NetworkServerConnect,
    ]
}

pub fn local_web_console_module(
    app_data_dir: PathBuf,
    app_handle: tauri::AppHandle,
) -> RegisteredModule {
    RegisteredModule {
        manifest: local_web_console_manifest(),
        lifecycle: Arc::new(LocalWebConsoleLifecycle::new(app_data_dir, app_handle)),
        diagnostic: false,
    }
}

fn local_web_console_manifest() -> ModuleManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("defaultEnabled".into(), Value::Bool(false));
    extensions.insert("exposure".into(), Value::String("localhost-only".into()));
    ModuleManifestV1 {
        schema_version: 1,
        id: LOCAL_WEB_CONSOLE_MODULE_ID.into(),
        name: "本机网页控制台".into(),
        description: "通过真实浏览器查看 Nexora 状态、修改部分设置并执行受限程序控制。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        scope: ModuleScope::Global,
        builtin: true,
        requires_modules: Vec::new(),
        optional_modules: Vec::new(),
        requires_components: Vec::new(),
        optional_components: Vec::new(),
        conflicts: Vec::new(),
        capabilities: local_web_console_capabilities(),
        background_services: vec!["localhost-http-console".into()],
        contributes: ModuleContributions {
            tools: vec![LOCAL_WEB_CONSOLE_TOOL_ID.into()],
            settings_sections: vec![LOCAL_WEB_CONSOLE_SETTINGS_SECTION_ID.into()],
            ..ModuleContributions::default()
        },
        data_policy: ModuleDataPolicy::default(),
        extensions,
    }
}

pub fn local_web_console_component() -> CapabilityComponentRegistration {
    CapabilityComponentRegistration {
        id: "builtin.local-web-console.service".into(),
        name: "本机网页控制台服务组件".into(),
        version: "1.0.0".into(),
        module_id: LOCAL_WEB_CONSOLE_MODULE_ID.into(),
        capabilities: local_web_console_capabilities(),
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

struct ContributionDiagnosticLifecycle;

impl ModuleLifecycle for ContributionDiagnosticLifecycle {
    fn start<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async { Ok(ModuleHealth::healthy("前端贡献目录运行正常")) })
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
        diagnostic_contribution_module(),
    ]
}

fn diagnostic_contribution_module() -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("diagnostic".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: DIAGNOSTIC_CONTRIBUTION_SAMPLE_ID.into(),
            name: "贡献隔离样本".into(),
            description: "验证工具、页面、Widget、DataSource 和工作流节点的动态贡献。".into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: Vec::new(),
            optional_modules: Vec::new(),
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: vec![Capability::AppSettingsRead],
            background_services: Vec::new(),
            contributes: ModuleContributions {
                tools: vec![DIAGNOSTIC_CONTRIBUTION_TOOL_ID.into()],
                workspace_tabs: vec![DIAGNOSTIC_CONTRIBUTION_WORKSPACE_TAB_ID.into()],
                surfaces: vec![DIAGNOSTIC_CONTRIBUTION_SURFACE_ID.into()],
                widgets: vec![DIAGNOSTIC_CONTRIBUTION_WIDGET_ID.into()],
                data_sources: vec![DIAGNOSTIC_CONTRIBUTION_DATA_SOURCE_ID.into()],
                workflow_nodes: vec![DIAGNOSTIC_CONTRIBUTION_WORKFLOW_NODE_ID.into()],
                ..ModuleContributions::default()
            },
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(ContributionDiagnosticLifecycle),
        diagnostic: true,
    }
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
            requires_components: Vec::new(),
            optional_components: Vec::new(),
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
    use std::collections::BTreeSet;

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
        assert_eq!(
            module.manifest.contributes.workspace_tabs,
            vec![PROJECT_CACHE_WORKSPACE_TAB_ID.to_string()]
        );
        assert!(module
            .manifest
            .contributes
            .tools
            .contains(&PROJECT_CACHE_TOOL_ID.to_string()));
        assert!(module
            .manifest
            .contributes
            .surfaces
            .contains(&PROJECT_CACHE_SURFACE_ID.to_string()));
        assert_eq!(
            module.manifest.contributes.context_commands,
            vec![PROJECT_COLLECTION_CONTEXT_COMMANDS_ID.to_string()]
        );
        assert_eq!(
            module.manifest.contributes.settings_sections,
            vec![
                PROJECT_GLOBAL_EXCLUSIONS_SETTINGS_SECTION_ID.to_string(),
                PROJECT_RULES_SETTINGS_SECTION_ID.to_string(),
            ]
        );
    }

    #[test]
    fn project_manager_manifest_owns_project_shell_and_depends_on_resources() {
        let module = project_manager_module();
        assert_eq!(module.manifest.id, PROJECT_MANAGER_MODULE_ID);
        assert_eq!(module.manifest.scope, ModuleScope::Global);
        assert_eq!(
            module.manifest.extensions.get("defaultEnabled"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            module.manifest.requires_modules,
            vec![ModuleDependency {
                id: PROJECT_RESOURCES_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            }]
        );
        assert_eq!(
            module.manifest.contributes.shell_tabs,
            vec![PROJECT_MANAGER_SHELL_TAB_ID.to_string()]
        );
        assert_eq!(
            module.manifest.contributes.surfaces,
            vec![
                PROJECT_MANAGER_HOME_SURFACE_ID.to_string(),
                PROJECT_MANAGER_WORKSPACE_SURFACE_ID.to_string(),
            ]
        );
        assert_eq!(module.manifest.contributes.widgets.len(), 4);
        assert_eq!(module.manifest.contributes.data_sources.len(), 4);
        assert_eq!(module.manifest.contributes.commands.len(), 9);
        assert_eq!(
            module.manifest.contributes.settings_sections,
            vec![PROJECT_MANAGER_HISTORY_SETTINGS_SECTION_ID.to_string()]
        );
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
        assert_eq!(
            module.manifest.requires_modules,
            vec![ModuleDependency {
                id: EXTERNAL_TOOLS_MODULE_ID.into(),
                version_requirement: "^1.0".into(),
            }]
        );
        assert!(module
            .manifest
            .capabilities
            .contains(&Capability::RenderWorkerExecute));
        assert!(module
            .manifest
            .capabilities
            .contains(&Capability::RenderResultCommit));
        assert_eq!(
            module.manifest.contributes.workspace_tabs,
            vec![RENDER_WORKSPACE_TAB_ID.to_string()]
        );
        assert_eq!(
            module.manifest.contributes.shell_tabs,
            vec![EXTERNAL_RENDER_STATION_SHELL_TAB_ID.to_string()]
        );
        assert_eq!(
            module.manifest.contributes.tools,
            vec![
                RENDER_TOOL_ID.to_string(),
                EXTERNAL_RENDER_STATION_TOOL_ID.to_string(),
            ]
        );
        assert_eq!(
            module.manifest.contributes.surfaces,
            vec![
                RENDER_SURFACE_ID.to_string(),
                EXTERNAL_RENDER_STATION_SURFACE_ID.to_string(),
            ]
        );
    }

    #[test]
    fn media_library_manifest_is_optional_and_project_free() {
        let module = media_library_module();
        assert_eq!(module.manifest.id, MEDIA_LIBRARY_MODULE_ID);
        assert_eq!(module.manifest.scope, ModuleScope::Global);
        assert_eq!(
            module.manifest.extensions.get("defaultEnabled"),
            Some(&Value::Bool(false))
        );
        assert!(module.manifest.requires_modules.is_empty());
        assert!(module.manifest.background_services.is_empty());
        assert_eq!(
            module.manifest.contributes.shell_tabs,
            vec![MEDIA_LIBRARY_SHELL_TAB_ID.to_string()]
        );
        assert_eq!(
            module.manifest.contributes.tools,
            vec![MEDIA_LIBRARY_TOOL_ID.to_string()]
        );
        assert_eq!(
            module.manifest.contributes.surfaces,
            vec![MEDIA_LIBRARY_SURFACE_ID.to_string()]
        );
    }

    #[test]
    fn render_farm_manifest_is_opt_in_and_does_not_start_a_transport() {
        let manifest = render_farm_manifest();
        assert_eq!(manifest.id, RENDER_FARM_MODULE_ID);
        assert_eq!(manifest.scope, ModuleScope::Global);
        assert_eq!(
            manifest.extensions.get("defaultEnabled"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            manifest
                .requires_modules
                .iter()
                .map(|dependency| dependency.id.as_str())
                .collect::<Vec<_>>(),
            vec![LAN_COLLABORATION_MODULE_ID, RENDER_CENTER_MODULE_ID]
        );
        assert!(manifest.background_services.is_empty());
        assert!(manifest.contributes.surfaces.is_empty());
        assert!(manifest
            .capabilities
            .contains(&Capability::RenderResultCommit));
        assert!(manifest
            .capabilities
            .contains(&Capability::NetworkLanTransfer));
    }

    #[test]
    fn builtin_contribution_ids_are_unique_within_each_kind() {
        let claims = [
            ("tools", AUTOMATION_PYTHON_TOOL_ID),
            ("tools", AUTOMATION_TASK_TOOL_ID),
            ("surfaces", AUTOMATION_PYTHON_SURFACE_ID),
            ("surfaces", AUTOMATION_TASK_SURFACE_ID),
            ("settingsSections", AUTOMATION_SETTINGS_SECTION_ID),
            ("settingsSections", AUTOMATION_PROJECT_SETTINGS_SECTION_ID),
            ("contextCommands", AUTOMATION_PLUGIN_CONTEXT_COMMANDS_ID),
            ("tools", RENDER_TOOL_ID),
            ("tools", EXTERNAL_RENDER_STATION_TOOL_ID),
            ("workspaceTabs", RENDER_WORKSPACE_TAB_ID),
            ("shellTabs", EXTERNAL_RENDER_STATION_SHELL_TAB_ID),
            ("surfaces", RENDER_SURFACE_ID),
            ("surfaces", EXTERNAL_RENDER_STATION_SURFACE_ID),
            ("shellTabs", MEDIA_LIBRARY_SHELL_TAB_ID),
            ("tools", MEDIA_LIBRARY_TOOL_ID),
            ("surfaces", MEDIA_LIBRARY_SURFACE_ID),
            ("tools", PROJECT_CACHE_TOOL_ID),
            ("tools", PROJECT_MDT_TOOL_ID),
            ("workspaceTabs", PROJECT_CACHE_WORKSPACE_TAB_ID),
            ("surfaces", PROJECT_CACHE_SURFACE_ID),
            ("surfaces", PROJECT_MDT_SURFACE_ID),
            (
                "settingsSections",
                PROJECT_GLOBAL_EXCLUSIONS_SETTINGS_SECTION_ID,
            ),
            ("settingsSections", PROJECT_RULES_SETTINGS_SECTION_ID),
            ("contextCommands", PROJECT_COLLECTION_CONTEXT_COMMANDS_ID),
            ("shellTabs", PROJECT_MANAGER_SHELL_TAB_ID),
            ("surfaces", PROJECT_MANAGER_HOME_SURFACE_ID),
            ("surfaces", PROJECT_MANAGER_WORKSPACE_SURFACE_ID),
            ("widgets", PROJECT_MANAGER_DIRECTORY_WIDGET_ID),
            ("widgets", PROJECT_MANAGER_QUICK_ACTIONS_WIDGET_ID),
            ("widgets", PROJECT_MANAGER_RECENT_PROJECTS_WIDGET_ID),
            ("widgets", PROJECT_MANAGER_PROJECT_CATALOG_WIDGET_ID),
            ("dataSources", PROJECT_MANAGER_DIRECTORY_DATA_SOURCE_ID),
            ("dataSources", PROJECT_MANAGER_QUICK_ACTIONS_DATA_SOURCE_ID),
            (
                "dataSources",
                PROJECT_MANAGER_RECENT_PROJECTS_DATA_SOURCE_ID,
            ),
            (
                "dataSources",
                PROJECT_MANAGER_PROJECT_CATALOG_DATA_SOURCE_ID,
            ),
            ("commands", PROJECT_MANAGER_SELECT_ROOT_COMMAND_ID),
            ("commands", PROJECT_MANAGER_CLEAR_ROOT_COMMAND_ID),
            ("commands", PROJECT_MANAGER_CREATE_PROJECT_COMMAND_ID),
            ("commands", PROJECT_MANAGER_IMPORT_PROJECT_COMMAND_ID),
            ("commands", PROJECT_MANAGER_OPEN_PROJECT_COMMAND_ID),
            ("commands", PROJECT_MANAGER_IGNORE_PROJECT_COMMAND_ID),
            (
                "commands",
                PROJECT_MANAGER_RESTORE_IGNORED_PROJECT_COMMAND_ID,
            ),
            ("commands", PROJECT_MANAGER_SHOW_IGNORED_PROJECTS_COMMAND_ID),
            ("commands", PROJECT_MANAGER_REMOVE_RECENT_PROJECT_COMMAND_ID),
            (
                "settingsSections",
                PROJECT_MANAGER_HISTORY_SETTINGS_SECTION_ID,
            ),
            ("tools", LAN_MAIN_TOOL_ID),
            ("tools", LAN_PROJECT_TOOL_ID),
            ("shellTabs", LAN_SHELL_TAB_ID),
            ("workspaceTabs", LAN_PROJECT_WORKSPACE_TAB_ID),
            ("surfaces", LAN_MAIN_SURFACE_ID),
            ("surfaces", LAN_PROJECT_SURFACE_ID),
            ("tools", SMART_CLIPBOARD_TOOL_ID),
            ("surfaces", SMART_CLIPBOARD_SURFACE_ID),
            ("tools", LOCAL_WEB_CONSOLE_TOOL_ID),
            ("settingsSections", LOCAL_WEB_CONSOLE_SETTINGS_SECTION_ID),
            ("tools", DIAGNOSTIC_CONTRIBUTION_TOOL_ID),
            ("workspaceTabs", DIAGNOSTIC_CONTRIBUTION_WORKSPACE_TAB_ID),
            ("surfaces", DIAGNOSTIC_CONTRIBUTION_SURFACE_ID),
            ("widgets", DIAGNOSTIC_CONTRIBUTION_WIDGET_ID),
            ("dataSources", DIAGNOSTIC_CONTRIBUTION_DATA_SOURCE_ID),
            ("workflowNodes", DIAGNOSTIC_CONTRIBUTION_WORKFLOW_NODE_ID),
        ];
        let mut seen = BTreeSet::new();

        for claim in claims {
            assert!(
                seen.insert(claim),
                "duplicate built-in contribution claim: {}:{}",
                claim.0,
                claim.1
            );
        }
    }

    #[test]
    fn automation_and_clipboard_manifests_declare_stable_contributions() {
        let automation = automation_runtime_module();
        assert_eq!(
            automation.manifest.contributes.tools,
            vec![
                AUTOMATION_PYTHON_TOOL_ID.to_string(),
                AUTOMATION_TASK_TOOL_ID.to_string(),
            ]
        );
        assert_eq!(
            automation.manifest.contributes.settings_sections,
            vec![
                AUTOMATION_SETTINGS_SECTION_ID.to_string(),
                AUTOMATION_PROJECT_SETTINGS_SECTION_ID.to_string(),
            ]
        );
        assert_eq!(
            automation.manifest.contributes.context_commands,
            vec![AUTOMATION_PLUGIN_CONTEXT_COMMANDS_ID.to_string()]
        );

        let clipboard = smart_clipboard_module(std::env::temp_dir());
        assert_eq!(
            clipboard.manifest.contributes.tools,
            vec![SMART_CLIPBOARD_TOOL_ID.to_string()]
        );
        assert_eq!(
            clipboard.manifest.contributes.surfaces,
            vec![SMART_CLIPBOARD_SURFACE_ID.to_string()]
        );
    }

    #[test]
    fn local_web_console_is_optional_and_localhost_scoped() {
        let manifest = local_web_console_manifest();
        assert_eq!(manifest.id, LOCAL_WEB_CONSOLE_MODULE_ID);
        assert_eq!(
            manifest.extensions.get("defaultEnabled"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            manifest.extensions.get("exposure"),
            Some(&Value::String("localhost-only".into()))
        );
        assert_eq!(
            manifest.contributes.tools,
            vec![LOCAL_WEB_CONSOLE_TOOL_ID.to_string()]
        );
        assert_eq!(
            manifest.contributes.settings_sections,
            vec![LOCAL_WEB_CONSOLE_SETTINGS_SECTION_ID.to_string()]
        );
        assert!(manifest
            .capabilities
            .contains(&Capability::AppSettingsWrite));
    }

    #[test]
    fn diagnostic_sample_declares_each_frontend_contribution_kind() {
        let sample = diagnostic_contribution_module();
        assert_eq!(sample.manifest.id, DIAGNOSTIC_CONTRIBUTION_SAMPLE_ID);
        assert_eq!(
            sample.manifest.contributes.tools,
            vec![DIAGNOSTIC_CONTRIBUTION_TOOL_ID.to_string()]
        );
        assert_eq!(
            sample.manifest.contributes.workspace_tabs,
            vec![DIAGNOSTIC_CONTRIBUTION_WORKSPACE_TAB_ID.to_string()]
        );
        assert_eq!(
            sample.manifest.contributes.surfaces,
            vec![DIAGNOSTIC_CONTRIBUTION_SURFACE_ID.to_string()]
        );
        assert_eq!(
            sample.manifest.contributes.widgets,
            vec![DIAGNOSTIC_CONTRIBUTION_WIDGET_ID.to_string()]
        );
        assert_eq!(
            sample.manifest.contributes.data_sources,
            vec![DIAGNOSTIC_CONTRIBUTION_DATA_SOURCE_ID.to_string()]
        );
        assert_eq!(
            sample.manifest.contributes.workflow_nodes,
            vec![DIAGNOSTIC_CONTRIBUTION_WORKFLOW_NODE_ID.to_string()]
        );
    }
}
