use image::io::Reader as ImageReader;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb, Rgba, RgbaImage};
use std::collections::HashSet;
use std::fs;
use std::hash::Hasher;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::watcher::{self, ThumbnailCacheUpdatedEvent};

const THUMBNAIL_SIZE: u32 = 320;

lazy_static::lazy_static! {
    static ref IN_FLIGHT_THUMBNAILS: Arc<Mutex<HashSet<String>>> =
        Arc::new(Mutex::new(HashSet::new()));
}

#[derive(Debug, Clone)]
pub struct ThumbnailSource {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ts: Option<i64>,
    pub extension: Option<String>,
}

#[derive(Debug, Clone)]
struct ThumbnailJob {
    source_path: String,
    target_path: PathBuf,
    cache_prefix: String,
    extension: Option<String>,
    inflight_key: String,
}

pub fn ensure_project_thumbnail_dir(project_path: &str) -> Result<(), String> {
    fs::create_dir_all(project_thumbnail_root(project_path))
        .map_err(|error| format!("failed to create thumbnail cache dir: {}", error))
}

pub fn resolve_cached_thumbnail_path(
    project_path: &str,
    source: &ThumbnailSource,
) -> Option<String> {
    let relative_path = build_thumbnail_relative_path(project_path, source)?;
    let absolute_path = project_thumbnail_root(project_path).join(relative_path);
    if absolute_path.exists() {
        Some(absolute_path.to_string_lossy().to_string())
    } else {
        None
    }
}

pub fn queue_directory_thumbnail_generation(
    project_path: String,
    directory_path: String,
    sources: Vec<ThumbnailSource>,
) {
    let mut jobs = Vec::new();

    for source in sources {
        let Some(relative_path) = build_thumbnail_relative_path(&project_path, &source) else {
            continue;
        };

        let target_path = project_thumbnail_root(&project_path).join(&relative_path);
        if target_path.exists() {
            continue;
        }

        let inflight_key = normalize_job_key(&target_path);
        if !register_in_flight(&inflight_key) {
            continue;
        }

        let Some(file_name) = target_path.file_name().and_then(|value| value.to_str()) else {
            unregister_in_flight(&inflight_key);
            continue;
        };
        let Some(separator_index) = file_name.rfind('-') else {
            unregister_in_flight(&inflight_key);
            continue;
        };
        let cache_prefix = file_name[..separator_index].to_string();

        jobs.push(ThumbnailJob {
            source_path: source.path,
            target_path,
            cache_prefix,
            extension: source.extension,
            inflight_key,
        });
    }

    if jobs.is_empty() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let inflight_keys = jobs
            .iter()
            .map(|job| job.inflight_key.clone())
            .collect::<Vec<_>>();
        let project_path_for_emit = project_path.clone();
        let directory_path_for_emit = directory_path.clone();

        let updated_count =
            tokio::task::spawn_blocking(move || generate_thumbnail_jobs(&project_path, &jobs))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(0);

        for inflight_key in inflight_keys {
            unregister_in_flight(&inflight_key);
        }

        if updated_count > 0 {
            watcher::emit_thumbnail_cache_updated(ThumbnailCacheUpdatedEvent {
                project_path: project_path_for_emit,
                directory_path: directory_path_for_emit,
                updated_count,
            });
        }
    });
}

fn generate_thumbnail_jobs(project_path: &str, jobs: &[ThumbnailJob]) -> Result<usize, String> {
    ensure_project_thumbnail_dir(project_path)?;

    let mut updated_count = 0usize;
    for job in jobs {
        let source_path = PathBuf::from(&job.source_path);
        if !source_path.exists() || !source_path.is_file() {
            continue;
        }

        let png_bytes = match render_thumbnail_png(&source_path, job.extension.as_deref()) {
            Ok(bytes) => bytes,
            Err(error) => {
                if error != "unsupported thumbnail format" {
                    eprintln!(
                        "[ThumbnailCache] failed to render thumbnail for {}: {}",
                        source_path.display(),
                        error
                    );
                }
                continue;
            }
        };

        persist_thumbnail_png(&job.target_path, &job.cache_prefix, &png_bytes)?;
        updated_count += 1;
    }

    Ok(updated_count)
}

pub fn store_thumbnail_png(
    project_path: &str,
    source: &ThumbnailSource,
    png_bytes: &[u8],
) -> Result<Option<String>, String> {
    let Some(relative_path) = build_thumbnail_relative_path(project_path, source) else {
        return Ok(None);
    };

    let target_path = project_thumbnail_root(project_path).join(relative_path);
    let Some(file_name) = target_path.file_name().and_then(|value| value.to_str()) else {
        return Ok(None);
    };
    let Some(separator_index) = file_name.rfind('-') else {
        return Ok(None);
    };
    let cache_prefix = file_name[..separator_index].to_string();

    persist_thumbnail_png(&target_path, &cache_prefix, png_bytes)?;
    Ok(Some(target_path.to_string_lossy().to_string()))
}

