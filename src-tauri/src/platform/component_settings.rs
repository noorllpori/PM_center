use chrono::Utc;
use pmc_platform::{
    validate_settings_value, ComponentManifestV1, ComponentSettingsSection, SettingsScope,
    PLATFORM_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSettingsRequest {
    pub component_id: String,
    pub section_id: String,
    pub scope: SettingsScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveComponentSettingsRequest {
    #[serde(flatten)]
    pub target: ComponentSettingsRequest,
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSettingsSnapshot {
    pub component_id: String,
    pub section_id: String,
    pub scope: SettingsScope,
    pub values: BTreeMap<String, Value>,
    pub storage_path: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComponentSettingsDocument {
    schema_version: u16,
    component_id: String,
    updated_at: i64,
    #[serde(default)]
    sections: BTreeMap<String, BTreeMap<String, Value>>,
}

pub struct ComponentSettingsStore {
    global_root: PathBuf,
    operation_lock: Mutex<()>,
}

impl ComponentSettingsStore {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            global_root: app_data_dir.join("component-settings"),
            operation_lock: Mutex::new(()),
        }
    }

    pub fn get(
        &self,
        manifest: &ComponentManifestV1,
        request: &ComponentSettingsRequest,
    ) -> Result<ComponentSettingsSnapshot, String> {
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| "组件设置存储锁已损坏".to_string())?;
        let section = find_section(manifest, request)?;
        let path = self.storage_path(request)?;
        let document = read_document(&path, &request.component_id)?;
        Ok(snapshot_from_document(request, section, &path, document))
    }

    pub fn save(
        &self,
        manifest: &ComponentManifestV1,
        request: &SaveComponentSettingsRequest,
    ) -> Result<ComponentSettingsSnapshot, String> {
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| "组件设置存储锁已损坏".to_string())?;
        let section = find_section(manifest, &request.target)?;
        let path = self.storage_path(&request.target)?;
        let values = validate_values(section, &request.values)?;
        let mut document = read_document(&path, &request.target.component_id)?;
        document.updated_at = Utc::now().timestamp_millis();
        document
            .sections
            .insert(request.target.section_id.clone(), values);
        write_document(&path, &document)?;
        Ok(snapshot_from_document(
            &request.target,
            section,
            &path,
            document,
        ))
    }

    fn storage_path(&self, request: &ComponentSettingsRequest) -> Result<PathBuf, String> {
        let root = match request.scope {
            SettingsScope::Global => self.global_root.clone(),
            SettingsScope::Project => {
                let project_path = request
                    .project_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| "项目范围组件设置需要 projectPath".to_string())?;
                let project_path = PathBuf::from(project_path);
                if !project_path.is_dir() {
                    return Err(format!("项目目录不存在: {}", project_path.display()));
                }
                project_path.join(".pm_center").join("component-settings")
            }
        };
        Ok(root.join(format!("{}.json", request.component_id)))
    }
}

fn find_section<'a>(
    manifest: &'a ComponentManifestV1,
    request: &ComponentSettingsRequest,
) -> Result<&'a ComponentSettingsSection, String> {
    if manifest.id != request.component_id {
        return Err("组件设置请求与组件清单不匹配".into());
    }
    let section = manifest
        .contributes
        .settings_sections
        .iter()
        .find(|section| section.id == request.section_id)
        .ok_or_else(|| format!("组件未声明设置区: {}", request.section_id))?;
    if section.scope != request.scope {
        return Err("组件设置区作用域与请求不一致".into());
    }
    Ok(section)
}

