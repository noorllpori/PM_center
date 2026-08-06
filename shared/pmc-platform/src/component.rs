use crate::ids::{
    validate_api_version, validate_local_id, validate_relative_path, validate_stable_id,
    validate_version, validate_version_requirement,
};
use crate::{
    parse_value, validate_schema_version, Capability, ContractError, ContractErrorCode,
    ContractResult, ExtensionFields, ValidateContract,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentRuntime {
    PythonAction,
    PythonWorker,
    NativeProcess,
    NativeLibrary,
    DataPack,
    BuiltinRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentRole {
    #[default]
    Service,
    Feature,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentDistribution {
    #[default]
    Local,
    Bundled,
    Marketplace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentUiMode {
    #[default]
    None,
    Hosted,
    Contributed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDependency {
    pub id: String,
    #[serde(default = "default_version_requirement")]
    pub version_requirement: String,
}

fn default_version_requirement() -> String {
    "*".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PlatformTarget {
    Any,
    WindowsX64,
    WindowsArm64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PortValueType {
    String,
    Integer,
    Number,
    Boolean,
    Json,
    Path,
    File,
    Directory,
    Artifact,
    StringList,
    FileList,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PortDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub value_type: PortValueType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowNodeContribution {
    pub id: String,
    pub command: String,
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<PortDefinition>,
    #[serde(default)]
    pub outputs: Vec<PortDefinition>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolActionContribution {
    pub id: String,
    pub command: String,
    pub name: String,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellTemplateContribution {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PageTemplateContribution {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThemePresetContribution {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub tokens: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsFieldType {
    String,
    Integer,
    Number,
    Boolean,
    Path,
    File,
    Directory,
    Enum,
    StringList,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsField {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: SettingsFieldType,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default)]
    pub options: Vec<SettingsOption>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSettingsSection {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub scope: SettingsScope,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub fields: Vec<SettingsField>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentContributions {
    #[serde(default)]
    pub workflow_nodes: Vec<WorkflowNodeContribution>,
    #[serde(default)]
    pub tool_actions: Vec<ToolActionContribution>,
    #[serde(default)]
    pub widgets: Vec<String>,
    #[serde(default)]
    pub data_sources: Vec<String>,
    #[serde(default)]
    pub settings_sections: Vec<ComponentSettingsSection>,
    #[serde(default)]
    pub shell_templates: Vec<ShellTemplateContribution>,
    #[serde(default)]
    pub page_templates: Vec<PageTemplateContribution>,
    #[serde(default)]
    pub theme_presets: Vec<ThemePresetContribution>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentResourceLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallelism: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentManifestV1 {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    pub api_version: String,
    pub runtime: ComponentRuntime,
    #[serde(default)]
    pub role: ComponentRole,
    #[serde(default)]
    pub distribution: ComponentDistribution,
    #[serde(default)]
    pub ui_mode: ComponentUiMode,
    #[serde(default)]
    pub platforms: Vec<PlatformTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub requires_components: Vec<ComponentDependency>,
    #[serde(default)]
    pub optional_components: Vec<ComponentDependency>,
    #[serde(default)]
    pub contributes: ComponentContributions,
    #[serde(default)]
    pub resources: ComponentResourceLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

pub fn parse_component_manifest(input: &str) -> ContractResult<ComponentManifestV1> {
    let value = parse_value(input)?;
    validate_schema_version(&value)?;
    validate_capability_names(&value)?;
    let manifest: ComponentManifestV1 = serde_json::from_value(value).map_err(|error| {
        ContractError::new(
            ContractErrorCode::MalformedDocument,
            "$",
            format!("组件清单字段无效: {error}"),
        )
    })?;
    manifest.validate_contract()?;
    Ok(manifest)
}

fn validate_capability_names(value: &Value) -> ContractResult<()> {
    if let Some(capabilities) = value.get("capabilities").and_then(Value::as_array) {
        for (index, capability) in capabilities.iter().enumerate() {
            let Some(name) = capability.as_str() else {
                continue;
            };
            if Capability::from_name(name).is_none() {
                return Err(ContractError::new(
                    ContractErrorCode::UnknownCapability,
                    format!("$.capabilities[{index}]"),
                    format!("未知 Capability: {name}"),
                ));
            }
        }
    }
    Ok(())
}

impl ValidateContract for ComponentManifestV1 {
    fn validate_contract(&self) -> ContractResult<()> {
        if self.schema_version != crate::PLATFORM_SCHEMA_VERSION {
            return Err(ContractError::new(
                ContractErrorCode::UnsupportedSchemaVersion,
                "$.schemaVersion",
                format!("不支持 schemaVersion {}", self.schema_version),
            ));
        }
        validate_stable_id(&self.id, "$.id")?;
        validate_version(&self.version, "$.version")?;
        validate_api_version(&self.api_version, "$.apiVersion")?;
        if self.name.trim().is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MalformedDocument,
                "$.name",
                "组件名称不能为空",
            ));
        }

        let requires_entry = !matches!(self.runtime, ComponentRuntime::BuiltinRust);
        if requires_entry && self.entry.is_none() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRuntimeConfiguration,
                "$.entry",
                "该组件运行时必须声明入口",
            ));
        }
        if matches!(self.runtime, ComponentRuntime::BuiltinRust) && self.entry.is_some() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRuntimeConfiguration,
                "$.entry",
                "builtin-rust 不能声明包内可执行入口",
            ));
        }
        if let Some(entry) = &self.entry {
            validate_relative_path(entry, "$.entry")?;
        }
        if self.platforms.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRuntimeConfiguration,
                "$.platforms",
                "至少声明一个目标平台",
            ));
        }
        let mut platform_set = BTreeSet::new();
        for (index, platform) in self.platforms.iter().enumerate() {
            if !platform_set.insert(format!("{platform:?}")) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("$.platforms[{index}]"),
                    "重复目标平台",
                ));
            }
        }
        if self.platforms.len() > 1 && self.platforms.contains(&PlatformTarget::Any) {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRuntimeConfiguration,
                "$.platforms",
                "any 不能与具体平台同时声明",
            ));
        }

        let mut capabilities = BTreeSet::new();
        for (index, capability) in self.capabilities.iter().enumerate() {
            if !capabilities.insert(capability.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("$.capabilities[{index}]"),
                    format!("重复 Capability: {}", capability.as_str()),
                ));
            }
        }

        let mut dependencies = BTreeSet::new();
        for (kind, list) in [
            ("requiresComponents", &self.requires_components),
            ("optionalComponents", &self.optional_components),
        ] {
            for (index, dependency) in list.iter().enumerate() {
                let path = format!("$.{kind}[{index}]");
                validate_stable_id(&dependency.id, &format!("{path}.id"))?;
                validate_version_requirement(
                    &dependency.version_requirement,
                    &format!("{path}.versionRequirement"),
                )?;
                if dependency.id == self.id {
                    return Err(ContractError::new(
                        ContractErrorCode::SelfDependency,
                        path,
                        "组件不能依赖自身",
                    ));
                }
                if !dependencies.insert(dependency.id.as_str()) {
                    return Err(ContractError::new(
                        ContractErrorCode::DuplicateId,
                        path,
                        format!("重复组件依赖: {}", dependency.id),
                    ));
                }
            }
        }

        let mut contribution_ids = BTreeSet::new();
        for (index, node) in self.contributes.workflow_nodes.iter().enumerate() {
            validate_stable_id(
                &node.id,
                &format!("$.contributes.workflowNodes[{index}].id"),
            )?;
            validate_local_id(
                &node.command,
                &format!("$.contributes.workflowNodes[{index}].command"),
            )?;
            if !contribution_ids.insert(node.id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("$.contributes.workflowNodes[{index}].id"),
                    format!("重复贡献 ID: {}", node.id),
                ));
            }
            validate_ports(
                &node.inputs,
                &format!("$.contributes.workflowNodes[{index}].inputs"),
            )?;
            validate_ports(
                &node.outputs,
                &format!("$.contributes.workflowNodes[{index}].outputs"),
            )?;
        }
        for (index, action) in self.contributes.tool_actions.iter().enumerate() {
            validate_stable_id(
                &action.id,
                &format!("$.contributes.toolActions[{index}].id"),
            )?;
            validate_local_id(
                &action.command,
                &format!("$.contributes.toolActions[{index}].command"),
            )?;
            if !contribution_ids.insert(action.id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("$.contributes.toolActions[{index}].id"),
                    format!("重复贡献 ID: {}", action.id),
                ));
            }
        }
        for (field, values) in [
            ("widgets", &self.contributes.widgets),
            ("dataSources", &self.contributes.data_sources),
        ] {
            for (index, id) in values.iter().enumerate() {
                validate_stable_id(id, &format!("$.contributes.{field}[{index}]"))?;
                if !contribution_ids.insert(id.as_str()) {
                    return Err(ContractError::new(
                        ContractErrorCode::DuplicateId,
                        format!("$.contributes.{field}[{index}]"),
                        format!("重复贡献 ID: {id}"),
                    ));
                }
            }
        }
        for (index, template) in self.contributes.shell_templates.iter().enumerate() {
            let path = format!("$.contributes.shellTemplates[{index}]");
            validate_stable_id(&template.id, &format!("{path}.id"))?;
            validate_version(&template.version, &format!("{path}.version"))?;
            validate_named_contribution(&template.name, &path)?;
            if !contribution_ids.insert(template.id.as_str()) {
                return Err(duplicate_contribution(&path, &template.id));
            }
            let mut variants = BTreeSet::new();
            for (variant_index, variant) in template.variants.iter().enumerate() {
                validate_local_id(variant, &format!("{path}.variants[{variant_index}]"))?;
                if !variants.insert(variant.as_str()) {
                    return Err(ContractError::new(
                        ContractErrorCode::DuplicateId,
                        format!("{path}.variants[{variant_index}]"),
                        format!("重复模板变体: {variant}"),
                    ));
                }
            }
            if let Some(adapter) = &template.adapter {
                validate_local_id(adapter, &format!("{path}.adapter"))?;
            }
        }
        for (index, template) in self.contributes.page_templates.iter().enumerate() {
            let path = format!("$.contributes.pageTemplates[{index}]");
            validate_stable_id(&template.id, &format!("{path}.id"))?;
            validate_version(&template.version, &format!("{path}.version"))?;
            validate_named_contribution(&template.name, &path)?;
            if !contribution_ids.insert(template.id.as_str()) {
                return Err(duplicate_contribution(&path, &template.id));
            }
            let mut regions = BTreeSet::new();
            for (region_index, region) in template.regions.iter().enumerate() {
                validate_local_id(region, &format!("{path}.regions[{region_index}]"))?;
                if !regions.insert(region.as_str()) {
                    return Err(ContractError::new(
                        ContractErrorCode::DuplicateId,
                        format!("{path}.regions[{region_index}]"),
                        format!("重复页面区域: {region}"),
                    ));
                }
            }
        }
        for (index, preset) in self.contributes.theme_presets.iter().enumerate() {
            let path = format!("$.contributes.themePresets[{index}]");
            validate_stable_id(&preset.id, &format!("{path}.id"))?;
            validate_version(&preset.version, &format!("{path}.version"))?;
            validate_named_contribution(&preset.name, &path)?;
            if !contribution_ids.insert(preset.id.as_str()) {
                return Err(duplicate_contribution(&path, &preset.id));
            }
        }
        validate_settings_sections(&self.contributes.settings_sections)?;

        if self.resources.max_parallelism == Some(0)
            || self.resources.max_memory_mb == Some(0)
            || self.resources.timeout_ms == Some(0)
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRuntimeConfiguration,
                "$.resources",
                "资源限制必须大于 0",
            ));
        }
        Ok(())
    }
}