fn persist_thumbnail_png(
    target_path: &Path,
    cache_prefix: &str,
    png_bytes: &[u8],
) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        cleanup_stale_thumbnails(parent, cache_prefix, target_path)?;
    }

    fs::write(target_path, png_bytes).map_err(|error| {
        format!(
            "failed to write thumbnail {}: {}",
            target_path.display(),
            error
        )
    })
}

fn cleanup_stale_thumbnails(
    directory: &Path,
    cache_prefix: &str,
    current_target_path: &Path,
) -> Result<(), String> {
    let Some(current_file_name) = current_target_path
        .file_name()
        .and_then(|value| value.to_str())
    else {
        return Ok(());
    };

    let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };

        if file_name == current_file_name {
            continue;
        }

        if !file_name.starts_with(cache_prefix) || !file_name.ends_with(".png") {
            continue;
        }

        let _ = fs::remove_file(path);
    }

    Ok(())
}

fn render_thumbnail_png(path: &Path, extension: Option<&str>) -> Result<Vec<u8>, String> {
    if can_decode_with_image_crate(extension) {
        if let Ok(bytes) = render_raster_thumbnail_png(path) {
            return Ok(bytes);
        }
    }

    #[cfg(windows)]
    if let Some(bytes) = render_shell_thumbnail_png(path) {
        return Ok(bytes);
    }

    Err("unsupported thumbnail format".to_string())
}

fn render_raster_thumbnail_png(path: &Path) -> Result<Vec<u8>, String> {
    let image = ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;

    let thumbnail = image.thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE);
    encode_thumbnail_png(thumbnail)
}

fn encode_thumbnail_png(image: DynamicImage) -> Result<Vec<u8>, String> {
    let rgba = match image {
        DynamicImage::ImageRgb32F(buffer) => tone_map_rgb32f(&buffer),
        DynamicImage::ImageRgba32F(buffer) => tone_map_rgba32f(&buffer),
        other => other.to_rgba8(),
    };

    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(bytes.into_inner())
}

fn tone_map_rgb32f(buffer: &ImageBuffer<Rgb<f32>, Vec<f32>>) -> RgbaImage {
    let (width, height) = buffer.dimensions();
    let mut output = RgbaImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels() {
        output.put_pixel(
            x,
            y,
            Rgba([
                linear_to_srgb_u8(pixel[0]),
                linear_to_srgb_u8(pixel[1]),
                linear_to_srgb_u8(pixel[2]),
                255,
            ]),
        );
    }
    output
}

fn tone_map_rgba32f(buffer: &ImageBuffer<Rgba<f32>, Vec<f32>>) -> RgbaImage {
    let (width, height) = buffer.dimensions();
    let mut output = RgbaImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels() {
        output.put_pixel(
            x,
            y,
            Rgba([
                linear_to_srgb_u8(pixel[0]),
                linear_to_srgb_u8(pixel[1]),
                linear_to_srgb_u8(pixel[2]),
                float_to_u8(pixel[3]),
            ]),
        );
    }
    output
}

fn linear_to_srgb_u8(value: f32) -> u8 {
    let mapped = aces_filmic(value.max(0.0));
    let srgb = if mapped <= 0.003_130_8 {
        mapped * 12.92
    } else {
        1.055 * mapped.powf(1.0 / 2.4) - 0.055
    };
    float_to_u8(srgb)
}

fn float_to_u8(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }

    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn aces_filmic(value: f32) -> f32 {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    ((value * (a * value + b)) / (value * (c * value + d) + e)).clamp(0.0, 1.0)
}

fn can_decode_with_image_crate(extension: Option<&str>) -> bool {
    matches!(
        extension.map(|value| value.to_ascii_lowercase()),
        Some(ext)
            if matches!(
                ext.as_str(),
                "bmp"
                    | "exr"
                    | "gif"
                    | "hdr"
                    | "jpeg"
                    | "jpg"
                    | "png"
                    | "tif"
                    | "tiff"
                    | "webp"
            )
    )
}

fn is_thumbnail_candidate(extension: Option<&str>) -> bool {
    matches!(
        extension.map(|value| value.to_ascii_lowercase()),
        Some(ext)
            if matches!(
                ext.as_str(),
                "bmp"
                    | "exr"
                    | "gif"
                    | "hdr"
                    | "heic"
                    | "heif"
                    | "jpeg"
                    | "jpg"
                    | "png"
                    | "psd"
                    | "tif"
                    | "tiff"
                    | "webp"
            )
    )
}

