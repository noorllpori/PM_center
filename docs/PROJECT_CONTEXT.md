# PM Center 项目总览与开发约定

> 当前基线：`2.8.4`。本文件是 PM Center 的长期开发上下文；新增、修改或排查功能前，先确认其归属模块和下面的固定交互规则。README 保留对外简介和较长的愿望清单，本文件记录当前实际架构与已确定的产品约束。
>
> 下一超大版本将把现有单体功能升级为用户可 DIY 的模块装配平台。项目管理器、媒体资料管理器、局域网通信端和 Blender 渲染控制端/节点只是参考成品，不是系统写死的四种模式。完整架构见 `docs/NEXT_MAJOR_MODULAR_PLATFORM.md`，逐步开发顺序、退出门槛和确认模板见 `docs/NEXT_MAJOR_IMPLEMENTATION_ROADMAP.md`，R1 合同候选见 `docs/NEXT_MAJOR_SCHEMA_V1.md`。

## 1. 产品定位

PM Center 是面向本地制作项目的 Windows 优先桌面工作台。一个项目就是一个普通目录，应用在不破坏项目业务文件的前提下，为它提供：

- 文件树、文件列表、搜索、标签、集合、拖放、复制/移动/系统右键；
- 图片、视频、文本、Markdown、Blender 文件和目录的工作区预览；
- 缩略图、文件详情与目录索引缓存；
- Python 任务、项目脚本、插件和 Python 环境管理；
- Blender 场景批渲染、帧队列、常驻 Worker、性能采样、ETA、帧预览及帧序列视频打包；
- 局域网发现、消息和确认式传输能力；
- 多窗口、会话恢复、系统托盘、全局唤醒和 Windows 原生智能剪贴板。

业务项目可独立存在。PM Center 的项目级状态都放在项目根目录的 `.pm_center/`，此目录默认从文件树、扫描、插件扫描和渲染源扫描中排除。

## 2. 技术与入口

| 层级 | 主要技术 | 入口与职责 |
| --- | --- | --- |
| 桌面壳 | Tauri v2 | `src-tauri/src/lib.rs`：应用启动、单实例、托盘、窗口、Tauri 命令注册、项目初始化。 |
| 后端 | Rust 2021、Tokio、SQLite (`rusqlite`) | `src-tauri/src/*`：文件系统、缓存、渲染、Python、插件、任务、P2P。 |
| 前端 | React 19、TypeScript、Vite、Tailwind、Zustand | `src/main.tsx` -> `src/App.tsx` -> `FileManager` / `ProjectWorkspace`。 |
| 本地配置 | `@tauri-apps/plugin-store` | `settingsStore`、任务状态、会话及部分调度预设。 |
| 项目数据 | SQLite + 项目目录 | `.pm_center/data.db`、`.pm_center/tree_cache.db` 和缓存目录。 |
| 应用级数据 | SQLite + 应用数据目录 | `smart_clipboard/clipboard_history.db`、图像载荷及设备协作数据；不写入项目 `.pm_center`。 |

开发命令：

```powershell
npm run tauri dev
npm run build
cd src-tauri; cargo check
cd src-tauri; cargo test --lib
```

版本号由 `scripts/sync-version.mjs` 保持 `package.json`、`Cargo.toml` 和 `tauri.conf.json` 一致。修改版本时只修改项目约定的源版本位置后运行 `npm run sync:version`。

## 3. 运行结构

```text
App
  FileManager
    WelcomeScreen / Launcher
    ProjectWorkspace
      ProjectSessionProvider
      WorkspaceTabBar
      FileTree + FileList + FileDetail
      files / directory / image / video / text / blend / collection / cache / render / p2p tabs
    SettingsPanel / TaskPanel
  WindowManager
  FileOperationPanel

React invoke/listen
  Tauri command handler (src-tauri/src/lib.rs)
    fs, db, watcher, tree_cache, thumbnail_cache
    file_details, cache_manager, render_center
    task, python, python_env, plugin, p2p, tools
  Native Win32 thread (smart_clipboard)
    clipboard listener + standalone history window + Ctrl+` hotkey
```

前端命令的唯一注册表是 `src-tauri/src/lib.rs` 中的 `tauri::generate_handler!`。新增 `#[tauri::command]` 后必须在这里注册；前端参数使用 camelCase，Rust 结构体使用 `#[serde(rename_all = "camelCase")]`。

