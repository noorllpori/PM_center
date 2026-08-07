//! R12 reference assemblies are testable Profile contracts, not runtime modes.
//!
//! A reference assembly captures the minimum contributions needed to reproduce
//! one supported product shape. It deliberately does not create, select, or
//! branch on a user Profile ID. The normal Profile runtime remains the only
//! runtime source of module/component selection.

use pmc_platform::{
    validate_profile_with_catalogs, ComponentManifestV1, ModuleManifestV1, WorkspaceProfileV1,
};
use serde::Serialize;
use std::collections::BTreeSet;

pub(crate) const PROJECT_MANAGER_REFERENCE_ASSEMBLY_ID: &str = "reference.project-manager";
pub(crate) const LAN_COMMUNICATIONS_REFERENCE_ASSEMBLY_ID: &str = "reference.lan-communications";
pub(crate) const EXTERNAL_BLENDER_RENDERER_REFERENCE_ASSEMBLY_ID: &str =
    "reference.external-blender-renderer";
pub(crate) const MEDIA_LIBRARY_REFERENCE_ASSEMBLY_ID: &str = "reference.media-library";

const PROJECT_MANAGER_MODULE_ID: &str = "builtin.project-manager";
const PROJECT_RESOURCES_MODULE_ID: &str = "builtin.project-resources";
const LAN_COLLABORATION_MODULE_ID: &str = "builtin.lan-collaboration";
const RENDER_CENTER_MODULE_ID: &str = "builtin.render-center";
const MEDIA_LIBRARY_MODULE_ID: &str = "builtin.media-library";
const PROJECT_HOME_SURFACE_ID: &str = "pm-center-project-home";
const PROJECT_HOME_CONTRIBUTION_ID: &str = "builtin.project-manager.home-surface";
const LAN_MAIN_SURFACE_ID: &str = "lan-collaboration-main";
const LAN_MAIN_CONTRIBUTION_ID: &str = "builtin.lan-collaboration.main-surface";
const EXTERNAL_RENDER_STATION_SURFACE_ID: &str = "external-render-station";
const EXTERNAL_RENDER_STATION_CONTRIBUTION_ID: &str =
    "builtin.render-center.external-station-surface";
const MEDIA_LIBRARY_SURFACE_ID: &str = "media-library";
const MEDIA_LIBRARY_CONTRIBUTION_ID: &str = "builtin.media-library.surface";
const PROJECT_DIRECTORY_WIDGET_ID: &str = "builtin.project-manager.project-directory-widget";
const PROJECT_QUICK_ACTIONS_WIDGET_ID: &str = "builtin.project-manager.quick-actions-widget";
const RECENT_PROJECTS_WIDGET_ID: &str = "builtin.project-manager.recent-projects-widget";
const PROJECT_CATALOG_WIDGET_ID: &str = "builtin.project-manager.project-catalog-widget";
const PROJECT_DIRECTORY_DATA_SOURCE_ID: &str =
    "builtin.project-manager.project-directory-data-source";
const PROJECT_QUICK_ACTIONS_DATA_SOURCE_ID: &str =
    "builtin.project-manager.quick-actions-data-source";
