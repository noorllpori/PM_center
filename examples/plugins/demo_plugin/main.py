from pathlib import Path

from pmc_plugin import get_setting, get_settings, progress, refresh, result, run, toast

SUPPORTED_EXTENSIONS = {
    "png",
    "jpg",
    "jpeg",
    "webp",
    "bmp",
    "tif",
    "tiff",
}


def normalize_extension(value):
    return (value or "").lower().lstrip(".")


def is_supported_image(item):
    if item.get("isDir"):
        return False

    extension = normalize_extension(item.get("extension"))
    if extension:
        return extension in SUPPORTED_EXTENSIONS

    return normalize_extension(Path(item.get("path", "")).suffix) in SUPPORTED_EXTENSIONS


def unique_output_path(source_path, suffix):
    candidate = source_path.with_name(f"{source_path.stem}{suffix}{source_path.suffix}")
    counter = 2
    while candidate.exists():
        candidate = source_path.with_name(
            f"{source_path.stem}{suffix}_{counter}{source_path.suffix}"
        )
        counter += 1
    return candidate


def load_pillow():
    try:
        from PIL import Image, ImageEnhance, ImageOps

        return Image, ImageEnhance, ImageOps
    except ModuleNotFoundError as exc:
        message = (
            "Demo Plugin 缺少 Pillow 依赖，请先到设置 > 插件里安装依赖。"
        )
        toast(message, title="Demo Plugin", tone="error")
        result(
            {
                "success": False,
                "message": message,
                "missingDependency": "Pillow",
                "details": str(exc),
            }
        )
        return None, None, None


def build_effective_settings(request):
    settings = get_settings(request)
    brightness_factor = float(get_setting(request, "brightnessFactor", 1.3) or 1.3)
    quality_mode = str(get_setting(request, "qualityMode", "balanced") or "balanced")
    enable_mirror = bool(get_setting(request, "enableMirror", True))
    output_suffix = str(get_setting(request, "outputSuffix", "_demo_adjusted") or "_demo_adjusted")
    sample_image = settings.get("sampleImage") or None

    if quality_mode == "bright":
        brightness_factor += 0.2
    if quality_mode == "light-only":
        enable_mirror = False

    return {
        "brightnessFactor": round(brightness_factor, 3),
        "enableMirror": enable_mirror,
        "outputSuffix": output_suffix,
        "qualityMode": quality_mode,
        "sampleImage": sample_image,
        "settingsStoragePath": request.get("settingsStoragePath"),
        "settingsFilesDir": request.get("settingsFilesDir"),
    }


def handle_show_settings(request):
    settings = build_effective_settings(request)
    toast(
        f"当前亮度倍数 {settings['brightnessFactor']}x，输出后缀 {settings['outputSuffix']}",
        title="Demo Plugin",
        tone="success",
    )
    result(
        {
            "mode": "show-settings",
            "settings": settings,
            "sampleImageConfigured": bool(settings["sampleImage"]),
        }
    )


def process_single_image(source_path, settings):
    Image, ImageEnhance, ImageOps = load_pillow()
    if Image is None:
        return None

    output_path = unique_output_path(source_path, settings["outputSuffix"])
    with Image.open(source_path) as source_image:
        image = ImageOps.exif_transpose(source_image)
        if settings["enableMirror"]:
            image = ImageOps.mirror(image)
        image = ImageEnhance.Brightness(image).enhance(settings["brightnessFactor"])
        if output_path.suffix.lower() in {".jpg", ".jpeg"} and image.mode in {"RGBA", "LA", "P"}:
            image = image.convert("RGB")
        image.save(output_path)
    return output_path


def handle_process_images(request):
    selected_items = request.get("selectedItems", [])
    image_items = [item for item in selected_items if is_supported_image(item)]
    settings = build_effective_settings(request)

    if not image_items:
        message = "当前选中内容里没有可处理的图片文件。"
        toast(message, title="Demo Plugin", tone="warning")
        result(
            {
                "mode": "process-images",
                "message": message,
                "settings": settings,
                "processedCount": 0,
            }
        )
        return

    processed = []
    failed = []
    progress(5)

    for index, item in enumerate(image_items, start=1):
        source_path = Path(item["path"])
        try:
            output_path = process_single_image(source_path, settings)
            if output_path is None:
                return
            processed.append(
                {
                    "source": str(source_path),
                    "output": str(output_path),
                }
            )
        except Exception as exc:
            failed.append(
                {
                    "source": str(source_path),
                    "error": str(exc),
                }
            )
        progress(index * 100 / len(image_items))

    if processed:
        refresh(scope="project", path=request.get("currentPath"))

    tone = "warning" if failed else "success"
    toast(
        f"已处理 {len(processed)} 张图片，失败 {len(failed)} 张。",
        title="Demo Plugin",
        tone=tone,
    )
    result(
        {
            "mode": "process-images",
            "settings": settings,
            "processedCount": len(processed),
            "failedCount": len(failed),
            "processed": processed,
            "failed": failed,
        }
    )


def handle(request):
    command_id = request.get("commandId")
    if command_id == "show-demo-settings":
        handle_show_settings(request)
        return

    handle_process_images(request)


if __name__ == "__main__":
    run(handle)
