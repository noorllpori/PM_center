use chrono::{DateTime, Local};
use exif::{In, Reader as ExifReader, Tag as ExifTag};
use image::codecs::gif::GifDecoder;
use image::io::Reader as ImageReader;
use image::AnimationDecoder;
use lofty::file::AudioFile;
use lofty::prelude::{Accessor, TaggedFileExt};
use lofty::probe::Probe;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::python::{get_blender_file_info, resolve_blender_path};
use crate::tools::{resolve_ffprobe_path, ToolPathsInput};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDetailsResponse {
    pub basic: FileDetailsBasic,
    pub parser: FileDetailsParser,
    pub sections: Vec<FileDetailsSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDetailsBasic {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub size_formatted: String,
    pub is_dir: bool,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub accessed: Option<String>,
    pub readonly: bool,
    pub hidden: bool,
    pub extension: Option<String>,
    pub mime: Option<String>,
    pub detected_kind: String,
    pub display_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDetailsParser {
    pub id: String,
    pub source: String,
    pub status: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDetailsSection {
    pub id: String,
    pub title: String,
    pub items: Vec<FileDetailsItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDetailsItem {
    pub label: String,
    pub value: String,
}

struct ParserOutcome {
    parser: FileDetailsParser,
    sections: Vec<FileDetailsSection>,
}

struct ImageMetadata {
    format_name: Option<String>,
    width: u32,
    height: u32,
    color_type: Option<String>,
    frame_count: Option<usize>,
}

#[tauri::command]
pub async fn get_file_details(
    path: String,
    _view: Option<String>,
    tool_paths: Option<ToolPathsInput>,
) -> Result<FileDetailsResponse, String> {
    let path_buf = PathBuf::from(&path);
    let basic = build_basic_details(&path_buf).await?;
    let ffprobe_path = resolve_ffprobe_path(tool_paths.as_ref().and_then(|paths| paths.ffprobe.as_deref()));
    let blender_path = tool_paths
        .as_ref()
        .and_then(|paths| paths.blender.as_deref())
        .and_then(|path| resolve_blender_path(Some(path)));

    let parser_outcome = match basic.display_type.as_str() {
        "image" => parse_image_details(&path_buf).await,
        "audio" => parse_audio_details(&path_buf).await,
        "video" => parse_video_details(&path_buf, ffprobe_path).await,
        "blender" => parse_blender_details(&path_buf, blender_path).await,
        "folder" => basic_outcome("folder"),
        _ => basic_outcome("basic"),
    };

    let mut sections = vec![build_basic_section(&basic)];
    sections.extend(parser_outcome.sections);
    sections.push(build_parser_section(&parser_outcome.parser));

    Ok(FileDetailsResponse {
        basic,
        parser: parser_outcome.parser,
        sections,
    })
}

async fn build_basic_details(path: &PathBuf) -> Result<FileDetailsBasic, String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| error.to_string())?;

    let name = path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let extension = path.extension()
        .map(|ext| ext.to_string_lossy().to_string().to_lowercase());

    let inferred_type = infer::get_from_path(path)
        .ok()
        .flatten();
    let mime = inferred_type
        .as_ref()
        .map(|file_type| file_type.mime_type().to_string())
        .or_else(|| mime_guess::from_path(path).first_raw().map(str::to_string));

    let detected_kind = mime
        .as_deref()
        .and_then(|value| value.split('/').next())
        .unwrap_or("unknown")
        .to_string();

    let display_type = determine_display_type(metadata.is_dir(), extension.as_deref(), mime.as_deref());

    let hidden = {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            (metadata.file_attributes() & 0x2) != 0
        }
        #[cfg(not(windows))]
        {
            name.starts_with('.')
        }
    };

    Ok(FileDetailsBasic {
        name,
        path: path.to_string_lossy().to_string(),
        size: metadata.len(),
        size_formatted: format_size(metadata.len()),
        is_dir: metadata.is_dir(),
        created: metadata.created().ok().map(format_timestamp),
        modified: metadata.modified().ok().map(format_timestamp),
        accessed: metadata.accessed().ok().map(format_timestamp),
        readonly: metadata.permissions().readonly(),
        hidden,
        extension,
        mime,
        detected_kind,
        display_type,
    })
}

fn determine_display_type(is_dir: bool, extension: Option<&str>, mime: Option<&str>) -> String {
    if is_dir {
        return "folder".to_string();
    }

    let extension = extension.unwrap_or_default();

    if extension == "blend" {
        return "blender".to_string();
    }

    if matches!(extension, "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff") {
        return "image".to_string();
    }

    if matches!(extension, "mp3" | "flac" | "wav" | "ogg" | "opus" | "m4a" | "aac") {
        return "audio".to_string();
    }

    if matches!(extension, "mp4" | "mov" | "m4v" | "mkv" | "webm" | "avi") {
        return "video".to_string();
    }

    match mime {
        Some(value) if value.starts_with("image/") => "image".to_string(),
        Some(value) if value.starts_with("audio/") => "audio".to_string(),
        Some(value) if value.starts_with("video/") => "video".to_string(),
        _ => "file".to_string(),
    }
}

fn basic_outcome(parser_id: &str) -> ParserOutcome {
    ParserOutcome {
        parser: FileDetailsParser {
            id: parser_id.to_string(),
            source: "none".to_string(),
            status: "basic".to_string(),
            warning: None,
        },
        sections: Vec::new(),
    }
}

fn warning_outcome(parser_id: &str, source: &str, warning: String) -> ParserOutcome {
    ParserOutcome {
        parser: FileDetailsParser {
            id: parser_id.to_string(),
            source: source.to_string(),
            status: "warning".to_string(),
            warning: Some(warning),
        },
        sections: Vec::new(),
    }
}

fn build_basic_section(basic: &FileDetailsBasic) -> FileDetailsSection {
    let mut items = vec![
        item("类型", display_type_label(basic)),
        item("大小", basic.size_formatted.clone()),
    ];

    if let Some(mime) = &basic.mime {
        items.push(item("MIME", mime.clone()));
    }

    if let Some(modified) = &basic.modified {
        items.push(item("修改时间", format_display_timestamp(modified)));
    }

    if let Some(created) = &basic.created {
        items.push(item("创建时间", format_display_timestamp(created)));
    }

    if let Some(accessed) = &basic.accessed {
        items.push(item("访问时间", format_display_timestamp(accessed)));
    }

    items.push(item("只读", bool_label(basic.readonly)));
    items.push(item("隐藏", bool_label(basic.hidden)));

    section("basic", "基础信息", items)
}

fn build_parser_section(parser: &FileDetailsParser) -> FileDetailsSection {
    let mut items = vec![
        item("解析器", parser.id.clone()),
        item("来源", parser_source_label(&parser.source)),
        item("状态", parser_status_label(&parser.status)),
    ];

    if let Some(warning) = &parser.warning {
        items.push(item("说明", warning.clone()));
    }

    section("parser-status", "解析状态", items)
}

async fn parse_image_details(path: &Path) -> ParserOutcome {
    let image_info = match read_image_metadata(path) {
        Ok(info) => info,
        Err(error) => return warning_outcome("image", "native", format!("图片信息解析失败：{}", error)),
    };

    let mut sections = vec![section(
        "media",
        "媒体信息",
        build_image_items(&image_info),
    )];

    let exif_items = read_exif_items(path);
    if !exif_items.is_empty() {
        sections.push(section("metadata", "元数据/标签", exif_items));
    }

    ParserOutcome {
        parser: FileDetailsParser {
            id: "image".to_string(),
            source: "native".to_string(),
            status: "ok".to_string(),
            warning: None,
        },
        sections,
    }
}

fn read_image_metadata(path: &Path) -> Result<ImageMetadata, String> {
    let reader = ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let format_name = reader.format().map(|format| format!("{:?}", format));
    let (width, height) = reader.into_dimensions()
        .map_err(|error| error.to_string())?;

    let decoded = ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;

    let frame_count = if path.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .as_deref() == Some("gif")
    {
        count_gif_frames(path).ok()
    } else {
        None
    };

    Ok(ImageMetadata {
        format_name,
        width,
        height,
        color_type: Some(format!("{:?}", decoded.color())),
        frame_count,
    })
}

fn count_gif_frames(path: &Path) -> Result<usize, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let decoder = GifDecoder::new(BufReader::new(file))
        .map_err(|error| error.to_string())?;
    let frames = decoder.into_frames()
        .collect_frames()
        .map_err(|error| error.to_string())?;
    Ok(frames.len())
}

