use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

pub type ContractResult<T> = Result<T, ContractError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContractErrorCode {
    MalformedDocument,
    UnsupportedSchemaVersion,
    InvalidStableId,
    InvalidLocalId,
    InvalidVersion,
    InvalidVersionRequirement,
    InvalidRelativePath,
    InvalidDigest,
    UnknownCapability,
    DuplicateId,
    SelfDependency,
    MissingDependency,
    DependencyCycle,
    ModuleConflict,
    InvalidReference,
    InvalidPort,
    TypeMismatch,
    WorkflowCycle,
    InvalidRuntimeConfiguration,
    InvalidPackageHeader,
}

impl ContractErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MalformedDocument => "MALFORMED_DOCUMENT",
            Self::UnsupportedSchemaVersion => "UNSUPPORTED_SCHEMA_VERSION",
            Self::InvalidStableId => "INVALID_STABLE_ID",
            Self::InvalidLocalId => "INVALID_LOCAL_ID",
            Self::InvalidVersion => "INVALID_VERSION",
            Self::InvalidVersionRequirement => "INVALID_VERSION_REQUIREMENT",
            Self::InvalidRelativePath => "INVALID_RELATIVE_PATH",
            Self::InvalidDigest => "INVALID_DIGEST",
            Self::UnknownCapability => "UNKNOWN_CAPABILITY",
            Self::DuplicateId => "DUPLICATE_ID",
            Self::SelfDependency => "SELF_DEPENDENCY",
            Self::MissingDependency => "MISSING_DEPENDENCY",
            Self::DependencyCycle => "DEPENDENCY_CYCLE",
            Self::ModuleConflict => "MODULE_CONFLICT",
            Self::InvalidReference => "INVALID_REFERENCE",
            Self::InvalidPort => "INVALID_PORT",
            Self::TypeMismatch => "TYPE_MISMATCH",
            Self::WorkflowCycle => "WORKFLOW_CYCLE",
            Self::InvalidRuntimeConfiguration => "INVALID_RUNTIME_CONFIGURATION",
            Self::InvalidPackageHeader => "INVALID_PACKAGE_HEADER",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractError {
    pub code: ContractErrorCode,
    pub path: String,
    pub message: String,
}

impl ContractError {
    pub fn new(
        code: ContractErrorCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}

impl Display for ContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} at {}: {}",
            self.code.as_str(),
            self.path,
            self.message
        )
    }
}

impl std::error::Error for ContractError {}
