# -*- coding: utf-8 -*-
from __future__ import annotations

import math
import re
import sys
from pathlib import Path

from pmc_plugin import progress, refresh, result, run, toast

try:
    import tkinter as tk
    from tkinter import filedialog, messagebox, ttk

    TK_IMPORT_ERROR: Exception | None = None
except Exception as exc:  # pragma: no cover - depends on local Python runtime
    tk = None
    filedialog = None
    messagebox = None
    ttk = None
    TK_IMPORT_ERROR = exc

try:
    from PIL import Image, ImageDraw, ImageFont, ImageOps

    PIL_IMPORT_ERROR: Exception | None = None
except Exception as exc:  # pragma: no cover - handled at runtime
    Image = None
    ImageDraw = None
    ImageFont = None
    ImageOps = None
    PIL_IMPORT_ERROR = exc

try:
    from tkinterdnd2 import DND_FILES, TkinterDnD

    DND_AVAILABLE = True
except Exception:
    DND_AVAILABLE = False
    DND_FILES = None
    TkinterDnD = None


PLUGIN_TITLE = "图片排序排版工具"

IMAGE_EXTENSIONS = {
    ".png",
    ".jpg",
    ".jpeg",
    ".bmp",
    ".gif",
    ".tif",
    ".tiff",
    ".webp",
}


def natural_key(path: Path) -> list[object]:
    parts = re.split(r"(\d+)", path.name.casefold())
    return [int(part) if part.isdigit() else part for part in parts]


def collect_image_paths(paths: list[str]) -> list[Path]:
    images: list[Path] = []
    for raw_path in paths:
        path = Path(raw_path).expanduser()
        if path.is_dir():
            images.extend(
                child
                for child in path.iterdir()
                if child.is_file() and child.suffix.casefold() in IMAGE_EXTENSIONS
            )
        elif path.is_file() and path.suffix.casefold() in IMAGE_EXTENSIONS:
            images.append(path)
    return images


def choose_grid(count: int) -> tuple[int, int]:
    columns = math.ceil(math.sqrt(count))
    rows = math.ceil(count / columns)
    return rows, columns


def parse_positive_int(value: str, field_name: str, allow_zero: bool = False) -> int | None:
    value = value.strip()
    if not value:
        return None
    try:
        number = int(value)
    except ValueError as exc:
        raise ValueError(f"{field_name} 必须是整数") from exc
    minimum = 0 if allow_zero else 1
    if number < minimum:
        raise ValueError(f"{field_name} 不能小于 {minimum}")
    return number


def ensure_pillow_available() -> None:
    if PIL_IMPORT_ERROR is not None:
        raise RuntimeError("缺少 Pillow 依赖，请先到设置 > 插件里安装依赖。") from PIL_IMPORT_ERROR


def fit_without_upscale(image: Image.Image, max_size: tuple[int, int]) -> Image.Image:
    width, height = image.size
    max_width, max_height = max_size
    scale = min(max_width / width, max_height / height, 1.0)
    if scale >= 1.0:
        return image.copy()
    new_size = (max(1, round(width * scale)), max(1, round(height * scale)))
    return image.resize(new_size, Image.Resampling.LANCZOS)


def load_label_font(content_width: int) -> ImageFont.ImageFont:
    font_size = max(14, min(48, round(content_width / 36)))
    font_candidates = [
        Path("C:/Windows/Fonts/msyh.ttc"),
        Path("C:/Windows/Fonts/simhei.ttf"),
        Path("C:/Windows/Fonts/simsun.ttc"),
        Path("C:/Windows/Fonts/arial.ttf"),
    ]
    for font_path in font_candidates:
        if font_path.is_file():
            return ImageFont.truetype(str(font_path), font_size)
    return ImageFont.load_default()


def text_size(draw: ImageDraw.ImageDraw, text: str, font: ImageFont.ImageFont) -> tuple[int, int]:
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    return right - left, bottom - top


