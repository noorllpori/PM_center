use pmc_platform::{
    AutomationCapabilityOperation, AutomationCommandContribution, AutomationContextRequirement,
    AutomationExecutionSemantics, Capability, ComponentCategory, ComponentContributions,
    ComponentDependency, ComponentDistribution, ComponentManifestV1, ComponentResourceLimits,
    ComponentRole, ComponentRuntime, ComponentUiMode, ExtensionFields, FileHandlerContribution,
    PageTemplateContribution, PlatformTarget, PortDefinition, PortValueType,
    ShellTemplateContribution, ThemePresetContribution, ToolActionContribution,
    UiExtensionMultiplicity, UiExtensionPointContribution, UiExtensionPointKind,
    WorkflowNodeContribution, PLATFORM_SCHEMA_VERSION,
};
use serde_json::json;

use super::file_operations::FILE_OPERATIONS_COMPONENT_ID;

pub const PM_BLENDIO_COMPONENT_ID: &str = "pmc.blendio";
pub const PROJECT_MANAGER_COMPONENT_ID: &str = "nexora.project-manager";
pub const BLENDER_WORKSPACE_COMPONENT_ID: &str = "nexora.blender.workspace";
pub const IMAGE_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.image";
pub const VIDEO_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.video";
pub const TEXT_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.text";
pub const DIRECTORY_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.directory";
pub const PRESENTATION_TEMPLATES_COMPONENT_ID: &str = "nexora.presentation.templates";

pub(crate) fn builtin_component_manifests() -> Vec<ComponentManifestV1> {
    vec![
        file_operations_component_manifest(),
        project_manager_component_manifest(),
        blendio_component_manifest(),
        blender_workspace_component_manifest(),
        image_file_handler_component_manifest(),
        video_file_handler_component_manifest(),
        text_file_handler_component_manifest(),
        directory_file_handler_component_manifest(),
        presentation_templates_manifest(),
    ]
}

fn project_manager_component_manifest() -> ComponentManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("hostAdapter".into(), json!("builtin-catalog"));
    extensions.insert(
        "compatibilityModuleId".into(),
        json!("builtin.project-manager"),
    );
    extensions.insert("defaultEnabled".into(), json!(true));
    ComponentManifestV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: PROJECT_MANAGER_COMPONENT_ID.into(),
        name: "Nexora 项目管理器".into(),
        description: "提供项目主页、项目标签和文件工作区，并公开受控的界面扩展点。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        category: Some(ComponentCategory::Workspace),
        tags: vec![
            "project".into(),
            "workspace".into(),
            "ui-extension-target".into(),
        ],
        runtime: ComponentRuntime::BuiltinRust,
        role: ComponentRole::Feature,
        distribution: ComponentDistribution::Bundled,
        ui_mode: ComponentUiMode::Hosted,
        platforms: vec![PlatformTarget::Any],
        entry: None,
        capabilities: Vec::new(),
        requires_components: vec![ComponentDependency {
            id: FILE_OPERATIONS_COMPONENT_ID.into(),
            version_requirement: "^1.0".into(),
        }],
        optional_components: Vec::new(),
        contributes: ComponentContributions {
            ui_extension_points: vec![
                ui_extension_point(
                    "nexora.project-manager.project-toolbar",
                    "项目工具栏",
                    UiExtensionPointKind::Slot,
                    UiExtensionMultiplicity::Many,
                    Some(44),
                    Some(160),
                ),
                ui_extension_point(
                    "nexora.project-manager.project-sidebar",
                    "项目侧栏",
                    UiExtensionPointKind::Slot,
                    UiExtensionMultiplicity::Many,
                    Some(96),
                    Some(480),
                ),
                ui_extension_point(
                    "nexora.project-manager.file-details",
                    "文件详情区域",
                    UiExtensionPointKind::Slot,
                    UiExtensionMultiplicity::Many,
                    Some(96),
                    Some(480),
                ),
                ui_extension_point(
                    "nexora.project-manager.file-context-menu",
                    "文件右键菜单",
                    UiExtensionPointKind::Slot,
                    UiExtensionMultiplicity::Many,
                    None,
                    Some(360),
                ),
                ui_extension_point(
                    "nexora.project-manager.project-home-widgets",
                    "项目主页 Widget 区",
                    UiExtensionPointKind::Slot,
                    UiExtensionMultiplicity::Many,
                    Some(120),
                    Some(640),
                ),
                ui_extension_point(
                    "nexora.project-manager.project-status-bar",
                    "项目状态栏",
                    UiExtensionPointKind::Slot,
                    UiExtensionMultiplicity::Many,
                    Some(28),
                    Some(120),
                ),
                ui_extension_point(
                    "nexora.project-manager.project-workspace",
                    "项目工作区整页替换",
                    UiExtensionPointKind::Surface,
                    UiExtensionMultiplicity::One,
                    Some(240),
                    None,
                ),
            ],
            ..ComponentContributions::default()
        },
        resources: ComponentResourceLimits::default(),
        publisher: Some("Nexora".into()),
        extensions,
    }
}

