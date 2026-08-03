use crate::ids::{validate_digest, validate_relative_path, validate_stable_id, validate_version};
use crate::{
    parse_contract, ContractError, ContractErrorCode, ContractResult, ExtensionFields,
    ValidateContract,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
}