def shorten_text_to_width(
    draw: ImageDraw.ImageDraw,
    text: str,
    font: ImageFont.ImageFont,
    max_width: int,
) -> str:
    if text_size(draw, text, font)[0] <= max_width:
        return text

    suffix = "..."
    if text_size(draw, suffix, font)[0] > max_width:
        return ""

    low = 0
    high = len(text)
    best = suffix
    while low <= high:
        middle = (low + high) // 2
        candidate = text[:middle].rstrip() + suffix
        if text_size(draw, candidate, font)[0] <= max_width:
            best = candidate
            low = middle + 1
        else:
            high = middle - 1
    return best


def make_grid_image(
    image_paths: list[Path],
    output_path: Path,
    border_px: int = 3,
    tile_width: int | None = None,
    tile_height: int | None = None,
    show_names: bool = False,
) -> tuple[int, int, int, int]:
    ensure_pillow_available()
    opened: list[tuple[Path, Image.Image]] = []
    try:
        for path in image_paths:
            image = Image.open(path)
            image.load()
            image = ImageOps.exif_transpose(image).convert("RGB")
            opened.append((path, image))

        if not opened:
            raise ValueError("没有可用图片")

        content_width = tile_width or max(image.width for _, image in opened)
        content_height = tile_height or max(image.height for _, image in opened)
        rows, columns = choose_grid(len(opened))
        label_font = load_label_font(content_width) if show_names else None
        label_height = 0
        if label_font is not None:
            temp = Image.new("RGB", (1, 1), "black")
            temp_draw = ImageDraw.Draw(temp)
            _, text_height = text_size(temp_draw, "Sample", label_font)
            label_height = text_height + max(10, border_px * 2)

        cell_width = content_width + border_px * 2
        cell_height = content_height + label_height + border_px * 2
        result_image = Image.new("RGB", (columns * cell_width, rows * cell_height), "black")
        draw = ImageDraw.Draw(result_image)

        for index, (path, image) in enumerate(opened):
            fitted = fit_without_upscale(image, (content_width, content_height))
            row, column = divmod(index, columns)
            left = column * cell_width + border_px + (content_width - fitted.width) // 2
            top = row * cell_height + border_px + (content_height - fitted.height) // 2
            result_image.paste(fitted, (left, top))

            if label_font is not None:
                label = shorten_text_to_width(
                    draw,
                    path.stem,
                    label_font,
                    max(1, content_width - border_px * 2),
                )
                label_width, label_text_height = text_size(draw, label, label_font)
                label_left = column * cell_width + border_px + (content_width - label_width) // 2
                label_top = (
                    row * cell_height
                    + border_px
                    + content_height
                    + (label_height - label_text_height) // 2
                )
                draw.text((label_left, label_top), label, font=label_font, fill="white")

        output_path.parent.mkdir(parents=True, exist_ok=True)
        result_image.save(output_path)
        return rows, columns, result_image.width, result_image.height
    finally:
        for _, image in opened:
            image.close()