fn validate_named_contribution(name: &str, path: &str) -> ContractResult<()> {
    if name.trim().is_empty() {
        return Err(ContractError::new(
            ContractErrorCode::MalformedDocument,
            format!("{path}.name"),
            "贡献名称不能为空",
        ));
    }
    Ok(())
}

fn duplicate_contribution(path: &str, id: &str) -> ContractError {
    ContractError::new(
        ContractErrorCode::DuplicateId,
        format!("{path}.id"),
        format!("重复贡献 ID: {id}"),
    )
}

fn validate_settings_sections(sections: &[ComponentSettingsSection]) -> ContractResult<()> {
    let mut section_ids = BTreeSet::new();
    for (section_index, section) in sections.iter().enumerate() {
        let section_path = format!("$.contributes.settingsSections[{section_index}]");
        validate_stable_id(&section.id, &format!("{section_path}.id"))?;
        if !section_ids.insert(section.id.as_str()) {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateId,
                format!("{section_path}.id"),
                format!("重复设置区 ID: {}", section.id),
            ));
        }
        if section.title.trim().is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MalformedDocument,
                format!("{section_path}.title"),
                "设置区标题不能为空",
            ));
        }
        if section.fields.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MalformedDocument,
                format!("{section_path}.fields"),
                "设置区至少声明一个字段",
            ));
        }

        let mut field_ids = BTreeSet::new();
        for (field_index, field) in section.fields.iter().enumerate() {
            let field_path = format!("{section_path}.fields[{field_index}]");
            validate_local_id(&field.id, &format!("{field_path}.id"))?;
            if !field_ids.insert(field.id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("{field_path}.id"),
                    format!("重复设置字段: {}", field.id),
                ));
            }
            if field.label.trim().is_empty() {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("{field_path}.label"),
                    "设置字段标签不能为空",
                ));
            }
            if field.sensitive
                && !matches!(
                    field.field_type,
                    SettingsFieldType::String
                        | SettingsFieldType::Path
                        | SettingsFieldType::File
                        | SettingsFieldType::Directory
                )
            {
                return Err(ContractError::new(
                    ContractErrorCode::TypeMismatch,
                    format!("{field_path}.sensitive"),
                    "敏感字段必须使用字符串或路径类型",
                ));
            }
            if let (Some(minimum), Some(maximum)) = (field.minimum, field.maximum) {
                if minimum > maximum {
                    return Err(ContractError::new(
                        ContractErrorCode::InvalidRuntimeConfiguration,
                        field_path.clone(),
                        "设置字段 minimum 不能大于 maximum",
                    ));
                }
            }
            if matches!(field.field_type, SettingsFieldType::Enum) {
                if field.options.is_empty() {
                    return Err(ContractError::new(
                        ContractErrorCode::MalformedDocument,
                        format!("{field_path}.options"),
                        "枚举字段至少声明一个选项",
                    ));
                }
                let mut option_values = BTreeSet::new();
                for (option_index, option) in field.options.iter().enumerate() {
                    if option.value.is_empty() || option.label.trim().is_empty() {
                        return Err(ContractError::new(
                            ContractErrorCode::MalformedDocument,
                            format!("{field_path}.options[{option_index}]"),
                            "枚举选项的 value 和 label 不能为空",
                        ));
                    }
                    if !option_values.insert(option.value.as_str()) {
                        return Err(ContractError::new(
                            ContractErrorCode::DuplicateId,
                            format!("{field_path}.options[{option_index}].value"),
                            format!("重复枚举值: {}", option.value),
                        ));
                    }
                }
            } else if !field.options.is_empty() {
                return Err(ContractError::new(
                    ContractErrorCode::TypeMismatch,
                    format!("{field_path}.options"),
                    "只有枚举字段可以声明 options",
                ));
            }
            if let Some(default_value) = &field.default_value {
                validate_settings_value(field, default_value, &format!("{field_path}.defaultValue"))?;
            }
        }
    }
    Ok(())
}

