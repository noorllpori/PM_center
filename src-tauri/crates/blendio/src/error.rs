use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, BlendError>;

#[derive(Debug, Error)]
pub enum BlendError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid blend file magic")]
    InvalidMagic,
    #[error("blend file header is truncated")]
    TruncatedHeader,
    #[error("unsupported big-endian blend file")]
    UnsupportedEndian,
    #[error("invalid blend header: {0}")]
    InvalidHeader(String),
    #[error("block header at offset 0x{offset:X} is truncated")]
    TruncatedBlockHeader { offset: usize },
    #[error("block payload at offset 0x{offset:X} is truncated")]
    TruncatedBlock { offset: usize },
    #[error("blend file is missing ENDB block")]
    MissingEndBlock,
    #[error("blend file is missing DNA1 block")]
    MissingDnaBlock,
    #[error("invalid SDNA section: {0}")]
    InvalidSdna(&'static str),
    #[error("invalid SDNA section: {0}")]
    InvalidSdnaOwned(String),
    #[error("block {code} at offset 0x{offset:X} cannot be viewed as a struct")]
    NonStructBlock { code: String, offset: usize },
    #[error("old pointer 0x{ptr:X} could not be resolved")]
    MissingOldPointer { ptr: u64 },
    #[error("scene not found: {selector}")]
    SceneNotFound { selector: String },
    #[error("field {struct_name}.{field_name} is missing")]
    MissingField {
        struct_name: String,
        field_name: String,
    },
    #[error("field {struct_name}.{field_name} has unsupported type or size")]
    FieldTypeMismatch {
        struct_name: String,
        field_name: String,
    },
    #[error("string for field {field_name} is too long: {actual} bytes, capacity is {capacity}")]
    StringTooLong {
        field_name: String,
        actual: usize,
        capacity: usize,
    },
    #[error("patch range 0x{start:X}..0x{end:X} is invalid")]
    InvalidPatchRange { start: usize, end: usize },
    #[error("patch ranges overlap at 0x{offset:X}")]
    PatchConflict { offset: usize },
    #[error("patch at 0x{offset:X} no longer matches the source bytes")]
    PatchOldBytesMismatch { offset: usize },
    #[error("compression write error: {0}")]
    CompressionWrite(String),
    #[error("write verification failed: {0}")]
    Verification(String),
    #[error("export error: {0}")]
    Export(String),
}
