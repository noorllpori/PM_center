use pmc_platform::{
    Capability, ComponentContributions, ComponentDependency, ComponentDistribution,
    ComponentManifestV1, ComponentResourceLimits, ComponentRole, ComponentRuntime, ComponentUiMode,
    ExtensionFields, FileHandlerContribution, PageTemplateContribution, PlatformTarget,
    PortDefinition, PortValueType, ShellTemplateContribution, ThemePresetContribution,
    ToolActionContribution, WorkflowNodeContribution, PLATFORM_SCHEMA_VERSION,
};
use serde_json::json;

pub const PM_BLENDIO_COMPONENT_ID: &str = "pmc.blendio";
pub const BLENDER_WORKSPACE_COMPONENT_ID: &str = "nexora.blender.workspace";
pub const IMAGE_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.image";
pub const VIDEO_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.video";
pub const TEXT_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.text";
pub const DIRECTORY_FILE_HANDLER_COMPONENT_ID: &str = "nexora.file-handler.directory";
pub const PRESENTATION_TEMPLATES_COMPONENT_ID: &str = "nexora.presentation.templates";

pub(crate) fn builtin_component_manifests() -> Vec<ComponentManifestV1> {
    vec![
        blendio_component_manifest(),
        blender_workspace_component_manifest(),
        image_file_handler_component_manifest(),
        video_file_handler_component_manifest(),
        text_file_handler_component_manifest(),
        directory_file_handler_component_manifest(),
        presentation_templates_manifest(),
    ]
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
        description: "提供顶部、侧边、紧凑 Shell 适配器以及通用页面和主题预设。".into(),
        version: "1.0.0".into(),
        api_version: "1".into(),
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
