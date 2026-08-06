use nexora_lib::component_packager::{generate_signing_key, pack_component, ComponentPackRequest};
use std::path::PathBuf;

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
            let result = generate_signing_key(&destination)?;
            println!(
                "[nexora-component-pack] 已生成 Ed25519 私钥：{}",
                result.path
            );
            println!("[nexora-component-pack] 公钥：{}", result.public_key);
            Ok(())
        }
        "pack" => {
            let result = pack_component(parse_pack_request(&args[1..])?)?;
            println!(
                "[nexora-component-pack] 已生成 {} {} -> {}",
                result.component_id, result.component_version, result.destination_path
            );
            println!(
                "[nexora-component-pack] 内容摘要：{}",
                result.content_digest
            );
            Ok(())
        }
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

fn parse_pack_request(args: &[String]) -> Result<ComponentPackRequest, String> {
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
            "--key" => key_path = Some(value.clone()),
            "--publisher-id" => publisher_id = Some(value.trim().to_string()),
            "--publisher-name" => publisher_name = Some(value.trim().to_string()),
            "--license" => license = value.trim().to_string(),
            "--producer-version" => producer_version = value.trim().to_string(),
            other => return Err(format!("未知参数：{other}")),
        }
        index += 2;
    }
    Ok(ComponentPackRequest {
        source_path: args[0].clone(),
        destination_path: args[1].clone(),
        key_path: key_path.ok_or_else(|| "缺少 --key".to_string())?,
        publisher_id: publisher_id
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "缺少 --publisher-id".to_string())?,
        publisher_name: publisher_name
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "缺少 --publisher-name".to_string())?,
        license,
        producer_version,
    })
}