class ImageGridApp:
    def __init__(self, root: tk.Tk, initial_paths: list[str] | None = None) -> None:
        self.root = root
        self.root.title(PLUGIN_TITLE)
        self.root.geometry("640x560")
        self.root.minsize(560, 480)

        self.image_paths: list[Path] = []
        self.output_var = tk.StringVar(value=str(Path.cwd() / "combined_grid.png"))
        self.auto_output_path = True
        self.updating_output_path = False
        self.output_var.trace_add("write", self._on_output_changed)
        self.border_var = tk.StringVar(value="3")
        self.tile_width_var = tk.StringVar(value="")
        self.tile_height_var = tk.StringVar(value="")
        self.show_names_var = tk.BooleanVar(value=False)
        self.status_var = tk.StringVar(value="拖入图片，或点击添加图片/文件夹。")
        self.grid_var = tk.StringVar(value="未选择图片")
        self.last_generated: dict[str, object] | None = None

        self._build_ui()
        if initial_paths:
            self.add_paths(initial_paths)

    def _build_ui(self) -> None:
        style = ttk.Style()
        style.configure("Title.TLabel", font=("Microsoft YaHei UI", 14, "bold"))
        style.configure("Drop.TLabel", font=("Microsoft YaHei UI", 12))

        outer = ttk.Frame(self.root, padding=14)
        outer.pack(fill=tk.BOTH, expand=True)
        outer.columnconfigure(0, weight=1)
        outer.rowconfigure(2, weight=1)

        title = ttk.Label(outer, text=PLUGIN_TITLE, style="Title.TLabel")
        title.grid(row=0, column=0, sticky="w")

        drop_text = "把图片或图片文件夹拖到这里"
        if not DND_AVAILABLE:
            drop_text = "未安装拖拽依赖，可使用下方按钮添加图片"
        self.drop_label = tk.Label(
            outer,
            text=drop_text,
            height=5,
            bd=2,
            relief=tk.GROOVE,
            bg="#f7f7f7",
            fg="#222222",
            font=("Microsoft YaHei UI", 12),
        )
        self.drop_label.grid(row=1, column=0, sticky="ew", pady=(12, 10))

        if DND_AVAILABLE:
            self.drop_label.drop_target_register(DND_FILES)
            self.drop_label.dnd_bind("<<Drop>>", self._on_drop)

        list_frame = ttk.Frame(outer)
        list_frame.grid(row=2, column=0, sticky="nsew")
        list_frame.columnconfigure(0, weight=1)
        list_frame.rowconfigure(0, weight=1)

        self.listbox = tk.Listbox(
            list_frame,
            activestyle="none",
            selectmode=tk.EXTENDED,
            font=("Consolas", 10),
        )
        self.listbox.grid(row=0, column=0, sticky="nsew")
        scrollbar = ttk.Scrollbar(list_frame, orient=tk.VERTICAL, command=self.listbox.yview)
        scrollbar.grid(row=0, column=1, sticky="ns")
        self.listbox.configure(yscrollcommand=scrollbar.set)

        button_frame = ttk.Frame(outer)
        button_frame.grid(row=3, column=0, sticky="ew", pady=(10, 0))
        for index in range(5):
            button_frame.columnconfigure(index, weight=1)

        ttk.Button(button_frame, text="添加图片", command=self.add_files).grid(
            row=0, column=0, sticky="ew", padx=(0, 6)
        )
        ttk.Button(button_frame, text="添加文件夹", command=self.add_folder).grid(
            row=0, column=1, sticky="ew", padx=6
        )
        ttk.Button(button_frame, text="移除选中", command=self.remove_selected).grid(
            row=0, column=2, sticky="ew", padx=6
        )
        ttk.Button(button_frame, text="清空", command=self.clear).grid(
            row=0, column=3, sticky="ew", padx=6
        )
        ttk.Button(button_frame, text="生成图片", command=self.generate).grid(
            row=0, column=4, sticky="ew", padx=(6, 0)
        )

        options = ttk.LabelFrame(outer, text="输出设置", padding=10)
        options.grid(row=4, column=0, sticky="ew", pady=(12, 0))
        options.columnconfigure(1, weight=1)
        options.columnconfigure(3, weight=1)

        ttk.Label(options, text="黑边(px)").grid(row=0, column=0, sticky="w")
        ttk.Entry(options, textvariable=self.border_var, width=8).grid(
            row=0, column=1, sticky="w", padx=(8, 16)
        )
        ttk.Label(options, text="单格宽").grid(row=0, column=2, sticky="w")
        ttk.Entry(options, textvariable=self.tile_width_var, width=10).grid(
            row=0, column=3, sticky="w", padx=(8, 16)
        )
        ttk.Label(options, text="单格高").grid(row=0, column=4, sticky="w")
        ttk.Entry(options, textvariable=self.tile_height_var, width=10).grid(
            row=0, column=5, sticky="w", padx=(8, 0)
        )

        ttk.Label(options, text="保存到").grid(row=1, column=0, sticky="w", pady=(10, 0))
        ttk.Entry(options, textvariable=self.output_var).grid(
            row=1, column=1, columnspan=4, sticky="ew", padx=(8, 8), pady=(10, 0)
        )
        ttk.Button(options, text="浏览", command=self.choose_output).grid(
            row=1, column=5, sticky="ew", pady=(10, 0)
        )
        ttk.Checkbutton(
            options,
            text="在图片下方写入名称",
            variable=self.show_names_var,
        ).grid(row=2, column=1, columnspan=4, sticky="w", padx=(8, 0), pady=(10, 0))

        footer = ttk.Frame(outer)
        footer.grid(row=5, column=0, sticky="ew", pady=(10, 0))
        footer.columnconfigure(0, weight=1)
        ttk.Label(footer, textvariable=self.status_var).grid(row=0, column=0, sticky="w")
        ttk.Label(footer, textvariable=self.grid_var).grid(row=0, column=1, sticky="e")

    def _on_drop(self, event: tk.Event) -> None:
        data = str(event.data)
        paths = list(self.root.tk.splitlist(data))
        self.add_paths(paths)

    def add_files(self) -> None:
        paths = filedialog.askopenfilenames(
            title="选择图片",
            filetypes=[
                ("图片文件", "*.png *.jpg *.jpeg *.bmp *.gif *.tif *.tiff *.webp"),
                ("所有文件", "*.*"),
            ],
        )
        self.add_paths(list(paths))

    def add_folder(self) -> None:
        folder = filedialog.askdirectory(title="选择图片文件夹")
        if folder:
            self.add_paths([folder])

    def add_paths(self, paths: list[str]) -> None:
        new_images = collect_image_paths(paths)
        if not new_images:
            self.status_var.set("没有找到支持的图片格式。")
            return

        existing = {path.resolve() for path in self.image_paths}
        added_count = 0
        for path in new_images:
            resolved = path.resolve()
            if resolved not in existing:
                self.image_paths.append(path)
                existing.add(resolved)
                added_count += 1

        if self.auto_output_path:
            self.set_output_path(new_images[0].parent / "combined_grid.png", auto=True)

        self.sort_and_refresh()
        self.status_var.set(f"已添加 {added_count} 张图片。")

    def sort_and_refresh(self) -> None:
        self.image_paths.sort(key=natural_key)
        self.listbox.delete(0, tk.END)
        for index, path in enumerate(self.image_paths, start=1):
            self.listbox.insert(tk.END, f"{index:03d}  {path.name}")
        if self.image_paths:
            rows, columns = choose_grid(len(self.image_paths))
            empty_slots = rows * columns - len(self.image_paths)
            self.grid_var.set(f"{len(self.image_paths)} 张：{rows}x{columns}，补黑 {empty_slots} 格")
        else:
            self.grid_var.set("未选择图片")

    def remove_selected(self) -> None:
        selected = list(self.listbox.curselection())
        if not selected:
            return
        for index in reversed(selected):
            del self.image_paths[index]
        self.sort_and_refresh()
        self.status_var.set("已移除选中的图片。")

    def clear(self) -> None:
        self.image_paths.clear()
        self.set_output_path(Path.cwd() / "combined_grid.png", auto=True)
        self.sort_and_refresh()
        self.status_var.set("已清空。")

    def choose_output(self) -> None:
        current_output = Path(self.output_var.get().strip() or "combined_grid.png").expanduser()
        initial_dir = current_output.parent if current_output.parent.exists() else Path.cwd()
        initial_file = current_output.name or "combined_grid.png"
        path = filedialog.asksaveasfilename(
            title="保存合成图片",
            defaultextension=".png",
            initialdir=str(initial_dir),
            initialfile=initial_file,
            filetypes=[
                ("PNG 图片", "*.png"),
                ("JPEG 图片", "*.jpg"),
                ("TIFF 图片", "*.tif"),
                ("所有文件", "*.*"),
            ],
        )
        if path:
            self.set_output_path(Path(path), auto=False)

    def set_output_path(self, path: Path, auto: bool) -> None:
        self.updating_output_path = True
        try:
            self.output_var.set(str(path))
        finally:
            self.updating_output_path = False
        self.auto_output_path = auto

    def _on_output_changed(self, *_: object) -> None:
        if not self.updating_output_path:
            self.auto_output_path = False

    def generate(self) -> None:
        if not self.image_paths:
            messagebox.showwarning("没有图片", "请先拖入或添加图片。")
            return

        try:
            progress(5)
            border_px = parse_positive_int(self.border_var.get(), "黑边", allow_zero=True)
            tile_width = parse_positive_int(self.tile_width_var.get(), "单格宽")
            tile_height = parse_positive_int(self.tile_height_var.get(), "单格高")
            output_path = Path(self.output_var.get().strip()).expanduser()
            if not output_path.name:
                raise ValueError("请设置保存路径")
            if border_px is None:
                border_px = 3
            rows, columns, width, height = make_grid_image(
                self.image_paths,
                output_path,
                border_px=border_px,
                tile_width=tile_width,
                tile_height=tile_height,
                show_names=self.show_names_var.get(),
            )
            progress(100)
        except Exception as exc:
            messagebox.showerror("生成失败", str(exc))
            self.status_var.set("生成失败。")
            print(f"生成失败: {exc}", flush=True)
            return

        self.last_generated = {
            "output": str(output_path),
            "sourceCount": len(self.image_paths),
            "rows": rows,
            "columns": columns,
            "width": width,
            "height": height,
        }
        self.status_var.set(f"已生成：{output_path}")
        self.grid_var.set(f"{rows}x{columns}，输出 {width}x{height}px")
        toast(f"已生成 {rows}x{columns} 网格图。", title=PLUGIN_TITLE, tone="success")
        refresh(scope="project", path=str(output_path.parent))
        result({"mode": "generated", **self.last_generated})
        messagebox.showinfo("完成", f"合成图片已保存：\n{output_path}")