## 4. 项目生命周期与持久化数据

### 4.1 打开项目

1. 前端通过 `projectStore` 选择或恢复项目。
2. `init_project` / `activate_project` 创建支持目录、初始化数据库与目录缓存，并把该项目设为 watcher 活动项目。
3. `ProjectSessionProvider` 恢复工作区标签、路径、树状态及窗口相关会话。
4. `watcher` 只监听当前活动项目，事件合并后写入变更记录并使相关目录缓存变脏。

### 4.2 `.pm_center/` 边界

| 路径 | 所有者 | 处理规则 |
| --- | --- | --- |
| `data.db` | `db`、`render_center` | 项目元数据、标签、集合、渲染任务等；属于受保护数据，不自动删除或修复。 |
| `tree_cache.db` 与 `-wal/-shm` | `tree_cache` | 目录索引和 `file_details_cache`；索引可原子重建，解析缓存可清理。 |
| `thumbnails/` | `thumbnail_cache` | 可清理，浏览时按需重新生成。 |
| `scripts/` | 项目脚本 | 用户数据，缓存管理只检查，不清理。 |
| `plugins/` | 项目插件 | 用户数据，缓存管理只检查，不清理。 |
| `render_jobs/` | `render_center` | Worker 脚本、每任务执行描述与日志等运行资料；不应被通用缓存清理删除。 |

渲染图片和视频默认写入项目 `renders/`，不属于 `.pm_center` 缓存。清理缓存绝不能改动它们。

## 5. 前端模块地图

| 模块 | 主要文件 | 责任与接入点 |
| --- | --- | --- |
| 项目与文件工作区 | `components/file-manager/ProjectWorkspace.tsx`、`FileTree.tsx`、`FileList.tsx`、`FileDetail.tsx` | 当前项目的三栏文件工作区、快捷键、拖放、文件刷新与标签页承载。 |
| 文件操作 | `FileContextMenu.tsx`、`FileOperationPanel.tsx`、`dragDrop.ts`、`externalImport.ts` | 应用内移动/复制、冲突处理、系统拖出、外部拖入。危险操作要有明确确认与结果反馈。 |
| 内置选择器 | `ProjectFilePickerDialog.tsx` | 从项目文件树单选/多选文件或目录，也可转到系统选择器；调用方必须指定 `target`、`selectionMode` 和可选扩展名。 |
| 工作区标签 | `workspaceTabStore.ts`、`WorkspaceTabBar.tsx`、`utils/appSession.ts` | 文件、目录、图片、视频、文本、Blender、集合、缓存和渲染标签；会话恢复需显式序列化。 |
| 预览与编辑 | `image-viewer/`、`video-player/`、`text-editor/`、`BlenderFileTab.tsx` | 预览与独立窗口回归；文本传输有 ready/payload/ack，不能丢失未保存内容。 |
| 缓存管理 | `CacheManagerSurface.tsx` | `.pm_center` 报告、快速/深度检查、清理和重建，进度进入右下角任务体验。 |
| 渲染中心 | `RenderCenterSurface.tsx` | 批次、作业、帧、预设、性能、ETA、Worker、右键与视频打包入口。 |
| 脚本/任务 | `ScriptRunner.tsx`、`TaskPanel/`、`taskStore.ts` | 用户脚本、日志、取消/重试与任务面板聚合。 |
| 设置 | `SettingsPanel.tsx`、`settingsStore.ts` | 全局工具路径、Blender 版本、排除规则、启动偏好、插件等。 |
| 功能中心 | `features/builtinTools.ts`、`components/file-manager/index.tsx` | `Alt+Q` 工具入口；智能剪贴板由此调用后端显示独立 Win32 窗口，不创建工作区标签或 WebView。 |
| 说明组件 | `components/ui/HelpAssistant.tsx` | 复杂或不可逆概念旁的问号说明；支持文字、图片、视频及自动避让定位。 |

状态 Store 的所有权：