const RECENT_PROJECTS_DATA_SOURCE_ID: &str = "builtin.project-manager.recent-projects-data-source";
const PROJECT_CATALOG_DATA_SOURCE_ID: &str = "builtin.project-manager.project-catalog-data-source";
const CREATE_PROJECT_COMMAND_ID: &str = "builtin.project-manager.create-project-command";
const IMPORT_PROJECT_COMMAND_ID: &str = "builtin.project-manager.import-project-command";
const OPEN_PROJECT_COMMAND_ID: &str = "builtin.project-manager.open-project-command";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceAssemblySpec {
    pub id: &'static str,
    pub schema_version: u16,
    pub name: &'static str,
    pub description: &'static str,
    pub project_context_required: bool,
    pub required_module_ids: Vec<&'static str>,
    pub forbidden_module_ids: Vec<&'static str>,
    pub home_surface_id: &'static str,
    pub required_surface_contribution_ids: Vec<&'static str>,
    pub required_widget_ids: Vec<&'static str>,
    pub required_data_source_ids: Vec<&'static str>,
    pub required_command_ids: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceAssemblyValidation {
    pub assembly_id: String,
    pub valid: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceAssemblySnapshot {
    pub assembly_id: String,
    pub profile_id: String,
    pub enabled_module_ids: Vec<String>,
    pub effective_component_ids: Vec<String>,
    pub validation: ReferenceAssemblyValidation,
}

pub(crate) fn project_manager_reference_spec() -> ReferenceAssemblySpec {
    ReferenceAssemblySpec {
        id: PROJECT_MANAGER_REFERENCE_ASSEMBLY_ID,
        schema_version: 1,
        name: "项目管理器参考装配",
        description: "R12-A 的最小项目管理器装配合同；允许叠加局域网、渲染和其他组件。",
        project_context_required: true,
        required_module_ids: vec![PROJECT_MANAGER_MODULE_ID],
        forbidden_module_ids: Vec::new(),
        home_surface_id: PROJECT_HOME_SURFACE_ID,
        required_surface_contribution_ids: vec![PROJECT_HOME_CONTRIBUTION_ID],
        required_widget_ids: vec![
            PROJECT_DIRECTORY_WIDGET_ID,
            PROJECT_QUICK_ACTIONS_WIDGET_ID,
            RECENT_PROJECTS_WIDGET_ID,
            PROJECT_CATALOG_WIDGET_ID,
        ],
        required_data_source_ids: vec![
            PROJECT_DIRECTORY_DATA_SOURCE_ID,
            PROJECT_QUICK_ACTIONS_DATA_SOURCE_ID,
            RECENT_PROJECTS_DATA_SOURCE_ID,
            PROJECT_CATALOG_DATA_SOURCE_ID,
        ],
        required_command_ids: vec![
            CREATE_PROJECT_COMMAND_ID,
            IMPORT_PROJECT_COMMAND_ID,
            OPEN_PROJECT_COMMAND_ID,
        ],
    }
}

pub(crate) fn lan_communications_reference_spec() -> ReferenceAssemblySpec {
    ReferenceAssemblySpec {
        id: LAN_COMMUNICATIONS_REFERENCE_ASSEMBLY_ID,
        schema_version: 1,
        name: "局域网通信端参考装配",
        description: "R12-C 的独立通信端合同；不允许隐式带入项目管理器或项目资源运行时。",
        project_context_required: false,
        required_module_ids: vec![LAN_COLLABORATION_MODULE_ID],
        forbidden_module_ids: vec![PROJECT_MANAGER_MODULE_ID, PROJECT_RESOURCES_MODULE_ID],
        home_surface_id: LAN_MAIN_SURFACE_ID,
        required_surface_contribution_ids: vec![LAN_MAIN_CONTRIBUTION_ID],
        required_widget_ids: Vec::new(),
        required_data_source_ids: Vec::new(),
        required_command_ids: Vec::new(),
    }
}

pub(crate) fn external_blender_renderer_reference_spec() -> ReferenceAssemblySpec {
    ReferenceAssemblySpec {
        id: EXTERNAL_BLENDER_RENDERER_REFERENCE_ASSEMBLY_ID,
        schema_version: 1,
        name: "Blender 外部渲染器参考装配",
        description: "R12-D 的独立 Blender 渲染器合同；队列命名空间位于应用数据目录，不允许隐式带入项目管理器或项目资源运行时。",
        project_context_required: false,
        required_module_ids: vec![RENDER_CENTER_MODULE_ID],
        forbidden_module_ids: vec![PROJECT_MANAGER_MODULE_ID, PROJECT_RESOURCES_MODULE_ID],
        home_surface_id: EXTERNAL_RENDER_STATION_SURFACE_ID,
        required_surface_contribution_ids: vec![EXTERNAL_RENDER_STATION_CONTRIBUTION_ID],
        required_widget_ids: Vec::new(),
        required_data_source_ids: Vec::new(),
        required_command_ids: Vec::new(),
    }
}

pub(crate) fn media_library_reference_spec() -> ReferenceAssemblySpec {
    ReferenceAssemblySpec {
        id: MEDIA_LIBRARY_REFERENCE_ASSEMBLY_ID,
        schema_version: 1,
        name: "媒体资料库参考装配",
        description: "R12-B 的独立媒体资料库合同；媒体索引位于用户选择的资料库目录，不允许隐式带入项目管理器或项目资源运行时。",
        project_context_required: false,
        required_module_ids: vec![MEDIA_LIBRARY_MODULE_ID],
        forbidden_module_ids: vec![PROJECT_MANAGER_MODULE_ID, PROJECT_RESOURCES_MODULE_ID],
        home_surface_id: MEDIA_LIBRARY_SURFACE_ID,
        required_surface_contribution_ids: vec![MEDIA_LIBRARY_CONTRIBUTION_ID],
        required_widget_ids: Vec::new(),
        required_data_source_ids: Vec::new(),
        required_command_ids: Vec::new(),
    }
}

pub(crate) fn validate_reference_assembly(
    spec: &ReferenceAssemblySpec,
    profile: &WorkspaceProfileV1,
    modules: &[ModuleManifestV1],
    components: &[ComponentManifestV1],
) -> ReferenceAssemblySnapshot {
    let mut issues = Vec::new();
    let effective_components = match validate_profile_with_catalogs(profile, modules, components) {
        Ok(effective) => effective,
        Err(error) => {
            issues.push(format!(
                "Profile 合同或依赖无效：{}（{}）",
                error.message, error.path
            ));
            BTreeSet::new()
        }
    };

    let enabled_modules = profile
        .enabled_modules
        .iter()
        .map(|selection| selection.id.as_str())
        .collect::<BTreeSet<_>>();
    for required in &spec.required_module_ids {
        if !enabled_modules.contains(required) {
            issues.push(format!("缺少必需模块：{required}"));
        }
    }
    for forbidden in &spec.forbidden_module_ids {
        if enabled_modules.contains(forbidden) {
            issues.push(format!("不应启用模块：{forbidden}"));
        }
    }
    if profile.shell_layout.home.as_deref() != Some(spec.home_surface_id) {
        issues.push(format!("主页必须是：{}", spec.home_surface_id));
    }

    let surface_contributions = profile
        .surfaces
        .iter()
        .filter_map(|surface| surface.contribution.as_deref())
        .collect::<BTreeSet<_>>();
    let widget_ids = profile
        .surfaces
        .iter()
        .flat_map(|surface| surface.widgets.iter().map(|widget| widget.widget.as_str()))
        .collect::<BTreeSet<_>>();
    let data_source_ids = profile
        .data_sources
        .iter()
        .map(|source| source.source.as_str())
        .collect::<BTreeSet<_>>();
    let command_ids = profile
        .command_bindings
        .iter()
        .map(|binding| binding.command.as_str())
        .collect::<BTreeSet<_>>();

    require_all(
        "页面贡献",
        &spec.required_surface_contribution_ids,
        &surface_contributions,
        &mut issues,
    );
    require_all(
        "Widget",
        &spec.required_widget_ids,
        &widget_ids,
        &mut issues,
    );
    require_all(
        "数据源",
        &spec.required_data_source_ids,
        &data_source_ids,
        &mut issues,
    );
    require_all(
        "命令",
        &spec.required_command_ids,
        &command_ids,
        &mut issues,
    );

    let ownership = ContributionOwnership::from_enabled_catalogs(
        &enabled_modules,
        &effective_components,
        modules,
        components,
    );
    ownership.validate_profile(profile, &mut issues);

    ReferenceAssemblySnapshot {
        assembly_id: spec.id.into(),
        profile_id: profile.id.clone(),
        enabled_module_ids: enabled_modules.into_iter().map(str::to_owned).collect(),
        effective_component_ids: effective_components.into_iter().collect(),
        validation: ReferenceAssemblyValidation {
            assembly_id: spec.id.into(),
            valid: issues.is_empty(),
            issues,
        },
    }
}

fn require_all(kind: &str, required: &[&str], actual: &BTreeSet<&str>, issues: &mut Vec<String>) {
    for value in required {
        if !actual.contains(value) {
            issues.push(format!("缺少必需{kind}：{value}"));
        }
    }
}

#[derive(Default)]
struct ContributionOwnership {
    surfaces: BTreeSet<String>,
    widgets: BTreeSet<String>,
    data_sources: BTreeSet<String>,
    commands: BTreeSet<String>,
    tools: BTreeSet<String>,
}

impl ContributionOwnership {
    fn from_enabled_catalogs(
        enabled_modules: &BTreeSet<&str>,
        effective_components: &BTreeSet<String>,
        modules: &[ModuleManifestV1],
        components: &[ComponentManifestV1],
    ) -> Self {
        let mut ownership = Self::default();
        for module in modules
            .iter()
            .filter(|module| enabled_modules.contains(module.id.as_str()))
        {
            ownership
                .surfaces
                .extend(module.contributes.surfaces.iter().cloned());
            ownership
                .widgets
                .extend(module.contributes.widgets.iter().cloned());
            ownership
                .data_sources
                .extend(module.contributes.data_sources.iter().cloned());
            ownership
                .commands
                .extend(module.contributes.commands.iter().cloned());
            ownership
                .tools
                .extend(module.contributes.tools.iter().cloned());
        }
        for component in components
            .iter()
            .filter(|component| effective_components.contains(&component.id))
        {
            ownership
                .widgets
                .extend(component.contributes.widgets.iter().cloned());
            ownership
                .data_sources
                .extend(component.contributes.data_sources.iter().cloned());
            ownership.tools.extend(
                component
                    .contributes
                    .tool_actions
                    .iter()
                    .map(|action| action.id.clone()),
            );
        }
        ownership
    }

    fn validate_profile(&self, profile: &WorkspaceProfileV1, issues: &mut Vec<String>) {
        for surface in &profile.surfaces {
            if let Some(contribution) = surface.contribution.as_deref() {
                require_owned("页面贡献", contribution, &self.surfaces, issues);
            }
            for widget in &surface.widgets {
                require_owned("Widget", &widget.widget, &self.widgets, issues);
            }
        }
        for source in &profile.data_sources {
            require_owned("数据源", &source.source, &self.data_sources, issues);
        }
        for binding in &profile.command_bindings {
            require_owned("命令", &binding.command, &self.commands, issues);
        }
        for tool in &profile.shell_layout.pinned_tools {
            require_owned("固定工具", tool, &self.tools, issues);
        }
    }
}

fn require_owned(kind: &str, value: &str, owners: &BTreeSet<String>, issues: &mut Vec<String>) {
    if !owners.contains(value) {
        issues.push(format!("{kind} 未由当前有效模块或组件贡献：{value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::builtin_components::builtin_component_manifests;
    use crate::platform::builtin_modules::{
        external_tools_module, lan_collaboration_manifest, media_library_manifest,
        project_manager_module, project_resources_manifest, render_center_manifest,
    };
    use crate::platform::profile_runtime::{
        build_default_profile, build_external_blender_renderer_reference_profile,
        build_lan_communications_reference_profile, build_media_library_reference_profile,
        build_project_manager_reference_profile,
    };
    use pmc_platform::{ExtensionFields, ModuleContributions, ModuleDataPolicy, ModuleScope};
    use std::fs;
    use std::path::Path;

    fn project_manager_catalog() -> (Vec<ModuleManifestV1>, Vec<ComponentManifestV1>) {
        (
            vec![
                project_resources_manifest(),
                project_manager_module().manifest,
            ],
            builtin_component_manifests(),
        )
    }

    fn external_blender_renderer_catalog() -> (Vec<ModuleManifestV1>, Vec<ComponentManifestV1>) {
        (
            vec![external_tools_module().manifest, render_center_manifest()],
            builtin_component_manifests(),
        )
    }

    fn media_library_catalog() -> (Vec<ModuleManifestV1>, Vec<ComponentManifestV1>) {
        (
            vec![media_library_manifest()],
            builtin_component_manifests(),
        )
    }

    #[test]
    fn project_manager_reference_profile_is_valid_and_complete() {
        let (modules, components) = project_manager_catalog();
        let profile = build_project_manager_reference_profile(&modules).unwrap();
        let snapshot = validate_reference_assembly(
            &project_manager_reference_spec(),
            &profile,
            &modules,
            &components,
        );

        assert!(
            snapshot.validation.valid,
            "{:?}",
            snapshot.validation.issues
        );
        assert_eq!(snapshot.profile_id, PROJECT_MANAGER_REFERENCE_ASSEMBLY_ID);
        assert_eq!(
            snapshot.enabled_module_ids,
            vec![
                "builtin.project-manager".to_string(),
                "builtin.project-resources".to_string(),
            ]
        );
    }

    #[test]
    fn current_default_profile_preserves_project_manager_reference_contract() {
        let (modules, components) = project_manager_catalog();
        let profile = build_default_profile(&modules);
        let snapshot = validate_reference_assembly(
            &project_manager_reference_spec(),
            &profile,
            &modules,
            &components,
        );

        assert!(
            snapshot.validation.valid,
            "默认装配不再满足项目管理器参考合同：{:?}",
            snapshot.validation.issues
        );
    }

    #[test]
    fn inactive_module_contributions_cannot_leak_into_reference_profile() {
        let (mut modules, components) = project_manager_catalog();
        modules.push(ModuleManifestV1 {
            schema_version: 1,
            id: "builtin.inactive-reference-fixture".into(),
            name: "Inactive fixture".into(),
            description: String::new(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: Vec::new(),
            optional_modules: Vec::new(),
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: Vec::new(),
            background_services: Vec::new(),
            contributes: ModuleContributions {
                tools: vec!["builtin.inactive-reference-fixture.tool".into()],
                ..ModuleContributions::default()
            },
            data_policy: ModuleDataPolicy::default(),
            extensions: ExtensionFields::new(),
        });
        let mut profile = build_project_manager_reference_profile(&modules).unwrap();
        profile
            .shell_layout
            .pinned_tools
            .push("builtin.inactive-reference-fixture.tool".into());

        let snapshot = validate_reference_assembly(
            &project_manager_reference_spec(),
            &profile,
            &modules,
            &components,
        );

        assert!(!snapshot.validation.valid);
        assert!(snapshot.validation.issues.iter().any(|issue| {
            issue.contains("builtin.inactive-reference-fixture.tool")
                && issue.contains("未由当前有效模块或组件贡献")
        }));
    }

    #[test]
    fn lan_communications_reference_profile_is_project_free_and_complete() {
        let modules = vec![lan_collaboration_manifest()];
        let components = builtin_component_manifests();
        let profile = build_lan_communications_reference_profile(&modules).unwrap();
        let snapshot = validate_reference_assembly(
            &lan_communications_reference_spec(),
            &profile,
            &modules,
            &components,
        );

        assert!(
            snapshot.validation.valid,
            "{:?}",
            snapshot.validation.issues
        );
        assert_eq!(
            snapshot.profile_id,
            LAN_COMMUNICATIONS_REFERENCE_ASSEMBLY_ID
        );
        assert_eq!(
            snapshot.enabled_module_ids,
            vec![LAN_COLLABORATION_MODULE_ID.to_string()]
        );
        let lan = &modules[0];
        assert!(lan.requires_modules.is_empty());
        assert!(lan.background_services.iter().all(|service| {
            !matches!(
                service.as_str(),
                "project-database-registry"
                    | "tree-cache-registry"
                    | "active-project-watcher"
                    | "dirty-directory-repair"
            )
        }));
    }

    #[test]
    fn external_blender_renderer_reference_profile_is_project_free_and_complete() {
        let (modules, components) = external_blender_renderer_catalog();
        let profile = build_external_blender_renderer_reference_profile(&modules).unwrap();
        let snapshot = validate_reference_assembly(
            &external_blender_renderer_reference_spec(),
            &profile,
            &modules,
            &components,
        );

        assert!(
            snapshot.validation.valid,
            "{:?}",
            snapshot.validation.issues
        );
        assert_eq!(
            snapshot.profile_id,
            EXTERNAL_BLENDER_RENDERER_REFERENCE_ASSEMBLY_ID
        );
        assert_eq!(
            snapshot.enabled_module_ids,
            vec![
                "builtin.external-tools".to_string(),
                RENDER_CENTER_MODULE_ID.to_string(),
            ]
        );
        assert!(modules.iter().all(|module| {
            !module.background_services.iter().any(|service| {
                matches!(
                    service.as_str(),
                    "project-database-registry"
                        | "tree-cache-registry"
                        | "active-project-watcher"
                        | "dirty-directory-repair"
                )
            })
        }));
    }

    #[test]
    fn media_library_reference_profile_is_project_free_and_complete() {
        let (modules, components) = media_library_catalog();
        let profile = build_media_library_reference_profile(&modules).unwrap();
        let snapshot = validate_reference_assembly(
            &media_library_reference_spec(),
            &profile,
            &modules,
            &components,
        );

        assert!(
            snapshot.validation.valid,
            "{:?}",
            snapshot.validation.issues
        );
        assert_eq!(snapshot.profile_id, MEDIA_LIBRARY_REFERENCE_ASSEMBLY_ID);
        assert_eq!(
            snapshot.enabled_module_ids,
            vec![MEDIA_LIBRARY_MODULE_ID.to_string()]
        );
        assert!(modules[0].background_services.is_empty());
    }

    #[test]
    fn reference_assembly_ids_are_not_runtime_branch_keys() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sources = Vec::new();
        collect_rust_sources(&source_root, &mut sources);
        let prohibited = sources
            .iter()
            .flat_map(|path| {
                fs::read_to_string(path)
                    .unwrap_or_default()
                    .lines()
                    .enumerate()
                    .filter_map(|(line_number, line)| {
                        let trimmed = line.trim();
                        ((trimmed.starts_with("if ") || trimmed.starts_with("match "))
                            && (trimmed.contains("REFERENCE_ASSEMBLY_ID")
                                || trimmed.contains("reference.")))
                        .then(|| format!("{}:{}", path.display(), line_number + 1))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert!(
            prohibited.is_empty(),
            "参考装配 ID 只能用于规范/夹具，不能成为运行时模式分支：{prohibited:?}"
        );
    }

    fn collect_rust_sources(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                collect_rust_sources(&path, output);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                output.push(path);
            }
        }
    }
}
