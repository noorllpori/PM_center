# PM Center

PM Center 是一个基于 `Tauri v2 + React 19 + Rust` 的桌面项目管理器，目标是把项目浏览、脚本任务、Python 环境、局域网协作和轻量编辑能力放到一个应用里。

项目目前仍在开发中，但已经有比较完整的主架构和一批可继续迭代的核心模块。当前实现看起来是以 Windows 使用场景为主，同时保留了 Tauri 跨平台应用的整体结构。

## 当前已实现的方向

- 项目打开与初始化：支持打开本地项目，并在项目目录下初始化 `.pm_center` 运行数据目录
- 文件管理：目录树、文件列表、文件详情、搜索、排序、列配置
- 标签与元数据：通过 SQLite 保存标签、文件标签和变更信息
- 脚本任务：内置 Python 任务运行、日志输出、任务队列、取消与重试
- 进度解析：支持在脚本输出中使用 `/***50*/` 这类标记更新任务进度
- Python 环境管理：检测系统 Python、扫描应用 venv、创建/删除虚拟环境、安装/卸载包
- P2P 局域网通信：发现在线用户并发送消息
- 多窗口系统：内置窗口管理、代码编辑窗口、图片查看窗口
- 桌面集成：系统托盘、单实例、后台扫描等 Tauri 桌面能力

## 技术栈

- 前端：React 19、TypeScript、Vite、Tailwind CSS、Zustand
- 编辑器：CodeMirror 6
- 桌面壳：Tauri v2
- 后端：Rust、Tokio
- 数据存储：SQLite（`rusqlite` bundled）

## 目录结构

```text
pm_center/
├── src/                     # React 前端
│   ├── components/          # UI 与业务组件
│   ├── stores/              # Zustand 状态管理
│   ├── types/               # 类型定义
│   └── App.tsx              # 应用入口
├── src-tauri/               # Tauri / Rust 后端
│   └── src/
│       ├── db/              # SQLite 与元数据
│       ├── fs/              # 文件系统能力
│       ├── p2p/             # 局域网通信
│       ├── python/          # Python/Blender 相关调用
│       ├── python_env/      # Python 环境管理
│       ├── task/            # 任务执行
│       └── watcher/         # 文件变更监听
└── AGENTS.md                # 当前项目的开发说明
```

## 本地开发

开始前请先准备：

- Node.js 与 npm
- Rust stable toolchain
- Tauri 对应平台的系统依赖

安装依赖：

```bash
npm install
```

启动开发环境：

```bash
npm run tauri dev
```

构建前端：

```bash
npm run build
```

构建桌面应用：

```bash
npm run tauri build
```

## 使用说明

- 首次打开一个项目时，后端会初始化该项目的 `.pm_center/` 目录
- 这个目录会保存脚本、数据库和部分运行时信息，通常不建议提交到业务项目仓库
- Python 任务输出里如果包含 `/***N*/`，界面会把它当作 `0-100` 的进度更新


## 后续可以继续做的方向

- 更完整的项目列表与最近项目管理
- 更成熟的窗口系统与编辑器集成
- 更稳定的任务编排和日志查看
- Blender / 渲染工作流专项能力
- 文件预览、缩略图和缓存体系

## License

本项目当前按 `GPL-2.0-only` 提供，详见 [LICENSE](./LICENSE)。