pub fn validate_settings_value(
    field: &SettingsField,
    value: &Value,
    path: &str,
) -> ContractResult<()> {
    let valid_type = match field.field_type {
        SettingsFieldType::Boolean => value.is_boolean(),
        SettingsFieldType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
        SettingsFieldType::Number => value.is_number(),
        SettingsFieldType::String
        | SettingsFieldType::Path
        | SettingsFieldType::File
        | SettingsFieldType::Directory
        | SettingsFieldType::Enum => value.is_string(),
        SettingsFieldType::StringList => value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string)),
    };
    if !valid_type {
        return Err(ContractError::new(
            ContractErrorCode::TypeMismatch,
            path,
            format!("设置值类型与 {:?} 不匹配", field.field_type),
        ));
    }
    if matches!(field.field_type, SettingsFieldType::Enum) {
        let selected = value.as_str().unwrap_or_default();
        if !field.options.iter().any(|option| option.value == selected) {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReference,
                path,
                format!("枚举值不在允许范围内: {selected}"),
            ));
        }
    }
    if matches!(field.field_type, SettingsFieldType::Integer | SettingsFieldType::Number) {
        let number = value.as_f64().unwrap_or_default();
        if field.minimum.is_some_and(|minimum| number < minimum)
            || field.maximum.is_some_and(|maximum| number > maximum)
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRuntimeConfiguration,
                path,
                "数值超出允许范围",
            ));
        }
    }
    Ok(())
}

