use std::path::PathBuf;
use std::process::ExitCode;

use blendio::{ExportOptions, UnsupportedPolicy, export_glb};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "blend2glb", version, about = "Export Blender 4.5 .blend files to .glb")]
struct Cli {
    input: PathBuf,
    output: PathBuf,
    #[arg(long = "no-cameras")]
    no_cameras: bool,
    #[arg(long = "no-lights")]
    no_lights: bool,
    #[arg(long = "no-animation")]
    no_animation: bool,
    #[arg(long = "strict")]
    strict: bool,
    #[arg(long = "report-json")]
    report_json: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> blendio::Result<()> {
    let cli = Cli::parse();
    let mut options = ExportOptions::default();
    options.include_cameras = !cli.no_cameras;
    options.include_lights = !cli.no_lights;
    options.include_object_trs_animation = !cli.no_animation;
    if cli.strict {
        options.unsupported_policy = UnsupportedPolicy::Strict;
    }

    let report = export_glb(&cli.input, &cli.output, &options)?;
    if let Some(path) = cli.report_json {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }

    println!("GLB Export Report");
    println!("Input: {}", cli.input.display());
    println!("Output: {}", cli.output.display());
    println!("Meshes: {}", report.exported_mesh_count);
    println!("Materials: {}", report.exported_material_count);
    println!("Animations: {}", report.exported_animation_count);
    println!("Warnings: {}", report.warnings.len());
    println!("Skipped objects: {}", report.skipped_objects.len());
    println!(
        "Unsupported features: {}",
        report.unsupported_features.len()
    );

    if !report.unsupported_features.is_empty() {
        println!();
        println!("Unsupported feature list");
        for feature in &report.unsupported_features {
            println!("  {feature}");
        }
    }

    if !report.skipped_objects.is_empty() {
        println!();
        println!("Skipped objects");
        for name in &report.skipped_objects {
            println!("  {name}");
        }
    }

    if !report.warnings.is_empty() {
        println!();
        println!("Warnings");
        for warning in &report.warnings {
            let object_name = warning.object_name.as_deref().unwrap_or("-");
            println!("  {:?} object={} message={}", warning.kind, object_name, warning.message);
        }
    }

    Ok(())
}
