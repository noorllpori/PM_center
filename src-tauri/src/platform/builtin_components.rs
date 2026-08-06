use pmc_platform::{
    Capability, ComponentContributions, ComponentDistribution, ComponentManifestV1,
    ComponentResourceLimits, ComponentRole, ComponentRuntime, ComponentUiMode, ExtensionFields,
    PageTemplateContribution, PlatformTarget, PortDefinition, PortValueType,
    ShellTemplateContribution, ThemePresetContribution, ToolActionContribution,
    WorkflowNodeContribution, PLATFORM_SCHEMA_VERSION,
};
use serde_json::json;

pub const PM_BLENDIO_COMPONENT_ID: &str = "pmc.blendio";
pub const PRESENTATION_TEMPLATES_COMPONENT_ID: &str = "nexora.presentation.templates";

pub fn builtin_component_manifests() -> Vec<ComponentManifestV1> {
    vec![
        blendio_component_manifest(),
        presentation_templates_manifest(),
    ]
}

fn blendio_component_manifest() -> ComponentManifestV1 {
    let mut extensions = ExtensionFields::new();
    extensions.insert("removable".into(), json!(true));
    extensions.insert("installSource".into(), json!("installer-bundle"));
    extensions.insert("hostAdapter".into(), json!("builtin-rust"));

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
        entry: Some("bin/windows-x64/blendio.exe".into()),
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
