use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use pmc_platform::{
    parse_component_manifest, ContentDigest, DigestAlgorithm, ExtensionFields, PackageHeaderV1,
    PackageKind, PackagePayloadDescriptor, PACKAGE_FORMAT_VERSION, PACKAGE_MAGIC,
    PLATFORM_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const COMPONENT_MANIFEST_PATH: &str = "component.json";
const PACKAGE_MANIFEST_PATH: &str = "manifest.json";
const SIGNATURE_PREFIX: &[u8] = b"nexora.component-pack.v1\0";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPackRequest {
    pub source_path: String,
    pub destination_path: String,
    pub key_path: String,
    pub publisher_id: String,
    pub publisher_name: String,
    #[serde(default = "default_license")]
    pub license: String,
    #[serde(default = "default_producer_version")]
    pub producer_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPackResult {
    pub component_id: String,
    pub component_version: String,
    pub destination_path: String,
    pub content_digest: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningKeyResult {
    pub path: String,
    pub public_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SigningKeyFile {
    algorithm: Option<String>,
    private_key: String,
    public_key: Option<String>,
}

fn default_license() -> String {
    "NOASSERTION".into()
}

fn default_producer_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

pub fn generate_signing_key(destination: &Path) -> Result<SigningKeyResult, String> {
    if destination.exists() {
        return Err(format!("密钥文件已存在：{}", destination.display()));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "密钥文件路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建密钥目录失败：{error}"))?;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut seed = [0u8; 32];
    seed[..16].copy_from_slice(first.as_bytes());
    seed[16..].copy_from_slice(second.as_bytes());
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key =
        base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes());
    let document = json!({
        "algorithm": "ed25519",
        "privateKey": base64::engine::general_purpose::STANDARD.encode(seed),
        "publicKey": public_key,
    });
    write_new_json(destination, &document)?;
    Ok(SigningKeyResult {
        path: destination.to_string_lossy().into_owned(),
        public_key,
    })
}

pub fn pack_component(request: ComponentPackRequest) -> Result<ComponentPackResult, String> {
    let source = fs::canonicalize(&request.source_path)
        .map_err(|error| format!("组件目录不可用：{error}"))?;
    if !source.is_dir() {
        return Err("组件来源必须是目录".into());
    }
    let destination = PathBuf::from(&request.destination_path);
    if destination.extension().and_then(|value| value.to_str()) != Some("pmc-pack") {
        return Err("组件包输出文件必须以 .pmc-pack 结尾".into());
    }
    if request.publisher_id.trim().is_empty() || request.publisher_name.trim().is_empty() {
        return Err("发布者 ID 和名称不能为空".into());
    }
    let key = read_signing_key(Path::new(&request.key_path))?;
    let component_path = source.join(COMPONENT_MANIFEST_PATH);
    let component_bytes =
        fs::read(&component_path).map_err(|error| format!("读取 component.json 失败：{error}"))?;
    let component_text = std::str::from_utf8(&component_bytes)
        .map_err(|error| format!("component.json 必须是 UTF-8：{error}"))?;
    let component = parse_component_manifest(component_text).map_err(|error| {
        format!(
            "component.json 合同无效：{} ({})",
            error.message, error.path
        )
    })?;
    let files = collect_component_files(&source, &destination)?;
    let mut content_hasher = blake3::Hasher::new();
    for (path, bytes) in &files {
        content_hasher.update(path.as_bytes());
        content_hasher.update(&[0]);
        content_hasher.update(bytes);
    }
    let content_digest = content_hasher.finalize().to_hex().to_string();
    let component_digest = blake3::hash(&component_bytes).to_hex().to_string();
    let mut extensions = ExtensionFields::new();
    extensions.insert(
        "contentDigest".into(),
        Value::String(content_digest.clone()),
    );
    extensions.insert(
        "publisher".into(),
        json!({
            "id": request.publisher_id.trim(),
            "displayName": request.publisher_name.trim(),
            "publicKey": base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes()),
        }),
    );
    extensions.insert("license".into(), Value::String(request.license));
    let mut header = PackageHeaderV1 {
        magic: PACKAGE_MAGIC.into(),
        schema_version: PLATFORM_SCHEMA_VERSION,
        format_version: PACKAGE_FORMAT_VERSION,
        kind: PackageKind::ComponentPack,
        package_id: format!("pack.{}-{}", component.id, component.version),
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("读取系统时间失败：{error}"))?
            .as_millis() as u64,
        producer_version: request.producer_version,
        payload: PackagePayloadDescriptor {
            path: COMPONENT_MANIFEST_PATH.into(),
            digest: ContentDigest {
                algorithm: DigestAlgorithm::Blake3,
                value: component_digest,
            },
            size_bytes: component_bytes.len() as u64,
            extensions: ExtensionFields::new(),
        },
        extensions,
    };
    let signature = key.sign(&signature_material(&header, &content_digest)?);
    header.extensions.insert(
        "signature".into(),
        json!({
            "algorithm": "ed25519",
            "value": base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        }),
    );
    let header_bytes =
        serde_json::to_vec_pretty(&header).map_err(|error| format!("序列化包头失败：{error}"))?;
    write_package(&destination, &header_bytes, &files)?;
    Ok(ComponentPackResult {
        component_id: component.id,
        component_version: component.version,
        destination_path: destination.to_string_lossy().into_owned(),
        content_digest,
        file_count: files.len(),
    })
}

