use pmc_platform::{
    Capability, ComponentContributions, ComponentDistribution, ComponentManifestV1,
    ComponentResourceLimits, ComponentRole, ComponentRuntime, ComponentUiMode, ExtensionFields,
    PlatformTarget, PLATFORM_SCHEMA_VERSION,
};
use serde_json::json;

pub const PM_BLENDIO_COMPONENT_ID: &str = "pmc.blendio";

pub fn builtin_component_manifests() -> Vec<ComponentManifestV1> {
    vec![blendio_component_manifest()]
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
        contributes: ComponentContributions::default(),
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