- `projectStore`：项目路径、目录、文件、刷新与排除显示；
- `workspaceTabStore`：标签打开、关闭、排序、独立窗口与恢复；
- `renderStore`：按项目缓存渲染作业、待创建渲染源、后端事件刷新；
- `taskStore`：脚本任务持久化、日志、任务执行状态；
- `settingsStore`：全局持久化设置和工具路径；
- `uiStore`：toast、确认框等临时 UI 反馈；
- 其他 store 按领域命名，禁止把跨领域状态塞进无关 store。

## 6. Rust 后端模块地图

| 模块 | 路径 | 责任 |
| --- | --- | --- |
| 应用编排 | `lib.rs` | Tauri 注册、项目初始化、窗口/托盘、基础项目与标签命令。 |
| 项目数据库 | `db/mod.rs` | 标签、集合、文件元数据、变更记录和项目级 SQLite 访问。 |
| 文件系统 | `fs/mod.rs` | 目录读取、搜索、读写、移动/复制/删除/重命名、剪贴板、系统文件操作。 |
| 文件详情 | `file_details/mod.rs` | 图片、媒体、Blender 等文件详情与外部工具解析。 |
| 目录缓存 | `tree_cache/mod.rs` | `tree_cache.db`、目录条目、脏目录及索引扫描。 |
| 缩略图 | `thumbnail_cache.rs` | 缩略图生成、缓存键、失效与磁盘写入。 |
| Watcher | `watcher/mod.rs` | 当前活动项目的递归监听、事件合并、缓存失效。 |
| 缓存中心 | `cache_manager/mod.rs` | `.pm_center` 统计、完整性检查、清理与原子重建。 |
| 渲染中心 | `render_center/mod.rs` | 渲染数据库迁移、队列调度、常驻 Blender Worker、帧领取、ETA、性能、视频打包。 |
| 任务 | `task/mod.rs` | Python/脚本任务生命周期、输出及进度事件。 |
| Python | `python/mod.rs`、`python_env/mod.rs` | PMC 内置/系统 Python、Blender 解析、venv 与 pip 管理。 |
| 插件 | `plugin/mod.rs` | 插件发现、校验、启停、依赖、设置与动作。 |
| 工具路径 | `tools.rs` | FFprobe、FFmpeg、Blender 路径校验与系统自动检测。 |
| 局域网 | `p2p/mod.rs` | 全局联系人数据库、双向在线发现、个人资料/头像同步、大厅与私聊消息；由 `builtin.lan-collaboration` 生命周期统一管理 UDP/TCP、服务器连接和传输取消。 |
| 智能剪贴板 | `smart_clipboard/` | Windows 剪贴板监听、历史 SQLite/图像载荷、原生窗口绘制、全局快捷键、内容恢复与自动粘贴。 |
| 模块与权限运行时 | `platform/` | R2 Module Manager/ResourceRegistry 与 R3 CapabilityGateway，负责状态、资源、授权、短期 token、路径边界和审计；R4 已接入智能剪贴板、局域网协同，并正在接入项目资源。 |

长耗时后端操作应避免阻塞 Tauri 主线程：使用 Tokio / `spawn_blocking`，向前端发进度事件，提供合理的取消与失败状态。Windows 子进程统一使用 `process_utils::{std_command, tokio_command}`，避免弹出控制台窗口。

## 7. 已确定的核心行为

### 7.1 文件、缓存与选择

- `.pm_center` 默认隐藏且排除，不得作为普通项目内容、插件目录或渲染源处理。
- 缓存管理的可清理范围仅为缩略图和解析缓存；目录索引只能“重建”，不能让项目停在无索引状态；`data.db`、脚本、插件、未知文件受保护。
- 需要选择项目内文件时优先使用 `ProjectFilePickerDialog`，不要在每个模块重写目录选择列表。调用者决定文件/目录、单选/多选、扩展名和初始目录。
- 需要选择项目外路径时才使用系统文件选择器；界面需明确来源。
- 复制、导入、移动、缓存维护等长操作应进入任务/进度体验，不能静默卡住界面。

### 7.2 工作区、右键和帮助

- 复杂功能优先在可预期的位置提供动作：工具栏命令、任务卡片/帧行右键或详情面板，不依赖浏览器原生右键菜单。
- 项目工作区已经全局拦截原生右键；新增可用右键菜单必须显式 `preventDefault` / `stopPropagation`，并支持点击外部和 Escape 关闭。
- 图标式或有副作用的动作应有 title/tooltip；陌生概念使用 `HelpAssistant`，不要把操作说明长期铺在主界面上。
- 新工作区标签类型必须同步修改 `workspaceTabStore`、会话序列化/恢复、`ProjectWorkspace` 渲染分派和标签栏图标/关闭规则。

