use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::process_utils::std_command;

const TOOL_VERSION_TIMEOUT: Duration = Duration::from_secs(8);
const BLENDER_SCAN_HOME_MAX_DEPTH: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolPathsInput {
    pub ffprobe: Option<String>,
    pub ffmpeg: Option<String>,
    pub blender: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub label: String,
    pub configured_path: Option<String>,
    pub detected_path: Option<String>,
    pub resolved_path: Option<String>,
    pub source: String,
    pub status: String,
    pub version: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlenderInstallationInfo {
    pub path: String,
    pub version: Option<String>,
    pub version_line: Option<String>,
    pub status: String,
    pub source: String,
    pub last_checked_at: i64,
    pub message: Option<String>,
}

#[tauri::command]
pub async fn inspect_tool_paths(
    tool_paths: Option<ToolPathsInput>,
) -> Result<Vec<ToolStatus>, String> {
    Ok(vec![
        build_ffprobe_status(
            tool_paths
                .as_ref()
                .and_then(|paths| paths.ffprobe.as_deref()),
        ),
        build_ffmpeg_status(
            tool_paths
                .as_ref()
                .and_then(|paths| paths.ffmpeg.as_deref()),
        ),
    ])
}

#[tauri::command]
pub async fn inspect_blender_executable(path: String) -> Result<BlenderInstallationInfo, String> {
    let path = path.trim().to_string();
    tokio::task::spawn_blocking(move || inspect_blender_path(&path, "manual"))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn scan_blender_installations() -> Result<Vec<BlenderInstallationInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let mut seen = HashSet::new();
        let mut results = Vec::new();

        for path in blender_scan_candidates() {
            let key = normalize_path_key(&path);
            if !seen.insert(key) {
                continue;
            }

            results.push(inspect_blender_path(&path.to_string_lossy(), "scan"));
        }

        results.sort_by(compare_blender_installations);
        results
    })
    .await
    .map_err(|error| error.to_string())
}

pub fn resolve_ffprobe_path(configured_path: Option<&str>) -> Option<String> {
    configured_path
        .and_then(validate_tool_path)
        .or_else(detect_ffprobe_on_path)
}

pub fn resolve_ffmpeg_path(configured_path: Option<&str>) -> Option<String> {
    configured_path
        .and_then(validate_tool_path)
        .or_else(detect_ffmpeg_on_path)
}

static DETECTED_FFPROBE_PATH: OnceLock<Option<String>> = OnceLock::new();
static DETECTED_FFMPEG_PATH: OnceLock<Option<String>> = OnceLock::new();

fn build_ffprobe_status(configured_path: Option<&str>) -> ToolStatus {
    let configured_path = configured_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    let configured_valid = configured_path.as_deref().and_then(validate_tool_path);
    let detected_path = detect_ffprobe_on_path();
    let resolved_path = configured_valid.clone().or_else(|| detected_path.clone());

    let source = if configured_valid.is_some() {
        "configured"
    } else if detected_path.is_some() {
        "system"
    } else {
        "missing"
    };

    let message = if configured_path.is_some() && configured_valid.is_none() {
        Some("已配置的 ffprobe 路径无效，当前已回退到自动探测结果或基础信息模式".to_string())
    } else if resolved_path.is_none() {
        Some("未检测到 ffprobe，视频高级信息将只显示基础属性。你可以在设置中手动指定路径，或点击打开下载页。".to_string())
    } else {
        Some(match source {
            "configured" => "正在使用你指定的 ffprobe 路径".to_string(),
            _ => "正在使用系统环境中的 ffprobe".to_string(),
        })
    };

    ToolStatus {
        id: "ffprobe".to_string(),
        label: "FFprobe".to_string(),
        configured_path,
        detected_path,
        resolved_path: resolved_path.clone(),
        source: source.to_string(),
        status: if resolved_path.is_some() {
            "ready".to_string()
        } else {
            "missing".to_string()
        },
        version: resolved_path
            .as_deref()
            .and_then(|path| read_tool_version(path, "-version")),
        message,
    }
}