fn ui_extension_point(
    id: &str,
    name: &str,
    kind: UiExtensionPointKind,
    multiplicity: UiExtensionMultiplicity,
    min_height: Option<u16>,
    max_height: Option<u16>,
) -> UiExtensionPointContribution {
    UiExtensionPointContribution {
        id: id.into(),
        name: name.into(),
        kind,
        multiplicity,
        context_schema: json!({
            "type": "object",
            "properties": {
                "projectId": {"type": ["string", "null"]},
                "projectPath": {"type": ["string", "null"]},
                "relativeSelection": {"type": "array", "items": {"type": "string"}},
                "theme": {"type": "string"},
                "language": {"type": "string"},
                "size": {"type": "object"}
            }
        }),
        min_height,
        max_height,
        extensions: ExtensionFields::new(),
    }
}

fn file_operations_component_manifest() -> ComponentManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("hostAdapter".into(), json!("nexora-file-operations"));
    extensions.insert(
        "compatibilityModuleId".into(),
        json!("builtin.project-resources"),
    );
    extensions.insert("defaultEnabled".into(), json!(true));
    ComponentManifestV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: FILE_OPERATIONS_COMPONENT_ID.into(),
        name: "文件操作".into(),
        description: "为项目文件、受授权外部目录、目录缓存和文件监听提供受控的公共操作。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        category: Some(ComponentCategory::Service),
        tags: vec![
            "file".into(),
            "project".into(),
            "cache".into(),
            "service".into(),
        ],
        runtime: ComponentRuntime::BuiltinRust,
        role: ComponentRole::Service,
        distribution: ComponentDistribution::Bundled,
        ui_mode: ComponentUiMode::Hosted,
        platforms: vec![PlatformTarget::Any],
        entry: None,
        capabilities: vec![
            Capability::ProjectFilesRead,
            Capability::ProjectFilesWrite,
            Capability::FilesystemExternalRead,
            Capability::FilesystemExternalWrite,
            Capability::FilesystemDialogOpen,
            Capability::CacheInspect,
            Capability::CacheMaintain,
        ],
        requires_components: Vec::new(),
        optional_components: Vec::new(),
        contributes: ComponentContributions {
            automation_commands: vec![
                file_operation_command(
                    "nexora.file-operations.project-describe",
                    "project.describe",
                    "读取项目上下文",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::Pure,
                    &[Capability::ProjectFilesRead],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.directory-list",
                    "directory.list",
                    "列出目录",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.entry-stat",
                    "entry.stat",
                    "读取文件元数据",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.entry-exists",
                    "entry.exists",
                    "检查文件是否存在",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.entry-search",
                    "entry.search",
                    "搜索文件",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.file-read",
                    "file.read",
                    "读取文件内容",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.file-hash",
                    "file.hash",
                    "计算文件摘要",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.file-write",
                    "file.write",
                    "原子写入文件",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Idempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.directory-create",
                    "directory.create",
                    "创建目录",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Idempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.entry-copy",
                    "entry.copy",
                    "复制文件或目录",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalRead,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.entry-move",
                    "entry.move",
                    "移动文件或目录",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.entry-rename",
                    "entry.rename",
                    "重命名文件或目录",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.entry-delete",
                    "entry.delete",
                    "删除文件或目录",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Delete,
                ),
                file_operation_command(
                    "nexora.file-operations.batch-execute",
                    "batch.execute",
                    "批量执行文件操作",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.stream-open-read",
                    "stream.open-read",
                    "打开文件读取流",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.stream-read",
                    "stream.read",
                    "读取文件流分块",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Pure,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.stream-open-write",
                    "stream.open-write",
                    "打开文件写入流",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Idempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.stream-write",
                    "stream.write",
                    "写入文件流分块",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Idempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.stream-commit",
                    "stream.commit",
                    "提交文件写入流",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Idempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.stream-abort",
                    "stream.abort",
                    "取消文件流",
                    AutomationContextRequirement::Either,
                    AutomationExecutionSemantics::Idempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.external-select",
                    "external.select",
                    "创建单次外部路径授权",
                    AutomationContextRequirement::Global,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[Capability::FilesystemDialogOpen],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.external-grant-directory",
                    "external.grant-directory",
                    "授权外部目录",
                    AutomationContextRequirement::Global,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[Capability::FilesystemDialogOpen],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.external-list-grants",
                    "external.list-grants",
                    "列出外部路径授权",
                    AutomationContextRequirement::Global,
                    AutomationExecutionSemantics::Pure,
                    &[Capability::FilesystemExternalRead],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.external-revoke-grant",
                    "external.revoke-grant",
                    "撤销外部路径授权",
                    AutomationContextRequirement::Global,
                    AutomationExecutionSemantics::Idempotent,
                    &[Capability::FilesystemExternalRead],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.external-import",
                    "external.import",
                    "导入外部文件",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[
                        Capability::ProjectFilesWrite,
                        Capability::FilesystemExternalRead,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.external-export",
                    "external.export",
                    "导出项目文件",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::NonIdempotent,
                    &[
                        Capability::ProjectFilesRead,
                        Capability::FilesystemExternalWrite,
                    ],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.cache-status",
                    "cache.status",
                    "读取目录缓存状态",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::Pure,
                    &[Capability::CacheInspect],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.cache-query",
                    "cache.query",
                    "查询目录缓存",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::Pure,
                    &[Capability::CacheInspect],
                    AutomationCapabilityOperation::Read,
                ),
                file_operation_command(
                    "nexora.file-operations.cache-invalidate",
                    "cache.invalidate",
                    "失效项目目录缓存",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::Idempotent,
                    &[Capability::CacheMaintain],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.cache-refresh-directory",
                    "cache.refresh-directory",
                    "刷新目录缓存",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::Idempotent,
                    &[Capability::CacheMaintain],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.cache-rebuild-project",
                    "cache.rebuild-project",
                    "重建项目目录缓存",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::Idempotent,
                    &[Capability::CacheMaintain],
                    AutomationCapabilityOperation::Write,
                ),
                file_operation_command(
                    "nexora.file-operations.watcher-status",
                    "watcher.status",
                    "读取文件监听状态",
                    AutomationContextRequirement::ProjectRequired,
                    AutomationExecutionSemantics::Pure,
                    &[Capability::CacheInspect],
                    AutomationCapabilityOperation::Read,
                ),
            ],
            ..ComponentContributions::default()
        },
        resources: ComponentResourceLimits {
            max_parallelism: Some(4),
            timeout_ms: Some(600_000),
            ..ComponentResourceLimits::default()
        },
        publisher: Some("Nexora".into()),
        extensions,
    }
}

