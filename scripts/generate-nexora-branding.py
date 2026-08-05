from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


CANVAS_SIZE = 1024
MAX_CONTENT_SIZE = 880
BRAND_ORANGE = (231, 101, 41)


def build_logo(source_path: Path) -> Image.Image:
    source = Image.open(source_path).convert("RGBA")
    bounding_box = source.getchannel("A").getbbox()
    if bounding_box is None:
        raise ValueError("Logo source has no visible pixels")

    cropped = source.crop(bounding_box)
    scale = min(MAX_CONTENT_SIZE / cropped.width, MAX_CONTENT_SIZE / cropped.height)
    resized = cropped.resize(
        (round(cropped.width * scale), round(cropped.height * scale)),
        Image.Resampling.LANCZOS,
    )
    canvas = Image.new("RGBA", (CANVAS_SIZE, CANVAS_SIZE), (0, 0, 0, 0))
    position = (
        (CANVAS_SIZE - resized.width) // 2,
        (CANVAS_SIZE - resized.height) // 2,
    )
    canvas.alpha_composite(resized, position)
    return canvas


def build_installer_dialog(logo: Image.Image) -> Image.Image:
    dialog = Image.new("RGB", (493, 312), (250, 250, 250))
    draw = ImageDraw.Draw(dialog)
    draw.rectangle((0, 0, 164, 311), fill=(255, 255, 255))
    draw.rectangle((0, 0, 8, 311), fill=BRAND_ORANGE)

    mark = logo.copy()
    mark.thumbnail((126, 126), Image.Resampling.LANCZOS)
    dialog.paste(mark, ((164 - mark.width) // 2 + 4, 72), mark)

    font_path = Path("C:/Windows/Fonts/segoeuib.ttf")
    font = ImageFont.truetype(str(font_path), 27) if font_path.exists() else ImageFont.load_default()
    label = "Nexora"
    label_box = draw.textbbox((0, 0), label, font=font)
    label_width = label_box[2] - label_box[0]
    draw.text(((164 - label_width) // 2 + 4, 215), label, fill=(36, 36, 36), font=font)
    return dialog


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate Nexora application branding assets")
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--workspace", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()

    workspace = args.workspace.resolve()
    logo = build_logo(args.source.resolve())
    output_paths = [
        workspace / "src/assets/nexora-logo.png",
        workspace / "public/favicon.png",
    ]
    for output_path in output_paths:
        output_path.parent.mkdir(parents=True, exist_ok=True)
        logo.save(output_path, optimize=True)

    installer_path = workspace / "installer/dialog.bmp"
    build_installer_dialog(logo).save(installer_path)

    for output_path in [*output_paths, installer_path]:
        print(output_path)


if __name__ == "__main__":
    main()