fn build_ffmpeg_status(configured_path: Option<&str>) -> ToolStatus {
    let configured_path = configured_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    let configured_valid = configured_path.as_deref().and_then(validate_tool_path);
    let detected_path = detect_ffmpeg_on_path();
    let resolved_path = configured_valid.clone().or_else(|| detected_path.clone());

    let source = if configured_valid.is_some() {
        "configured"
    } else if detected_path.is_some() {
        "system"
    } else {
        "missing"
    };

    let message = if configured_path.is_some() && configured_valid.is_none() {
        Some("已配置的 ffmpeg 路径无效，当前已回退到自动探测结果或无法打包视频".to_string())
    } else if resolved_path.is_none() {
        Some("未检测到 ffmpeg，渲染帧无法打包为视频。请在此处手动指定 ffmpeg。".to_string())
    } else {
        Some(match source {
            "configured" => "正在使用你指定的 ffmpeg 路径".to_string(),
            _ => "正在使用系统环境中的 ffmpeg".to_string(),
        })
    };

    ToolStatus {
        id: "ffmpeg".to_string(),
        label: "FFmpeg".to_string(),
        configured_path,
        detected_path,
        resolved_path: resolved_path.clone(),
        source: source.to_string(),
        status: if resolved_path.is_some() {
            "ready".to_string()
        } else {
            "missing".to_string()
        },
        version: resolved_path
            .as_deref()
            .and_then(|path| read_tool_version(path, "-version")),
        message,
    }
}

fn validate_tool_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || !Path::new(trimmed).exists() {
        return None;
    }
    Some(trimmed.to_string())
}

fn detect_ffprobe_on_path() -> Option<String> {
    DETECTED_FFPROBE_PATH
        .get_or_init(detect_ffprobe_on_path_uncached)
        .clone()
}

fn detect_ffmpeg_on_path() -> Option<String> {
    DETECTED_FFMPEG_PATH
        .get_or_init(detect_ffmpeg_on_path_uncached)
        .clone()
}

fn detect_ffprobe_on_path_uncached() -> Option<String> {
    detect_tool_on_path("ffprobe")
}

fn detect_ffmpeg_on_path_uncached() -> Option<String> {
    detect_tool_on_path("ffmpeg")
}

fn detect_tool_on_path(tool: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    let lookup_cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let lookup_cmd = "which";

    let output = std_command(lookup_cmd).arg(tool).output().ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && Path::new(line).exists())
        .map(str::to_string)
}

fn read_tool_version(path: &str, version_arg: &str) -> Option<String> {
    let output = command_output_with_timeout(path, &[version_arg], TOOL_VERSION_TIMEOUT).ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

fn inspect_blender_path(path: &str, source: &str) -> BlenderInstallationInfo {
    let trimmed = path.trim();
    let last_checked_at = current_timestamp_millis();
    if trimmed.is_empty() {
        return BlenderInstallationInfo {
            path: trimmed.to_string(),
            version: None,
            version_line: None,
            status: "missing".to_string(),
            source: source.to_string(),
            last_checked_at,
            message: Some("路径为空".to_string()),
        };
    }

    let normalized_path = normalize_existing_path(trimmed).unwrap_or_else(|| trimmed.to_string());
    if !Path::new(&normalized_path).is_file() {
        return BlenderInstallationInfo {
            path: normalized_path,
            version: None,
            version_line: None,
            status: "missing".to_string(),
            source: source.to_string(),
            last_checked_at,
            message: Some("未找到 blender 可执行文件".to_string()),
        };
    }

    match read_blender_version_line(&normalized_path) {
        Ok(version_line) => BlenderInstallationInfo {
            version: parse_blender_version(&version_line),
            path: normalized_path,
            version_line: Some(version_line),
            status: "ready".to_string(),
            source: source.to_string(),
            last_checked_at,
            message: Some("Blender 可用".to_string()),
        },
        Err(error) => BlenderInstallationInfo {
            path: normalized_path,
            version: None,
            version_line: None,
            status: "unavailable".to_string(),
            source: source.to_string(),
            last_checked_at,
            message: Some(error),
        },
    }
}

fn read_blender_version_line(path: &str) -> Result<String, String> {
    let output = command_output_with_timeout(path, &["--version"], TOOL_VERSION_TIMEOUT)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Blender 版本检测失败".to_string()
        } else {
            stderr
        });
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Blender 未输出版本信息".to_string())
}

