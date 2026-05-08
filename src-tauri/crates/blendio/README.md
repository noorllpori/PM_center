# blendio

`blendio` 是一个面向 Blender 4.5 `.blend` 文件的 Rust 读取与诊断工具集。

当前仓库只保留一个可执行入口：

- `blendio`：读取 `.blend`、查看块表 / SDNA / ID、输出高层摘要

当前目标版本固定为 Blender 4.5.x，默认完整支持 little-endian 文件。

## 文档

更详细的说明已经整理成 HTML 手册，直接打开这个文件即可：

- [`docs/index.html`](docs/index.html)

README 这里保留快速上手和最常用命令。

## 运行时依赖

运行 `blendio` 不需要安装 Blender。

只要能运行生成出来的 `exe`，并且输入文件本身是 Blender 4.5 little-endian `.blend`，就可以直接解析。

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

## 构建

开发模式运行读取器：

```powershell
cargo run --bin blendio -- info "D:\test\demo.blend"
```

发布版构建：

```powershell
cargo build --release
```

生成的可执行文件位于：

```text
target/release/blendio.exe
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

## 作为库使用

如果你要在另一个 Rust 程序里直接引用这个库，最简单的方式是先使用 path dependency：

```toml
[dependencies]
blendio = { path = "../BlendIO_rustc" }
```

读取摘要：

```rust
use blendio::{BlendFile, collect_external_data_with_base, summarize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new("D:\\test\\demo.blend");
    let file = BlendFile::open(path)?;
    let summary = summarize(&file)?;
    let external_data = collect_external_data_with_base(&file, Some(path))?;

    println!("blend version: {}", summary.header.file_version);
    println!("object count: {}", summary.objects.len());
    println!("image / texture count: {}", external_data.images.len());
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
- `collect_external_data_with_base(&BlendFile, Some(path))`

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

- 只完整支持 little-endian `.blend`
- big-endian 文件会明确返回不支持
- 主要面向 Blender 4.5.x
- 不递归展开外部链接库 `.blend`
- 不支持写回 `.blend`
- 不提供 `.blend` 到 `.glb` 的导出能力

## 项目结构

```text
src/
  array_view.rs
  bhead.rs
  cli.rs
  error.rs
  header.rs
  input.rs
  lib.rs
  sdna.rs
  summary.rs
  view.rs
src/bin/
  blendio.rs
tests/
  blendio_integration.rs
docs/
  index.html
```