fn build_image_items(info: &ImageMetadata) -> Vec<FileDetailsItem> {
    let mut items = vec![item("尺寸", format!("{} x {}", info.width, info.height))];

    if let Some(format_name) = &info.format_name {
        items.push(item("格式", format_name.clone()));
    }

    if let Some(color_type) = &info.color_type {
        items.push(item("像素格式", color_type.clone()));
    }

    if let Some(frame_count) = info.frame_count {
        items.push(item("帧数", frame_count.to_string()));
    }

    items
}

fn read_exif_items(path: &Path) -> Vec<FileDetailsItem> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let mut reader = BufReader::new(file);
    let exif = match ExifReader::new().read_from_container(&mut reader) {
        Ok(exif) => exif,
        Err(_) => return Vec::new(),
    };

    let mut items = Vec::new();

    push_exif_field(&mut items, &exif, ExifTag::DateTimeOriginal, "拍摄时间");
    push_exif_field(&mut items, &exif, ExifTag::Model, "相机");
    push_exif_field(&mut items, &exif, ExifTag::LensModel, "镜头");
    push_exif_field(&mut items, &exif, ExifTag::Orientation, "方向");
    push_exif_field(&mut items, &exif, ExifTag::GPSLatitude, "GPS 纬度");
    push_exif_field(&mut items, &exif, ExifTag::GPSLongitude, "GPS 经度");

    items
}

