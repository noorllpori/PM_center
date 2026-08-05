use super::{
    WorkspaceProfileDraftValidation, WorkspaceProfileMutationResult, WorkspaceProfileRuntime,
    WorkspaceProfileRuntimeError, WorkspaceProfileRuntimeErrorCode,
    WorkspaceProfileRuntimeSnapshot,
};
use chrono::Utc;
use pmc_platform::{
    parse_package_header, parse_workspace_profile, ContentDigest, DigestAlgorithm, ExtensionFields,
    ModuleManifestV1, PackageHeaderV1, PackageKind, PackagePayloadDescriptor, ProfilePathVariable,
    ProfilePathVariableKind, ProfileToolAlias, WorkspaceProfileV1, PACKAGE_FORMAT_VERSION,
    PACKAGE_MAGIC, PLATFORM_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MANIFEST_PATH: &str = "manifest.json";
const PROFILE_PAYLOAD_PATH: &str = "profile.json";
const MAX_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 8;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWorkspaceProfilePackageRequest {
    pub profile_id: String,
    pub destination_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePackageExportResult {
    pub package_id: String,
    pub destination_path: String,
    pub payload_digest: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWorkspaceProfilePackageRequest {
    pub package_path: String,
    pub name: String,
    #[serde(default)]
    pub tool_mappings: Vec<ProfileLocalBindingInput>,
    #[serde(default)]
    pub path_mappings: Vec<ProfileLocalBindingInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileLocalBindingMode {
    Automatic,
    Path,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLocalBindingInput {
    pub id: String,
    pub mode: ProfileLocalBindingMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfilePackageIssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePackageIssue {
    pub code: String,
    pub severity: ProfilePackageIssueSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePackageImportPreview {
    pub package_path: String,
    pub package_id: String,
    pub producer_version: String,
    pub profile_name: String,
    pub description: String,
    pub suggested_name: String,
    pub payload_digest: String,
    pub package_size_bytes: u64,
    pub module_count: usize,
    pub component_count: usize,
    pub surface_count: usize,
    pub widget_count: usize,
    pub pinned_tool_count: usize,
    pub tool_aliases: Vec<ProfileToolAlias>,
    pub path_variables: Vec<ProfilePathVariable>,
    pub reusable_binding_presets: Vec<ProfileLocalBindingPreset>,
    pub missing_module_ids: Vec<String>,
    pub missing_component_ids: Vec<String>,
    pub issues: Vec<ProfilePackageIssue>,
    pub can_import: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLocalBindingPreset {
    pub profile_id: String,
    pub profile_name: String,
    pub tool_mappings: Vec<ProfileLocalBindingInput>,
    pub path_mappings: Vec<ProfileLocalBindingInput>,
}

struct InspectedProfilePackage {
    preview: ProfilePackageImportPreview,
    profile: WorkspaceProfileV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProfileLocalBindings {
    schema_version: u16,
    profile_id: String,
    package_id: String,
    created_at: i64,
    tool_mappings: Vec<ProfileLocalBindingInput>,
    path_mappings: Vec<ProfileLocalBindingInput>,
}

pub fn export_profile_package(
    profile: &WorkspaceProfileV1,
    destination_path: &str,
) -> Result<ProfilePackageExportResult, WorkspaceProfileRuntimeError> {
    let destination = PathBuf::from(destination_path);
    let parent = destination.parent().ok_or_else(|| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfileIoError,
            "导出路径缺少父目录",
            Some(&destination),
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| package_io_error("创建导出目录失败", parent, error))?;

    let portable_profile = portable_profile(profile);
    let profile_value = serde_json::to_value(&portable_profile).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("序列化导出方案失败：{error}"),
            None,
        )
    })?;
    let safety_issues = collect_safety_issues(&profile_value);
    if !safety_issues.is_empty() {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsafe,
            "装配方案包含敏感字段或本机绝对路径，R8-1 拒绝导出",
            None,
        )
        .with_details(
            safety_issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.path.unwrap_or_default(), issue.message))
                .collect(),
        ));
    }

    let payload_bytes = canonical_json_bytes(&portable_profile)?;
    let payload_digest = blake3::hash(&payload_bytes).to_hex().to_string();
    let package_id = format!("profile.p{}", &payload_digest[..24]);
    let header = PackageHeaderV1 {
        magic: PACKAGE_MAGIC.into(),
        schema_version: PLATFORM_SCHEMA_VERSION,
        format_version: PACKAGE_FORMAT_VERSION,
        kind: PackageKind::Profile,
        package_id: package_id.clone(),
        created_at: 0,
        producer_version: env!("CARGO_PKG_VERSION").into(),
        payload: PackagePayloadDescriptor {
            path: PROFILE_PAYLOAD_PATH.into(),
            digest: ContentDigest {
                algorithm: DigestAlgorithm::Blake3,
                value: payload_digest.clone(),
            },
            size_bytes: payload_bytes.len() as u64,
            extensions: ExtensionFields::new(),
        },
        extensions: ExtensionFields::new(),
    };
    let manifest_bytes = canonical_json_bytes(&header)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("profile.pmc-profile"),
        Uuid::new_v4().simple()
    ));

    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| package_io_error("创建方案包临时文件失败", &temporary, error))?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        archive
            .start_file(MANIFEST_PATH, options)
            .map_err(|error| package_zip_error("写入方案包清单失败", &temporary, error))?;
        archive
            .write_all(&manifest_bytes)
            .map_err(|error| package_io_error("写入方案包清单失败", &temporary, error))?;
        archive
            .start_file(PROFILE_PAYLOAD_PATH, options)
            .map_err(|error| package_zip_error("写入方案包内容失败", &temporary, error))?;
        archive
            .write_all(&payload_bytes)
            .map_err(|error| package_io_error("写入方案包内容失败", &temporary, error))?;
        let file = archive
            .finish()
            .map_err(|error| package_zip_error("完成方案包失败", &temporary, error))?;
        file.sync_all()
            .map_err(|error| package_io_error("同步方案包失败", &temporary, error))?;
        replace_export_file(&temporary, &destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;

    let size_bytes = fs::metadata(&destination)
        .map_err(|error| package_io_error("读取导出方案包大小失败", &destination, error))?
        .len();
    Ok(ProfilePackageExportResult {
        package_id,
        destination_path: destination.to_string_lossy().into_owned(),
        payload_digest,
        size_bytes,
    })
}

