use crate::ids::{validate_local_id, validate_relative_path, validate_stable_id, validate_version};
use crate::{
    ContractError, ContractErrorCode, ContractResult, ExtensionFields, ValidateContract,
    PLATFORM_SCHEMA_VERSION,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PresentationTemplateKind {
    Shell,
    Page,
    Theme,
}

impl PresentationTemplateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Page => "page",
            Self::Theme => "theme",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTemplateDocumentV1 {
    pub schema_version: u16,
    pub id: String,
    pub kind: PresentationTemplateKind,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styles: Option<String>,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default)]
    pub assets: Vec<String>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

impl ValidateContract for PresentationTemplateDocumentV1 {
    fn validate_contract(&self) -> ContractResult<()> {
        if self.schema_version != PLATFORM_SCHEMA_VERSION {
            return Err(ContractError::new(
                ContractErrorCode::UnsupportedSchemaVersion,
                "$.schemaVersion",
                format!("不支持的表现模板 schemaVersion {}", self.schema_version),
            ));
        }
        validate_stable_id(&self.id, "$.id")?;
        validate_version(&self.version, "$.version")?;
        match self.kind {
            PresentationTemplateKind::Shell | PresentationTemplateKind::Page => {
                let base_html = self.base_html.as_deref().ok_or_else(|| {
                    ContractError::new(
                        ContractErrorCode::InvalidReference,
                        "$.baseHtml",
                        "Shell 和 Page 模板必须声明 baseHtml",
                    )
                })?;
                validate_relative_path(base_html, "$.baseHtml")?;
            }
            PresentationTemplateKind::Theme if self.base_html.is_some() => {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReference,
                    "$.baseHtml",
                    "Theme 模板不能声明 baseHtml",
                ))
            }
            PresentationTemplateKind::Theme => {}
        }
        if let Some(styles) = &self.styles {
            validate_relative_path(styles, "$.styles")?;
        }
        for (index, region) in self.regions.iter().enumerate() {
            validate_local_id(region, &format!("$.regions[{index}]"))?;
        }
        for (index, asset) in self.assets.iter().enumerate() {
            validate_relative_path(asset, &format!("$.assets[{index}]"))?;
        }
        Ok(())
    }
}

pub fn parse_presentation_template(input: &str) -> ContractResult<PresentationTemplateDocumentV1> {
    crate::parse_contract(input)
}
