from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image, ImageOps


TARGET_SIZE = (493, 312)


def export_dialog_image(
    source_path: Path,
    output_path: Path,
    *,
    background: tuple[int, int, int] = (255, 255, 255),
) -> None:
    with Image.open(source_path) as source:
        image = source.convert("RGBA")

    # WiX dialog images are landscape. If the artwork matches the required size
    # but is stored vertically, rotate it into the expected orientation.
    if image.size == TARGET_SIZE[::-1]:
        image = image.rotate(90, expand=True)

    if image.size != TARGET_SIZE:
        image = ImageOps.fit(image, TARGET_SIZE, method=Image.Resampling.LANCZOS)

    flattened = Image.new("RGB", TARGET_SIZE, background)
    flattened.paste(image, mask=image.getchannel("A"))

    output_path.parent.mkdir(parents=True, exist_ok=True)
    flattened.save(output_path, format="BMP")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export a WiX MSI dialog image BMP from a PSD or other image.",
    )
    parser.add_argument(
        "source",
        nargs="?",
        default="Art/dialog_luckystar.psd",
        help="Source image path. Defaults to Art/dialog_luckystar.psd",
    )
    parser.add_argument(
        "output",
        nargs="?",
        default="installer/dialog.bmp",
        help="Output BMP path. Defaults to installer/dialog.bmp",
    )
    args = parser.parse_args()

    source_path = Path(args.source)
    output_path = Path(args.output)

    if not source_path.is_file():
        raise SystemExit(f"Source file not found: {source_path}")

    export_dialog_image(source_path, output_path)
    print(f"Exported WiX dialog image: {output_path}")
    print(f"Target size: {TARGET_SIZE[0]}x{TARGET_SIZE[1]}")


if __name__ == "__main__":
    main()