pub fn inspect_profile_package(
    package_path: &str,
    runtime: &WorkspaceProfileRuntime,
    manifests: &[ModuleManifestV1],
) -> Result<ProfilePackageImportPreview, WorkspaceProfileRuntimeError> {
    inspect_profile_package_internal(package_path, runtime, manifests).map(|value| value.preview)
}

pub fn import_profile_package(
    request: &ImportWorkspaceProfilePackageRequest,
    runtime: &WorkspaceProfileRuntime,
    manifests: &[ModuleManifestV1],
) -> Result<WorkspaceProfileMutationResult, WorkspaceProfileRuntimeError> {
    let inspected = inspect_profile_package_internal(&request.package_path, runtime, manifests)?;
    if !inspected.preview.can_import {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            "方案包存在阻塞问题，未导入任何内容",
            Some(Path::new(&request.package_path)),
        )
        .with_details(
            inspected
                .preview
                .issues
                .iter()
                .filter(|issue| issue.severity == ProfilePackageIssueSeverity::Error)
                .map(|issue| format!("{}: {}", issue.code, issue.message))
                .collect(),
        ));
    }
    let bindings = validate_local_bindings(&inspected.profile, request)?;
    let imported = runtime.import_profile_document(
        &inspected.profile,
        &request.name,
        &inspected.preview.package_id,
        manifests,
    )?;
    let stored = StoredProfileLocalBindings {
        schema_version: 1,
        profile_id: imported.profile.id.clone(),
        package_id: inspected.preview.package_id,
        created_at: Utc::now().timestamp_millis(),
        tool_mappings: bindings.0,
        path_mappings: bindings.1,
    };
    if let Err(mut binding_error) = write_local_bindings(runtime, &stored) {
        if let Err(rollback_error) = runtime.delete_profile(&imported.profile.id, manifests) {
            binding_error.details.push(format!(
                "本机映射写入失败后，导入方案回滚也失败：{}",
                rollback_error.message
            ));
        }
        return Err(binding_error);
    }
    Ok(imported)
}

