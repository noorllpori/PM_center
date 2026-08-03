use crate::component::{
    ComponentManifestV1, PortDefinition, PortValueType, WorkflowNodeContribution,
};
use crate::ids::{validate_local_id, validate_stable_id, validate_version};
use crate::{
    parse_contract, ContractError, ContractErrorCode, ContractResult, ExtensionFields,
    ValidateContract,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WorkflowTrigger {
    Manual,
    Event { event: String },
    Schedule { cron: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "kebab-case")]
pub enum WorkflowInput {
    Literal { value: Value },
    ProfileVariable { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowExecutionTarget {
    Local,
    PreferRemote,
    RequireRemote,
}

fn default_execution_target() -> WorkflowExecutionTarget {
    WorkflowExecutionTarget::Local
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRetryPolicy {
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u8,
    #[serde(default = "default_retry_delay_ms")]
    pub delay_ms: u64,
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: u8,
}

const fn default_max_attempts() -> u8 {
    1
}

const fn default_retry_delay_ms() -> u64 {
    5_000
}

const fn default_backoff_multiplier() -> u8 {
    1
}

impl Default for WorkflowRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            delay_ms: default_retry_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowNode {
    pub id: String,
    pub node_type: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, WorkflowInput>,
    #[serde(default = "default_execution_target")]
    pub execution: WorkflowExecutionTarget,
    #[serde(default)]
    pub retry: WorkflowRetryPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPortReference {
    pub node: String,
    pub port: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEdge {
    pub from: WorkflowPortReference,
    pub to: WorkflowPortReference,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowManifestV1 {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    pub trigger: WorkflowTrigger,
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

pub fn parse_workflow_manifest(input: &str) -> ContractResult<WorkflowManifestV1> {
    parse_contract(input)
}

impl ValidateContract for WorkflowManifestV1 {
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
        if self.name.trim().is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MalformedDocument,
                "$.name",
                "工作流名称不能为空",
            ));
        }
        match &self.trigger {
            WorkflowTrigger::Manual => {}
            WorkflowTrigger::Event { event } => validate_stable_id(event, "$.trigger.event")?,
            WorkflowTrigger::Schedule { cron } if cron.trim().is_empty() => {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    "$.trigger.cron",
                    "定时表达式不能为空",
                ));
            }
            WorkflowTrigger::Schedule { .. } => {}
        }
        if self.nodes.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::MalformedDocument,
                "$.nodes",
                "工作流至少需要一个节点",
            ));
        }

        let mut nodes = BTreeSet::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let path = format!("$.nodes[{index}]");
            validate_local_id(&node.id, &format!("{path}.id"))?;
            validate_stable_id(&node.node_type, &format!("{path}.nodeType"))?;
            if !nodes.insert(node.id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("{path}.id"),
                    format!("重复节点: {}", node.id),
                ));
            }
            if node.retry.max_attempts == 0 || node.retry.max_attempts > 10 {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("{path}.retry.maxAttempts"),
                    "重试次数必须在 1-10 之间",
                ));
            }
            if node.retry.backoff_multiplier == 0 || node.retry.backoff_multiplier > 10 {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("{path}.retry.backoffMultiplier"),
                    "退避倍数必须在 1-10 之间",
                ));
            }
            if node.timeout_ms == Some(0) {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("{path}.timeoutMs"),
                    "超时必须大于 0",
                ));
            }
            for (name, input) in &node.inputs {
                validate_local_id(name, &format!("{path}.inputs.{name}"))?;
                if let WorkflowInput::ProfileVariable { name } = input {
                    validate_local_id(name, &format!("{path}.inputs.{name}.name"))?;
                }
            }
        }
        for name in self.variables.keys() {
            validate_local_id(name, &format!("$.variables.{name}"))?;
        }

        let mut incoming_ports = BTreeSet::new();
        let mut unique_edges = BTreeSet::new();
        for (index, edge) in self.edges.iter().enumerate() {
            let path = format!("$.edges[{index}]");
            for (direction, reference) in [("from", &edge.from), ("to", &edge.to)] {
                validate_local_id(&reference.node, &format!("{path}.{direction}.node"))?;
                validate_local_id(&reference.port, &format!("{path}.{direction}.port"))?;
                if !nodes.contains(reference.node.as_str()) {
                    return Err(ContractError::new(
                        ContractErrorCode::InvalidReference,
                        format!("{path}.{direction}.node"),
                        format!("节点不存在: {}", reference.node),
                    ));
                }
            }
            let identity = (
                edge.from.node.as_str(),
                edge.from.port.as_str(),
                edge.to.node.as_str(),
                edge.to.port.as_str(),
            );
            if !unique_edges.insert(identity) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    path,
                    "重复连线",
                ));
            }
            if !incoming_ports.insert((edge.to.node.as_str(), edge.to.port.as_str())) {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidPort,
                    format!("{path}.to"),
                    "一个输入端口只能连接一个输出",
                ));
            }
        }
        validate_dag(&self.nodes, &self.edges)
    }
}

