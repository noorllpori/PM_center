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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateSlotAccepts {
    ActiveSurface,
    ComponentSurface,
    Widget,
    Navigation,
    Tabs,
    Toolbar,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateSlotMultiplicity {
    One,
    #[default]
    Many,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateSlotLayout {
    #[default]
    Flow,
    Single,
    Stack,
    Tabs,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSlotDefinition {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub accepts: Vec<TemplateSlotAccepts>,
    #[serde(default)]
    pub multiplicity: TemplateSlotMultiplicity,
    #[serde(default)]
    pub layout: TemplateSlotLayout,
    #[serde(default)]
    pub required: bool,
    #[serde(default = "default_true")]
    pub collapse_when_empty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u16>,
    #[serde(flatten)]
    pub extensions: ExtensionFields,
}

const fn default_true() -> bool { true }

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
    pub slots: Vec<TemplateSlotDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_version: Option<String>,
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
        let mut slots = std::collections::BTreeSet::new();
        let mut has_primary = false;
        for (index, slot) in self.slots.iter().enumerate() {
            let path = format!("$.slots[{index}]");
            validate_local_id(&slot.id, &format!("{path}.id"))?;
            if !slots.insert(slot.id.as_str()) {
                return Err(ContractError::new(
                    ContractErrorCode::DuplicateId,
                    format!("{path}.id"),
                    format!("重复模板插槽: {}", slot.id),
                ));
            }
            if slot.accepts.is_empty() {
                return Err(ContractError::new(
                    ContractErrorCode::MalformedDocument,
                    format!("{path}.accepts"),
                    "模板插槽至少声明一种可接受内容",
                ));
            }
            if slot.accepts.contains(&TemplateSlotAccepts::ActiveSurface) {
                has_primary = true;
            }
            if slot.multiplicity == TemplateSlotMultiplicity::One
                && slot.layout != TemplateSlotLayout::Single
            {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidRuntimeConfiguration,
                    format!("{path}.layout"),
                    "单个插槽只能使用 single 布局",
                ));
            }
            for (name, value) in [
                ("minWidth", slot.min_width), ("minHeight", slot.min_height),
                ("maxWidth", slot.max_width), ("maxHeight", slot.max_height),
            ] {
                if value == Some(0) {
                    return Err(ContractError::new(
                        ContractErrorCode::InvalidRuntimeConfiguration,
                        format!("{path}.{name}"),
                        "插槽尺寸必须大于 0",
                    ));
                }
            }
            if slot.min_width.zip(slot.max_width).is_some_and(|(min, max)| min > max)
                || slot.min_height.zip(slot.max_height).is_some_and(|(min, max)| min > max)
            {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidRuntimeConfiguration,
                    format!("{path}"),
                    "插槽尺寸范围无效",
                ));
            }
        }
        if self.kind == PresentationTemplateKind::Shell && !self.slots.is_empty() && !has_primary {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRuntimeConfiguration,
                "$.slots",
                "界面模板必须声明一个接受 active-surface 的主插槽",
            ));
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
