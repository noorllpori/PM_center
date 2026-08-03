mod capability;
mod component;
mod error;
mod ids;
mod module_manifest;
mod package;
mod profile;
mod workflow;

pub use capability::{Capability, CapabilityRisk};
pub use component::*;
pub use error::{ContractError, ContractErrorCode, ContractResult};
pub use module_manifest::*;
pub use package::*;
pub use profile::*;
pub use workflow::*;

use schemars::{schema::RootSchema, schema_for, JsonSchema};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const PLATFORM_SCHEMA_VERSION: u16 = 1;
pub type ExtensionFields = BTreeMap<String, Value>;

pub trait PlatformContract:
    Sized + Serialize + DeserializeOwned + JsonSchema + ValidateContract
{
}

impl<T> PlatformContract for T where
    T: Sized + Serialize + DeserializeOwned + JsonSchema + ValidateContract
{
}

pub trait ValidateContract {
    fn validate_contract(&self) -> ContractResult<()>;
}

pub fn parse_contract<T: PlatformContract>(input: &str) -> ContractResult<T> {
    let value: Value = serde_json::from_str(input).map_err(|error| {
        ContractError::new(
            ContractErrorCode::MalformedDocument,
            "$",
            format!("JSON 无法解析: {error}"),
        )
    })?;
    validate_schema_version(&value)?;
    let document: T = serde_json::from_value(value).map_err(|error| {
        ContractError::new(
            ContractErrorCode::MalformedDocument,
            "$",
            format!("合同字段无效: {error}"),
        )
    })?;
    document.validate_contract()?;
    Ok(document)
}

pub(crate) fn parse_value(input: &str) -> ContractResult<Value> {
    serde_json::from_str(input).map_err(|error| {
        ContractError::new(
            ContractErrorCode::MalformedDocument,
            "$",
            format!("JSON 无法解析: {error}"),
        )
    })
}

pub(crate) fn validate_schema_version(value: &Value) -> ContractResult<()> {
    let Some(version) = value.get("schemaVersion").and_then(Value::as_u64) else {
        return Err(ContractError::new(
            ContractErrorCode::MalformedDocument,
            "$.schemaVersion",
            "缺少整数 schemaVersion",
        ));
    };
    if version != u64::from(PLATFORM_SCHEMA_VERSION) {
        return Err(ContractError::new(
            ContractErrorCode::UnsupportedSchemaVersion,
            "$.schemaVersion",
            format!("不支持 schemaVersion {version}，当前仅支持 {PLATFORM_SCHEMA_VERSION}"),
        ));
    }
    Ok(())
}

pub fn schema_documents() -> Vec<(&'static str, RootSchema)> {
    vec![
        ("module-manifest.schema.json", schema_for!(ModuleManifestV1)),
        (
            "component-manifest.schema.json",
            schema_for!(ComponentManifestV1),
        ),
        (
            "workspace-profile.schema.json",
            schema_for!(WorkspaceProfileV1),
        ),
        (
            "workflow-manifest.schema.json",
            schema_for!(WorkflowManifestV1),
        ),
        ("package-header.schema.json", schema_for!(PackageHeaderV1)),
        ("contract-error.schema.json", schema_for!(ContractError)),
    ]
}

pub fn render_schema_document(file_name: &str, schema: &RootSchema) -> String {
    let mut value = serde_json::to_value(schema).expect("RootSchema must serialize");
    let object = value
        .as_object_mut()
        .expect("RootSchema must serialize to an object");
    if file_name != "contract-error.schema.json" {
        set_schema_property_const(
            object,
            "schemaVersion",
            Value::from(PLATFORM_SCHEMA_VERSION),
        );
    }
    if file_name == "package-header.schema.json" {
        set_schema_property_const(object, "magic", Value::String(PACKAGE_MAGIC.into()));
        set_schema_property_const(object, "formatVersion", Value::from(PACKAGE_FORMAT_VERSION));
    }
    let mut ordered = Map::new();
    ordered.insert(
        "$id".into(),
        Value::String(format!("https://pm-center.local/schema/v1/{file_name}")),
    );
    for (key, value) in std::mem::take(object) {
        ordered.insert(key, value);
    }
    value = Value::Object(ordered);
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("schema must render")
    )
}

fn set_schema_property_const(object: &mut Map<String, Value>, property: &str, value: Value) {
    object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .and_then(|properties| properties.get_mut(property))
        .and_then(Value::as_object_mut)
        .expect("versioned schema property must be an object")
        .insert("const".into(), value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("v1")
            .join(name);
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn valid_fixtures_parse_and_validate() {
        let module = parse_module_manifest(&fixture("valid/module-manifest.json")).unwrap();
        let component =
            parse_component_manifest(&fixture("valid/component-manifest.json")).unwrap();
        parse_workspace_profile(&fixture("valid/workspace-profile.json")).unwrap();
        let workflow = parse_workflow_manifest(&fixture("valid/workflow-manifest.json")).unwrap();
        parse_package_header(&fixture("valid/package-header.json")).unwrap();
        let contract_error: ContractError =
            serde_json::from_str(&fixture("valid/contract-error.json")).unwrap();

        validate_workflow_with_components(&workflow, &[component]).unwrap();
        assert_eq!(contract_error.code, ContractErrorCode::InvalidReference);
        let value = serde_json::to_value(module).unwrap();
        assert_eq!(value["futureOptionalField"]["enabled"], true);
    }

    #[test]
    fn invalid_fixtures_return_stable_semantic_codes() {
        let cases = [
            (
                parse_module_manifest(&fixture("invalid/module-unknown-capability.json"))
                    .unwrap_err()
                    .code,
                ContractErrorCode::UnknownCapability,
            ),
            (
                parse_component_manifest(&fixture("invalid/component-parent-entry.json"))
                    .unwrap_err()
                    .code,
                ContractErrorCode::InvalidRelativePath,
            ),
            (
                parse_workspace_profile(&fixture("invalid/profile-duplicate-surface.json"))
                    .unwrap_err()
                    .code,
                ContractErrorCode::DuplicateId,
            ),
            (
                parse_workflow_manifest(&fixture("invalid/workflow-cycle.json"))
                    .unwrap_err()
                    .code,
                ContractErrorCode::WorkflowCycle,
            ),
        ];
        for (actual, expected) in cases {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn future_schema_versions_are_rejected_before_deserialization() {
        let error =
            parse_workspace_profile(r#"{"schemaVersion":2,"id":"test.profile","name":"Future"}"#)
                .unwrap_err();
        assert_eq!(error.code, ContractErrorCode::UnsupportedSchemaVersion);
    }

    #[test]
    fn committed_json_schemas_match_rust_contracts() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("schemas")
            .join("v1");
        for (file_name, schema) in schema_documents() {
            let committed = fs::read_to_string(root.join(file_name)).unwrap();
            assert_eq!(committed, render_schema_document(file_name, &schema));
        }
    }
}