fn inspect_profile_package_internal(
    package_path: &str,
    runtime: &WorkspaceProfileRuntime,
    manifests: &[ModuleManifestV1],
) -> Result<InspectedProfilePackage, WorkspaceProfileRuntimeError> {
    let path = PathBuf::from(package_path);
    let metadata =
        fs::metadata(&path).map_err(|error| package_io_error("读取方案包失败", &path, error))?;
    if !metadata.is_file() {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            "选择的路径不是方案包文件",
            Some(&path),
        ));
    }
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageTooLarge,
            format!("方案包超过 {} MiB 限制", MAX_ARCHIVE_BYTES / 1024 / 1024),
            Some(&path),
        ));
    }

    let file =
        File::open(&path).map_err(|error| package_io_error("打开方案包失败", &path, error))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| package_zip_error("方案包不是有效 ZIP 容器", &path, error))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageTooLarge,
            format!("方案包条目超过 {MAX_ARCHIVE_ENTRIES} 个限制"),
            Some(&path),
        ));
    }

    let mut entries = BTreeMap::<String, Vec<u8>>::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| package_zip_error("读取方案包条目失败", &path, error))?;
        let name = entry.name().to_string();
        if invalid_archive_path(&name) {
            return Err(package_error(
                WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
                format!("方案包包含不安全路径：{name}"),
                Some(&path),
            ));
        }
        if entry.is_dir() || !matches!(name.as_str(), MANIFEST_PATH | PROFILE_PAYLOAD_PATH) {
            return Err(package_error(
                WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
                format!("方案包包含未声明条目：{name}"),
                Some(&path),
            ));
        }
        if entries.contains_key(&name) {
            return Err(package_error(
                WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
                format!("方案包包含重复条目：{name}"),
                Some(&path),
            ));
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(package_error(
                WorkspaceProfileRuntimeErrorCode::ProfilePackageTooLarge,
                format!("方案包条目过大：{name}"),
                Some(&path),
            ));
        }
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(package_error(
                WorkspaceProfileRuntimeErrorCode::ProfilePackageTooLarge,
                "方案包解压后内容超过安全限制",
                Some(&path),
            ));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .by_ref()
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                package_error(
                    WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
                    format!("解压方案包条目失败：{error}"),
                    Some(&path),
                )
            })?;
        if bytes.len() as u64 > MAX_ENTRY_BYTES {
            return Err(package_error(
                WorkspaceProfileRuntimeErrorCode::ProfilePackageTooLarge,
                format!("方案包条目解压后过大：{name}"),
                Some(&path),
            ));
        }
        entries.insert(name, bytes);
    }

    let manifest_bytes = entries.get(MANIFEST_PATH).ok_or_else(|| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            "方案包缺少 manifest.json",
            Some(&path),
        )
    })?;
    let payload_bytes = entries.get(PROFILE_PAYLOAD_PATH).ok_or_else(|| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            "方案包缺少 profile.json",
            Some(&path),
        )
    })?;
    let manifest_text = std::str::from_utf8(manifest_bytes).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("方案包清单不是 UTF-8：{error}"),
            Some(&path),
        )
    })?;
    let manifest_value: Value = serde_json::from_str(manifest_text).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("方案包清单 JSON 无法解析：{error}"),
            Some(&path),
        )
    })?;
    reject_unsafe_json(&manifest_value, &path)?;
    let header = parse_package_header(manifest_text).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsupported,
            format!("方案包清单不受支持：{}（{}）", error.message, error.path),
            Some(&path),
        )
    })?;
    if header.kind != PackageKind::Profile {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsupported,
            "选择的包不是 Profile 方案包",
            Some(&path),
        ));
    }
    if header.payload.path != PROFILE_PAYLOAD_PATH {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            "方案包清单的 payload.path 必须是 profile.json",
            Some(&path),
        ));
    }
    if header.payload.size_bytes != payload_bytes.len() as u64 {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageDigestMismatch,
            "方案包内容大小与清单不一致",
            Some(&path),
        ));
    }
    if header.payload.digest.algorithm != DigestAlgorithm::Blake3 {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsupported,
            "R8-1 仅支持 BLAKE3 内容摘要",
            Some(&path),
        ));
    }
    let actual_digest = blake3::hash(payload_bytes).to_hex().to_string();
    if actual_digest != header.payload.digest.value {
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageDigestMismatch,
            "方案包内容摘要校验失败",
            Some(&path),
        ));
    }

    let profile_text = std::str::from_utf8(payload_bytes).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("方案内容不是 UTF-8：{error}"),
            Some(&path),
        )
    })?;
    let profile_value: Value = serde_json::from_str(profile_text).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("方案内容 JSON 无法解析：{error}"),
            Some(&path),
        )
    })?;
    reject_unsafe_json(&profile_value, &path)?;
    let profile = parse_workspace_profile(profile_text).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsupported,
            format!("方案合同不受支持：{}（{}）", error.message, error.path),
            Some(&path),
        )
    })?;

    let validation = runtime.validate_draft(&profile, manifests);
    let installed_modules = manifests
        .iter()
        .map(|manifest| manifest.id.as_str())
        .collect::<BTreeSet<_>>();
    let missing_module_ids = profile
        .enabled_modules
        .iter()
        .filter(|selection| !installed_modules.contains(selection.id.as_str()))
        .map(|selection| selection.id.clone())
        .collect::<Vec<_>>();
    let missing_component_ids = profile
        .enabled_components
        .iter()
        .filter(|selection| runtime.component_manifest(&selection.id).is_none())
        .map(|selection| selection.id.clone())
        .collect::<Vec<_>>();
    let snapshot = runtime.snapshot(manifests)?;
    let suggested_name = unique_profile_name(
        &profile.name,
        snapshot
            .profiles
            .iter()
            .map(|profile| profile.name.as_str()),
    );
    let issues = validation_issues(&validation);
    let can_import = validation.valid
        && !issues
            .iter()
            .any(|issue| issue.severity == ProfilePackageIssueSeverity::Error);
    let widget_count = profile
        .surfaces
        .iter()
        .map(|surface| surface.widgets.len())
        .sum();
    let reusable_binding_presets =
        collect_reusable_binding_presets(&profile, runtime, &snapshot);
    let preview = ProfilePackageImportPreview {
        package_path: path.to_string_lossy().into_owned(),
        package_id: header.package_id,
        producer_version: header.producer_version,
        profile_name: profile.name.clone(),
        description: profile.description.clone(),
        suggested_name,
        payload_digest: actual_digest,
        package_size_bytes: metadata.len(),
        module_count: profile.enabled_modules.len(),
        component_count: profile.enabled_components.len(),
        surface_count: profile.surfaces.len(),
        widget_count,
        pinned_tool_count: profile.shell_layout.pinned_tools.len(),
        tool_aliases: profile.tool_aliases.clone(),
        path_variables: profile.path_variables.clone(),
        reusable_binding_presets,
        missing_module_ids,
        missing_component_ids,
        issues,
        can_import,
    };
    Ok(InspectedProfilePackage { preview, profile })
}

fn collect_reusable_binding_presets(
    target_profile: &WorkspaceProfileV1,
    runtime: &WorkspaceProfileRuntime,
    snapshot: &WorkspaceProfileRuntimeSnapshot,
) -> Vec<ProfileLocalBindingPreset> {
    let mut presets = Vec::new();
    for summary in &snapshot.profiles {
        let bindings_path = runtime.local_bindings_path(&summary.id);
        let Ok(bytes) = fs::read(&bindings_path) else {
            continue;
        };
        let Ok(stored) = serde_json::from_slice::<StoredProfileLocalBindings>(&bytes) else {
            continue;
        };
        if stored.profile_id != summary.id {
            continue;
        }
        let Ok(source_profile) = runtime.profile_document(&summary.id) else {
            continue;
        };

        let stored_tools = stored
            .tool_mappings
            .iter()
            .map(|mapping| (mapping.id.as_str(), mapping))
            .collect::<BTreeMap<_, _>>();
        let mut tool_mappings = target_profile
            .tool_aliases
            .iter()
            .filter_map(|target_alias| {
                source_profile
                    .tool_aliases
                    .iter()
                    .find(|source_alias| source_alias.tool == target_alias.tool)
                    .and_then(|source_alias| stored_tools.get(source_alias.id.as_str()))
                    .filter(|mapping| reusable_mapping_is_available(mapping, true))
                    .map(|mapping| ProfileLocalBindingInput {
                        id: target_alias.id.clone(),
                        mode: mapping.mode,
                        path: mapping.path.clone(),
                    })
            })
            .collect::<Vec<_>>();

        let source_variables = source_profile
            .path_variables
            .iter()
            .map(|variable| (variable.id.as_str(), variable))
            .collect::<BTreeMap<_, _>>();
        let stored_paths = stored
            .path_mappings
            .iter()
            .map(|mapping| (mapping.id.as_str(), mapping))
            .collect::<BTreeMap<_, _>>();
        let mut path_mappings = target_profile
            .path_variables
            .iter()
            .filter_map(|target_variable| {
                source_variables
                    .get(target_variable.id.as_str())
                    .filter(|source_variable| source_variable.kind == target_variable.kind)
                    .and_then(|_| stored_paths.get(target_variable.id.as_str()))
                    .filter(|mapping| {
                        reusable_mapping_is_available(
                            mapping,
                            matches!(target_variable.kind, ProfilePathVariableKind::File),
                        )
                    })
                    .map(|mapping| ProfileLocalBindingInput {
                        id: target_variable.id.clone(),
                        mode: mapping.mode,
                        path: mapping.path.clone(),
                    })
            })
            .collect::<Vec<_>>();

        if tool_mappings.is_empty() && path_mappings.is_empty() {
            continue;
        }
        tool_mappings.sort_by(|left, right| left.id.cmp(&right.id));
        path_mappings.sort_by(|left, right| left.id.cmp(&right.id));
        presets.push(ProfileLocalBindingPreset {
            profile_id: summary.id.clone(),
            profile_name: summary.name.clone(),
            tool_mappings,
            path_mappings,
        });
    }
    presets.sort_by(|left, right| left.profile_name.cmp(&right.profile_name));
    presets
}