fn file_operation_command(
    id: &str,
    command: &str,
    name: &str,
    context_requirement: AutomationContextRequirement,
    execution_semantics: AutomationExecutionSemantics,
    capabilities: &[Capability],
    capability_operation: AutomationCapabilityOperation,
) -> AutomationCommandContribution {
    AutomationCommandContribution {
        id: id.into(),
        command: command.into(),
        name: name.into(),
        description: "Nexora 文件操作公共服务命令。".into(),
        context_requirement,
        execution_semantics,
        required_capability: capabilities.first().copied(),
        required_capabilities: capabilities.to_vec(),
        capability_operation,
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        max_attempts: 1,
        max_parallelism: Some(4),
        timeout_ms: Some(600_000),
        extensions: ExtensionFields::new(),
    }
}

fn blendio_component_manifest() -> ComponentManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("removable".into(), json!(true));
    extensions.insert("installSource".into(), json!("installer-bundle"));
    extensions.insert("hostAdapter".into(), json!("bundled-resource-process"));

    ComponentManifestV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: PM_BLENDIO_COMPONENT_ID.into(),
        name: "BlenderIO 文件服务".into(),
        description: "读取、诊断和受控编辑 .blend 文件，并向宿主功能提供结构化结果。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        category: Some(ComponentCategory::Service),
        tags: vec!["blender".into(), "parser".into(), "service".into()],
        runtime: ComponentRuntime::NativeProcess,
        role: ComponentRole::Service,
        distribution: ComponentDistribution::Bundled,
        ui_mode: ComponentUiMode::Hosted,
        platforms: vec![PlatformTarget::WindowsX64],
        entry: Some("pmc-blendio-service.exe".into()),
        capabilities: vec![
            Capability::ProjectFilesRead,
            Capability::ProjectFilesWrite,
            Capability::FilesystemExternalRead,
            Capability::FilesystemExternalWrite,
        ],
        requires_components: Vec::new(),
        optional_components: Vec::new(),
        contributes: ComponentContributions {
            automation_commands: vec![AutomationCommandContribution {
                id: "pmc.blendio.inspect".into(),
                command: "inspect".into(),
                name: "读取 Blender 文件".into(),
                description: "读取 .blend 文件并返回结构化场景摘要。".into(),
                context_requirement: AutomationContextRequirement::Either,
                execution_semantics: AutomationExecutionSemantics::Pure,
                required_capability: Some(Capability::ProjectFilesRead),
                required_capabilities: Vec::new(),
                capability_operation: AutomationCapabilityOperation::Read,
                input_schema: json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "additionalProperties": false
                }),
                output_schema: json!({"type": "object"}),
                max_attempts: 1,
                max_parallelism: Some(4),
                timeout_ms: Some(600_000),
                extensions: ExtensionFields::new(),
            }],
            tool_actions: vec![ToolActionContribution {
                id: "blender.inspect-file".into(),
                command: "inspect".into(),
                name: "解析 Blender 文件".into(),
                extensions: ExtensionFields::new(),
            }],
            workflow_nodes: vec![WorkflowNodeContribution {
                id: "blender.inspect".into(),
                command: "inspect".into(),
                name: "读取 Blender 场景".into(),
                inputs: vec![PortDefinition {
                    name: "file".into(),
                    value_type: PortValueType::File,
                    required: true,
                    description: "要解析的 .blend 文件".into(),
                    extensions: ExtensionFields::new(),
                }],
                outputs: vec![PortDefinition {
                    name: "summary".into(),
                    value_type: PortValueType::Json,
                    required: true,
                    description: "Blender 文件结构摘要".into(),
                    extensions: ExtensionFields::new(),
                }],
                extensions: ExtensionFields::new(),
            }],
            data_sources: vec!["blender.file-summary".into()],
            ..ComponentContributions::default()
        },
        resources: ComponentResourceLimits {
            max_memory_mb: Some(1024),
            max_parallelism: Some(4),
            timeout_ms: Some(600_000),
            extensions: ExtensionFields::new(),
        },
        publisher: Some("Nexora".into()),
        extensions,
    }
}