def selected_paths_from_request(request: dict) -> list[str]:
    selected_items = request.get("selectedItems", []) or []
    paths: list[str] = []
    for item in selected_items:
        raw_path = item.get("path")
        if not raw_path:
            continue
        if item.get("isDir"):
            paths.append(raw_path)
            continue
        extension = (item.get("extension") or Path(raw_path).suffix).casefold()
        if not extension.startswith("."):
            extension = f".{extension}"
        if extension in IMAGE_EXTENSIONS:
            paths.append(raw_path)
    return paths


def handle(request: dict) -> None:
    if TK_IMPORT_ERROR is not None:
        message = (
            "当前插件 Python 运行时缺少 tkinter，无法打开图片排版窗口。"
            "请改用包含 Tcl/Tk 的 Python 运行时，或在开发模式启用系统 Python 回退。"
        )
        toast(message, title=PLUGIN_TITLE, tone="error")
        result({"success": False, "message": message, "details": str(TK_IMPORT_ERROR)})
        return

    if PIL_IMPORT_ERROR is not None:
        message = "缺少 Pillow 依赖，请先到设置 > 插件里安装依赖。"
        toast(message, title=PLUGIN_TITLE, tone="error")
        result({"success": False, "message": message, "missingDependency": "Pillow"})
        return

    initial_paths = selected_paths_from_request(request)
    root_class = TkinterDnD.Tk if DND_AVAILABLE else tk.Tk
    root = root_class()
    app = ImageGridApp(root, initial_paths=initial_paths)
    if initial_paths:
        toast(f"已带入 {len(app.image_paths)} 张图片。", title=PLUGIN_TITLE, tone="info")
    else:
        toast("已打开图片排版窗口。", title=PLUGIN_TITLE, tone="info")

    root.mainloop()
    result(
        {
            "mode": "closed",
            "generated": app.last_generated,
            "loadedCount": len(app.image_paths),
        }
    )


if __name__ == "__main__":
    run(handle)