### 7.3 智能剪贴板

- 仅在 Windows 启用，使用独立 Win32 消息线程、原生窗口和 GDI 绘制；功能中心只负责调用 `open_smart_clipboard`，不得改成 Tauri WebView 或工作区标签。
- 应用在后台运行时通过 `AddClipboardFormatListener` 记录 `CF_UNICODETEXT`、`CF_DIB/CF_DIBV5` 和 `CF_HDROP`，并遵守 Windows 的剪贴板历史排除格式。
- 历史属于软件级全局数据，最多 500 条、保留 30 天，使用 BLAKE3 去重；数据库和图像载荷位于应用数据目录的 `smart_clipboard/`，清理过期记录时同步清理孤立载荷。
- 功能中心 `Alt+Q` 和全局 `Ctrl+\`` 均可打开窗口；快捷键注册冲突只禁用该快捷键，不能阻止 PM Center 启动。
- 智能剪贴板由 `builtin.smart-clipboard` 模块生命周期托管。首次升级默认启用；用户停用后会关闭原生窗口、注销监听和快捷键并等待线程退出，历史数据库与载荷继续保留；再次启用读取原数据。
- `open_smart_clipboard` 必须验证模块处于 `running`，停用后旧入口也不能绕过模块管理直接唤起窗口。
- 搜索框支持输入筛选；上下键选择，`Enter` 恢复并向之前的外部窗口粘贴，`Ctrl+Enter` 仅恢复，双击等同 `Enter`，`Delete` 删除，`Esc` 关闭。
- 自动粘贴不得发送到 PM Center 自身。恢复历史前必须先验证文本、图像载荷或文件路径有效，再清空系统剪贴板。

### 7.4 渲染中心（重要）

渲染层级：`批次 (render_batch)` -> `作业 (render_job)` -> `帧 (render_frame)` -> `尝试/Worker`。

- 新建批次、重试失败帧、重新渲染、编辑帧范围、排序、跳过帧都只修改等待队列，**绝不自动开始或暂停渲染**。只有“开始/继续队列”或明确的继续动作才可启动。
- 批次按顺序运行；任务卡片只允许在同一批次内排序，批次标题用于调整批次顺序。运行中的批次/任务不能拖动；排序必须在事务中完成。
- 归档任务不参与活跃批次生命周期判定，不能阻塞后续批次。
- 默认执行模式为常驻 Worker：每个并发槽只加载一次 `.blend`，随后经 stdin/stdout 协议连续领取帧；逐帧独立 Blender 是兼容回退。
- 多 Worker 使用渐进启动，并受“作业并发”和全局 `maxBlenderProcesses` 双重限制。GPU/驱动级多开崩溃时自动降至单 Worker，不把中断帧计为失败。
- 帧领取使用 `claimToken` 和 SQLite 事务；结果写临时文件、校验后原子替换最终输出，旧 Worker/超时 Worker 不可覆盖新输出。
- 暂停/取消立即终止 Worker；暂停把未提交帧恢复为 pending，不增加失败次数。应用异常退出后运行帧会被恢复为可人工继续的状态。
- 源 `.blend` 有完成帧后发生变化时进入 `attention`，用户必须重新检查或接受新源并重新排队，不能混用两个场景版本。
- ETA 使用完成帧的鲁棒趋势预测并针对当前超时帧持续校准；前端只平滑显示后端 ETA，不应另写简单平均算法。
- 每个任务可单独设置帧多开、执行模式、帧顺序、范围、场景、分辨率、格式；全局限制只约束实际资源占用。
- 批次标题右键可以逐任务导出视频；任务卡片右键只导出该任务当前范围。视频默认 25 fps，支持 MP4/MOV/WebM；所有预期帧必须有效且尺寸一致，输出至 `renders/<批次>/videos/<时间戳>/`。

### 7.5 工具、Python、插件和 P2P