fn blender_workspace_component_manifest() -> ComponentManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("removable".into(), json!(true));
    extensions.insert("installSource".into(), json!("installer-bundle"));
    extensions.insert("autoInstall".into(), json!(false));
    ComponentManifestV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: BLENDER_WORKSPACE_COMPONENT_ID.into(),
        name: "Nexora Blender 工作区".into(),
        description: "提供 Blender 文件的内部工作区页面，依赖 BlenderIO 文件服务。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        category: Some(ComponentCategory::Workspace),
        tags: vec!["blender".into(), "workspace".into()],
        runtime: ComponentRuntime::BuiltinRust,
        role: ComponentRole::Feature,
        distribution: ComponentDistribution::Bundled,
        ui_mode: ComponentUiMode::Hosted,
        platforms: vec![PlatformTarget::Any],
        entry: None,
        capabilities: Vec::new(),
        requires_components: vec![ComponentDependency {
            id: PM_BLENDIO_COMPONENT_ID.into(),
            version_requirement: "*".into(),
        }],
        optional_components: Vec::new(),
        contributes: ComponentContributions {
            file_handlers: vec![file_handler(
                "nexora.blender.open-workspace",
                "Blender 文件工作区",
                &["open", "open-internal", "preview"],
                &["blend"],
                &["application/x-blender"],
                200,
                Some("blend"),
                &["file"],
            )],
            ..ComponentContributions::default()
        },
        resources: ComponentResourceLimits::default(),
        publisher: Some("Nexora".into()),
        extensions,
    }
}