fn reusable_mapping_is_available(mapping: &ProfileLocalBindingInput, expect_file: bool) -> bool {
    match mapping.mode {
        ProfileLocalBindingMode::Automatic => mapping
            .path
            .as_deref()
            .is_none_or(|path| path.trim().is_empty()),
        ProfileLocalBindingMode::Path => mapping.path.as_deref().is_some_and(|value| {
            let metadata = fs::metadata(value);
            metadata.is_ok_and(|metadata| {
                if expect_file {
                    metadata.is_file()
                } else {
                    metadata.is_dir()
                }
            })
        }),
    }
}

fn portable_profile(profile: &WorkspaceProfileV1) -> WorkspaceProfileV1 {
    let mut portable = profile.clone();
    portable.id = "portable.profile.template".into();
    portable.revision = 1;
    for key in [
        "migration",
        "template",
        "editorMetadata",
        "shellHomeMigration",
        "settingsOwnershipMigration",
        "brandMigration",
    ] {
        portable.extensions.remove(key);
    }
    ensure_builtin_tool_aliases(&mut portable);
    portable
}

fn ensure_builtin_tool_aliases(profile: &mut WorkspaceProfileV1) {
    let enabled = profile
        .enabled_modules
        .iter()
        .map(|module| module.id.clone())
        .collect::<BTreeSet<_>>();
    if enabled.contains("builtin.render-center") {
        ensure_tool_alias(
            profile,
            "render-blender",
            "nexora.tool.blender",
            true,
            "渲染中心使用的 Blender 可执行文件",
        );
        ensure_tool_alias(
            profile,
            "render-ffmpeg",
            "nexora.tool.ffmpeg",
            false,
            "渲染结果打包使用的 FFmpeg",
        );
    }
    if enabled.contains("builtin.automation-runtime") {
        ensure_tool_alias(
            profile,
            "automation-python",
            "nexora.tool.python",
            true,
            "任务、Python 和旧插件使用的 Python 环境",
        );
    }
    if enabled.contains("builtin.external-tools") {
        ensure_tool_alias(
            profile,
            "media-ffprobe",
            "nexora.tool.ffprobe",
            false,
            "视频信息分析使用的 FFprobe",
        );
    }
    profile
        .tool_aliases
        .sort_by(|left, right| left.id.cmp(&right.id));
}

fn ensure_tool_alias(
    profile: &mut WorkspaceProfileV1,
    id: &str,
    tool: &str,
    required: bool,
    description: &str,
) {
    if profile.tool_aliases.iter().any(|alias| alias.id == id) {
        return;
    }
    profile.tool_aliases.push(ProfileToolAlias {
        id: id.into(),
        tool: tool.into(),
        version_requirement: "*".into(),
        required,
        description: description.into(),
        extensions: ExtensionFields::new(),
    });
}

fn validate_local_bindings(
    profile: &WorkspaceProfileV1,
    request: &ImportWorkspaceProfilePackageRequest,
) -> Result<
    (Vec<ProfileLocalBindingInput>, Vec<ProfileLocalBindingInput>),
    WorkspaceProfileRuntimeError,
