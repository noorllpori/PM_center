use crate::ids::{
    validate_api_version, validate_local_id, validate_stable_id, validate_version,
    validate_version_requirement,
};
use crate::{
    parse_value, validate_schema_version, Capability, ComponentDependency, ContractError,
    ContractErrorCode, ContractResult, ExtensionFields, ValidateContract,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModuleScope {
    Global,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDependency {
    pub id: String,
    #[serde(default = "default_version_requirement")]
    pub version_requirement: String,
}

fn default_version_requirement() -> String {
    "*".into()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModuleContributions {
    #[serde(default)]
    pub shell_tabs: Vec<String>,
    #[serde(default)]
    pub workspace_tabs: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub surfaces: Vec<String>,
    #[serde(default)]
    pub widgets: Vec<String>,
    #[serde(default)]
    pub data_sources: Vec<String>,
    #[serde(default)]
    pub settings_sections: Vec<String>,
    #[serde(default)]
    pub context_commands: Vec<String>,
    #[serde(default)]
    pub workflow_nodes: Vec<String>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDataPolicy {
    #[serde(default = "default_true")]
    pub retain_on_disable: bool,
    #[serde(default = "default_true")]
    pub delete_requires_explicit_action: bool,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

impl Default for ModuleDataPolicy {
    fn default() -> Self {
        Self {
            retain_on_disable: true,
            delete_requires_explicit_action: true,
            extensions: ExtensionFields::new(),
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModuleManifestV1 {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    pub api_version: String,
    pub scope: ModuleScope,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub requires_modules: Vec<ModuleDependency>,
    #[serde(default)]
    pub optional_modules: Vec<ModuleDependency>,
    #[serde(default)]
    pub requires_components: Vec<ComponentDependency>,
    #[serde(default)]
    pub optional_components: Vec<ComponentDependency>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub background_services: Vec<String>,
    #[serde(default)]
    pub contributes: ModuleContributions,
    #[serde(default)]
    pub data_policy: ModuleDataPolicy,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

pub fn parse_module_manifest(input: &str) -> ContractResult<ModuleManifestV1> {
    let value = parse_value(input)?;
    validate_schema_version(&value)?;
    validate_capability_names(&value)?;
    let manifest: ModuleManifestV1 = serde_json::from_value(value).map_err(|error| {
        ContractError::new(
            ContractErrorCode::MalformedDocument,
            "$",
            format!("模块清单字段无效: {error}"),
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

impl ValidateContract for ModuleManifestV1 {
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
                "模块名称不能为空",
            ));
        }

        let mut dependencies = BTreeSet::new();
        for (kind, list) in [
            ("requiresModules", &self.requires_modules),
            ("optionalModules", &self.optional_modules),
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
                        "模块不能依赖自身",
                    ));
                }
                if !dependencies.insert(dependency.id.as_str()) {
                    return Err(ContractError::new(
                        ContractErrorCode::DuplicateId,
                        path,
                        format!("重复模块依赖: {}", dependency.id),
                    ));
                }
            }
        }

        let mut component_dependencies = BTreeSet::new();
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
                if !component_dependencies.insert(dependency.id.as_str()) {
                    return Err(ContractError::new(
                        ContractErrorCode::DuplicateId,
                        path,
                        format!("重复组件依赖: {}", dependency.id),
                    ));
                }
            }
        }

        validate_unique_stable_ids(&self.conflicts, "$.conflicts")?;
        if self.conflicts.iter().any(|id| id == &self.id) {
            return Err(ContractError::new(
                ContractErrorCode::SelfDependency,
                "$.conflicts",
                "模块不能与自身冲突",
            ));
        }
        if let Some(conflict) = self
            .conflicts
            .iter()
            .find(|conflict| dependencies.contains(conflict.as_str()))
        {
            return Err(ContractError::new(
                ContractErrorCode::ModuleConflict,
                "$.conflicts",
                format!("模块不能同时依赖并冲突: {conflict}"),
            ));
        }
        validate_unique_capabilities(&self.capabilities, "$.capabilities")?;
        validate_unique_local_ids(&self.background_services, "$.backgroundServices")?;

        for (name, ids) in [
            ("shellTabs", &self.contributes.shell_tabs),
            ("workspaceTabs", &self.contributes.workspace_tabs),
            ("tools", &self.contributes.tools),
            ("surfaces", &self.contributes.surfaces),
            ("widgets", &self.contributes.widgets),
            ("dataSources", &self.contributes.data_sources),
            ("settingsSections", &self.contributes.settings_sections),
            ("contextCommands", &self.contributes.context_commands),
            ("workflowNodes", &self.contributes.workflow_nodes),
        ] {
            validate_unique_stable_ids(ids, &format!("$.contributes.{name}"))?;
        }
        Ok(())
    }
}

fn validate_unique_stable_ids(ids: &[String], path: &str) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    for (index, id) in ids.iter().enumerate() {
        validate_stable_id(id, &format!("{path}[{index}]"))?;
        if !seen.insert(id) {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateId,
                format!("{path}[{index}]"),
                format!("重复标识符: {id}"),
            ));
        }
    }
    Ok(())
}

