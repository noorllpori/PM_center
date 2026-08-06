use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use pmc_platform::{
    parse_component_manifest, ContentDigest, DigestAlgorithm, ExtensionFields, PackageHeaderV1,
    PackageKind, PackagePayloadDescriptor, PACKAGE_FORMAT_VERSION, PACKAGE_MAGIC,
    PLATFORM_SCHEMA_VERSION,
};
use serde::Deserialize;
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SigningKeyFile {
    algorithm: Option<String>,
    private_key: String,
    public_key: Option<String>,
}

struct PackOptions {
    source: PathBuf,
    destination: PathBuf,
    key_path: PathBuf,
    publisher_id: String,
    publisher_name: String,
    license: String,
    producer_version: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("[nexora-component-pack] {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Err("缺少命令".into());
    };
    match command {
        "keygen" => {
            let destination = args
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| "用法：keygen <private-key.json>".to_string())?;
            generate_key(&destination)
        }
        "pack" => pack(parse_pack_options(&args[1..])?),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        _ => Err(format!("未知命令：{command}")),
    }
}

fn print_usage() {
    println!(
        "Nexora component packer\n\n\
Usage:\n\
  cargo run --manifest-path src-tauri/Cargo.toml --bin nexora-component-pack -- keygen <private-key.json>\n\
  cargo run --manifest-path src-tauri/Cargo.toml --bin nexora-component-pack -- pack <component-dir> <output.pmc-pack> --key <private-key.json> --publisher-id <id> --publisher-name <name> [--license <SPDX>] [--producer-version <semver>]\n\n\
The private key file is local signing material. Do not add it to a component package or source control."
    );
}

fn parse_pack_options(args: &[String]) -> Result<PackOptions, String> {
    if args.len() < 2 {
        return Err("用法：pack <component-dir> <output.pmc-pack> ...".into());
    }
    let mut key_path = None;
    let mut publisher_id = None;
    let mut publisher_name = None;
    let mut license = "NOASSERTION".to_string();
    let mut producer_version = env!("CARGO_PKG_VERSION").to_string();
    let mut index = 2;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("参数 {} 缺少值", args[index]))?;
        match args[index].as_str() {
            "--key" => key_path = Some(PathBuf::from(value)),
            "--publisher-id" => publisher_id = Some(value.trim().to_string()),
            "--publisher-name" => publisher_name = Some(value.trim().to_string()),
            "--license" => license = value.trim().to_string(),
            "--producer-version" => producer_version = value.trim().to_string(),
            other => return Err(format!("未知参数：{other}")),
        }
        index += 2;
    }
    let destination = PathBuf::from(&args[1]);
    if destination.extension().and_then(|item| item.to_str()) != Some("pmc-pack") {
        return Err("组件包输出文件必须以 .pmc-pack 结尾".into());
    }
    let options = PackOptions {
        source: PathBuf::from(&args[0]),
        destination,
        key_path: key_path.ok_or_else(|| "缺少 --key".to_string())?,
        publisher_id: publisher_id
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "缺少 --publisher-id".to_string())?,
        publisher_name: publisher_name
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "缺少 --publisher-name".to_string())?,
        license,
        producer_version,
    };
    Ok(options)
}

fn generate_key(destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err(format!("密钥文件已存在：{}", destination.display()));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "密钥文件路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建密钥目录失败：{error}"))?;
    // UUID v4 comes from the operating system CSPRNG. Two values provide the
    // 32 bytes Ed25519 uses as its seed without adding another runtime crate.
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut seed = [0u8; 32];
    seed[..16].copy_from_slice(first.as_bytes());
    seed[16..].copy_from_slice(second.as_bytes());
    let signing_key = SigningKey::from_bytes(&seed);
    let document = json!({
        "algorithm": "ed25519",
        "privateKey": base64::engine::general_purpose::STANDARD.encode(seed),
        "publicKey": base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes()),
    });
    write_new_json(destination, &document)?;
    println!(
        "[nexora-component-pack] 已生成 Ed25519 私钥：{}",
        destination.display()
    );
    println!(
        "[nexora-component-pack] 公钥：{}",
        base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes())
    );
    Ok(())
}

fn pack(options: PackOptions) -> Result<(), String> {
    let source =
        fs::canonicalize(&options.source).map_err(|error| format!("组件目录不可用：{error}"))?;
    if !source.is_dir() {
        return Err("组件来源必须是目录".into());
    }
    let key = read_signing_key(&options.key_path)?;
    let component_json = source.join(COMPONENT_MANIFEST_PATH);
    let component_bytes =
        fs::read(&component_json).map_err(|error| format!("读取 component.json 失败：{error}"))?;
    let component_text = std::str::from_utf8(&component_bytes)
        .map_err(|error| format!("component.json 必须是 UTF-8：{error}"))?;
    let component = parse_component_manifest(component_text).map_err(|error| {
        format!(
            "component.json 合同无效：{} ({})",
            error.message, error.path
        )
    })?;
    let files = collect_component_files(&source, &options.destination)?;
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
            "id": options.publisher_id,
            "displayName": options.publisher_name,
            "publicKey": base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes()),
        }),
    );
    extensions.insert("license".into(), Value::String(options.license));
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
        producer_version: options.producer_version,
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
    write_package(&options.destination, &header_bytes, &files)?;
    println!(
        "[nexora-component-pack] 已生成 {} {} -> {}",
        component.id,
        component.version,
        options.destination.display()
    );
    println!("[nexora-component-pack] 内容摘要：{content_digest}");
    Ok(())
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
            .any(|part| matches!(part, Component::Normal(value) if value == ".git"))
        {
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
