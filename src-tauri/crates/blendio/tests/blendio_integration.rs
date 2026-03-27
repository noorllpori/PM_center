use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use blendio::{BlendFile, CompressionKind, summarize};
use serde_json::Value;

struct SamplePaths {
    uncompressed: PathBuf,
    compressed: PathBuf,
}

fn sample_paths() -> &'static SamplePaths {
    static SAMPLES: OnceLock<SamplePaths> = OnceLock::new();
    SAMPLES.get_or_init(|| {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("blendio-tests-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&root).unwrap();

        let uncompressed = root.join("sample_uncompressed.blend");
        let compressed = root.join("sample_compressed.blend");
        let expr = format!(
            "import bpy; \
             bpy.data.objects['Cube'].name='MyCube'; \
             bpy.data.collections['Collection'].name='MainCollection'; \
             bpy.ops.wm.save_as_mainfile(filepath={}, compress=False); \
             bpy.ops.wm.save_as_mainfile(filepath={}, compress=True)",
            python_string(&uncompressed),
            python_string(&compressed)
        );

        let output = Command::new(blender_exe())
            .arg("--background")
            .arg("--factory-startup")
            .arg("--python-expr")
            .arg(expr)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "failed to generate Blender samples\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        SamplePaths {
            uncompressed,
            compressed,
        }
    })
}

fn blender_exe() -> PathBuf {
    std::env::var_os("BLENDER_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"D:\Blender_4.5\blender.exe"))
}

fn python_string(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("'{}'", escaped)
}

fn run_blendio_json(args: impl IntoIterator<Item = OsString>) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_blendio"))
        .arg("--json")
        .args(args)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "blendio command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_blendio_text(args: impl IntoIterator<Item = OsString>) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_blendio"))
        .args(args)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "blendio command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn library_reads_uncompressed_and_compressed_samples() {
    let samples = sample_paths();

    let uncompressed = BlendFile::open(&samples.uncompressed).unwrap();
    assert_eq!(uncompressed.header().compression, CompressionKind::None);

    let compressed = BlendFile::open(&samples.compressed).unwrap();
    assert_eq!(compressed.header().compression, CompressionKind::Zstd);

    let summary = summarize(&compressed).unwrap();
    let object_names = summary
        .objects
        .iter()
        .map(|object| object.name.as_str())
        .collect::<Vec<_>>();
    assert!(object_names.contains(&"MyCube"));
    assert!(
        summary
            .collections
            .iter()
            .any(|collection| collection.name == "MainCollection")
    );
}

#[test]
fn info_command_matches_expected_scene_summary() {
    let samples = sample_paths();

    for path in [&samples.uncompressed, &samples.compressed] {
        let value = run_blendio_json(vec![
            OsString::from("info"),
            path.as_os_str().to_os_string(),
        ]);

        assert_eq!(value["scenes"][0]["name"], "Scene");
        assert_eq!(value["scenes"][0]["camera"], "Camera");
        assert!(
            value["objects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["name"] == "MyCube" && entry["data_target"]["code"] == "ME")
        );
        assert!(
            value["collections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["name"] == "MainCollection")
        );
        assert_eq!(value["meshes"][0]["totvert"], 8);
        assert_eq!(value["meshes"][0]["totedge"], 12);
        assert_eq!(value["meshes"][0]["totloop"], 24);
        assert_eq!(value["meshes"][0]["totpoly"], 6);
    }
}

#[test]
fn blocks_command_lists_core_blocks() {
    let samples = sample_paths();
    let value = run_blendio_json(vec![
        OsString::from("blocks"),
        samples.uncompressed.as_os_str().to_os_string(),
    ]);

    let codes = value
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["code"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();

    for expected in ["REND", "GLOB", "DNA1", "ENDB", "SC", "OB", "ME", "GR"] {
        assert!(
            codes.iter().any(|code| code == expected),
            "missing block {expected}"
        );
    }
}

#[test]
fn sdna_command_describes_object_fields() {
    let samples = sample_paths();
    let value = run_blendio_json(vec![
        OsString::from("sdna"),
        samples.uncompressed.as_os_str().to_os_string(),
        OsString::from("--type"),
        OsString::from("Object"),
    ]);

    let fields = value["fields"].as_array().unwrap();
    assert!(fields.iter().any(|field| field["name"] == "id"));
    assert!(fields.iter().any(|field| field["name"] == "*data"));
    assert!(
        fields
            .iter()
            .any(|field| field["name"] == "*parent" && field["normalized_name"] == "parent")
    );
}

#[test]
fn ids_command_resolves_object_targets() {
    let samples = sample_paths();
    let value = run_blendio_json(vec![
        OsString::from("ids"),
        samples.uncompressed.as_os_str().to_os_string(),
    ]);

    let cube = value
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["code"] == "OB" && entry["name"] == "MyCube")
        .unwrap();

    assert_eq!(cube["object_type"], "Mesh");
    assert_eq!(cube["data_target"]["code"], "ME");
    assert_eq!(cube["data_target"]["name"], "Cube");
}

#[test]
fn info_command_defaults_to_human_readable_text() {
    let samples = sample_paths();
    let text = run_blendio_text(vec![
        OsString::from("info"),
        samples.uncompressed.as_os_str().to_os_string(),
    ]);

    assert!(text.contains("Blend File Summary"));
    assert!(text.contains("Objects ("));
    assert!(text.contains("MyCube"));
    assert!(text.contains("MainCollection"));
    assert!(!text.trim_start().starts_with('{'));
}