> {
    let tool_aliases = profile
        .tool_aliases
        .iter()
        .map(|alias| (alias.id.as_str(), alias))
        .collect::<BTreeMap<_, _>>();
    let path_variables = profile
        .path_variables
        .iter()
        .map(|variable| (variable.id.as_str(), variable))
        .collect::<BTreeMap<_, _>>();
    let mut tool_mappings = BTreeMap::new();
    for mapping in &request.tool_mappings {
        let Some(alias) = tool_aliases.get(mapping.id.as_str()) else {
            return Err(mapping_error(
                request,
                format!("工具映射引用了不存在的别名：{}", mapping.id),
            ));
        };
        if tool_mappings.contains_key(mapping.id.as_str()) {
            return Err(mapping_error(
                request,
                format!("工具别名重复映射：{}", mapping.id),
            ));
        }
        let normalized = match mapping.mode {
            ProfileLocalBindingMode::Automatic => {
                if mapping
                    .path
                    .as_deref()
                    .is_some_and(|path| !path.trim().is_empty())
                {
                    return Err(mapping_error(
                        request,
                        format!("自动工具映射不能同时指定路径：{}", mapping.id),
                    ));
                }
                ProfileLocalBindingInput {
                    id: mapping.id.clone(),
                    mode: ProfileLocalBindingMode::Automatic,
                    path: None,
                }
            }
            ProfileLocalBindingMode::Path => ProfileLocalBindingInput {
                id: mapping.id.clone(),
                mode: ProfileLocalBindingMode::Path,
                path: Some(validate_mapping_path(
                    mapping.path.as_deref(),
                    true,
                    &format!("工具 {} ({})", alias.id, alias.tool),
                    request,
                )?),
            },
        };
        tool_mappings.insert(mapping.id.as_str(), normalized);
    }

    let mut path_mappings = BTreeMap::new();
    for mapping in &request.path_mappings {
        let Some(variable) = path_variables.get(mapping.id.as_str()) else {
            return Err(mapping_error(
                request,
                format!("路径映射引用了不存在的变量：{}", mapping.id),
            ));
        };
        if path_mappings.contains_key(mapping.id.as_str()) {
            return Err(mapping_error(
                request,
                format!("路径变量重复映射：{}", mapping.id),
            ));
        }
        if mapping.mode != ProfileLocalBindingMode::Path {
            return Err(mapping_error(
                request,
                format!("路径变量必须选择本机文件或目录：{}", mapping.id),
            ));
        }
        let expect_file = matches!(variable.kind, ProfilePathVariableKind::File);
        let normalized = ProfileLocalBindingInput {
            id: mapping.id.clone(),
            mode: ProfileLocalBindingMode::Path,
            path: Some(validate_mapping_path(
                mapping.path.as_deref(),
                expect_file,
                &format!("路径变量 {}", variable.id),
                request,
            )?),
        };
        path_mappings.insert(mapping.id.as_str(), normalized);
    }

    let mut missing = profile
        .tool_aliases
        .iter()
        .filter(|alias| alias.required && !tool_mappings.contains_key(alias.id.as_str()))
        .map(|alias| format!("工具 {} ({})", alias.id, alias.tool))
        .chain(
            profile
                .path_variables
                .iter()
                .filter(|variable| {
                    variable.required && !path_mappings.contains_key(variable.id.as_str())
                })
                .map(|variable| format!("路径 {} ({:?})", variable.id, variable.kind)),
        )
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        missing.sort();
        return Err(package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageMappingRequired,
            "导入前必须完成本机工具和路径映射",
            Some(Path::new(&request.package_path)),
        )
        .with_details(missing));
    }

    Ok((
        tool_mappings.into_values().collect(),
        path_mappings.into_values().collect(),
    ))
}

fn validate_mapping_path(
    value: Option<&str>,
    expect_file: bool,
    label: &str,
    request: &ImportWorkspaceProfilePackageRequest,
) -> Result<String, WorkspaceProfileRuntimeError> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| mapping_error(request, format!("{label} 尚未选择本机路径")))?;
    if !is_absolute_machine_path(value) || value.to_ascii_lowercase().starts_with("file://") {
        return Err(mapping_error(
            request,
            format!("{label} 必须使用本机绝对路径"),
        ));
    }
    let path = PathBuf::from(value);
    let metadata = fs::metadata(&path)
        .map_err(|error| mapping_error(request, format!("{label} 路径不可用：{error}")))?;
    if expect_file && !metadata.is_file() {
        return Err(mapping_error(request, format!("{label} 必须指向文件")));
    }
    if !expect_file && !metadata.is_dir() {
        return Err(mapping_error(request, format!("{label} 必须指向目录")));
    }
    fs::canonicalize(&path)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| mapping_error(request, format!("{label} 路径无法规范化：{error}")))
}

fn mapping_error(
    request: &ImportWorkspaceProfilePackageRequest,
    message: impl Into<String>,
) -> WorkspaceProfileRuntimeError {
    package_error(
        WorkspaceProfileRuntimeErrorCode::ProfilePackageMappingRequired,
        message,
        Some(Path::new(&request.package_path)),
    )
}