fn push_exif_field(items: &mut Vec<FileDetailsItem>, exif: &exif::Exif, tag: ExifTag, label: &str) {
    if let Some(field) = exif.get_field(tag, In::PRIMARY) {
        let value = field.display_value().with_unit(exif).to_string();
        if !value.is_empty() {
            items.push(item(label, value));
        }
    }
}

async fn parse_audio_details(path: &Path) -> ParserOutcome {
    let tagged_file = match Probe::open(path).and_then(|probe| probe.read()) {
        Ok(file) => file,
        Err(error) => return warning_outcome("audio", "native", format!("音频信息解析失败：{}", error)),
    };

    let properties = tagged_file.properties();
    let mut media_items = Vec::new();

    media_items.push(item("容器", format!("{:?}", tagged_file.file_type())));
    media_items.push(item("时长", format_duration(properties.duration())));

    if let Some(sample_rate) = properties.sample_rate() {
        media_items.push(item("采样率", format!("{} Hz", sample_rate)));
    }

    if let Some(channels) = properties.channels() {
        media_items.push(item("声道", channels.to_string()));
    }

    if let Some(bit_depth) = properties.bit_depth() {
        media_items.push(item("位深", format!("{} bit", bit_depth)));
    }

    if let Some(audio_bitrate) = properties.audio_bitrate() {
        media_items.push(item("音频码率", format!("{} kbps", audio_bitrate)));
    }

    if let Some(overall_bitrate) = properties.overall_bitrate() {
        media_items.push(item("总码率", format!("{} kbps", overall_bitrate)));
    }

    let mut sections = vec![section("media", "媒体信息", media_items)];

    let mut metadata_items = Vec::new();
    if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
        push_optional(&mut metadata_items, "标题", tag.title().map(|value| value.into_owned()));
        push_optional(&mut metadata_items, "艺术家", tag.artist().map(|value| value.into_owned()));
        push_optional(&mut metadata_items, "专辑", tag.album().map(|value| value.into_owned()));
        push_optional(&mut metadata_items, "流派", tag.genre().map(|value| value.into_owned()));
    }

    if !metadata_items.is_empty() {
        sections.push(section("metadata", "元数据/标签", metadata_items));
    }

    ParserOutcome {
        parser: FileDetailsParser {
            id: "audio".to_string(),
            source: "native".to_string(),
            status: "ok".to_string(),
            warning: None,
        },
        sections,
    }
}

