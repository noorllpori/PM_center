use crate::component::{component_map, ComponentManifestV1};
use crate::ids::{validate_local_id, validate_stable_id, validate_version_requirement};
use crate::module_manifest::{module_map, ModuleManifestV1};
use crate::{
    parse_contract, ContractError, ContractErrorCode, ContractResult, ExtensionFields,
    ValidateContract,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileModuleSelection {
    pub id: String,
    #[serde(default = "default_version_requirement")]
    pub version_requirement: String,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileComponentSelection {
    pub id: String,
    #[serde(default = "default_version_requirement")]
    pub version_requirement: String,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

fn default_version_requirement() -> String {
    "*".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePresentationBinding {
    pub id: String,
    #[serde(default = "default_version_requirement")]
    pub version_requirement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ShellNavigationKind {
    TopBar,
    SideBar,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileShellLayout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    #[serde(default)]
    pub navigation: Vec<String>,
    #[serde(default)]
    pub pinned_tools: Vec<String>,
    #[serde(default = "default_navigation_kind")]
    pub navigation_kind: ShellNavigationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_template: Option<ProfilePresentationBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_preset: Option<ProfilePresentationBinding>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

fn default_navigation_kind() -> ShellNavigationKind {
    ShellNavigationKind::TopBar
}

impl Default for ProfileShellLayout {
    fn default() -> Self {
        Self {
            home: None,
            navigation: Vec::new(),
            pinned_tools: Vec::new(),
            navigation_kind: default_navigation_kind(),
            shell_template: None,
            theme_preset: None,
            extensions: ExtensionFields::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SurfaceKind {
    Dashboard,
    ShellPage,
    WorkspaceTab,
    IndependentWindow,
    DetailPanel,
    Sidebar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SurfaceLayoutKind {
    ResponsiveGrid,
    Stack,
    Split,
    ListDetail,
    ContributionDefined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GridPlacement {
    pub column: u16,
    pub row: u16,
    pub width: u16,
    pub height: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum VisibilityRule {
    ModuleEnabled {
        #[serde(rename = "moduleId")]
        #[schemars(rename = "moduleId")]
        module_id: String,
    },
    VariableEquals {
        name: String,
        value: String,
    },
    Not {
        rule: Box<VisibilityRule>,
    },
    All {
        rules: Vec<VisibilityRule>,
    },
    Any {
        rules: Vec<VisibilityRule>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileWidget {
    pub id: String,
    pub widget: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub order: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridPlacement>,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<VisibilityRule>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSurface {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub kind: SurfaceKind,
    pub layout: SurfaceLayoutKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<ProfilePresentationBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_preset: Option<ProfilePresentationBinding>,
    #[serde(default)]
    pub widgets: Vec<ProfileWidget>,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DataSourceScope {
    Global,
    Project,
    Profile,
    Surface,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDataSource {
    pub id: String,
    pub source: String,
    pub scope: DataSourceScope,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CommandPlacement {
    Toolbar,
    FeatureCenter,
    ContextMenu,
    Shortcut,
    SurfaceAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCommandBinding {
    pub id: String,
    pub command: String,
    pub placement: CommandPlacement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileWorkflowBinding {
    pub id: String,
    pub trigger: String,
    pub workflow: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProfileV1 {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub enabled_modules: Vec<ProfileModuleSelection>,
    #[serde(default)]
    pub enabled_components: Vec<ProfileComponentSelection>,
    #[serde(default)]
    pub module_settings: BTreeMap<String, Value>,
    #[serde(default)]
    pub component_settings: BTreeMap<String, Value>,
    #[serde(default)]
    pub shell_layout: ProfileShellLayout,
    #[serde(default)]
    pub surfaces: Vec<ProfileSurface>,
    #[serde(default)]
    pub data_sources: Vec<ProfileDataSource>,
    #[serde(default)]
    pub command_bindings: Vec<ProfileCommandBinding>,
    #[serde(default)]
    pub workflow_bindings: Vec<ProfileWorkflowBinding>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

pub fn parse_workspace_profile(input: &str) -> ContractResult<WorkspaceProfileV1> {
    parse_contract(input)
}

impl ValidateContract for WorkspaceProfileV1 {
    fn validate_contract(&self) -> ContractResult<()> {
        if self.schema_version != crate::PLATFORM_SCHEMA_VERSION {
            return Err(ContractError::new(
                ContractErrorCode::UnsupportedSchemaVersion,
                "$.schemaVersion",
                format!("不支持 schemaVersion {}", self.schema_version),
            ));
        }
        let mut component_ids = BTreeSet::new();
        for (index, component) in self.enabled_components.iter().enumerate() {
            validate_stable_id(&component.id, &format!("$.enabledComponents[{index}].id"))?;
            validate_version_requirement(
                &component.version_requirement,
                &format!("$.enabledComponents[{index}].versionRequirement"),
            )?;
            if !component_ids.insert(component.id.as_str()) {
                return duplicate(format!("$.enabledComponents[{index}].id"), &component.id);
            }
        }
        for component_id in self.component_settings.keys() {
            validate_stable_id(component_id, &format!("$.componentSettings.{component_id}"))?;
        }
        validate_stable_id(&self.id, "$.id")?;
        if self.name.trim().is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MalformedDocument,
                "$.name",
                "装配方案名称不能为空",
            ));
        }

        let mut module_ids = BTreeSet::new();
        for (index, module) in self.enabled_modules.iter().enumerate() {
            validate_stable_id(&module.id, &format!("$.enabledModules[{index}].id"))?;
            validate_version_requirement(
                &module.version_requirement,
                &format!("$.enabledModules[{index}].versionRequirement"),
            )?;
            if !module_ids.insert(module.id.as_str()) {
                return duplicate(format!("$.enabledModules[{index}].id"), &module.id);
            }
        }
        for module_id in self.module_settings.keys() {
            validate_stable_id(module_id, &format!("$.moduleSettings.{module_id}"))?;
            if !module_ids.contains(module_id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReference,
                    format!("$.moduleSettings.{module_id}"),
                    "设置引用了未启用模块",
                ));
            }
        }

        if let Some(binding) = &self.shell_layout.shell_template {
            validate_presentation_binding(binding, "$.shellLayout.shellTemplate")?;
        }
        if let Some(binding) = &self.shell_layout.theme_preset {
            validate_presentation_binding(binding, "$.shellLayout.themePreset")?;
        }

        let mut data_source_ids = BTreeSet::new();
        for (index, source) in self.data_sources.iter().enumerate() {
            validate_local_id(&source.id, &format!("$.dataSources[{index}].id"))?;
            validate_stable_id(&source.source, &format!("$.dataSources[{index}].source"))?;
            if !data_source_ids.insert(source.id.as_str()) {
                return duplicate(format!("$.dataSources[{index}].id"), &source.id);
            }
        }

        let variable_ids: BTreeSet<&str> = self.variables.keys().map(String::as_str).collect();
        for variable in &variable_ids {
            validate_local_id(variable, &format!("$.variables.{variable}"))?;
        }

        let mut surface_ids = BTreeSet::new();
        for (index, surface) in self.surfaces.iter().enumerate() {
            validate_local_id(&surface.id, &format!("$.surfaces[{index}].id"))?;
            if !surface_ids.insert(surface.id.as_str()) {
                return duplicate(format!("$.surfaces[{index}].id"), &surface.id);
            }
            if let Some(contribution) = &surface.contribution {
                validate_stable_id(contribution, &format!("$.surfaces[{index}].contribution"))?;
            }
            if let Some(binding) = &surface.template {
                validate_presentation_binding(binding, &format!("$.surfaces[{index}].template"))?;
            }
            if let Some(binding) = &surface.theme_preset {
                validate_presentation_binding(
                    binding,
                    &format!("$.surfaces[{index}].themePreset"),
                )?;
            }
            let mut widget_ids = BTreeSet::new();
            for (widget_index, widget) in surface.widgets.iter().enumerate() {
                let path = format!("$.surfaces[{index}].widgets[{widget_index}]");
                validate_local_id(&widget.id, &format!("{path}.id"))?;
                validate_stable_id(&widget.widget, &format!("{path}.widget"))?;
                if !widget_ids.insert(widget.id.as_str()) {
                    return duplicate(format!("{path}.id"), &widget.id);
                }
                if let Some(data_source) = &widget.data_source {
                    validate_local_id(data_source, &format!("{path}.dataSource"))?;
                    if !data_source_ids.contains(data_source.as_str()) {
                        return Err(ContractError::new(
                            ContractErrorCode::InvalidReference,
                            format!("{path}.dataSource"),
                            format!("数据源不存在: {data_source}"),
                        ));
                    }
                }
                if let Some(grid) = &widget.grid {
                    if grid.width == 0 || grid.height == 0 {
                        return Err(ContractError::new(
                            ContractErrorCode::MalformedDocument,
                            format!("{path}.grid"),
                            "网格宽高必须大于 0",
                        ));
                    }
                }
                if let Some(rule) = &widget.visible_when {
                    validate_visibility_rule(
                        rule,
                        &format!("{path}.visibleWhen"),
                        &module_ids,
                        &variable_ids,
                    )?;
                }
            }
        }

        if let Some(home) = &self.shell_layout.home {
            validate_surface_reference(home, "$.shellLayout.home", &surface_ids)?;
        }
        validate_unique_surface_references(
            &self.shell_layout.navigation,
            "$.shellLayout.navigation",
            &surface_ids,
        )?;
        validate_unique_stable_ids(&self.shell_layout.pinned_tools, "$.shellLayout.pinnedTools")?;

        let mut command_ids = BTreeSet::new();
        for (index, binding) in self.command_bindings.iter().enumerate() {
            let path = format!("$.commandBindings[{index}]");
            validate_local_id(&binding.id, &format!("{path}.id"))?;
            validate_stable_id(&binding.command, &format!("{path}.command"))?;
            if !command_ids.insert(binding.id.as_str()) {
                return duplicate(format!("{path}.id"), &binding.id);
            }
            if let Some(surface) = &binding.surface {
                validate_surface_reference(surface, &format!("{path}.surface"), &surface_ids)?;
            }
            if matches!(binding.placement, CommandPlacement::Shortcut)
                && binding.shortcut.as_deref().is_none_or(str::is_empty)
            {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("{path}.shortcut"),
                    "快捷键入口必须声明 shortcut",
                ));
            }
        }

        let mut workflow_binding_ids = BTreeSet::new();
        for (index, binding) in self.workflow_bindings.iter().enumerate() {
            let path = format!("$.workflowBindings[{index}]");
            validate_local_id(&binding.id, &format!("{path}.id"))?;
            validate_stable_id(&binding.trigger, &format!("{path}.trigger"))?;
            validate_stable_id(&binding.workflow, &format!("{path}.workflow"))?;
            if !workflow_binding_ids.insert(binding.id.as_str()) {
                return duplicate(format!("{path}.id"), &binding.id);
            }
        }
        Ok(())
    }
}

fn validate_presentation_binding(
    binding: &ProfilePresentationBinding,
    path: &str,
) -> ContractResult<()> {
    validate_stable_id(&binding.id, &format!("{path}.id"))?;
    validate_version_requirement(
        &binding.version_requirement,
        &format!("{path}.versionRequirement"),
    )?;
    if let Some(variant) = &binding.variant {
        validate_local_id(variant, &format!("{path}.variant"))?;
    }
    Ok(())
}

fn validate_visibility_rule<'a>(
    rule: &'a VisibilityRule,
    path: &str,
    modules: &BTreeSet<&'a str>,
    variables: &BTreeSet<&'a str>,
) -> ContractResult<()> {
    match rule {
        VisibilityRule::ModuleEnabled { module_id } => {
            validate_stable_id(module_id, &format!("{path}.moduleId"))?;
            if !modules.contains(module_id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReference,
                    format!("{path}.moduleId"),
                    format!("可见条件引用了未启用模块: {module_id}"),
                ));
            }
        }
        VisibilityRule::VariableEquals { name, .. } => {
            validate_local_id(name, &format!("{path}.name"))?;
            if !variables.contains(name.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReference,
                    format!("{path}.name"),
                    format!("可见条件引用了不存在的变量: {name}"),
                ));
            }
        }
        VisibilityRule::Not { rule } => {
            validate_visibility_rule(rule, &format!("{path}.rule"), modules, variables)?
        }
        VisibilityRule::All { rules } | VisibilityRule::Any { rules } => {
            if rules.is_empty() {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("{path}.rules"),
                    "组合可见条件不能为空",
                ));
            }
            for (index, rule) in rules.iter().enumerate() {
                validate_visibility_rule(
                    rule,
                    &format!("{path}.rules[{index}]"),
                    modules,
                    variables,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_unique_surface_references(
    values: &[String],
    path: &str,
    surfaces: &BTreeSet<&str>,
) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        validate_surface_reference(value, &format!("{path}[{index}]"), surfaces)?;
        if !seen.insert(value.as_str()) {
            return duplicate(format!("{path}[{index}]"), value);
        }
    }
    Ok(())
}

fn validate_surface_reference(
    value: &str,
    path: &str,
    surfaces: &BTreeSet<&str>,
) -> ContractResult<()> {
    validate_local_id(value, path)?;
    if !surfaces.contains(value) {
        return Err(ContractError::new(
            ContractErrorCode::InvalidReference,
            path,
            format!("页面不存在: {value}"),
        ));
    }
    Ok(())
}

fn validate_unique_stable_ids(values: &[String], path: &str) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        validate_stable_id(value, &format!("{path}[{index}]"))?;
        if !seen.insert(value.as_str()) {
            return duplicate(format!("{path}[{index}]"), value);
        }
    }
    Ok(())
}

fn duplicate(path: String, value: &str) -> ContractResult<()> {
    Err(ContractError::new(
        ContractErrorCode::DuplicateId,
        path,
        format!("重复标识符: {value}"),
    ))
}

pub fn validate_profile_with_modules(
    profile: &WorkspaceProfileV1,
    manifests: &[ModuleManifestV1],
) -> ContractResult<()> {
    profile.validate_contract()?;
    crate::validate_module_graph(manifests)?;
    let modules = module_map(manifests);
    let enabled: BTreeSet<&str> = profile
        .enabled_modules
        .iter()
        .map(|module| module.id.as_str())
        .collect();

    for selection in &profile.enabled_modules {
        let Some(manifest) = modules.get(selection.id.as_str()) else {
            return Err(ContractError::new(
                ContractErrorCode::MissingDependency,
                "$.enabledModules",
                format!("未安装模块: {}", selection.id),
            ));
        };
        let requirement = semver::VersionReq::parse(&selection.version_requirement).unwrap();
        let version = semver::Version::parse(&manifest.version).unwrap();
        if !requirement.matches(&version) {
            return Err(ContractError::new(
                ContractErrorCode::MissingDependency,
                "$.enabledModules",
                format!(
                    "模块 {} 版本 {} 不满足 {}",
                    selection.id, manifest.version, selection.version_requirement
                ),
            ));
        }
        for dependency in &manifest.requires_modules {
            if !enabled.contains(dependency.id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::MissingDependency,
                    "$.enabledModules",
                    format!("模块 {} 需要 {}", manifest.id, dependency.id),
                ));
            }
        }
        for conflict in &manifest.conflicts {
            if enabled.contains(conflict.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::ModuleConflict,
                    "$.enabledModules",
                    format!("模块 {} 与 {} 冲突", manifest.id, conflict),
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_profile_with_catalogs(
    profile: &WorkspaceProfileV1,
    module_manifests: &[ModuleManifestV1],
    component_manifests: &[ComponentManifestV1],
) -> ContractResult<BTreeSet<String>> {
    validate_profile_with_modules(profile, module_manifests)?;
    crate::validate_component_graph(component_manifests)?;
    let components = component_map(component_manifests);
    let modules = module_map(module_manifests);
    let mut effective = BTreeSet::new();

    for selection in &profile.enabled_components {
        validate_component_selection(selection, &components, "$.enabledComponents")?;
        add_component_closure(&selection.id, &components, &mut effective)?;
    }

    for selection in &profile.enabled_modules {
        let Some(module) = modules.get(selection.id.as_str()) else {
            continue;
        };
        for dependency in &module.requires_components {
            validate_component_dependency(
                &module.id,
                dependency,
                &components,
                "requiresComponents",
            )?;
            add_component_closure(&dependency.id, &components, &mut effective)?;
        }
        for dependency in &module.optional_components {
            if components.contains_key(dependency.id.as_str()) {
                validate_component_dependency(
                    &module.id,
                    dependency,
                    &components,
                    "optionalComponents",
                )?;
                add_component_closure(&dependency.id, &components, &mut effective)?;
            }
        }
    }

    for component_id in profile.component_settings.keys() {
        if !effective.contains(component_id) {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReference,
                format!("$.componentSettings.{component_id}"),
                "设置引用了未启用或未被依赖的组件",
            ));
        }
    }
    Ok(effective)
}

fn validate_component_selection(
    selection: &ProfileComponentSelection,
    components: &BTreeMap<&str, &ComponentManifestV1>,
    path: &str,
) -> ContractResult<()> {
    let Some(component) = components.get(selection.id.as_str()) else {
        return Err(ContractError::new(
            ContractErrorCode::MissingDependency,
            path,
            format!("未安装组件: {}", selection.id),
        ));
    };
    let requirement = semver::VersionReq::parse(&selection.version_requirement).unwrap();
    let version = semver::Version::parse(&component.version).unwrap();
    if !requirement.matches(&version) {
        return Err(ContractError::new(
            ContractErrorCode::MissingDependency,
            path,
            format!(
                "组件 {} 版本 {} 不满足 {}",
                selection.id, component.version, selection.version_requirement
            ),
        ));
    }
    Ok(())
}

fn validate_component_dependency(
    owner_id: &str,
    dependency: &crate::ComponentDependency,
    components: &BTreeMap<&str, &ComponentManifestV1>,
    field: &str,
) -> ContractResult<()> {
    let Some(component) = components.get(dependency.id.as_str()) else {
        return Err(ContractError::new(
            ContractErrorCode::MissingDependency,
            format!("$.{owner_id}.{field}"),
            format!("缺少必需组件: {}", dependency.id),
        ));
    };
    let requirement = semver::VersionReq::parse(&dependency.version_requirement).unwrap();
    let version = semver::Version::parse(&component.version).unwrap();
    if !requirement.matches(&version) {
        return Err(ContractError::new(
            ContractErrorCode::MissingDependency,
            format!("$.{owner_id}.{field}"),
            format!(
                "组件 {} 版本 {} 不满足 {}",
                component.id, component.version, dependency.version_requirement
            ),
        ));
    }
    Ok(())
}

fn add_component_closure(
    component_id: &str,
    components: &BTreeMap<&str, &ComponentManifestV1>,
    effective: &mut BTreeSet<String>,
) -> ContractResult<()> {
    if !effective.insert(component_id.to_string()) {
        return Ok(());
    }
    let component = components.get(component_id).ok_or_else(|| {
        ContractError::new(
            ContractErrorCode::MissingDependency,
            "$.enabledComponents",
            format!("未安装组件: {component_id}"),
        )
    })?;
    for dependency in &component.requires_components {
        validate_component_dependency(&component.id, dependency, components, "requiresComponents")?;
        add_component_closure(&dependency.id, components, effective)?;
    }
    for dependency in &component.optional_components {
        if components.contains_key(dependency.id.as_str()) {
            validate_component_dependency(
                &component.id,
                dependency,
                components,
                "optionalComponents",
            )?;
            add_component_closure(&dependency.id, components, effective)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_component_manifest, parse_module_manifest};

    #[test]
    fn unknown_top_level_fields_survive_round_trip() {
        let input = r#"{
          "schemaVersion": 1,
          "id": "test.profile",
          "name": "Test",
          "futureOptionalField": {"enabled": true}
        }"#;
        let profile = parse_workspace_profile(input).unwrap();
        let value = serde_json::to_value(profile).unwrap();
        assert_eq!(value["futureOptionalField"]["enabled"], true);
    }

    #[test]
    fn rejects_duplicate_surface_ids() {
        let input = r#"{
          "schemaVersion": 1,
          "id": "test.profile",
          "name": "Test",
          "surfaces": [
            {"id":"main","kind":"dashboard","layout":"stack"},
            {"id":"main","kind":"dashboard","layout":"stack"}
          ]
        }"#;
        let error = parse_workspace_profile(input).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::DuplicateId);
    }

    #[test]
    fn rejects_enabled_module_conflicts() {
        let alpha = parse_module_manifest(
            r#"{
              "schemaVersion":1,"id":"test.alpha","name":"Alpha","version":"1.0.0",
              "apiVersion":"1","scope":"global","conflicts":["test.beta"]
            }"#,
        )
        .unwrap();
        let beta = parse_module_manifest(
            r#"{
              "schemaVersion":1,"id":"test.beta","name":"Beta","version":"1.0.0",
              "apiVersion":"1","scope":"global"
            }"#,
        )
        .unwrap();
        let profile = parse_workspace_profile(
            r#"{
              "schemaVersion":1,"id":"test.profile","name":"Test",
              "enabledModules":[{"id":"test.alpha"},{"id":"test.beta"}]
            }"#,
        )
        .unwrap();
        let error = validate_profile_with_modules(&profile, &[alpha, beta]).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::ModuleConflict);
    }

    #[test]
    fn resolves_module_component_dependencies_into_effective_set() {
        let module = parse_module_manifest(
            r#"{
              "schemaVersion":1,"id":"test.viewer","name":"Viewer","version":"1.0.0",
              "apiVersion":"1","scope":"global",
              "requiresComponents":[{"id":"test.reader","versionRequirement":"^1.0"}]
            }"#,
        )
        .unwrap();
        let component = parse_component_manifest(
            r#"{
              "schemaVersion":1,"id":"test.reader","name":"Reader","version":"1.2.0",
              "apiVersion":"1","runtime":"native-process","platforms":["windows-x64"],
              "entry":"bin/reader.exe"
            }"#,
        )
        .unwrap();
        let profile = parse_workspace_profile(
            r#"{
              "schemaVersion":1,"id":"test.profile","name":"Test",
              "enabledModules":[{"id":"test.viewer"}]
            }"#,
        )
        .unwrap();

        let effective = validate_profile_with_catalogs(&profile, &[module], &[component]).unwrap();
        assert_eq!(effective, BTreeSet::from(["test.reader".to_string()]));
    }

    #[test]
    fn blocks_profile_when_required_component_is_not_installed() {
        let module = parse_module_manifest(
            r#"{
              "schemaVersion":1,"id":"test.viewer","name":"Viewer","version":"1.0.0",
              "apiVersion":"1","scope":"global",
              "requiresComponents":[{"id":"test.reader"}]
            }"#,
        )
        .unwrap();
        let profile = parse_workspace_profile(
            r#"{
              "schemaVersion":1,"id":"test.profile","name":"Test",
              "enabledModules":[{"id":"test.viewer"}]
            }"#,
        )
        .unwrap();

        let error = validate_profile_with_catalogs(&profile, &[module], &[]).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::MissingDependency);
    }
}