fn write_local_bindings(
    runtime: &WorkspaceProfileRuntime,
    bindings: &StoredProfileLocalBindings,
) -> Result<(), WorkspaceProfileRuntimeError> {
    let destination = runtime.local_bindings_path(&bindings.profile_id);
    let parent = destination.parent().ok_or_else(|| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfileIoError,
            "本机映射路径缺少父目录",
            Some(&destination),
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| package_io_error("创建本机映射目录失败", parent, error))?;
    let mut bytes = serde_json::to_vec_pretty(bindings).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("序列化本机映射失败：{error}"),
            Some(&destination),
        )
    })?;
    bytes.push(b'\n');
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("profile-bindings.json"),
        Uuid::new_v4().simple()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| package_io_error("创建本机映射临时文件失败", &temporary, error))?;
        file.write_all(&bytes)
            .map_err(|error| package_io_error("写入本机映射失败", &temporary, error))?;
        file.sync_all()
            .map_err(|error| package_io_error("同步本机映射失败", &temporary, error))?;
        fs::rename(&temporary, &destination)
            .map_err(|error| package_io_error("提交本机映射失败", &destination, error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, WorkspaceProfileRuntimeError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        package_error(
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
            format!("序列化方案包 JSON 失败：{error}"),
            None,
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validation_issues(validation: &WorkspaceProfileDraftValidation) -> Vec<ProfilePackageIssue> {
    validation
        .issues
        .iter()
        .map(|issue| ProfilePackageIssue {
            code: issue.code.clone(),
            severity: match issue.severity {
                super::profile_runtime::WorkspaceProfileSwitchIssueSeverity::Info => {
                    ProfilePackageIssueSeverity::Info
                }
                super::profile_runtime::WorkspaceProfileSwitchIssueSeverity::Warning => {
                    ProfilePackageIssueSeverity::Warning
                }
                super::profile_runtime::WorkspaceProfileSwitchIssueSeverity::Error => {
                    ProfilePackageIssueSeverity::Error
                }
            },
            message: issue.message.clone(),
            path: issue
                .module_id
                .clone()
                .or_else(|| issue.contribution_id.clone()),
        })
        .collect()
}

fn unique_profile_name<'a>(base: &str, existing_names: impl Iterator<Item = &'a str>) -> String {
    let existing = existing_names
        .map(|name| name.to_lowercase())
        .collect::<BTreeSet<_>>();
    if !existing.contains(&base.to_lowercase()) {
        return base.to_string();
    }
    for index in 1..10_000 {
        let candidate = if index == 1 {
            format!("{base} 副本")
        } else {
            format!("{base} 副本 {index}")
        };
        if !existing.contains(&candidate.to_lowercase()) {
            return candidate;
        }
    }
    format!("{base} 导入")
}

fn reject_unsafe_json(
    value: &Value,
    package_path: &Path,
) -> Result<(), WorkspaceProfileRuntimeError> {
    let issues = collect_safety_issues(value);
    if issues.is_empty() {
        return Ok(());
    }
    Err(package_error(
        WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsafe,
        "方案包包含敏感字段或本机绝对路径",
        Some(package_path),
    )
    .with_details(
        issues
            .into_iter()
            .map(|issue| format!("{}: {}", issue.path.unwrap_or_default(), issue.message))
            .collect(),
    ))
}

fn collect_safety_issues(value: &Value) -> Vec<ProfilePackageIssue> {
    let mut issues = Vec::new();
    scan_json(value, "$", &mut issues);
    issues
}

fn scan_json(value: &Value, path: &str, issues: &mut Vec<ProfilePackageIssue>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let child_path = format!("{path}.{key}");
                let normalized = key
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                if sensitive_key(&normalized) {
                    issues.push(ProfilePackageIssue {
                        code: "SENSITIVE_FIELD".into(),
                        severity: ProfilePackageIssueSeverity::Error,
                        message: format!("字段 {key} 可能包含凭据或私密数据"),
                        path: Some(child_path.clone()),
                    });
                } else if private_data_key(&normalized) {
                    issues.push(ProfilePackageIssue {
                        code: "PRIVATE_DATA_FIELD".into(),
                        severity: ProfilePackageIssueSeverity::Error,
                        message: format!("字段 {key} 属于聊天、联系人或本机资料数据"),
                        path: Some(child_path.clone()),
                    });
                }
                scan_json(child, &child_path, issues);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                scan_json(child, &format!("{path}[{index}]"), issues);
            }
        }
        Value::String(text) if is_absolute_machine_path(text) => {
            issues.push(ProfilePackageIssue {
                code: "ABSOLUTE_PATH".into(),
                severity: ProfilePackageIssueSeverity::Error,
                message: "包含本机绝对路径；R8-2 才会提供路径变量映射".into(),
                path: Some(path.into()),
            });
        }
        _ => {}
    }
}

fn sensitive_key(normalized: &str) -> bool {
    matches!(
        normalized,
        "password"
            | "passphrase"
            | "secret"
            | "token"
            | "apikey"
            | "accesstoken"
            | "refreshtoken"
            | "authtoken"
            | "sessiontoken"
            | "privatekey"
            | "credential"
            | "credentials"
            | "cookie"
            | "cookies"
    ) || normalized.ends_with("password")
        || normalized.ends_with("apikey")
        || normalized.ends_with("privatekey")
}

fn private_data_key(normalized: &str) -> bool {
    matches!(
        normalized,
        "chathistory"
            | "messagehistory"
            | "messages"
            | "conversations"
            | "contacts"
            | "contactlist"
            | "avatarpath"
            | "avatarcache"
    )
}

fn is_absolute_machine_path(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let bytes = value.as_bytes();
    let windows_drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    windows_drive
        || value.starts_with("\\\\")
        || value.starts_with("//")
        || value.starts_with('/')
        || value.to_ascii_lowercase().starts_with("file://")
}

fn invalid_archive_path(name: &str) -> bool {
    name.is_empty()
        || name.contains('\\')
        || name.starts_with('/')
        || name
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
}

fn replace_export_file(
    temporary: &Path,
    destination: &Path,
) -> Result<(), WorkspaceProfileRuntimeError> {
    if !destination.exists() {
        return fs::rename(temporary, destination)
            .map_err(|error| package_io_error("提交导出方案包失败", destination, error));
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let backup = parent.join(format!(
        ".{}.{}.bak",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("profile.pmc-profile"),
        Uuid::new_v4().simple()
    ));
    fs::rename(destination, &backup)
        .map_err(|error| package_io_error("备份已有方案包失败", destination, error))?;
    match fs::rename(temporary, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup, destination);
            Err(package_io_error("提交导出方案包失败", destination, error))
        }
    }
}

fn package_error(
    code: WorkspaceProfileRuntimeErrorCode,
    message: impl Into<String>,
    path: Option<&Path>,
) -> WorkspaceProfileRuntimeError {
    WorkspaceProfileRuntimeError::new(code, message, path)
}

fn package_io_error(
    context: &str,
    path: &Path,
    error: std::io::Error,
) -> WorkspaceProfileRuntimeError {
    package_error(
        WorkspaceProfileRuntimeErrorCode::ProfileIoError,
        format!("{context}：{error}"),
        Some(path),
    )
}