fn image_file_handler_component_manifest() -> ComponentManifestV1 {
    file_handler_component(
        IMAGE_FILE_HANDLER_COMPONENT_ID,
        "Nexora 图像预览",
        "提供可替换的图像工作区页面。",
        "image",
        &[
            "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "svg", "ico", "avif",
        ],
        &["image/*"],
        &["open", "open-internal", "preview", "thumbnail"],
    )
}

fn video_file_handler_component_manifest() -> ComponentManifestV1 {
    file_handler_component(
        VIDEO_FILE_HANDLER_COMPONENT_ID,
        "Nexora 视频播放器",
        "提供可替换的视频播放工作区页面。",
        "video",
        &[
            "mp4", "m4v", "mov", "avi", "mkv", "webm", "wmv", "flv", "mpeg", "mpg", "m2ts",
        ],
        &["video/*"],
        &["open", "open-internal", "preview"],
    )
}

fn text_file_handler_component_manifest() -> ComponentManifestV1 {
    file_handler_component(
        TEXT_FILE_HANDLER_COMPONENT_ID,
        "Nexora 文本编辑器",
        "提供可替换的文本和 Markdown 工作区页面。",
        "text",
        &[
            "txt", "md", "markdown", "mdx", "json", "jsonc", "js", "ts", "tsx", "jsx", "html",
            "htm", "css", "scss", "py", "rs", "c", "h", "cpp", "hpp", "java", "go", "php", "rb",
            "xml", "yml", "yaml", "toml", "ini", "conf", "log", "csv", "tsv",
        ],
        &["text/plain", "text/markdown", "application/json"],
        &["open", "open-internal", "preview", "edit"],
    )
}

fn directory_file_handler_component_manifest() -> ComponentManifestV1 {
    file_handler_component_with_kinds(
        DIRECTORY_FILE_HANDLER_COMPONENT_ID,
        "Nexora 目录工作区",
        "提供可替换的目录浏览工作区页面。",
        "directory",
        &[],
        &["inode/directory"],
        &["open", "open-internal"],
        &["directory"],
    )
}

fn file_handler_component(
    id: &str,
    name: &str,
    description: &str,
    target: &str,
    extensions: &[&str],
    mime_types: &[&str],
    intents: &[&str],
) -> ComponentManifestV1 {
    file_handler_component_with_kinds(
        id,
        name,
        description,
        target,
        extensions,
        mime_types,
        intents,
        &["file"],
    )
}

fn file_handler_component_with_kinds(
    id: &str,
    name: &str,
    description: &str,
    target: &str,
    extensions: &[&str],
    mime_types: &[&str],
    intents: &[&str],
    file_kinds: &[&str],
) -> ComponentManifestV1 {
    let mut extra = ExtensionFields::new();
    extra.insert("removable".into(), json!(true));
    extra.insert("installSource".into(), json!("installer-bundle"));
    ComponentManifestV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: id.into(),
        name: name.into(),
        description: description.into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        category: Some(ComponentCategory::FileHandler),
        tags: vec!["file-handler".into(), target.into()],
        runtime: ComponentRuntime::BuiltinRust,
        role: ComponentRole::Feature,
        distribution: ComponentDistribution::Bundled,
        ui_mode: ComponentUiMode::Hosted,
        platforms: vec![PlatformTarget::Any],
        entry: None,
        capabilities: Vec::new(),
        requires_components: Vec::new(),
        optional_components: Vec::new(),
        contributes: ComponentContributions {
            file_handlers: vec![file_handler(
                &format!("{id}.open"),
                name,
                intents,
                extensions,
                mime_types,
                100,
                Some(target),
                file_kinds,
            )],
            ..ComponentContributions::default()
        },
        resources: ComponentResourceLimits::default(),
        publisher: Some("Nexora".into()),
        extensions: extra,
    }
}