fn command_output_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    use std::thread;

    let mut child = std_command(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child.wait_with_output().map_err(|error| error.to_string());
            }
            Ok(None) => {
                if started_at.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("检测 Blender 版本超时".to_string());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(error.to_string());
            }
        }
    }
}

fn parse_blender_version(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let marker = "blender";
    let marker_index = lower.find(marker)?;
    let rest = line[marker_index + marker.len()..].trim();
    let version = rest
        .split_whitespace()
        .find(|part| {
            let mut has_digit = false;
            part.chars().all(|ch| {
                let ok = ch.is_ascii_digit() || ch == '.';
                if ch.is_ascii_digit() {
                    has_digit = true;
                }
                ok
            }) && has_digit
        })?
        .trim_matches('.');

    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

fn compare_blender_installations(
    left: &BlenderInstallationInfo,
    right: &BlenderInstallationInfo,
) -> std::cmp::Ordering {
    compare_versions(right.version.as_deref(), left.version.as_deref())
        .then_with(|| left.path.to_lowercase().cmp(&right.path.to_lowercase()))
}

fn compare_versions(left: Option<&str>, right: Option<&str>) -> std::cmp::Ordering {
    let left_parts = parse_version_parts(left);
    let right_parts = parse_version_parts(right);
    left_parts.cmp(&right_parts)
}

fn parse_version_parts(version: Option<&str>) -> Vec<u32> {
    version
        .unwrap_or_default()
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect()
}

fn blender_scan_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut roots = blender_common_scan_roots();

    for root in roots.drain(..) {
        collect_blender_candidates(&root, 5, &mut candidates);
    }

    if let Some(home) = user_home_dir() {
        collect_blender_candidates(&home, BLENDER_SCAN_HOME_MAX_DEPTH, &mut candidates);
    }

    candidates
}

fn collect_blender_candidates(root: &Path, max_depth: usize, candidates: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }

    for entry in walkdir::WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !should_skip_scan_dir(entry.path()))
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }

        if is_blender_executable_path(entry.path()) {
            candidates.push(entry.path().to_path_buf());
        }
    }
}

fn is_blender_executable_path(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    #[cfg(target_os = "windows")]
    return file_name.eq_ignore_ascii_case("blender.exe");

    #[cfg(not(target_os = "windows"))]
    return file_name == "blender" || file_name == "Blender";
}

fn should_skip_scan_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    matches!(
        name.to_ascii_lowercase().as_str(),
        "appdata"
            | "node_modules"
            | ".git"
            | "target"
            | "windows"
            | "$recycle.bin"
            | "system volume information"
    )
}

fn blender_common_scan_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(value) = std::env::var(key) {
                let path = PathBuf::from(value);
                if key == "LOCALAPPDATA" {
                    roots.push(path.join("Programs"));
                } else {
                    roots.push(path.join("Blender Foundation"));
                    roots.push(path);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Applications"));
    }

    #[cfg(target_os = "linux")]
    {
        roots.push(PathBuf::from("/usr/bin"));
        roots.push(PathBuf::from("/usr/local/bin"));
        roots.push(PathBuf::from("/opt"));
    }

    roots
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn normalize_existing_path(path: &str) -> Option<String> {
    std::fs::canonicalize(path)
        .ok()
        .map(|path| path.to_string_lossy().to_string())
}

fn normalize_path_key(path: &Path) -> String {
    let normalized = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let value = normalized.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

fn current_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{compare_versions, parse_blender_version};

    #[test]
    fn parses_blender_version_line() {
        assert_eq!(
            parse_blender_version("Blender 4.5.1"),
            Some("4.5.1".to_string())
        );
        assert_eq!(
            parse_blender_version("Blender 5.0.0 Alpha"),
            Some("5.0.0".to_string())
        );
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(compare_versions(Some("4.10.0"), Some("4.9.0")).is_gt());
        assert!(compare_versions(Some("5.0"), Some("4.5.2")).is_gt());
    }
}