pub fn validate_component_graph(manifests: &[ComponentManifestV1]) -> ContractResult<()> {
    let mut components = HashMap::new();
    for (index, manifest) in manifests.iter().enumerate() {
        manifest.validate_contract()?;
        if components.insert(manifest.id.as_str(), manifest).is_some() {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateId,
                format!("$[{index}].id"),
                format!("重复组件: {}", manifest.id),
            ));
        }
    }

    for manifest in manifests {
        for dependency in &manifest.requires_components {
            validate_installed_component_dependency(manifest, dependency, &components)?;
        }
        for dependency in &manifest.optional_components {
            if components.contains_key(dependency.id.as_str()) {
                validate_installed_component_dependency(manifest, dependency, &components)?;
            }
        }
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for manifest in manifests {
        visit_component(
            manifest.id.as_str(),
            &components,
            &mut visiting,
            &mut visited,
        )?;
    }
    Ok(())
}

fn validate_installed_component_dependency<'a>(
    manifest: &ComponentManifestV1,
    dependency: &ComponentDependency,
    components: &HashMap<&'a str, &'a ComponentManifestV1>,
) -> ContractResult<()> {
    let Some(target) = components.get(dependency.id.as_str()) else {
        return Err(ContractError::new(
            ContractErrorCode::MissingDependency,
            format!("$.{}.requiresComponents", manifest.id),
            format!("缺少必需组件: {}", dependency.id),
        ));
    };
    let requirement = semver::VersionReq::parse(&dependency.version_requirement).unwrap();
    let target_version = semver::Version::parse(&target.version).unwrap();
    if !requirement.matches(&target_version) {
        return Err(ContractError::new(
            ContractErrorCode::MissingDependency,
            format!("$.{}.requiresComponents", manifest.id),
            format!(
                "组件 {} 版本 {} 不满足 {}",
                target.id, target.version, dependency.version_requirement
            ),
        ));
    }
    Ok(())
}