fn validate_values(
    section: &ComponentSettingsSection,
    values: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, String> {
    let fields = section
        .fields
        .iter()
        .map(|field| (field.id.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    for key in values.keys() {
        if !fields.contains_key(key.as_str()) {
            return Err(format!("设置区 {} 不接受字段 {}", section.id, key));
        }
    }

    let mut normalized = BTreeMap::new();
    for field in &section.fields {
        let value = values
            .get(&field.id)
            .cloned()
            .or_else(|| field.default_value.clone());
        let Some(value) = value else {
            if field.required {
                return Err(format!("必填设置缺失: {}", field.label));
            }
            continue;
        };
        validate_settings_value(field, &value, &format!("$.{}", field.id))
            .map_err(|error| format!("{}: {}", error.path, error.message))?;
        normalized.insert(field.id.clone(), value);
    }
    Ok(normalized)
}

fn snapshot_from_document(
    request: &ComponentSettingsRequest,
    section: &ComponentSettingsSection,
    path: &Path,
    document: ComponentSettingsDocument,
) -> ComponentSettingsSnapshot {
    let stored = document
        .sections
        .get(&request.section_id)
        .cloned()
        .unwrap_or_default();
    let values = section
        .fields
        .iter()
        .filter_map(|field| {
            stored
                .get(&field.id)
                .cloned()
                .or_else(|| field.default_value.clone())
                .map(|value| (field.id.clone(), value))
        })
        .collect();
    ComponentSettingsSnapshot {
        component_id: request.component_id.clone(),
        section_id: request.section_id.clone(),
        scope: request.scope,
        values,
        storage_path: path.to_string_lossy().into_owned(),
        updated_at: document.updated_at,
    }
}

fn read_document(path: &Path, component_id: &str) -> Result<ComponentSettingsDocument, String> {
    if !path.exists() {
        return Ok(ComponentSettingsDocument {
            schema_version: PLATFORM_SCHEMA_VERSION,
            component_id: component_id.into(),
            updated_at: 0,
            sections: BTreeMap::new(),
        });
    }
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("读取组件设置失败 {}: {error}", path.display()))?;
    let document: ComponentSettingsDocument = serde_json::from_str(&raw)
        .map_err(|error| format!("组件设置文件无效 {}: {error}", path.display()))?;
    if document.schema_version != PLATFORM_SCHEMA_VERSION || document.component_id != component_id {
        return Err(format!("组件设置文件标识不匹配: {}", path.display()));
    }
    Ok(document)
}

fn write_document(path: &Path, document: &ComponentSettingsDocument) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("组件设置路径缺少父目录: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("创建组件设置目录失败 {}: {error}", parent.display()))?;
    let temp_path = parent.join(format!(".{}.{}.tmp", document.component_id, Uuid::new_v4()));
    let backup_path = parent.join(format!(".{}.{}.bak", document.component_id, Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("序列化组件设置失败: {error}"))?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| format!("创建组件设置临时文件失败: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("写入组件设置失败: {error}"))?;
        if path.exists() {
            fs::rename(path, &backup_path)
                .map_err(|error| format!("备份旧组件设置失败 {}: {error}", path.display()))?;
        }
        if let Err(error) = fs::rename(&temp_path, path) {
            if backup_path.exists() {
                let _ = fs::rename(&backup_path, path);
            }
            return Err(format!("提交组件设置失败 {}: {error}", path.display()));
        }
        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|error| {
                format!("清理旧组件设置备份失败 {}: {error}", backup_path.display())
            })?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmc_platform::{ComponentContributions, SettingsField, SettingsFieldType};

    fn section() -> ComponentSettingsSection {
        ComponentSettingsSection {
            id: "test.component.general".into(),
            title: "常规".into(),
            description: String::new(),
            scope: SettingsScope::Global,
            order: 0,
            fields: vec![SettingsField {
                id: "parallelism".into(),
                label: "并行数".into(),
                field_type: SettingsFieldType::Integer,
                description: String::new(),
                required: true,
                sensitive: false,
                default_value: Some(Value::from(2)),
                placeholder: None,
                minimum: Some(1.0),
                maximum: Some(8.0),
                options: Vec::new(),
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        }
    }

    #[test]
    fn validates_and_applies_defaults() {
        let settings = section();
        let values = validate_values(&settings, &BTreeMap::new()).unwrap();
        assert_eq!(values.get("parallelism"), Some(&Value::from(2)));
        assert!(validate_values(
            &settings,
            &BTreeMap::from([("parallelism".into(), Value::from(12))]),
        )
        .is_err());
        let _ = ComponentContributions::default();
    }
}
