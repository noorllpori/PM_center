use crate::ids::{
    validate_api_version, validate_local_id, validate_relative_path, validate_stable_id,
    validate_version,
};
use crate::{
    parse_value, validate_schema_version, Capability, ContractError, ContractErrorCode,
    ContractResult, ExtensionFields, ValidateContract,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

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
    pub platforms: Vec<PlatformTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
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
}
