use crate::ids::{
    validate_digest, validate_local_id, validate_relative_path, validate_stable_id,
    validate_version,
};
use crate::{
    parse_contract, ContractError, ContractErrorCode, ContractResult, ExtensionFields,
    ValidateContract, WorkspaceProfileV1,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PACKAGE_MAGIC: &str = "PMC_PACKAGE";
pub const PACKAGE_FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PackageKind {
    Profile,
    Workspace,
    ComponentPack,
    RenderPack,
}

impl PackageKind {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Profile => "pmc-profile",
            Self::Workspace => "pmc-workspace",
            Self::ComponentPack => "pmc-pack",
            Self::RenderPack => "pmc-renderpack",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DigestAlgorithm {
    Blake3,
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentDigest {
    pub algorithm: DigestAlgorithm,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackagePayloadDescriptor {
    pub path: String,
    pub digest: ContentDigest,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageHeaderV1 {
    pub magic: String,
    pub schema_version: u16,
    pub format_version: u16,
    pub kind: PackageKind,
    pub package_id: String,
    pub created_at: u64,
    pub producer_version: String,
    pub payload: PackagePayloadDescriptor,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePackageV1 {
    pub schema_version: u16,
    pub profile: WorkspaceProfileV1,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub open_surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_surface_id: Option<String>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

pub fn parse_workspace_package(input: &str) -> ContractResult<WorkspacePackageV1> {
    parse_contract(input)
}

impl ValidateContract for WorkspacePackageV1 {
    fn validate_contract(&self) -> ContractResult<()> {
        if self.schema_version != crate::PLATFORM_SCHEMA_VERSION {
            return Err(ContractError::new(
                ContractErrorCode::UnsupportedSchemaVersion,
                "$.schemaVersion",
                format!("不支持 schemaVersion {}", self.schema_version),
            ));
        }
        self.profile.validate_contract()?;
        for (key, value) in &self.variables {
            validate_local_id(key, &format!("$.variables.{key}"))?;
            if value.len() > 16 * 1024 {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("$.variables.{key}"),
                    "装配空间变量超过 16 KiB 限制",
                ));
            }
        }
        let mut surfaces = BTreeSet::new();
        for (index, surface_id) in self.open_surface_ids.iter().enumerate() {
            validate_local_id(surface_id, &format!("$.openSurfaceIds[{index}]"))?;
            if !surfaces.insert(surface_id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("$.openSurfaceIds[{index}]"),
                    format!("重复打开页面: {surface_id}"),
                ));
            }
        }
        if let Some(active_surface_id) = &self.active_surface_id {
            validate_local_id(active_surface_id, "$.activeSurfaceId")?;
            if !surfaces.contains(active_surface_id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReference,
                    "$.activeSurfaceId",
                    "活动页面必须出现在 openSurfaceIds 中",
                ));
            }
        }
        Ok(())
    }
}

pub fn parse_package_header(input: &str) -> ContractResult<PackageHeaderV1> {
    parse_contract(input)
}

impl ValidateContract for PackageHeaderV1 {
    fn validate_contract(&self) -> ContractResult<()> {
        if self.schema_version != crate::PLATFORM_SCHEMA_VERSION {
            return Err(ContractError::new(
                ContractErrorCode::UnsupportedSchemaVersion,
                "$.schemaVersion",
                format!("不支持 schemaVersion {}", self.schema_version),
            ));
        }
        if self.magic != PACKAGE_MAGIC {
            return Err(ContractError::new(
                ContractErrorCode::InvalidPackageHeader,
                "$.magic",
                format!("包头 magic 必须是 {PACKAGE_MAGIC}"),
            ));
        }
        if self.format_version != PACKAGE_FORMAT_VERSION {
            return Err(ContractError::new(
                ContractErrorCode::InvalidPackageHeader,
                "$.formatVersion",
                format!("不支持包格式版本 {}", self.format_version),
            ));
        }
        validate_stable_id(&self.package_id, "$.packageId")?;
        validate_version(&self.producer_version, "$.producerVersion")?;
        validate_relative_path(&self.payload.path, "$.payload.path")?;
        validate_digest(&self.payload.digest.value, "$.payload.digest.value")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_stable() {
        assert_eq!(PackageKind::Profile.extension(), "pmc-profile");
        assert_eq!(PackageKind::Workspace.extension(), "pmc-workspace");
        assert_eq!(PackageKind::ComponentPack.extension(), "pmc-pack");
        assert_eq!(PackageKind::RenderPack.extension(), "pmc-renderpack");
    }

    #[test]
    fn rejects_invalid_package_digest() {
        let input = r#"{
          "magic":"PMC_PACKAGE","schemaVersion":1,"formatVersion":1,"kind":"profile",
          "packageId":"test.profile","createdAt":0,"producerVersion":"1.0.0",
          "payload":{"path":"payload/profile.json","digest":{"algorithm":"blake3","value":"bad"}}
        }"#;
        let error = parse_package_header(input).unwrap_err();
        assert_eq!(error.code, ContractErrorCode::InvalidDigest);
    }

    #[test]
    fn workspace_accepts_local_surface_ids() {
        let input = r#"{
          "schemaVersion":1,
          "profile":{
            "schemaVersion":1,
            "id":"local.test-profile",
            "name":"Surface ID test",
            "shellLayout":{
              "home":"lan-collaboration-main",
              "navigation":["lan-collaboration-main"]
            },
            "surfaces":[{
              "id":"lan-collaboration-main",
              "kind":"workspace-tab",
              "layout":"contribution-defined"
            }]
          },
          "openSurfaceIds":["lan-collaboration-main"],
          "activeSurfaceId":"lan-collaboration-main"
        }"#;

        let workspace = parse_workspace_package(input).unwrap();
        assert_eq!(workspace.open_surface_ids, ["lan-collaboration-main"]);
        assert_eq!(
            workspace.active_surface_id.as_deref(),
            Some("lan-collaboration-main")
        );
    }
}