fn file_handler(
    id: &str,
    name: &str,
    intents: &[&str],
    extensions: &[&str],
    mime_types: &[&str],
    priority: i32,
    workspace_target: Option<&str>,
    file_kinds: &[&str],
) -> FileHandlerContribution {
    FileHandlerContribution {
        id: id.into(),
        name: name.into(),
        intents: intents.iter().map(|value| (*value).into()).collect(),
        extensions: extensions.iter().map(|value| (*value).into()).collect(),
        mime_types: mime_types.iter().map(|value| (*value).into()).collect(),
        file_kinds: file_kinds.iter().map(|value| (*value).into()).collect(),
        priority,
        workspace_target: workspace_target.map(str::to_string),
        extensions_extra: ExtensionFields::new(),
    }
}

fn presentation_templates_manifest() -> ComponentManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("removable".into(), json!(true));
    extensions.insert("installSource".into(), json!("installer-bundle"));
    extensions.insert("hostAdapter".into(), json!("builtin-catalog"));

    ComponentManifestV1 {
        schema_version: PLATFORM_SCHEMA_VERSION,
        id: PRESENTATION_TEMPLATES_COMPONENT_ID.into(),
        name: "Nexora 内置表现模板".into(),
        description: "提供顶部、侧边、紧凑、空白主页 Shell 适配器以及通用页面和主题预设。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
        category: Some(ComponentCategory::Appearance),
        tags: vec!["appearance".into(), "template".into(), "theme".into()],
        runtime: ComponentRuntime::DataPack,
        role: ComponentRole::Data,
        distribution: ComponentDistribution::Bundled,
        ui_mode: ComponentUiMode::None,
        platforms: vec![PlatformTarget::Any],
        entry: Some("presentation".into()),
        capabilities: Vec::new(),
        requires_components: Vec::new(),
        optional_components: Vec::new(),
        contributes: ComponentContributions {
            shell_templates: vec![
                shell_template("nexora.shell.top-bar", "顶部导航", "top-bar"),
                shell_template("nexora.shell.side-bar", "侧边导航", "side-bar"),
                shell_template("nexora.shell.minimal", "紧凑导航", "minimal"),
                shell_template("nexora.shell.blank-home", "空白主页", "blank-home"),
            ],
            page_templates: vec![
                page_template(
                    "nexora.page.dashboard-grid",
                    "仪表盘网格",
                    &["header", "content", "sidebar"],
                ),
                page_template("nexora.page.list-detail", "列表与详情", &["list", "detail"]),
                page_template("nexora.page.workspace", "工作区", &["toolbar", "content"]),
            ],
            theme_presets: vec![
                theme_preset("nexora.theme.system", "跟随系统", "system"),
                theme_preset("nexora.theme.light", "浅色", "light"),
                theme_preset("nexora.theme.dark", "深色", "dark"),
            ],
            ..ComponentContributions::default()
        },
        resources: ComponentResourceLimits::default(),
        publisher: Some("Nexora".into()),
        extensions,
    }
}

fn shell_template(id: &str, name: &str, adapter: &str) -> ShellTemplateContribution {
    ShellTemplateContribution {
        id: id.into(),
        name: name.into(),
        version: "1.0.0".into(),
        variants: Vec::new(),
        adapter: Some(adapter.into()),
        extensions: ExtensionFields::new(),
    }
}

fn page_template(id: &str, name: &str, regions: &[&str]) -> PageTemplateContribution {
    PageTemplateContribution {
        id: id.into(),
        name: name.into(),
        version: "1.0.0".into(),
        regions: regions.iter().map(|region| (*region).into()).collect(),
        extensions: ExtensionFields::new(),
    }
}

fn theme_preset(id: &str, name: &str, mode: &str) -> ThemePresetContribution {
    ThemePresetContribution {
        id: id.into(),
        name: name.into(),
        version: "1.0.0".into(),
        tokens: [("mode".into(), json!(mode))].into_iter().collect(),
        extensions: ExtensionFields::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmc_platform::{validate_component_graph, ValidateContract};

    #[test]
    fn bundled_component_catalog_is_valid() {
        let manifests = builtin_component_manifests();
        for manifest in &manifests {
            manifest.validate_contract().unwrap();
        }
        validate_component_graph(&manifests).unwrap();
    }
}