- Blender 下拉选择来自设置中的 Blender 版本管理，不在渲染表单重复维护路径列表。
- FFprobe 用于媒体详情；FFmpeg 用于序列帧打包。全局设置可以手动固定路径，后端在未指定时允许从系统 PATH 自动检测。前端不能只以“是否手动指定”判断工具可用性。
- Python 运行优先 PMC 内置 Python 或所选 Blender 自带 Python；插件不应自行创建第三套不透明运行时。
- 插件 API、项目脚本与用户数据均视为受保护数据。新增插件能力要经 `plugin` 模块暴露，不直接由任意 UI 执行未知脚本。
- 局域网联系人、消息和头像属于软件级全局数据，保存在应用数据目录的 `lan_collaboration.db` 与头像缓存中，不写入项目 `.pm_center`。外层 Shell 的“局域网主面板”承载联系人、大厅和私聊；项目内 `p2p` 标签是独立的功能预留入口，暂不承载聊天界面。
- `builtin.lan-collaboration` 默认启用并统一拥有 UDP 31523、TCP 31524、Server 连接和活动传输。停用先拒绝新命令，再取消传输、关闭连接并等待监听任务退出；联系人、聊天、头像、接收文件和设置继续保留。
- `builtin.project-resources` 默认启用并管理已打开项目的 `data.db` 注册表、TreeCache 注册表、当前活动项目 watcher 和脏目录修复任务。关闭外层项目标签先撤销项目租约，再释放句柄，防止迟到异步任务重新占用 `.pm_center`。
- 局域网提供两个稳定入口：主面板与主页、项目标签同级并保持全局单例；项目功能标签在每个项目内保持单例并参与项目会话恢复。当前不提供无项目独立窗口入口。
- 局域网消息保留一个大厅与一对一私聊，记录默认保留 30 天；联系人离线后继续保留。任何文件传输、远程执行或渲染农场扩展都必须先定义权限、路径边界、失败恢复和用户确认。
- 局域网传输使用独立于聊天消息的 `lan_transfers` 记录和 `offer -> accept/reject -> request -> stream -> verify -> receipt` 状态机。首版仅在私聊开放文件、多文件拖入、文件选择和剪贴板图片；对方确认并选择保存位置前不发送文件内容。
- 正在发送或接收的文件/目录允许任意一方主动中断。中断通过独立控制连接通知对端，并唤醒正在阻塞的 TCP 读写；记录进入 `cancelled`，接收端的 `.part` 临时文件或目录必须清理，不能提交到最终路径。已中断的接收记录允许从头重新接收，当前不做断点续传。
- 传输模型以 `kind`、版本化 `manifest`、`itemCount`、`totalBytes` 和 `payloadFormat` 描述内容。后续目录传输、项目清单同步和增量资源拉取必须扩展这套 manifest/payload 适配器并复用进度、临时文件、BLAKE3 校验和确认协议，不得另建平行 TCP 链路。
- 目录与项目同步的分层、manifest 规则、冲突处理和扩展阶段详见 `docs/LAN_TRANSFER_ARCHITECTURE.md`；实现新同步功能前必须先更新该契约。
- 浏览器无法提供真实路径的拖入文件和剪贴板图片暂存在应用数据目录 `lan_collaboration/staging/`，不写入项目 `.pm_center`；失败暂存立即清理，其余记录和暂存按 30 天生命周期维护。
- 局域网接收根目录是本机私有设置，默认位于系统下载目录的 `PM Center 接收文件/`，在“个人资料与局域网状态”中修改，绝不通过联系人资料广播。接收内容按安全化后的发送者名称分目录保存，同名自动编号；100 MB 以内且通过格式校验的图片自动接收，其他文件仍需用户确认。
- 跨网络协作使用可选的独立 PMC Server。服务器不可用时，UDP 31523 局域网发现、TCP 31524 私聊与文件直传保持完整可用。
- 同一设备按稳定 `deviceId` 合并局域网与服务器在线来源。文本默认局域网优先并可回退服务器；文件在报价前固定通道，失败不自动切换。
- PMC Server 使用 Axum、Tokio 与 SQLite WAL；消息每个会话保留最近 30 条，文件仅通过有界内存流式转发，不写入服务端磁盘。

## 8. 扩展流程

新增一个功能时按以下顺序实施，避免重复造模块或跨层耦合：