fn build_thumbnail_relative_path(project_path: &str, source: &ThumbnailSource) -> Option<PathBuf> {
    if source.is_dir || !is_thumbnail_candidate(source.extension.as_deref()) {
        return None;
    }

    let source_id = normalize_source_id(project_path, &source.path);
    let source_hash = stable_hash_hex(&source_id);
    let signature = stable_hash_hex(&format!(
        "{}:{}:{}",
        source.size,
        source.modified_ts.unwrap_or_default(),
        source.extension.as_deref().unwrap_or_default()
    ));
    let bucket = &source_hash[..2];

    Some(PathBuf::from(bucket).join(format!("{source_hash}-{signature}.png")))
}

fn normalize_source_id(project_path: &str, source_path: &str) -> String {
    let source = PathBuf::from(source_path);
    let project = PathBuf::from(project_path);
    let relative = source
        .strip_prefix(&project)
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|_| source_path.to_string());

    normalize_job_key(&relative)
}

fn stable_hash_hex(value: &str) -> String {
    let mut hasher = FnvHasher::default();
    hasher.write(value.as_bytes());
    format!("{:016x}", hasher.finish())
}

fn project_thumbnail_root(project_path: &str) -> PathBuf {
    PathBuf::from(project_path)
        .join(".pm_center")
        .join("thumbnails")
}

fn normalize_job_key(value: impl AsRef<Path>) -> String {
    let value = value.as_ref().to_string_lossy().to_string();
    #[cfg(windows)]
    {
        value.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        value
    }
}

fn register_in_flight(key: &str) -> bool {
    if let Ok(mut guard) = IN_FLIGHT_THUMBNAILS.lock() {
        return guard.insert(key.to_string());
    }

    false
}

fn unregister_in_flight(key: &str) {
    if let Ok(mut guard) = IN_FLIGHT_THUMBNAILS.lock() {
        guard.remove(key);
    }
}

#[derive(Default)]
struct FnvHasher(u64);

impl Hasher for FnvHasher {
    fn write(&mut self, bytes: &[u8]) {
        if self.0 == 0 {
            self.0 = 0xcbf2_9ce4_8422_2325;
        }

        for byte in bytes {
            self.0 ^= *byte as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        }
    }
}

#[cfg(windows)]
fn render_shell_thumbnail_png(path: &Path) -> Option<Vec<u8>> {
    use std::mem;

    use windows::core::{Interface, HSTRING};
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC,
        SelectObject, BITMAP, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{
        IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK,
        SIIGBF_THUMBNAILONLY,
    };

    unsafe {
        let com_initialized =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();

        let result = (|| {
            let shell_item: IShellItem = SHCreateItemFromParsingName(
                &HSTRING::from(path.to_string_lossy().to_string()),
                None,
            )
            .ok()?;
            let image_factory: IShellItemImageFactory = shell_item.cast().ok()?;
            let bitmap = image_factory
                .GetImage(
                    SIZE {
                        cx: THUMBNAIL_SIZE as i32,
                        cy: THUMBNAIL_SIZE as i32,
                    },
                    SIIGBF_BIGGERSIZEOK | SIIGBF_THUMBNAILONLY,
                )
                .ok()?;

            let hdc_screen = GetDC(None);
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            let _ = SelectObject(hdc_mem, bitmap);

            let mut bmp: BITMAP = mem::zeroed();
            if GetObjectW(
                bitmap,
                mem::size_of::<BITMAP>() as i32,
                Some(&mut bmp as *mut _ as *mut _),
            ) == 0
            {
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(None, hdc_screen);
                let _ = DeleteObject(bitmap);
                return None;
            }

            let width = bmp.bmWidth as u32;
            let height = bmp.bmHeight as u32;
            let mut info = BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };

            let mut buffer = vec![0u8; (width * height * 4) as usize];
            if GetDIBits(
                hdc_mem,
                bitmap,
                0,
                height,
                Some(buffer.as_mut_ptr() as *mut _),
                &mut info as *mut _ as *mut _,
                DIB_RGB_COLORS,
            ) == 0
            {
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(None, hdc_screen);
                let _ = DeleteObject(bitmap);
                return None;
            }

            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            let _ = DeleteObject(bitmap);

            let mut rgba = Vec::with_capacity(buffer.len());
            for chunk in buffer.chunks_exact(4) {
                rgba.push(chunk[2]);
                rgba.push(chunk[1]);
                rgba.push(chunk[0]);
                rgba.push(chunk[3]);
            }

            let image = RgbaImage::from_raw(width, height, rgba)?;
            encode_thumbnail_png(DynamicImage::ImageRgba8(image)).ok()
        })();

        if com_initialized {
            CoUninitialize();
        }

        result
    }
}