fn validate_dag(nodes: &[WorkflowNode], edges: &[WorkflowEdge]) -> ContractResult<()> {
    let mut indegree: HashMap<&str, usize> =
        nodes.iter().map(|node| (node.id.as_str(), 0)).collect();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        *indegree.get_mut(edge.to.node.as_str()).unwrap() += 1;
        outgoing
            .entry(edge.from.node.as_str())
            .or_default()
            .push(edge.to.node.as_str());
    }
    let mut ready: VecDeque<&str> = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        for target in outgoing.get(id).into_iter().flatten() {
            let degree = indegree.get_mut(target).unwrap();
            *degree -= 1;
            if *degree == 0 {
                ready.push_back(target);
            }
        }
    }
    if visited != nodes.len() {
        return Err(ContractError::new(
            ContractErrorCode::WorkflowCycle,
            "$.edges",
            "工作流必须是无环图",
        ));
    }
    Ok(())
}

pub fn validate_workflow_with_components(
    workflow: &WorkflowManifestV1,
    components: &[ComponentManifestV1],
) -> ContractResult<()> {
    workflow.validate_contract()?;
    let mut definitions: BTreeMap<&str, &WorkflowNodeContribution> = BTreeMap::new();
    for component in components {
        component.validate_contract()?;
        for definition in &component.contributes.workflow_nodes {
            if definitions
                .insert(definition.id.as_str(), definition)
                .is_some()
            {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    "$.components",
                    format!("重复工作流节点类型: {}", definition.id),
                ));
            }
        }
    }

    let workflow_nodes: BTreeMap<&str, &WorkflowNode> = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut incoming: BTreeMap<(&str, &str), &WorkflowEdge> = BTreeMap::new();
    for edge in &workflow.edges {
        incoming.insert((edge.to.node.as_str(), edge.to.port.as_str()), edge);
    }

    for node in &workflow.nodes {
        let Some(definition) = definitions.get(node.node_type.as_str()) else {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReference,
                format!("$.nodes.{}.nodeType", node.id),
                format!("节点类型不存在: {}", node.node_type),
            ));
        };
        let inputs: BTreeMap<&str, &PortDefinition> = definition
            .inputs
            .iter()
            .map(|port| (port.name.as_str(), port))
            .collect();
        for (input_name, input) in &node.inputs {
            let Some(port) = inputs.get(input_name.as_str()) else {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidPort,
                    format!("$.nodes.{}.inputs.{input_name}", node.id),
                    format!("节点类型 {} 没有该输入端口", node.node_type),
                ));
            };
            if let WorkflowInput::Literal { value } = input {
                if !literal_matches(value, port.value_type) {
                    return Err(ContractError::new(
                        ContractErrorCode::TypeMismatch,
                        format!("$.nodes.{}.inputs.{input_name}", node.id),
                        format!("字面量不符合 {:?}", port.value_type),
                    ));
                }
            }
        }
        if let Some((input_name, _)) = node
            .inputs
            .iter()
            .find(|(input_name, _)| incoming.contains_key(&(node.id.as_str(), input_name.as_str())))
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidPort,
                format!("$.nodes.{}.inputs.{input_name}", node.id),
                "输入端口不能同时使用固定输入和节点连线",
            ));
        }
        for port in definition.inputs.iter().filter(|port| port.required) {
            let has_literal = node.inputs.contains_key(&port.name);
            let has_edge = incoming.contains_key(&(node.id.as_str(), port.name.as_str()));
            if !has_literal && !has_edge {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidPort,
                    format!("$.nodes.{}.inputs.{}", node.id, port.name),
                    "缺少必需输入",
                ));
            }
        }
    }

    for (index, edge) in workflow.edges.iter().enumerate() {
        let source_node = workflow_nodes[edge.from.node.as_str()];
        let target_node = workflow_nodes[edge.to.node.as_str()];
        let source_definition = definitions[source_node.node_type.as_str()];
        let target_definition = definitions[target_node.node_type.as_str()];
        let Some(source_port) = source_definition
            .outputs
            .iter()
            .find(|port| port.name == edge.from.port)
        else {
            return Err(ContractError::new(
                ContractErrorCode::InvalidPort,
                format!("$.edges[{index}].from.port"),
                format!("输出端口不存在: {}", edge.from.port),
            ));
        };
        let Some(target_port) = target_definition
            .inputs
            .iter()
            .find(|port| port.name == edge.to.port)
        else {
            return Err(ContractError::new(
                ContractErrorCode::InvalidPort,
                format!("$.edges[{index}].to.port"),
                format!("输入端口不存在: {}", edge.to.port),
            ));
        };
        if !port_types_compatible(source_port.value_type, target_port.value_type) {
            return Err(ContractError::new(
                ContractErrorCode::TypeMismatch,
                format!("$.edges[{index}]"),
                format!(
                    "端口类型不兼容: {:?} -> {:?}",
                    source_port.value_type, target_port.value_type
                ),
            ));
        }
    }
    Ok(())
}

