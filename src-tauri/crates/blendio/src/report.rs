use serde::Serialize;

use crate::error::{BlendError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureMode {
    PackedOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierMode {
    SupportedSubset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedPolicy {
    BestEffortWarn,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisMode {
    BlenderGltfCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportWarningKind {
    UnsupportedModifier,
    UnsupportedMaterialNode,
    MissingPackedTexture,
    UnsupportedAnimation,
    UnsupportedObjectType,
    MeshDataFallbackUsed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportWarning {
    pub kind: ExportWarningKind,
    pub message: String,
    pub object_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportOptions {
    pub include_meshes: bool,
    pub include_cameras: bool,
    pub include_lights: bool,
    pub include_object_trs_animation: bool,
    pub texture_mode: TextureMode,
    pub modifier_mode: ModifierMode,
    pub unsupported_policy: UnsupportedPolicy,
    pub axis_mode: AxisMode,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportReport {
    pub warnings: Vec<ExportWarning>,
    pub exported_mesh_count: usize,
    pub exported_material_count: usize,
    pub exported_animation_count: usize,
    pub skipped_objects: Vec<String>,
    pub unsupported_features: Vec<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_meshes: true,
            include_cameras: true,
            include_lights: true,
            include_object_trs_animation: true,
            texture_mode: TextureMode::PackedOnly,
            modifier_mode: ModifierMode::SupportedSubset,
            unsupported_policy: UnsupportedPolicy::BestEffortWarn,
            axis_mode: AxisMode::BlenderGltfCompatible,
        }
    }
}

impl ExportReport {
    pub(crate) fn warn(
        &mut self,
        options: &ExportOptions,
        kind: ExportWarningKind,
        object_name: Option<&str>,
        message: impl Into<String>,
    ) -> Result<()> {
        let message = message.into();
        if matches!(options.unsupported_policy, UnsupportedPolicy::Strict) {
            return Err(BlendError::Export(message));
        }

        self.warnings.push(ExportWarning {
            kind,
            message,
            object_name: object_name.map(str::to_owned),
        });
        Ok(())
    }

    pub(crate) fn skip_object(&mut self, name: impl Into<String>) {
        self.skipped_objects.push(name.into());
    }

    pub(crate) fn add_unsupported_feature(&mut self, feature: impl Into<String>) {
        let feature = feature.into();
        if !self.unsupported_features.iter().any(|item| item == &feature) {
            self.unsupported_features.push(feature);
        }
    }
}
