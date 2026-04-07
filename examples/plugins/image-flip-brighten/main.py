from pathlib import Path

try:
    from PIL import Image, ImageEnhance, ImageOps
    PIL_IMPORT_ERROR = None
except ModuleNotFoundError as exc:
    Image = None
    ImageEnhance = None
    ImageOps = None
    PIL_IMPORT_ERROR = exc

from pmc_plugin import error, progress, refresh, result, run, toast

SUPPORTED_EXTENSIONS = {
    "png",
    "jpg",
    "jpeg",
    "webp",
    "bmp",
    "tif",
    "tiff",
}
BRIGHTNESS_FACTOR = 1.12
OUTPUT_SUFFIX = "_flip_bright"


def normalize_extension(value):
    return (value or "").lower().lstrip(".")


def is_supported_image(item):
    if item.get("isDir"):
        return False

    extension = normalize_extension(item.get("extension"))
    if extension:
        return extension in SUPPORTED_EXTENSIONS

    return normalize_extension(Path(item.get("path", "")).suffix) in SUPPORTED_EXTENSIONS


def build_output_path(source_path):
    candidate = source_path.with_name(
        f"{source_path.stem}{OUTPUT_SUFFIX}{source_path.suffix}"
    )
    counter = 2
    while candidate.exists():
        candidate = source_path.with_name(
            f"{source_path.stem}{OUTPUT_SUFFIX}_{counter}{source_path.suffix}"
        )
        counter += 1
    return candidate


def save_processed_image(image, output_path):
    image_to_save = image
    suffix = output_path.suffix.lower()

    if suffix in {".jpg", ".jpeg"} and image.mode in {"RGBA", "LA", "P"}:
        image_to_save = image.convert("RGB")

    image_to_save.save(output_path)


def process_image(source_path):
    output_path = build_output_path(source_path)

    with Image.open(source_path) as source_image:
        image = ImageOps.exif_transpose(source_image)
        flipped = ImageOps.mirror(image)
        brightened = ImageEnhance.Brightness(flipped).enhance(BRIGHTNESS_FACTOR)
        save_processed_image(brightened, output_path)

    return output_path


def emit_missing_dependency_error():
    message = "插件缺少 Pillow 依赖，请在设置 > 插件里安装依赖，或先用 plugin-tool pack 打包后再部署。"
    print(f"Missing dependency: {PIL_IMPORT_ERROR}", flush=True)
    error(message)
    toast(message, title="翻转并提亮图片", tone="error")
    result(
        {
            "processedCount": 0,
            "failedCount": 1,
            "skippedCount": 0,
            "message": message,
            "missingDependency": "Pillow",
        }
    )


def handle(request):
    if PIL_IMPORT_ERROR is not None:
        emit_missing_dependency_error()
        return

    selected_items = request.get("selectedItems", [])
    image_items = [item for item in selected_items if is_supported_image(item)]

    if not image_items:
        message = "当前选中内容里没有可处理的图片文件。"
        error(message)
        toast(message, title="翻转并提亮图片", tone="warning")
        result(
            {
                "processedCount": 0,
                "skippedCount": len(selected_items),
                "message": message,
            }
        )
        return

    processed = []
    failed = []
    skipped = [
        item.get("path")
        for item in selected_items
        if not is_supported_image(item)
    ]

    total = len(image_items)
    progress(5)

    for index, item in enumerate(image_items, start=1):
        source_path = Path(item["path"])
        print(f"Processing image: {source_path}", flush=True)

        try:
            output_path = process_image(source_path)
            processed.append(
                {
                    "source": str(source_path),
                    "output": str(output_path),
                }
            )
            print(f"Saved processed image: {output_path}", flush=True)
        except Exception as exc:
            failed.append(
                {
                    "source": str(source_path),
                    "error": str(exc),
                }
            )
            print(f"Failed to process {source_path}: {exc}", flush=True)

        progress(index * 100 / total)

    if processed:
        refresh(scope="project", path=request.get("currentPath"))

    if failed and processed:
        toast(
            f"已处理 {len(processed)} 张图片，失败 {len(failed)} 张。",
            title="翻转并提亮图片",
            tone="warning",
        )
    elif processed:
        toast(
            f"已生成 {len(processed)} 张处理后的图片。",
            title="翻转并提亮图片",
            tone="success",
        )
    else:
        error("图片处理失败，没有生成任何输出文件。")

    result(
        {
            "processedCount": len(processed),
            "failedCount": len(failed),
            "skippedCount": len(skipped),
            "brightnessFactor": BRIGHTNESS_FACTOR,
            "flip": "horizontal",
            "processed": processed,
            "failed": failed,
            "skipped": skipped,
        }
    )


if __name__ == "__main__":
    run(handle)