async fn parse_video_details(path: &Path, ffprobe_path: Option<String>) -> ParserOutcome {
    let Some(ffprobe_path) = ffprobe_path else {
        return warning_outcome(
            "video",
            "external",
            "未检测到 ffprobe，仅显示基础信息。可在设置 > 工具路径里手动指定，或打开下载页后自行安装。".to_string(),
        );
    };

    let output = match Command::new(&ffprobe_path)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path.as_os_str())
        .output()
        .await
    {
        Ok(output) => output,
        Err(error) => {
            return warning_outcome("video", "external", format!("ffprobe 执行失败：{}", error))
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let warning = if stderr.is_empty() {
            "ffprobe 执行失败，仅显示基础信息".to_string()
        } else {
            format!("ffprobe 解析失败：{}", stderr)
        };
        return warning_outcome("video", "external", warning);
    }

    let value: Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(error) => return warning_outcome("video", "external", format!("ffprobe 输出解析失败：{}", error)),
    };

    ParserOutcome {
        parser: FileDetailsParser {
            id: "video".to_string(),
            source: "external".to_string(),
            status: "ok".to_string(),
            warning: None,
        },
        sections: build_video_sections(&value),
    }
}

fn build_video_sections(value: &Value) -> Vec<FileDetailsSection> {
    let streams = value.get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let format = value.get("format");

    let video_streams: Vec<&Value> = streams.iter()
        .filter(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        .collect();
    let audio_streams: Vec<&Value> = streams.iter()
        .filter(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"))
        .collect();
    let subtitle_streams: Vec<&Value> = streams.iter()
        .filter(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("subtitle"))
        .collect();

    let mut media_items = Vec::new();

    if let Some(container) = format.and_then(|entry| entry.get("format_name")).and_then(Value::as_str) {
        media_items.push(item("容器", container.to_string()));
    }

    if let Some(duration) = format.and_then(|entry| entry.get("duration")).and_then(Value::as_str).and_then(parse_f64) {
        media_items.push(item("时长", format_duration_seconds(duration)));
    }

    if let Some(bit_rate) = format.and_then(|entry| entry.get("bit_rate")).and_then(Value::as_str).and_then(parse_u64) {
        media_items.push(item("总码率", format!("{} kbps", bit_rate / 1000)));
    }

    media_items.push(item("视频轨数", video_streams.len().to_string()));
    media_items.push(item("音频轨数", audio_streams.len().to_string()));
    media_items.push(item("字幕轨数", subtitle_streams.len().to_string()));

    if let Some(video_stream) = video_streams.first() {
        let width = video_stream.get("width").and_then(Value::as_u64);
        let height = video_stream.get("height").and_then(Value::as_u64);
        if let (Some(width), Some(height)) = (width, height) {
            media_items.push(item("分辨率", format!("{} x {}", width, height)));
        }

        if let Some(frame_rate) = parse_frame_rate(video_stream) {
            media_items.push(item("帧率", format!("{:.2} FPS", frame_rate)));
        }

        if let Some(codec_name) = video_stream.get("codec_name").and_then(Value::as_str) {
            media_items.push(item("视频编码", codec_name.to_string()));
        }

        if let Some(pixel_format) = video_stream.get("pix_fmt").and_then(Value::as_str) {
            media_items.push(item("像素格式", pixel_format.to_string()));
        }
    }

    if let Some(audio_stream) = audio_streams.first() {
        if let Some(codec_name) = audio_stream.get("codec_name").and_then(Value::as_str) {
            media_items.push(item("音频编码", codec_name.to_string()));
        }

        if let Some(sample_rate) = audio_stream.get("sample_rate").and_then(Value::as_str) {
            media_items.push(item("采样率", format!("{} Hz", sample_rate)));
        }

        if let Some(channels) = audio_stream.get("channels").and_then(Value::as_u64) {
            media_items.push(item("声道", channels.to_string()));
        }
    }

    vec![section("media", "媒体信息", media_items)]
}

fn parse_frame_rate(stream: &Value) -> Option<f64> {
    stream.get("avg_frame_rate")
        .and_then(Value::as_str)
        .and_then(parse_rational)
        .filter(|value| *value > 0.0)
        .or_else(|| {
            stream.get("r_frame_rate")
                .and_then(Value::as_str)
                .and_then(parse_rational)
                .filter(|value| *value > 0.0)
        })
}

fn parse_rational(value: &str) -> Option<f64> {
    let (left, right) = value.split_once('/')?;
    let numerator = left.parse::<f64>().ok()?;
    let denominator = right.parse::<f64>().ok()?;
    if denominator == 0.0 {
        return None;
    }
    Some(numerator / denominator)
}

async fn parse_blender_details(path: &Path, blender_path: Option<String>) -> ParserOutcome {
    match parse_blendio_details(path).await {
        Ok(outcome) => outcome,
        Err(native_error) => parse_blender_fallback_details(path, blender_path, native_error).await,
    }
}

async fn parse_blendio_details(path: &Path) -> Result<ParserOutcome, String> {
    let path_buf = path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = blendio::BlendFile::open(&path_buf)
            .map_err(|error| error.to_string())?;
        let summary = blendio::summarize(&file)
            .map_err(|error| error.to_string())?;

        Ok(ParserOutcome {
            parser: FileDetailsParser {
                id: "blender".to_string(),
                source: "native".to_string(),
                status: "ok".to_string(),
                warning: None,
            },
            sections: build_blendio_sections(&summary),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn parse_blender_fallback_details(
    path: &Path,
    blender_path: Option<String>,
    native_error: String,
) -> ParserOutcome {
    let Some(blender_path) = blender_path else {
        return warning_outcome(
            "blender",
            "native",
            format!(
                "内置 BlendIO 解析失败：{}。如需兼容回退，可在设置 > 工具路径里手动指定 Blender。",
                native_error
            ),
        );
    };

    let info = match get_blender_file_info(blender_path, path.to_string_lossy().to_string()).await {
        Ok(value) => value,
        Err(error) => {
            return warning_outcome(
                "blender",
                "python",
                format!(
                    "内置 BlendIO 解析失败：{}；Blender 回退也失败：{}",
                    native_error,
                    error
                ),
            );
        }
    };

    build_blender_python_outcome(
        &info,
        Some(format!("内置 BlendIO 解析失败，已回退到 Blender：{}", native_error)),
    )
}

fn build_blendio_sections(summary: &blendio::FileSummary) -> Vec<FileDetailsSection> {
    let mut media_items = vec![
        item("Blender 版本", format_blend_file_version(summary.header.file_version)),
        item("压缩方式", blend_compression_label(summary.header.compression)),
        item("字节序", blend_endian_label(summary.header.endian)),
        item("块头类型", blend_bhead_label(summary.header.bhead_type)),
        item("指针大小", format!("{} 位", summary.header.pointer_size * 8)),
        item("块数量", summary.block_count.to_string()),
        item("ID 数量", summary.id_count.to_string()),
        item("场景数", summary.scenes.len().to_string()),
        item("对象数", summary.objects.len().to_string()),
        item("集合数", summary.collections.len().to_string()),
        item("网格数", summary.meshes.len().to_string()),
        item("材质数", summary.materials.len().to_string()),
        item("相机数", summary.cameras.len().to_string()),
        item("灯光数", summary.lights.len().to_string()),
        item("动作数", summary.actions.len().to_string()),
        item("图片数", summary.images.len().to_string()),
    ];

    if let Some(scene) = summary.scenes.first() {
        media_items.push(item(
            if summary.scenes.len() > 1 { "首场景" } else { "场景" },
            scene.name.clone(),
        ));

        if scene.resolution_x > 0 && scene.resolution_y > 0 {
            media_items.push(item(
                "分辨率",
                format!("{} x {}", scene.resolution_x, scene.resolution_y),
            ));
        }

        if scene.frame_start != 0 || scene.frame_end != 0 {
            media_items.push(item(
                "帧范围",
                format!("{} - {}", scene.frame_start, scene.frame_end),
            ));
        }

        if scene.fps > 0.0 {
            media_items.push(item("FPS", format_float(scene.fps as f64)));
        }

        push_optional(&mut media_items, "渲染引擎", scene.render_engine.clone());
        push_optional(&mut media_items, "输出路径", scene.output_path.clone());
        push_optional(&mut media_items, "世界", scene.world.clone());
        push_optional(&mut media_items, "主集合", scene.master_collection.clone());
    }

    let mut metadata_items = Vec::new();
    push_optional(
        &mut metadata_items,
        "场景列表",
        preview_values(summary.scenes.iter().map(|scene| scene.name.clone()).collect(), 6),
    );
    push_optional(
        &mut metadata_items,
        "相机列表",
        preview_values(summary.cameras.iter().map(|camera| camera.name.clone()).collect(), 6),
    );
    push_optional(
        &mut metadata_items,
        "灯光列表",
        preview_values(summary.lights.iter().map(|light| light.name.clone()).collect(), 6),
    );
    push_optional(
        &mut metadata_items,
        "集合列表",
        preview_values(
            summary
                .collections
                .iter()
                .map(|collection| collection.name.clone())
                .collect(),
            6,
        ),
    );
    push_optional(
        &mut metadata_items,
        "材质列表",
        preview_values(
            summary
                .materials
                .iter()
                .map(|material| material.name.clone())
                .collect(),
            6,
        ),
    );
    push_optional(
        &mut metadata_items,
        "动画列表",
        preview_values(summary.actions.iter().map(|action| action.name.clone()).collect(), 6),
    );
    push_optional(
        &mut metadata_items,
        "外部库",
        preview_values(
            summary
                .libraries
                .iter()
                .map(|library| library.filepath.clone().unwrap_or_else(|| library.name.clone()))
                .collect(),
            4,
        ),
    );

    let mut sections = vec![section("media", "媒体信息", media_items)];
    if !metadata_items.is_empty() {
        sections.push(section("metadata", "元数据/标签", metadata_items));
    }

    sections
}

fn build_blender_python_outcome(info: &Value, warning: Option<String>) -> ParserOutcome {
    ParserOutcome {
        parser: FileDetailsParser {
            id: "blender".to_string(),
            source: "python".to_string(),
            status: "ok".to_string(),
            warning,
        },
        sections: build_blender_python_sections(info),
    }
}

fn build_blender_python_sections(info: &Value) -> Vec<FileDetailsSection> {
    let mut media_items = Vec::new();

    let scenes = info.get("scenes").and_then(Value::as_array).map(|items| items.len()).unwrap_or(0);
    let cameras = info.get("cameras").and_then(Value::as_array).map(|items| items.len()).unwrap_or(0);
    let object_count = info.get("objects")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let material_count = info.get("materials")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    media_items.push(item("场景数", scenes.to_string()));
    media_items.push(item("相机数", cameras.to_string()));
    media_items.push(item("对象数", object_count.to_string()));
    media_items.push(item("材质数", material_count.to_string()));

    if let (Some(width), Some(height)) = (
        info.get("resolution").and_then(Value::as_array).and_then(|items| items.first()).and_then(Value::as_u64),
        info.get("resolution").and_then(Value::as_array).and_then(|items| items.get(1)).and_then(Value::as_u64),
    ) {
        media_items.push(item("分辨率", format!("{} x {}", width, height)));
    }

    if let (Some(frame_start), Some(frame_end)) = (
        info.get("frame_start").and_then(Value::as_i64),
        info.get("frame_end").and_then(Value::as_i64),
    ) {
        media_items.push(item("帧范围", format!("{} - {}", frame_start, frame_end)));
    }

    if let Some(render_engine) = info.get("render_engine").and_then(Value::as_str) {
        media_items.push(item("渲染引擎", render_engine.to_string()));
    }

    if let Some(version) = info.get("version").and_then(Value::as_array) {
        let version_text = version.iter()
            .filter_map(Value::as_u64)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(".");
        if !version_text.is_empty() {
            media_items.push(item("Blender 版本", version_text));
        }
    }

    let mut metadata_items = Vec::new();
    push_optional(
        &mut metadata_items,
        "场景列表",
        preview_values(
            info.get("scenes")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("name").and_then(Value::as_str).map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            6,
        ),
    );
    push_optional(
        &mut metadata_items,
        "相机列表",
        preview_values(
            info.get("cameras")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("name").and_then(Value::as_str).map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            6,
        ),
    );

    let mut sections = vec![section("media", "媒体信息", media_items)];
    if !metadata_items.is_empty() {
        sections.push(section("metadata", "元数据/标签", metadata_items));
    }

    sections
}

fn preview_values(mut values: Vec<String>, limit: usize) -> Option<String> {
    values.retain(|value| !value.trim().is_empty());
    if values.is_empty() {
        return None;
    }

    let total = values.len();
    let shown = values.into_iter().take(limit).collect::<Vec<_>>();
    let preview = shown.join("、");

    if total > limit {
        Some(format!("{} 等 {} 项", preview, total))
    } else {
        Some(preview)
    }
}

fn format_blend_file_version(version: u16) -> String {
    if version >= 100 {
        format!("{}.{} ({})", version / 100, version % 100, version)
    } else {
        version.to_string()
    }
}

fn blend_compression_label(kind: blendio::CompressionKind) -> &'static str {
    match kind {
        blendio::CompressionKind::None => "无压缩",
        blendio::CompressionKind::Gzip => "Gzip",
        blendio::CompressionKind::Zstd => "Zstd",
    }
}

fn blend_endian_label(endian: blendio::Endian) -> &'static str {
    match endian {
        blendio::Endian::Little => "Little Endian",
        blendio::Endian::Big => "Big Endian",
    }
}

fn blend_bhead_label(kind: blendio::BHeadType) -> &'static str {
    match kind {
        blendio::BHeadType::BHead4 => "BHead4",
        blendio::BHeadType::SmallBHead8 => "SmallBHead8",
        blendio::BHeadType::LargeBHead8 => "LargeBHead8",
    }
}

fn item(label: impl Into<String>, value: impl Into<String>) -> FileDetailsItem {
    FileDetailsItem {
        label: label.into(),
        value: value.into(),
    }
}

fn section(id: impl Into<String>, title: impl Into<String>, items: Vec<FileDetailsItem>) -> FileDetailsSection {
    FileDetailsSection {
        id: id.into(),
        title: title.into(),
        items,
    }
}

fn push_optional(items: &mut Vec<FileDetailsItem>, label: &str, value: Option<String>) {
    if let Some(value) = value {
        if !value.trim().is_empty() {
            items.push(item(label, value));
        }
    }
}

fn format_float(value: f64) -> String {
    let mut text = format!("{:.2}", value);
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn format_timestamp(value: std::time::SystemTime) -> String {
    let datetime: DateTime<Local> = value.into();
    datetime.to_rfc3339()
}

fn format_display_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|datetime| datetime.with_timezone(&Local).format("%Y/%m/%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| value.to_string())
}

fn format_duration(duration: std::time::Duration) -> String {
    format_duration_seconds(duration.as_secs_f64())
}

fn format_duration_seconds(value: f64) -> String {
    if value <= 0.0 {
        return "0 秒".to_string();
    }

    let total_seconds = value.round() as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "-".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    while size >= 1024.0 && unit_index < units.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    format!("{:.1} {}", size, units[unit_index])
}

fn bool_label(value: bool) -> String {
    if value {
        "是".to_string()
    } else {
        "否".to_string()
    }
}

fn display_type_label(basic: &FileDetailsBasic) -> String {
    match basic.display_type.as_str() {
        "folder" => "文件夹".to_string(),
        "image" => "图片".to_string(),
        "audio" => "音频".to_string(),
        "video" => "视频".to_string(),
        "blender" => "Blender 文件".to_string(),
        _ => basic.extension
            .as_ref()
            .map(|ext| ext.to_uppercase())
            .unwrap_or_else(|| "文件".to_string()),
    }
}

fn parser_status_label(status: &str) -> String {
    match status {
        "ok" => "解析完成".to_string(),
        "warning" => "部分可用".to_string(),
        "error" => "解析失败".to_string(),
        _ => "基础信息".to_string(),
    }
}

fn parser_source_label(source: &str) -> String {
    match source {
        "native" => "内置".to_string(),
        "external" => "外部工具".to_string(),
        "python" => "Python/Blender".to_string(),
        _ => "无".to_string(),
    }
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn parse_u64(value: &str) -> Option<u64> {
    value.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        build_video_sections, determine_display_type, format_blend_file_version, parse_rational,
        preview_values,
    };
    use serde_json::json;

    #[test]
    fn detects_media_display_type() {
        assert_eq!(determine_display_type(false, Some("jpg"), None), "image");
        assert_eq!(determine_display_type(false, Some("mp3"), None), "audio");
        assert_eq!(determine_display_type(false, Some("mp4"), None), "video");
        assert_eq!(determine_display_type(false, Some("blend"), None), "blender");
        assert_eq!(determine_display_type(true, None, None), "folder");
    }

    #[test]
    fn parses_frame_rate_rational() {
        assert_eq!(parse_rational("30000/1001").map(|value| value.round() as i32), Some(30));
        assert_eq!(parse_rational("0/0"), None);
    }

    #[test]
    fn formats_blend_version_for_display() {
        assert_eq!(format_blend_file_version(405), "4.5 (405)");
        assert_eq!(format_blend_file_version(293), "2.93 (293)");
    }

    #[test]
    fn previews_long_value_lists() {
        assert_eq!(
            preview_values(
                vec![
                    "Scene".to_string(),
                    "Camera".to_string(),
                    "World".to_string(),
                    "Collection".to_string(),
                ],
                2,
            ),
            Some("Scene、Camera 等 4 项".to_string())
        );
    }

    #[test]
    fn builds_video_section_from_ffprobe_json() {
        let value = json!({
            "format": {
                "format_name": "mov,mp4,m4a,3gp,3g2,mj2",
                "duration": "12.345",
                "bit_rate": "4567000"
            },
            "streams": [
                {
                    "codec_type": "video",
                    "width": 1920,
                    "height": 1080,
                    "avg_frame_rate": "30000/1001",
                    "codec_name": "h264",
                    "pix_fmt": "yuv420p"
                },
                {
                    "codec_type": "audio",
                    "codec_name": "aac",
                    "sample_rate": "48000",
                    "channels": 2
                }
            ]
        });

        let sections = build_video_sections(&value);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].items.iter().any(|item| item.label == "分辨率" && item.value == "1920 x 1080"));
        assert!(sections[0].items.iter().any(|item| item.label == "视频编码" && item.value == "h264"));
    }
}