1. **归类**：先放到现有领域（文件、缓存、渲染、任务、插件、设置、预览、P2P）之一；只有跨领域的稳定能力才建立新模块。
2. **数据边界**：明确数据属于项目 `.pm_center`、全局设置还是业务输出。缓存和用户数据不能混放。
3. **后端契约**：定义 Rust request/result、错误码或稳定错误文字、Tauri 命令和进度事件。命令在 `lib.rs` 注册。
4. **前端接入**：在对应 store/表面接入 `invoke` 和 `listen`；复用文件选择器、任务面板、toast、确认对话框、HelpAssistant 与现有图标体系。
5. **状态机**：列出 waiting/running/success/failure/cancelled/attention 等状态；明确哪些操作仅排队、哪些操作真正开始、取消后怎样恢复。
6. **数据安全**：涉及文件写入、删除、覆盖、缓存清理、进程终止或跨项目访问时，增加预检、确认、原子提交或可恢复策略。
7. **会话与窄屏**：若是工作区功能，补会话恢复；检查桌面与窄窗口文字、菜单与弹窗不溢出。
8. **验证**：至少运行关联 Rust 测试、`cargo check`、`npm run build` 和 `git diff --check`；高风险文件/流程需补临时项目或真实工具验收。
9. **更新本文件**：改变模块边界、公开命令、数据目录或固定交互规则时，同步更新本文件和必要的 HelpAssistant 文案。

## 9. 验证基线

| 改动类型 | 最低验证 |
| --- | --- |
| 纯前端 UI | `npm run build`，在开发版检查桌面和窄窗口。 |
| Rust 命令/数据库/缓存 | 对应 Rust 单测、`cargo check`，必要时 `cargo test --lib`。 |
| 文件写入/删除/迁移 | 临时项目验证；对受保护数据做前后哈希或内容比对。 |
| 渲染调度/Worker | 单 Worker、多 Worker、暂停、取消、崩溃恢复、缺帧/重复帧与源文件变更。 |
| 外部工具（Blender/FFmpeg/Python） | 明确路径解析、不可用提示、真实最小输入输出与错误回传。 |
| 工作区/会话 | 标签单例、关闭/恢复、独立窗口回归和上下文菜单关闭行为。 |

格式检查可能暴露历史文件的既有格式差异。不要为了让 `cargo fmt --check` 全绿而重排无关的用户修改；仅格式化本次负责的文件或在单独整理提交中处理全仓格式化。

## 10. 当前演进方向

已有 README 的 TODO 是产品愿望清单。近期实现应优先延续现有基础，而不是重新造平行系统：

- 下一超大版本的主线是“统一内核 + Capability + Module + Component + Workflow + Profile”，不维护四套长期分叉代码；
- Profile 是用户可新建、修改、导出和分享的装配方案；项目管理器、媒体管理器、通信端和 Blender 渲染器只是验证系统能力的参考组合，不得写死为固定枚举；
- 当前功能中心 Pin 只是入口偏好，不能作为模块启停。真正停用必须停止端口、watcher、调度器、原生线程、子进程和数据库资源；
- 现有 Python 插件作为兼容组件保留，后续扩展到受控 Python Worker、原生独立进程、隔离 DLL 和资料包；
- 渲染农场复用局域网传输与项目同步，不依赖聊天 UI，远程执行必须增加设备信任、权限和任务协议；
- 完整架构统一维护在 `docs/NEXT_MAJOR_MODULAR_PLATFORM.md`，执行顺序和验收状态维护在 `docs/NEXT_MAJOR_IMPLEMENTATION_ROADMAP.md`；实现阶段不得跳过未验收前置里程碑，也不得在功能中心或 `lib.rs` 中继续增加缺少生命周期和权限边界的平行系统；
- 缓存中心：增加报告准确性、恢复提示和受保护数据审计；不做静默自动清理。
- 渲染中心：完善任务/批次管理、视频打包进度与取消、真实场景性能回归；后续渲染农场建立在当前 Worker 协议和全局资源模型之上。
- 文件体验：继续使用缓存、Watcher、内部选择器、文件操作面板和统一右键，而非在单个页面复制文件系统逻辑。
- 插件：将稳定的文件/渲染/任务能力逐步包装为受权限约束的插件接口。
- 预览与编辑：优先扩展现有工作区标签和独立窗口转移协议。