fn read_signing_key(path: &Path) -> Result<SigningKey, String> {
    let input = fs::read_to_string(path).map_err(|error| format!("读取签名私钥失败：{error}"))?;
    let stored: SigningKeyFile =
        serde_json::from_str(&input).map_err(|error| format!("签名私钥 JSON 无效：{error}"))?;
    if stored.algorithm.as_deref().unwrap_or("ed25519") != "ed25519" {
        return Err("仅支持 ed25519 签名私钥".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(stored.private_key.trim())
        .map_err(|_| "privateKey 不是有效 Base64".to_string())?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "privateKey 必须是 32 字节 Ed25519 种子".to_string())?;
    let key = SigningKey::from_bytes(&seed);
    if let Some(public_key) = stored.public_key {
        let expected =
            base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes());
        if public_key.trim() != expected {
            return Err("签名私钥中的 publicKey 与 privateKey 不匹配".into());
        }
    }
    Ok(key)
}

fn collect_component_files(
    root: &Path,
    destination: &Path,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let destination = destination.canonicalize().ok();
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| format!("遍历组件目录失败：{error}"))?;
        if entry.path() == root || entry.file_type().is_dir() {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(format!(
                "组件目录不允许包含符号链接：{}",
                entry.path().display()
            ));
        }
        if entry
            .path()
            .components()
            .any(|part| matches!(part, Component::Normal(value) if value == ".git" || value == "__pycache__"))
        {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) == Some("pyc") {
            continue;
        }
        if destination
            .as_ref()
            .is_some_and(|output| output == entry.path())
        {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| "组件文件超出来源目录".to_string())?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if name == PACKAGE_MANIFEST_PATH || name.is_empty() || name.contains("../") {
            return Err(format!("组件文件路径不安全：{name}"));
        }
        let bytes = fs::read(entry.path())
            .map_err(|error| format!("读取组件文件失败 {}：{error}", entry.path().display()))?;
        files.push((name, bytes));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if !files
        .iter()
        .any(|(path, _)| path == COMPONENT_MANIFEST_PATH)
    {
        return Err("组件目录缺少根 component.json".into());
    }
    Ok(files)
}

fn signature_material(header: &PackageHeaderV1, content_digest: &str) -> Result<Vec<u8>, String> {
    let mut unsigned = header.clone();
    unsigned.extensions.remove("signature");
    let mut material = SIGNATURE_PREFIX.to_vec();
    material.extend(
        serde_json::to_vec(&unsigned).map_err(|error| format!("序列化签名材料失败：{error}"))?,
    );
    material.push(0);
    material.extend(content_digest.as_bytes());
    Ok(material)
}

fn write_package(
    destination: &Path,
    header: &[u8],
    files: &[(String, Vec<u8>)],
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "组件包输出路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建组件包输出目录失败：{error}"))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("component.pmc-pack"),
        Uuid::new_v4().simple()
    ));
    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("创建组件包临时文件失败：{error}"))?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        archive
            .start_file(PACKAGE_MANIFEST_PATH, options)
            .map_err(|error| format!("写入组件包包头失败：{error}"))?;
        archive
            .write_all(header)
            .map_err(|error| format!("写入组件包包头失败：{error}"))?;
        for (path, bytes) in files {
            archive
                .start_file(path, options)
                .map_err(|error| format!("写入组件包条目失败：{error}"))?;
            archive
                .write_all(bytes)
                .map_err(|error| format!("写入组件包条目失败：{error}"))?;
        }
        let file = archive
            .finish()
            .map_err(|error| format!("完成组件包失败：{error}"))?;
        file.sync_all()
            .map_err(|error| format!("同步组件包失败：{error}"))?;
        if destination.exists() {
            fs::remove_file(destination).map_err(|error| format!("替换旧组件包失败：{error}"))?;
        }
        fs::rename(&temporary, destination).map_err(|error| format!("提交组件包失败：{error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_new_json(path: &Path, value: &Value) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| format!("序列化密钥文件失败：{error}"))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("创建密钥文件失败：{error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("写入密钥文件失败：{error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("写入密钥文件失败：{error}"))?;
    file.sync_all()
        .map_err(|error| format!("同步密钥文件失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_key_round_trip() {
        let root = std::env::temp_dir().join(format!("nexora-pack-key-{}", Uuid::new_v4()));
        let path = root.join("publisher.json");
        let result = generate_signing_key(&path).unwrap();
        assert!(!result.public_key.is_empty());
        assert!(read_signing_key(&path).is_ok());
        let _ = fs::remove_dir_all(root);
    }
}
