# blendio

`blendio` 是一个面向 Blender 4.5 `.blend` 文件的 Rust 工具集。

当前仓库包含两个可执行入口：

- `blendio`：读取 `.blend`、查看块表 / SDNA / ID、输出高层摘要
- `blend2glb`：不依赖 Blender 运行时，把 `.blend` 直接导出为单文件 `.glb`

当前目标版本固定为 Blender 4.5.x，默认完整支持 little-endian 文件。

## 文档

更详细的说明已经整理成 HTML 手册，直接打开这个文件即可：

- [`docs/index.html`](docs/index.html)

README 这里保留快速上手和最常用命令。

## 运行时依赖

这两个程序在目标机器上都不需要安装 Blender。

只要能运行生成出来的 `exe`，并且输入文件本身是 Blender 4.5 little-endian `.blend`，就可以直接解析或导出。

仓库里的集成测试会调用本机 Blender 4.5 来生成样本，这只是测试和逆向验证用，不是运行时依赖。

## 当前能力

读取层支持：

- 未压缩 `.blend`
- `gzip` 压缩 `.blend`
- `zstd` 压缩 `.blend`
- 12 字节和 17 字节 Blender 文件头
- `BHead4`、`SmallBHead8`、`LargeBHead8`
- `DNA1 / SDNA` 结构定义解析
- old-pointer 索引
- struct 视图、数组块读取、`ListBase` 链表遍历
- 更丰富的 `info` 摘要，包括 Scene / Object / Collection / Library / Image / Action / Text / Mesh / Camera / Light / Material / World

导出层支持：

- `Mesh`
- `Camera`
- `Light`
- 对象层级
- 对象 TRS 动画
- `Principled BSDF` 的 glTF 原生 PBR 子集
- packed 贴图
- `Triangulate` / `Mirror` / `Array` 三类 modifier

## 构建

开发模式运行读取器：

```powershell
cargo run --bin blendio -- info "D:\test\demo.blend"
```

开发模式运行导出器：

```powershell
cargo run --bin blend2glb -- "D:\test\demo.blend" "D:\test\demo.glb"
```

发布版构建：

```powershell
cargo build --release
```

生成的可执行文件位于：

```text
target/release/blendio.exe
target/release/blend2glb.exe
```

## `blendio` CLI

查看帮助：

```powershell
blendio --help
```

读取完整摘要：

```powershell
blendio info "D:\test\demo.blend"
```

查看块表：

```powershell
blendio blocks "D:\test\demo.blend"
```

查看全部 ID：

```powershell
blendio ids "D:\test\demo.blend"
```

查看 SDNA 概览：

```powershell
blendio sdna "D:\test\demo.blend"
```

查看某个结构体字段：

```powershell
blendio sdna "D:\test\demo.blend" --type Object
```

默认输出是逐行可读文本。如果需要 JSON：

```powershell
blendio --json info "D:\test\demo.blend"
```

如果希望 JSON 另存文件，但终端仍保持文本输出：

```powershell
blendio --json-out "D:\test\demo_info.json" info "D:\test\demo.blend"
```

可以同时使用：

```powershell
blendio --json --json-out "D:\test\demo_info.json" info "D:\test\demo.blend"
```

`info` 当前会输出这些高层内容：

- 文件头、压缩方式、块数量、ID 数量、SDNA 统计
- block code / id code 分布
- `Scene`
- `Object`
- `Collection`
- `Library`
- `Image`
- `Action`
- `Text`
- `Mesh`
- `Camera`
- `Light`
- `Material`
- `World`

## `blend2glb` CLI

基础用法：

```powershell
blend2glb "D:\test\demo.blend" "D:\test\demo.glb"
```

可选参数：

- `--no-cameras`
- `--no-lights`
- `--no-animation`
- `--strict`
- `--report-json <path>`

示例：

```powershell
blend2glb `
  "D:\test\demo.blend" `
  "D:\test\demo.glb" `
  --report-json "D:\test\demo_report.json"
```

默认输出是逐行可读文本报告；`--report-json` 会把导出报告写成 JSON 文件。

## 作为库使用

如果你要在另一个 Rust 程序里直接引用这个库，最简单的方式是先使用 path dependency：

```toml
[dependencies]
blendio = { path = "../BlendIO_rustc" }
```

读取摘要：

```rust
use blendio::{BlendFile, summarize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = BlendFile::open("D:\\test\\demo.blend")?;
    let summary = summarize(&file)?;

    println!("blend version: {}", summary.header.file_version);
    println!("object count: {}", summary.objects.len());
    Ok(())
}
```

直接导出 `.glb`：

```rust
use blendio::{ExportOptions, export_glb};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = ExportOptions::default();
    let report = export_glb(
        "D:\\test\\demo.blend",
        "D:\\test\\demo.glb",
        &options,
    )?;

    println!("meshes: {}", report.exported_mesh_count);
    println!("warnings: {}", report.warnings.len());
    Ok(())
}
```

常用库 API：

- `BlendFile::open(path)`
- `BlendFile::header()`
- `BlendFile::blocks()`
- `BlendFile::schema()`
- `BlendFile::ids()`
- `BlendFile::resolve_old_ptr(old_ptr)`
- `BlendFile::view_old_ptr_as_struct(old_ptr, struct_name)`
- `BlockRef::struct_view()`
- `StructView::field(name)`
- `read_struct_array(...)`
- `read_pointer_array(...)`
- `iter_listbase(...)`
- `summarize(&BlendFile)`
- `build_export_scene(...)`
- `export_glb(...)`

## 测试

运行全部测试：

```powershell
cargo test
```

默认集成测试会尝试使用：

```text
D:\Blender_4.5\blender.exe
```

如果 Blender 路径不同，可以设置：

```powershell
$env:BLENDER_EXE="D:\Your\Path\blender.exe"
cargo test
```

## 当前限制

读取层限制：

- 只完整支持 little-endian `.blend`
- big-endian 文件会明确返回不支持
- 主要面向 Blender 4.5.x
- 不递归展开外部链接库 `.blend`
- 不支持写回 `.blend`

导出层限制：

- 不追求与 Blender 官方 glTF 导出逐字节一致
- 只输出单文件 `.glb`
- 不支持骨骼 / 蒙皮
- 不支持 shape key
- 不支持几何节点完整求值
- 不支持复杂约束求值
- 外部贴图默认不导出，只支持 packed image
- modifier 只支持 `Triangulate` / `Mirror` / `Array`

## 项目结构

```text
src/
  animation.rs
  array_view.rs
  bhead.rs
  cli.rs
  error.rs
  gltf_export.rs
  header.rs
  input.rs
  lib.rs
  material.rs
  mesh.rs
  modifier.rs
  report.rs
  sdna.rs
  summary.rs
  view.rs
src/bin/
  blendio.rs
  blend2glb.rs
tests/
  blendio_integration.rs
  blend2glb_integration.rs
docs/
  index.html
```