fn package_zip_error(
    context: &str,
    path: &Path,
    error: zip::result::ZipError,
) -> WorkspaceProfileRuntimeError {
    package_error(
        WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid,
        format!("{context}：{error}"),
        Some(path),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn profile(name: &str) -> WorkspaceProfileV1 {
        parse_workspace_profile(&format!(
            r#"{{
              "schemaVersion":1,
              "id":"local.test-profile",
              "name":{name:?},
              "description":"portable test",
              "revision":7,
              "enabledModules":[],
              "enabledComponents":[],
              "moduleSettings":{{}},
              "componentSettings":{{}},
              "shellLayout":{{"navigation":[],"pinnedTools":[],"navigationKind":"top-bar"}},
              "surfaces":[],
              "dataSources":[],
              "commandBindings":[],
              "workflowBindings":[],
              "variables":{{}}
            }}"#
        ))
        .unwrap()
    }

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nexora-profile-package-{label}-{}",
            Uuid::new_v4().simple()
        ))
    }

    fn initialized_runtime(root: &Path) -> WorkspaceProfileRuntime {
        let runtime = WorkspaceProfileRuntime::new(root);
        runtime
            .initialize_from_current_configuration(&[], &BTreeSet::new(), &[])
            .unwrap();
        runtime
    }

    #[test]
    fn exports_deterministic_package_and_round_trips_without_switching() {
        let root = temp_root("round-trip");
        fs::create_dir_all(&root).unwrap();
        let first = root.join("first.pmc-profile");
        let second = root.join("second.pmc-profile");
        let source = profile("Portable profile");
        let first_result = export_profile_package(&source, first.to_str().unwrap()).unwrap();
        let second_result = export_profile_package(&source, second.to_str().unwrap()).unwrap();
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
        assert_eq!(first_result.payload_digest, second_result.payload_digest);

        let runtime = initialized_runtime(&root.join("runtime"));
        let before = runtime.snapshot(&[]).unwrap().current_profile.id;
        let preview = inspect_profile_package(first.to_str().unwrap(), &runtime, &[]).unwrap();
        assert!(preview.can_import);
        let imported = import_profile_package(
            &ImportWorkspaceProfilePackageRequest {
                package_path: first.to_string_lossy().into_owned(),
                name: preview.suggested_name,
                tool_mappings: Vec::new(),
                path_mappings: Vec::new(),
            },
            &runtime,
            &[],
        )
        .unwrap();
        assert_ne!(imported.profile.id, source.id);
        assert_eq!(imported.profile.revision, 1);
        assert_eq!(imported.snapshot.current_profile.id, before);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_sensitive_fields_and_absolute_paths_before_export() {
        let root = temp_root("unsafe");
        fs::create_dir_all(&root).unwrap();
        let mut unsafe_profile = profile("Unsafe profile");
        unsafe_profile.module_settings.insert(
            "builtin.test".into(),
            serde_json::json!({ "apiKey": "abc" }),
        );
        let error = export_profile_package(
            &unsafe_profile,
            root.join("secret.pmc-profile").to_str().unwrap(),
        )
        .unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsafe
        ));

        let mut path_profile = profile("Path profile");
        path_profile
            .variables
            .insert("blender".into(), r"C:\\Blender\\blender.exe".into());
        let error = export_profile_package(
            &path_profile,
            root.join("path.pmc-profile").to_str().unwrap(),
        )
        .unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePackageUnsafe
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_digest_mismatch_without_changing_repository() {
        let root = temp_root("digest");
        fs::create_dir_all(&root).unwrap();
        let package = root.join("profile.pmc-profile");
        export_profile_package(&profile("Digest profile"), package.to_str().unwrap()).unwrap();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&package)
            .unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let offset = archive.by_name(PROFILE_PAYLOAD_PATH).unwrap().data_start();
        drop(archive);
        let mut bytes = fs::read(&package).unwrap();
        bytes[offset as usize] ^= 1;
        fs::write(&package, bytes).unwrap();

        let runtime = initialized_runtime(&root.join("runtime"));
        let count_before = runtime.snapshot(&[]).unwrap().profiles.len();
        let error = inspect_profile_package(package.to_str().unwrap(), &runtime, &[]).unwrap_err();
        assert!(
            matches!(
                error.code,
                WorkspaceProfileRuntimeErrorCode::ProfilePackageDigestMismatch
                    | WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid
            ),
            "unexpected error: {:?} {}",
            error.code,
            error.message
        );
        assert_eq!(runtime.snapshot(&[]).unwrap().profiles.len(), count_before);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_traversal_and_unexpected_entries() {
        let root = temp_root("traversal");
        fs::create_dir_all(&root).unwrap();
        let package = root.join("bad.pmc-profile");
        let file = File::create(&package).unwrap();
        let mut archive = ZipWriter::new(file);
        archive
            .start_file("../profile.json", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"{}").unwrap();
        archive.finish().unwrap();
        let runtime = initialized_runtime(&root.join("runtime"));
        let error = inspect_profile_package(package.to_str().unwrap(), &runtime, &[]).unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_dependencies_block_import_without_writing_profile() {
        let root = temp_root("missing-dependency");
        fs::create_dir_all(&root).unwrap();
        let package = root.join("missing.pmc-profile");
        let mut source = profile("Missing dependency");
        source
            .enabled_modules
            .push(pmc_platform::ProfileModuleSelection {
                id: "vendor.missing-module".into(),
                version_requirement: "^1.0".into(),
                extensions: ExtensionFields::new(),
            });
        export_profile_package(&source, package.to_str().unwrap()).unwrap();
        let runtime = initialized_runtime(&root.join("runtime"));
        let before = runtime.snapshot(&[]).unwrap().profiles.len();
        let preview = inspect_profile_package(package.to_str().unwrap(), &runtime, &[]).unwrap();
        assert!(!preview.can_import);
        assert_eq!(preview.missing_module_ids, vec!["vendor.missing-module"]);
        let error = import_profile_package(
            &ImportWorkspaceProfilePackageRequest {
                package_path: package.to_string_lossy().into_owned(),
                name: "Should not import".into(),
                tool_mappings: Vec::new(),
                path_mappings: Vec::new(),
            },
            &runtime,
            &[],
        )
        .unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePackageInvalid
        ));
        assert_eq!(runtime.snapshot(&[]).unwrap().profiles.len(), before);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn required_tool_and_path_mappings_are_stored_outside_the_profile() {
        let root = temp_root("local-bindings");
        fs::create_dir_all(&root).unwrap();
        let package = root.join("mapped.pmc-profile");
        let executable = root.join("tool.exe");
        let output = root.join("output");
        fs::write(&executable, b"test executable").unwrap();
        fs::create_dir_all(&output).unwrap();

        let mut source = profile("Mapped profile");
        source.tool_aliases.push(ProfileToolAlias {
            id: "render-tool".into(),
            tool: "nexora.tool.blender".into(),
            version_requirement: "*".into(),
            required: true,
            description: "test tool".into(),
            extensions: ExtensionFields::new(),
        });
        source.path_variables.push(ProfilePathVariable {
            id: "output-root".into(),
            kind: ProfilePathVariableKind::Directory,
            required: true,
            description: "test output".into(),
            extensions: ExtensionFields::new(),
        });
        export_profile_package(&source, package.to_str().unwrap()).unwrap();

        let runtime = initialized_runtime(&root.join("runtime"));
        let preview = inspect_profile_package(package.to_str().unwrap(), &runtime, &[]).unwrap();
        assert_eq!(preview.tool_aliases.len(), 1);
        assert_eq!(preview.path_variables.len(), 1);
        let before = runtime.snapshot(&[]).unwrap().profiles.len();
        let missing_error = import_profile_package(
            &ImportWorkspaceProfilePackageRequest {
                package_path: package.to_string_lossy().into_owned(),
                name: "Missing mappings".into(),
                tool_mappings: Vec::new(),
                path_mappings: Vec::new(),
            },
            &runtime,
            &[],
        )
        .unwrap_err();
        assert!(matches!(
            missing_error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePackageMappingRequired
        ));
        assert_eq!(runtime.snapshot(&[]).unwrap().profiles.len(), before);

        let imported = import_profile_package(
            &ImportWorkspaceProfilePackageRequest {
                package_path: package.to_string_lossy().into_owned(),
                name: "Mapped import".into(),
                tool_mappings: vec![ProfileLocalBindingInput {
                    id: "render-tool".into(),
                    mode: ProfileLocalBindingMode::Path,
                    path: Some(executable.to_string_lossy().into_owned()),
                }],
                path_mappings: vec![ProfileLocalBindingInput {
                    id: "output-root".into(),
                    mode: ProfileLocalBindingMode::Path,
                    path: Some(output.to_string_lossy().into_owned()),
                }],
            },
            &runtime,
            &[],
        )
        .unwrap();
        let bindings_path = runtime.local_bindings_path(&imported.profile.id);
        assert!(bindings_path.is_file());
        let stored = fs::read_to_string(bindings_path).unwrap();
        assert!(stored.contains("render-tool"));
        assert!(stored.contains("output-root"));
        let profile_json = serde_json::to_string(&imported.profile).unwrap();
        assert!(!profile_json.contains(executable.to_string_lossy().as_ref()));
        assert!(!profile_json.contains(output.to_string_lossy().as_ref()));

        let reuse_preview =
            inspect_profile_package(package.to_str().unwrap(), &runtime, &[]).unwrap();
        assert_eq!(reuse_preview.reusable_binding_presets.len(), 1);
        let preset = &reuse_preview.reusable_binding_presets[0];
        let canonical_executable = fs::canonicalize(&executable).unwrap();
        let canonical_output = fs::canonicalize(&output).unwrap();
        assert_eq!(preset.profile_id, imported.profile.id);
        assert_eq!(preset.tool_mappings.len(), 1);
        assert_eq!(preset.tool_mappings[0].id, "render-tool");
        assert_eq!(
            preset.tool_mappings[0].path.as_deref(),
            Some(canonical_executable.to_string_lossy().as_ref())
        );
        assert_eq!(preset.path_mappings.len(), 1);
        assert_eq!(preset.path_mappings[0].id, "output-root");
        assert_eq!(
            preset.path_mappings[0].path.as_deref(),
            Some(canonical_output.to_string_lossy().as_ref())
        );
        runtime.delete_profile(&imported.profile.id, &[]).unwrap();
        assert!(!runtime.local_bindings_path(&imported.profile.id).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn name_conflict_never_overwrites_existing_profile() {
        let root = temp_root("name-conflict");
        fs::create_dir_all(&root).unwrap();
        let package = root.join("profile.pmc-profile");
        export_profile_package(&profile("Shared name"), package.to_str().unwrap()).unwrap();
        let runtime = initialized_runtime(&root.join("runtime"));
        let request = ImportWorkspaceProfilePackageRequest {
            package_path: package.to_string_lossy().into_owned(),
            name: "Shared name".into(),
            tool_mappings: Vec::new(),
            path_mappings: Vec::new(),
        };
        import_profile_package(&request, &runtime, &[]).unwrap();
        let count = runtime.snapshot(&[]).unwrap().profiles.len();
        let preview = inspect_profile_package(package.to_str().unwrap(), &runtime, &[]).unwrap();
        assert_eq!(preview.suggested_name, "Shared name 副本");
        let error = import_profile_package(&request, &runtime, &[]).unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePackageNameConflict
        ));
        assert_eq!(runtime.snapshot(&[]).unwrap().profiles.len(), count);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_oversized_archive_entry() {
        let root = temp_root("oversized");
        fs::create_dir_all(&root).unwrap();
        let package = root.join("oversized.pmc-profile");
        let file = File::create(&package).unwrap();
        let mut archive = ZipWriter::new(file);
        archive
            .start_file(MANIFEST_PATH, SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(&vec![b' '; MAX_ENTRY_BYTES as usize + 1])
            .unwrap();
        archive.finish().unwrap();
        let runtime = initialized_runtime(&root.join("runtime"));
        let error = inspect_profile_package(package.to_str().unwrap(), &runtime, &[]).unwrap_err();
        assert!(matches!(
            error.code,
            WorkspaceProfileRuntimeErrorCode::ProfilePackageTooLarge
        ));
        let _ = fs::remove_dir_all(root);
    }
}