fn literal_matches(value: &Value, value_type: PortValueType) -> bool {
    match value_type {
        PortValueType::String
        | PortValueType::Path
        | PortValueType::File
        | PortValueType::Directory
        | PortValueType::Artifact => value.is_string(),
        PortValueType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
        PortValueType::Number => value.is_number(),
        PortValueType::Boolean => value.is_boolean(),
        PortValueType::Json => true,
        PortValueType::StringList | PortValueType::FileList => value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string)),
    }
}

fn port_types_compatible(source: PortValueType, target: PortValueType) -> bool {
    source == target
        || matches!(
            (source, target),
            (PortValueType::Integer, PortValueType::Number)
                | (PortValueType::File, PortValueType::Path)
                | (PortValueType::Directory, PortValueType::Path)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_component_manifest;

    #[test]
    fn rejects_workflow_cycles() {
        let input = r#"{
          "schemaVersion": 1,
          "id": "test.workflow",
          "name": "Test",
          "version": "1.0.0",
          "trigger": {"kind":"manual"},
          "nodes": [
            {"id":"alpha","nodeType":"test.alpha"},
            {"id":"beta","nodeType":"test.beta"}
          ],
          "edges": [
            {"from":{"node":"alpha","port":"out"},"to":{"node":"beta","port":"in"}},
            {"from":{"node":"beta","port":"out"},"to":{"node":"alpha","port":"in"}}
          ]
        }"#;
        let error = parse_workflow_manifest(input).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::WorkflowCycle);
    }

    #[test]
    fn rejects_incompatible_component_ports() {
        let component = parse_component_manifest(
            r#"{
              "schemaVersion":1,"id":"test.component","name":"Test","version":"1.0.0",
              "apiVersion":"1","runtime":"builtin-rust","platforms":["any"],
              "contributes":{"workflowNodes":[
                {"id":"test.source","command":"source","name":"Source","outputs":[{"name":"out","type":"string"}]},
                {"id":"test.target","command":"target","name":"Target","inputs":[{"name":"in","type":"integer","required":true}]}
              ]}
            }"#,
        )
        .unwrap();
        let workflow = parse_workflow_manifest(
            r#"{
              "schemaVersion":1,"id":"test.workflow","name":"Test","version":"1.0.0",
              "trigger":{"kind":"manual"},
              "nodes":[{"id":"source","nodeType":"test.source"},{"id":"target","nodeType":"test.target"}],
              "edges":[{"from":{"node":"source","port":"out"},"to":{"node":"target","port":"in"}}]
            }"#,
        )
        .unwrap();
        let error = validate_workflow_with_components(&workflow, &[component]).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::TypeMismatch);
    }
}