fn visit_component<'a>(
    id: &'a str,
    components: &HashMap<&'a str, &'a ComponentManifestV1>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
) -> ContractResult<()> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(ContractError::new(
            ContractErrorCode::DependencyCycle,
            format!("$.{id}.requiresComponents"),
            format!("组件依赖形成循环，回到 {id}"),
        ));
    }
    if let Some(manifest) = components.get(id) {
        for dependency in &manifest.requires_components {
            visit_component(dependency.id.as_str(), components, visiting, visited)?;
        }
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

pub fn component_map(manifests: &[ComponentManifestV1]) -> BTreeMap<&str, &ComponentManifestV1> {
    manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect()
}

fn validate_ports(ports: &[PortDefinition], path: &str) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    for (index, port) in ports.iter().enumerate() {
        validate_local_id(&port.name, &format!("{path}[{index}].name"))?;
        if !seen.insert(port.name.as_str()) {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateId,
                format!("{path}[{index}].name"),
                format!("重复端口: {}", port.name),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_directory_component_entry() {
        let input = r#"{
          "schemaVersion": 1,
          "id": "test.component",
          "name": "Test",
          "version": "1.0.0",
          "apiVersion": "1",
          "runtime": "native-process",
          "platforms": ["windows-x64"],
          "entry": "../evil.exe"
        }"#;
        let error = parse_component_manifest(input).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::InvalidRelativePath);
    }

    fn component(id: &str, required: &[&str]) -> ComponentManifestV1 {
        ComponentManifestV1 {
            schema_version: 1,
            id: id.into(),
            name: id.into(),
            description: String::new(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            runtime: ComponentRuntime::NativeProcess,
            role: ComponentRole::Service,
            distribution: ComponentDistribution::Local,
            ui_mode: ComponentUiMode::None,
            platforms: vec![PlatformTarget::WindowsX64],
            entry: Some("bin/component.exe".into()),
            capabilities: Vec::new(),
            requires_components: required
                .iter()
                .map(|id| ComponentDependency {
                    id: (*id).into(),
                    version_requirement: "*".into(),
                })
                .collect(),
            optional_components: Vec::new(),
            contributes: ComponentContributions::default(),
            resources: ComponentResourceLimits::default(),
            publisher: None,
            extensions: ExtensionFields::new(),
        }
    }

    #[test]
    fn rejects_component_dependency_cycles() {
        let error = validate_component_graph(&[
            component("test.alpha", &["test.beta"]),
            component("test.beta", &["test.alpha"]),
        ])
        .unwrap_err();
        assert_eq!(error.code, ContractErrorCode::DependencyCycle);
    }
}