fn validate_unique_local_ids(ids: &[String], path: &str) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    for (index, id) in ids.iter().enumerate() {
        validate_local_id(id, &format!("{path}[{index}]"))?;
        if !seen.insert(id) {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateId,
                format!("{path}[{index}]"),
                format!("重复标识符: {id}"),
            ));
        }
    }
    Ok(())
}

fn validate_unique_capabilities(capabilities: &[Capability], path: &str) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    for (index, capability) in capabilities.iter().enumerate() {
        if !seen.insert(capability.as_str()) {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateId,
                format!("{path}[{index}]"),
                format!("重复 Capability: {}", capability.as_str()),
            ));
        }
    }
    Ok(())
}

pub fn validate_module_graph(manifests: &[ModuleManifestV1]) -> ContractResult<()> {
    let mut modules = HashMap::new();
    for (index, manifest) in manifests.iter().enumerate() {
        manifest.validate_contract()?;
        if modules.insert(manifest.id.as_str(), manifest).is_some() {
            return Err(ContractError::new(
                ContractErrorCode::DuplicateId,
                format!("$[{index}].id"),
                format!("重复模块: {}", manifest.id),
            ));
        }
    }

    for manifest in manifests {
        for dependency in &manifest.requires_modules {
            let Some(target) = modules.get(dependency.id.as_str()) else {
                return Err(ContractError::new(
                    ContractErrorCode::MissingDependency,
                    format!("$.{}.requiresModules", manifest.id),
                    format!("缺少必需模块: {}", dependency.id),
                ));
            };
            let requirement = semver::VersionReq::parse(&dependency.version_requirement).unwrap();
            let target_version = semver::Version::parse(&target.version).unwrap();
            if !requirement.matches(&target_version) {
                return Err(ContractError::new(
                    ContractErrorCode::MissingDependency,
                    format!("$.{}.requiresModules", manifest.id),
                    format!(
                        "模块 {} 版本 {} 不满足 {}",
                        target.id, target.version, dependency.version_requirement
                    ),
                ));
            }
        }
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for manifest in manifests {
        visit_module(manifest.id.as_str(), &modules, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_module<'a>(
    id: &'a str,
    modules: &HashMap<&'a str, &'a ModuleManifestV1>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
) -> ContractResult<()> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(ContractError::new(
            ContractErrorCode::DependencyCycle,
            format!("$.{id}.requiresModules"),
            format!("模块依赖形成循环，回到 {id}"),
        ));
    }
    if let Some(manifest) = modules.get(id) {
        for dependency in &manifest.requires_modules {
            visit_module(dependency.id.as_str(), modules, visiting, visited)?;
        }
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

pub fn module_map(manifests: &[ModuleManifestV1]) -> BTreeMap<&str, &ModuleManifestV1> {
    manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(id: &str, required: &[&str]) -> ModuleManifestV1 {
        ModuleManifestV1 {
            schema_version: 1,
            id: id.into(),
            name: id.into(),
            description: String::new(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: required
                .iter()
                .map(|id| ModuleDependency {
                    id: (*id).into(),
                    version_requirement: "*".into(),
                })
                .collect(),
            optional_modules: Vec::new(),
            requires_components: Vec::new(),
            optional_components: Vec::new(),
            conflicts: Vec::new(),
            capabilities: Vec::new(),
            background_services: Vec::new(),
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions: ExtensionFields::new(),
        }
    }

    #[test]
    fn rejects_dependency_cycles() {
        let error = validate_module_graph(&[
            module("test.alpha", &["test.beta"]),
            module("test.beta", &["test.alpha"]),
        ])
        .unwrap_err();
        assert_eq!(error.code, ContractErrorCode::DependencyCycle);
    }

    #[test]
    fn rejects_installed_dependency_version_mismatch() {
        let mut alpha = module("test.alpha", &["test.beta"]);
        alpha.requires_modules[0].version_requirement = "^2.0".into();
        let error = validate_module_graph(&[alpha, module("test.beta", &[])]).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::MissingDependency);
    }

    #[test]
    fn unknown_capability_has_stable_error_code() {
        let input = r#"{
          "schemaVersion": 1,
          "id": "test.module",
          "name": "Test",
          "version": "1.0.0",
          "apiVersion": "1",
          "scope": "global",
          "capabilities": ["unknown.capability"]
        }"#;
        let error = parse_module_manifest(input).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::UnknownCapability);
    }
}
