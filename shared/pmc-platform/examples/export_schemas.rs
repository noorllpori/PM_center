use pmc_platform::{render_schema_document, schema_documents};
use std::{env, fs, path::PathBuf};

fn main() {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/v1"));
    fs::create_dir_all(&output).expect("create schema output directory");
    for (file_name, schema) in schema_documents() {
        fs::write(
            output.join(file_name),
            render_schema_document(file_name, &schema),
        )
        .expect("write schema");
    }
}
