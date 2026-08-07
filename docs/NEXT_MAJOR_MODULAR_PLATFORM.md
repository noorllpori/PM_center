# Nexora 下一代 DIY 可组合平台与参考装配方案规划

> 文档状态：下一超大版本主设计草案
> 整理日期：2026-08-07
> 当前代码基线：2.8.5
> 目标版本：待正式排期后确定，不在本文中预先锁定版本号
> 范围：桌面端、内置模块、Python 插件、原生组件、媒体资料库、局域网协作和 Blender 外部渲染体系

## 1. 文档目的

本文件记录 Nexora 下一超大版本的产品方向、当前代码基础、目标架构、模块边界、组件协议、装配方案、数据模型、迁移路线和验收标准。

这次演进不是把 Nexora 拆成四套互相独立的软件，也不是只在功能中心隐藏若干按钮。目标是把现有单体功能升级成同一个稳定宿主运行时上的可组合平台：

- 用户可以关闭不需要的模块，并真正停止对应后台服务和资源占用；
- 用户可以从空白装配空间开始，也可以复制、修改或导入任意装配方案；
- Python、Rust/C++ 独立程序、隔离 DLL 和资料包使用统一组件协议；
- 配置、模块组合、工作流和界面布局可以导出给其他设备使用；
- 局域网通信、文件同步和批渲染共享底层能力，但不互相强制依赖；
- 现有项目、插件、渲染任务、聊天记录和 `.pm_center` 数据保持兼容。

本文是长期架构约束。开始下一超大版本实现前，应先把具体任务归入本文定义的层级和阶段，避免继续在 `lib.rs`、功能中心或单个页面中增加跨领域硬编码。

## 2. 已确定的产品决策

1. Nexora 保持一个主代码库和一个统一宿主运行时，不维护“项目版、聊天版、媒体版、渲染版”四个长期分支。宿主运行时不是组件等级，所有正式组件使用统一安装、卸载和权限模型。
2. `Workspace Profile` 是用户自己创建和分享的“装配方案”，不是系统预先规定的工作模式。项目管理器、媒体管理器、局域网通信端和 Blender 渲染器只是四个参考成品。
3. 第一阶段实现运行时模块化。关闭模块后代码仍可存在于安装目录，但入口、命令、服务和资源都停止使用。
4. 物理裁剪安装包属于后续能力。只有模块协议稳定后，才考虑按 Profile 构建精简安装包或下载可选组件包。
5. “隐藏”“停用”“卸载”是三种不同操作，界面和数据处理不能混为一谈。
6. 关闭模块不会自动删除历史数据、项目数据、聊天记录、渲染结果、脚本或插件。
7. Python 继续承担自动化、解析、胶水逻辑和快速扩展；高性能或高稳定性场景允许使用原生独立进程或隔离 DLL。
8. 第三方 DLL 默认不得直接加载到 Nexora 主进程，必须由独立组件宿主加载，避免组件崩溃拖垮应用。
9. 农场远程节点不得接收并直接执行任意 Python 或任意系统命令，只接受版本化、可验证的任务协议。
10. 系统不为任何参考装配提供特殊代码路径。示例、用户创建和外部导入的装配方案使用同一个 schema、依赖解析器和生命周期。
11. 装配方案和装配包导出时不得包含密码、私钥、聊天记录、用户身份、机器绝对路径或其他敏感数据。

## 3. 四个参考成品，不是系统内置模式

以下四种形态只用于验证 DIY 装配系统是否足够灵活。产品不应把它们写死为四个不可改变的按钮、枚举或分支，也不保证安装后必须附带这四份模板。

用户应能使用相同的模块、组件、页面、工作流和布局能力，继续组装出素材交付台、项目审片台、自动备份站、团队文件中转站或其他尚未定义的形态。

### 3.1 参考装配 A：当前 Nexora 项目管理器

这是为了兼容现有用户而由迁移程序生成的初始装配结果，不是不可删除或不可修改的内置模式。升级后首次启动仍保持当前项目管理器体验，但用户可以继续拆除、替换、重新布局和导出其中模块。

默认启用：

- 项目与文件管理；
- 文件树、列表、详情、搜索、标签、集合和序列帧；
- 缩略图、目录索引、文件解析和缓存管理；
- 任务中心、Python 环境和插件系统；
- 局域网主面板、消息和文件传输；
- Blender 文件解析和渲染中心；
- 智能剪贴板、设置和系统集成。

该装配允许用户停用任意非核心模块。例如只关闭局域网和渲染，但保留项目文件管理。

### 3.2 参考装配 B：图像与媒体资料管理器

面向图片、视频、序列帧、音频和参考资料的可视化收集、归类、备注、分组和归档。

核心体验：

- 建立一个或多个媒体资料库；
- 使用网格、瀑布流、时间线、分组和详情面板浏览内容；
- 文件支持“仅引用”“复制入库”“移动归档”三种收集方式；
- 同一个文件可以加入多个虚拟集合，不要求真实移动；
- 支持标签、颜色、评分、备注、来源、负责人、版权和归档状态；
- 使用 BLAKE3 识别内容重复项，文件名不同也可以识别；
- 保存导入时间、原始位置、移动记录、修改记录和归档记录；
- 生成缩略图、视频封面、媒体参数、可选代理文件和预览缓存；
- 支持按日期、格式、尺寸、项目、设备、标签和状态智能分组；
- 后续可通过组件接入 OCR、图像相似度、本地 AI 标签和视觉检索。

媒体资料库仍以普通目录为基础。建议每个资料库根目录使用 `.pm_center/media_catalog.db` 保存可迁移元数据，应用数据目录只保存最近资料库和界面偏好。外部引用文件不复制时，只在资料库中保存位置、哈希和状态。

### 3.3 参考装配 C：局域网通信端

面向类似内网通的轻量联系人、聊天和文件传输场景。该模式不要求打开项目，也不应初始化项目数据库、项目 watcher、缓存维护或渲染调度器。

默认启用：

- 联系人、部门和个人资料；
- 局域网发现和在线状态；
- 大厅、私聊和最近消息；
- 图片、文件、多文件和目录传输；
- 文件统筹与传输记录；
- Windows 通知、提示音和托盘运行；
- 局域网诊断、防火墙提示和可选 Nexora Server 连接。

默认停用项目文件、项目数据库、目录 watcher、Blender 解析、渲染和媒体归档。联系人、消息、头像和传输记录继续使用应用数据目录中的全局数据库，不写入任意项目 `.pm_center`。

### 3.4 参考装配 D：Blender 外部渲染工作站

面向 Blender 文件解析、批量渲染、外部渲染和局域网渲染农场。该模式包含两个可单独选择的角色。

控制端默认启用：

- Blender 文件解析；
- 渲染批次、作业、帧、预设和结果检查；
- 场景装配清单和资源打包；
- 节点发现、信任配对、能力盘点和任务分发；
- 缺失资源同步、结果回收和完整性校验；
- 队列、日志、性能、ETA 和失败恢复。

渲染节点默认启用：

- 设备身份和控制端配对；
- Blender 环境、插件、GPU、CPU、内存和磁盘能力报告；
- 资源接收和缓存；
- 受控 Render Worker；
- 任务状态、性能、日志和结果上传。

渲染节点可以使用极简界面，不必加载完整项目管理器。控制端和节点共享局域网传输能力，但不依赖聊天页面。

## 4. 参考装配与模块矩阵

| 模块 | 当前项目管理器 | 媒体管理器 | 通信端 | 渲染控制端 | 渲染节点 |
| --- | --- | --- | --- | --- | --- |
| 应用壳与设置 | 必需 | 必需 | 必需 | 必需 | 必需 |
| 项目文件管理 | 默认 | 可选 | 关闭 | 可选 | 关闭 |
| 项目元数据/集合 | 默认 | 默认 | 关闭 | 可选 | 关闭 |
| 媒体资料库 | 可选 | 默认 | 关闭 | 可选 | 关闭 |
| 缩略图与解析 | 默认 | 默认 | 关闭 | 默认 | 可选 |
| 任务中心 | 默认 | 默认 | 可选 | 默认 | 默认 |
| Python 运行时 | 默认 | 可选 | 关闭 | 可选 | 可选 |
| 插件系统 | 默认 | 默认 | 可选 | 默认 | 可选 |
| 局域网传输 | 默认 | 可选 | 默认 | 默认 | 默认 |
| 局域网聊天 | 默认 | 可选 | 默认 | 可选 | 关闭 |
| Blender 解析 | 默认 | 可选 | 关闭 | 默认 | 可选 |
| 渲染调度 | 默认 | 关闭 | 关闭 | 默认 | 关闭 |
| 渲染 Worker | 默认 | 关闭 | 关闭 | 可选 | 默认 |
| 项目资源同步 | 可选 | 可选 | 关闭 | 默认 | 默认 |
| 智能剪贴板 | 默认 | 可选 | 可选 | 可选 | 关闭 |

“默认”只表示这个参考装配示例会选择该模块，“可选”表示可以组合，“关闭”表示示例中不选择。它不是系统限制，用户装配方案可以自由覆盖非必需项。

## 5. 当前代码基础

### 5.1 前端已有基础

| 能力 | 当前代码 | 可复用内容 | 当前限制 |
| --- | --- | --- | --- |
| 前端贡献注册 | `src/features/contributionRegistry.ts`、`src/features/contributionDataSources.ts`、`src/features/contributionCatalogDiagnostics.ts`、`src/stores/contributionRegistryStore.ts` | 稳定贡献 ID、模块所有权、运行状态、冲突检测、Widget/DataSource/WorkflowNode 目录、Surface 配对、实现覆盖和订阅释放诊断 | R9 继续禁止任意第三方 React 代码；受控 HTML/CSS 表现层进入 R10-P |
| 功能 Pin 偏好 | `src/stores/builtinToolsStore.ts` | 全局持久化、排序、停用时隐藏并保留配置、Profile 切换严格替换 | 仍使用既有内置工具实现映射；第三方工具渲染器留待 R9 |
| 功能打开器 | `src/features/builtinTools.ts`、`src/components/file-manager/index.tsx` | 声明式 `openTarget` 集中打开标签页、弹窗、事件或命令 | 打开方式仍由主应用控制；R9 组件工具动作通过受控命令贡献，不直接注入 React |
| 项目工作区标签 | `src/stores/workspaceTabStore.ts`、`ContributedWorkspaceSurface.tsx`、`ContributedWidget.tsx` | 贡献式单例标签、通用 contribution 标签、动态撤下、会话恢复和 Widget 数据源连接 | Profile 尚未接管页面布局，Widget 网格和第三方组件渲染器仍待后续里程碑 |
| 外层 Shell 标签 | `src/stores/shellTabStore.ts` | home、project 与贡献式 LAN 主页面 | 尚未按 Profile 生成不同 Shell 布局 |
| 项目状态 | `projectStore` 和 `ProjectSessionProvider` | 项目打开、激活、会话恢复 | 项目能力默认与主应用绑定 |
| 任务与进度 | `taskStore`、任务面板和右下角进度体验 | 日志、进度、取消、失败反馈 | 尚未作为所有组件统一运行时 |

### 5.2 Rust 后端已有基础

| 能力 | 当前代码 | 可复用内容 | 当前限制 |
| --- | --- | --- | --- |
| 应用编排 | `src-tauri/src/lib.rs`、`src-tauri/src/platform/` | Tauri Builder、Module Manager、ResourceRegistry、CapabilityGateway、ComponentRuntime 和退出清理 | R9 已支持 Python action/worker、原生进程、资料包和宿主适配器；DLL 宿主仍待 R10 |
| 项目资源释放 | `release_project_resources`、`builtin.project-resources` | 关闭 watcher、数据库和 TreeCache 句柄，模块停用时统一释放 | 项目级 UI 贡献仍在 R5 继续拆分 |
| Python 插件 | `src-tauri/src/plugin/mod.rs`、`automation_runtime.rs`、`platform/component_runtime.rs` | manifest、依赖、动作、内置 Python、模块状态守卫和进程登记 | R9 组件宿主兼容 `python-action` 并新增常驻 `python-worker`；旧插件入口继续作为兼容层 |
| 插件 SDK | `src-tauri/resources/plugin-sdk/pmc_plugin` | 请求读取、progress、toast、confirm、refresh 和 result | 只支持一次性 Python CLI 动作 |
| Python 环境 | `python`、`python_env`、`automation_runtime.rs` | Nexora 内置 Python、系统/venv/Blender Python、pip 和进程生命周期 | 缺少组件级隔离、锁定依赖和长期 Worker 管理 |
| 任务执行 | `task/mod.rs`、`automation_runtime.rs` | 子进程、stdout/stderr、进度、取消和模块停用收敛 | 尚未抽象成通用组件宿主协议 |
| 局域网 | `p2p/`、`builtin.lan-collaboration` | 发现、联系人、聊天、传输、服务器通道和统一启停 | 聊天、传输和未来同步仍处于同一大领域模块 |
| 渲染中心 | `render_center/mod.rs`、`builtin.render-center` | 批次、常驻 Worker、ETA、性能、进程登记和停用恢复 | 调度主要面向本机项目，尚无远程节点与资源装配协议 |
| 缓存/Watcher | `cache_manager`、`tree_cache`、`watcher`、`builtin.project-resources` | 项目级缓存、Watcher、数据库注册表和模块关闭释放 | Profile 尚不能选择更细粒度的项目资源子模块 |
| 智能剪贴板 | `smart_clipboard/`、`builtin.smart-clipboard` | Win32 窗口、历史、快捷键与真实模块生命周期 | 仍是 Windows 原生专用 Surface，不是通用 WebView 组件 |

### 5.3 当前静态运行结构

当前 `src-tauri/src/lib.rs` 仍使用 `tauri::generate_handler!` 静态注册 Tauri 命令，但 R2-R4 已由 Module Manager、ResourceRegistry 和 CapabilityGateway 管理既有后台领域。R5 第一轮让功能中心、Pin、LAN Shell 标签及缓存/渲染/LAN 工作区标签读取模块贡献；停用模块时，前端入口撤下，后端命令守卫拒绝调用，实际端口、watcher、调度器、原生线程、数据库和子进程由各模块生命周期释放。

设置分区、右键动作、Widget 和 DataSource 已接入 R5 贡献目录，工作流节点已登记类型合同但尚不可执行。R6-2 让普通 Profile 经预检后事务式切换正式模块集合和快捷栏 Pin，失败会恢复旧模块集合；前端目录与 manifest 所有权、Surface/Widget/DataSource 实现覆盖和订阅释放继续自检。剩余限制是 Profile 尚未接管完整布局、第三方组件渲染器尚未进入统一运行时。下一版本仍不要求动态增删 Tauri 命令注册；静态命令必须继续经过模块与能力守卫。

## 6. 目标分层架构

```text
Workspace Profile
  选择启用模块、参数、工具栏、Shell 页面和布局
        |
Module Manager
  解析依赖、冲突、权限、作用域和生命周期
        |
Capability Gateway
  文件、项目、任务、Python、局域网、渲染、通知等稳定能力
        |
Component Runtime
  Python Action / Python Worker / Native Process / Isolated DLL / Data Pack
        |
Nexora Kernel
  Tauri、窗口、设置、日志、任务、事件、数据库基础设施和更新机制
```

### 6.1 Kernel 核心层

核心层始终存在，只负责应用启动、单实例、窗口和托盘、Profile 配置加载、模块生命周期编排、能力授权、统一任务/日志/事件/错误、组件校验、密钥和信任存储、崩溃恢复及退出清理。

项目文件管理、聊天、媒体库和渲染都不是 Kernel。它们是建立在 Kernel 能力上的模块。

### 6.2 Capability 能力层

Capability 是比模块更稳定、更细粒度的后端能力。建议使用分层命名：

```text
app.settings.read
app.settings.write
notification.send
project.open
project.files.read
project.files.write
project.metadata.read
project.metadata.write
cache.inspect
cache.maintain
task.run
task.cancel
python.execute
python.packages.manage
network.lan.discover
network.lan.message
network.lan.transfer
network.server.connect
render.inspect
render.queue.read
render.queue.write
render.worker.execute
render.result.commit
```

Capability 不是前端按钮，也不是任意脚本可以直接使用的 Tauri 命令。所有组件调用需要经过 `CapabilityGateway`，验证模块状态、Profile、manifest 权限、用户批准、路径范围、任务状态和短期组件令牌。

### 6.3 Module 模块层

建议首批模块 ID：

```text
core.project-shell
project.files
project.metadata
project.collections
project.cache
media.catalog
media.preview
task.center
python.runtime
plugin.legacy-python
clipboard.windows
lan.transport
lan.contacts
lan.chat
lan.transfer-manager
lan.project-sync
render.inspector
render.batch
render.farm-controller
render.farm-node
render.result-review
```

聊天依赖局域网传输和联系人，但农场控制端只复用传输、节点和同步模块，不依赖聊天 UI。

### 6.4 Component 组件层

| 类型 | 用途 | 默认隔离 |
| --- | --- | --- |
| `python-action` | 一次性整理、解析、导出和自动化 | 独立 Python 子进程 |
| `python-worker` | 需要缓存模型或长期监听的组件 | 受监督 Python 进程 |
| `native-process` | 高性能扫描、哈希、编解码、同步 | 独立 EXE/sidecar |
| `native-library` | 供应商 DLL 或算法库 | 独立组件宿主加载 |
| `data-pack` | 模板、预设、分类规则、图标、模型和资料 | 只读资源挂载 |
| `builtin-rust` | 迁移现有宿主 Rust 实现 | 临时兼容 adapter，不进入正式可安装组件目录 |

组件是可安装、卸载、升级和版本化的能力单元，不等同于页面或产品模块。组件不区分“内核”和“普通”等级；`role`、`distribution` 和 `uiMode` 只描述用途、来源和界面方式，不改变 Capability 与生命周期规则。模块通过 `requiresComponents` / `optionalComponents` 声明组件依赖；Profile 可以通过 `enabledComponents` 显式激活仅用于工具动作、Widget、DataSource 或工作流节点的组件。运行时会把显式组件与模块传递依赖合并为有效组件集合。

首个正式组件为 `pmc.blendio`：它默认随安装包分发但允许卸载、重装和升级；渲染中心依赖它完成 `.blend` 场景预检，项目资源模块可选使用它生成预览和结构化详情。“Blender 文件解析器”由宿主 UI 调用该无头服务组件。HDA、PPT、PDF 等格式读取器使用相同机制，详细边界见 `docs/NEXT_MAJOR_COMPONENT_DEPENDENCY_MODEL.md`。

### 6.5 Script Automation 脚本自动化层

R11 使用可安装 Python 脚本组件组合业务逻辑，不建设节点 DAG 或节点画布。组件通过 `automationCommands`、`automationEvents` 和 `scriptSurfaces` 声明能力，Profile 通过 `automationBindings` 决定手动、事件和 cron 触发。旧 Workflow 字段只保留兼容解析，不能作为可执行功能目录。

## 7. 模块清单与生命周期

### 7.1 Module Manifest v1

```json
{
  "schemaVersion": 1,
  "id": "render.farm-controller",
  "name": "渲染农场控制",
  "version": "1.0.0",
  "apiVersion": "1",
  "scope": "global",
  "builtin": true,
  "requiresModules": [
    {"id": "render.batch", "versionRequirement": "^1.0"},
    {"id": "lan.transport", "versionRequirement": "^1.0"},
    {"id": "lan.project-sync", "versionRequirement": "^1.0"}
  ],
  "requiresComponents": [
    {"id": "pmc.blendio", "versionRequirement": "^1.0"}
  ],
  "optionalModules": [
    {"id": "lan.chat", "versionRequirement": "*"}
  ],
  "conflicts": [],
  "capabilities": [
    "project.files.read",
    "render.queue.write",
    "network.lan.transfer"
  ],
  "backgroundServices": ["farm-scheduler"],
  "contributes": {
    "workspaceTabs": ["render.farm-tab"],
    "tools": ["render.farm-controller"],
    "settingsSections": ["render.farm-settings"],
    "workflowNodes": []
  },
  "dataPolicy": {
    "retainOnDisable": true,
    "deleteRequiresExplicitAction": true
  }
}
```

### 7.2 状态与启停

模块状态统一为：

```text
disabled
resolving
starting
running
stopping
blocked
error
restart-required
```

启用流程：解析依赖和冲突，检查包与平台，审批权限，按拓扑启动依赖，注册 UI 贡献，启动服务，执行健康检查。任一步失败时回滚本次启动内容。

停用流程：检查运行任务和未保存页面，阻止新请求，关闭模块页面，停止调度器/端口/watcher/Worker/原生线程，持久化未完成状态，清理临时文件并释放资源。停用不删除数据。

### 7.3 ResourceRegistry

Module Manager 应统一登记：

- 端口和网络监听器；
- Tokio 任务和取消令牌；
- 原生线程；
- 子进程和进程树；
- SQLite 连接/连接池；
- watcher；
- 全局快捷键；
- Tauri/前端事件订阅；
- 临时目录和 staging 文件。

应用退出、Profile 切换、项目关闭和模块停用都通过同一注册表释放资源。

## 8. 装配方案 Profile

### 8.1 Profile 数据模型

```json
{
  "schemaVersion": 1,
  "id": "example.render-controller",
  "name": "Blender 渲染控制端",
  "revision": 1,
  "enabledModules": [
    {"id": "task.center", "versionRequirement": "^1.0"},
    {"id": "lan.transport", "versionRequirement": "^1.0"},
    {"id": "lan.project-sync", "versionRequirement": "^1.0"},
    {"id": "render.inspector", "versionRequirement": "^1.0"},
    {"id": "render.batch", "versionRequirement": "^1.0"},
    {"id": "render.farm-controller", "versionRequirement": "^1.0"}
  ],
  "moduleSettings": {},
  "shellLayout": {
    "home": "render-dashboard",
    "navigation": ["render-dashboard", "render-queue", "node-manager"],
    "pinnedTools": ["render.center", "render.farm-controller"],
    "shellTemplate": {
      "id": "builtin.shell.top-bar",
      "versionRequirement": "*"
    },
    "themePreset": {
      "id": "builtin.theme.nexora-default",
      "versionRequirement": "*"
    }
  },
  "surfaces": [
    {
      "id": "render-dashboard",
      "kind": "dashboard",
      "layout": "responsive-grid",
    "widgets": [
      {"id": "queue-summary", "widget": "render.queue-summary", "order": 0},
      {"id": "node-status", "widget": "farm.node-status", "order": 1},
      {"id": "task-activity", "widget": "task.activity", "order": 2}
    ]
    }
  ],
  "commandBindings": [
    {
      "id": "create-render-batch",
      "command": "render.create-batch",
      "placement": "toolbar",
      "surface": "render-dashboard"
    }
  ],
  "workflowBindings": [
    {
      "id": "prepare-render",
      "trigger": "render.batch-created",
      "workflow": "render.prepare-and-dispatch"
    }
  ],
  "variables": {
    "blenderRequirement": "blender@5.x",
    "ffmpegRequirement": "ffmpeg@system-or-configured"
  }
}
```

Profile 分为迁移生成方案、用户创建方案、外部导入方案、可选示例方案和临时会话方案。所有方案使用相同数据结构，没有不可修改的内置 Profile 特权。项目只能建议导入或切换某个 Profile，不能静默修改用户当前装配。

用户可以选择“空白装配空间”，然后逐项添加模块、页面、工具入口、后台服务、工作流和组件；也可以从任何现有方案复制后继续修改。

切换前必须展示将启用/停止的模块、被运行任务阻止的项目、缺失工具、需重启项和将关闭的页面。切换事务失败时恢复旧 Profile。

### 8.2 DIY 装配自由度

装配系统至少需要允许用户配置八类内容：

1. **能力与模块**：选择文件、媒体、局域网、渲染、Python、任务等能力组合。
2. **页面与表面**：选择首页、工作区标签、独立页面、详情面板、侧栏和仪表盘。
3. **Shell 与页面模板**：选择可安装的基础 HTML 模板、命名插槽、页面结构和响应式变体。
4. **主题与外观**：选择颜色、字体、密度、图标、阴影、动画和页面级主题覆盖。
5. **入口与命令**：将模块命令放到工具栏、功能中心、右键菜单、快捷键或页面按钮。
6. **数据源**：绑定项目目录、媒体资料库、局域网全局数据库、渲染队列或组件数据。
7. **布局参数**：决定面板比例、响应式网格、默认展开状态和窄窗口策略。
8. **脚本自动化**：把组件命令与手动、应用事件或 cron 触发绑定起来。

外部工具和位置必须使用 `toolAliases` / `pathVariables` 声明逻辑需求。本机 Blender、FFmpeg、FFprobe、Python、文件和目录映射保存在 Profile 外部，导出包不得携带绝对路径。

所以 Profile 不是简单的 `enabledModules[]`。它是完整的应用装配描述，能够决定 Nexora 启动后“看起来像什么、先打开什么、能做什么、各动作怎样连接”。

### 8.3 最小 Shell、主页与启动目标

Nexora 宿主本身不等于项目管理器。最小 Shell 始终保留窗口与主题、工具中心、全局设置、Profile/模块/组件管理、切换恢复和故障回退；这些入口不能被普通装配撤下，否则错误 Profile 会失去自救路径。最小 Shell 不是“内核组件”等级，只是宿主维持可启动、可恢复所需的固定边界。

现有项目主页必须从写死的 `WelcomeScreen` 迁移为模块贡献。规划新增 `builtin.project-manager` 管理项目目录、创建/导入、最近项目、项目列表、项目 Shell 标签和文件工作区；既有 `builtin.project-resources` 保留数据库、TreeCache、Watcher 和项目关闭释放等资源层。项目管理器依赖项目资源层，渲染或媒体模块可以只依赖资源层，不被强制绑定到项目管理主页。

截图中的项目主页继续拆成可装配 Widget：项目目录、快速操作、最近打开和项目列表。它只是 `builtin.project-manager` 提供的一个普通 Surface，不再是宿主中特殊的固定主页。Profile 使用 `shellLayout.home` 指向任意有效主页 Surface；该 Surface 直接占据主页宿主，不会再打开一个同名标签。模块关闭时对应 Surface/Widget 自动撤下。没有配置主页、主页贡献缺失或所属模块不可用时，Shell 必须显示最小安全主页，不能白屏。

启动目标分为两层：

1. `shellLayout.home` 静态决定默认主页；
2. `automationBindings` 绑定受控 `app.started` 事件，通过 R11 脚本组件执行受权限约束的启动动作。

启动优先级固定为：完成崩溃与 Profile 切换恢复，应用用户的会话恢复策略，触发当前 Profile 的 `app.started` 自动化，最后回退到默认或最小安全主页。脚本不得直接创建任意窗口或绕过模块状态，只能使用清单声明且受 Capability 和贡献目录约束的宿主能力。

### 8.4 装配编辑器

第一版装配编辑器使用结构化表单和可视化布局编辑。后续允许安装带受控 `base.html` 和 CSS 的表现模板，但不开放任意 React、JavaScript 或未经净化的 HTML 注入：

- 左侧显示可用模块、页面、部件、命令和工作流；
- 中间是当前 Shell、导航、工具栏和页面布局预览；
- 右侧编辑选中项参数、数据源、权限和显示条件；
- 支持拖动页面、部件和工具入口；
- 支持实时预览，但实际启停后台模块仍走事务化应用；
- 保存前执行依赖、冲突、权限、路径和响应式布局检查；
- 任意装配都可以复制、重命名、导出和恢复历史版本。

布局 schema 必须是受控且版本化的数据结构。第三方组件只能贡献声明过的页面、部件、模板或主题，不能直接取得整个主 WebView 的任意执行权限。

### 8.5 表现模板、可替换 Shell 与主题

现有 `top-bar / side-bar / minimal` 只作为三个内置 ShellTemplate 的兼容入口。长期运行时由 `shellLayout.shellTemplate` 选择整个应用外壳，由 Surface 的 `template` 选择页面结构，由 `themePreset` 提供视觉令牌。旧 `navigationKind` 继续读取并映射到内置模板，但不再扩展新的写死布局分支。

模板包可以包含受控 `base.html`、CSS、图标、图片、预览和模板 schema。功能通过 `<pm-surface-host>`、`<pm-navigation>`、`<pm-toolbar>`、命名 Widget 区域和注册命令进入模板。模板不得包含脚本、任意远程资源、原始 Tauri 调用或绕过 CapabilityGateway 的行为。

Profile 保存模板 ID、版本约束、变体和参数；实际 HTML/CSS 属于可安装组件包。`.pmc-workspace` 可以在严格包检查后携带模板包，从而复现完整外观。模板缺失、损坏或不兼容时保留旧 Shell 或回退不可撤下的恢复 Shell。

完整包结构、安全白名单、编辑器、迁移和验收见 `NEXT_MAJOR_PRESENTATION_TEMPLATE_ARCHITECTURE.md`。

## 9. 配置、装配空间和组件包导出

### 9.1 `.pmc-profile`

轻量装配方案文件，包含模块 ID 和版本约束、普通设置、工具栏 Pin、Shell/页面模板引用与参数、主题、工作区布局、逻辑工具需求、文件处理器逻辑绑定及工作流引用，不包含模板资源、二进制组件和大型资料。

### 9.2 `.pmc-workspace`

装配空间文件，包含一个 Profile、工作流、渲染预设、媒体分类结构、标签模板、文件处理器绑定、组件依赖清单、项目模板和相对路径资源。通过 R10 包安全检查后，它可以使用 `references-only` 轻量模式，或携带许可证允许分发的组件包与表现模板包，实现功能和外观的自包含复现。它用于把一套工作方式交给别人，不默认包含业务项目文件。

### 9.3 `.pmc-pack`

离线组件包允许包含 Python 组件及锁定依赖、Windows x64 原生 EXE、隔离 DLL、数据资料包、模板、图标和预设，以及 manifest、哈希、签名和许可证。

组件包安装前需要显示来源、发布者、权限、平台、体积和写入位置。

### 9.4 `.pmc-renderpack`

Blender 渲染装配包包含：

- `.blend` 或受控副本；
- 外部贴图、字体、缓存和链接资源；
- Blender 版本、插件和组件要求；
- 场景、引擎、帧范围、分辨率、输出格式和色彩管理；
- Python 前后处理组件引用；
- 所有文件相对路径、大小和 BLAKE3；
- 创建时间、来源任务和 schema 版本。

默认不打包 Blender 安装本体和无分发权限的插件。接收端通过逻辑需求匹配本机环境。

### 9.5 导出安全

所有导出格式自动排除：

- 密码、访问令牌、私钥和 keyring 内容；
- 聊天记录、联系人身份和设备私密资料；
- 本机绝对工具路径；
- 未明确选择的业务文件；
- 临时文件、日志、数据库 WAL/SHM 和缓存；
- 许可证不允许重新分发的组件。

导入时使用变量重新绑定路径：

```text
${PROJECT_ROOT}
${MEDIA_LIBRARY_ROOT}
${PMC_APP_DATA}
${BLENDER:5.x}
${FFMPEG}
```

## 10. 组件运行时与扩展协议

### 10.1 Python 定位

Python 适合文件整理、Blender 内部自动化、元数据提取、工作流编排、调用 Python 生态和快速开发项目工具。

以下场景优先使用 Rust/C++ 原生进程或成熟库：

- 高频大文件哈希和块同步；
- 长时间高吞吐网络转发；
- 视频编解码内核；
- 严格内存控制的图像批处理；
- 高频 Windows 底层 API 常驻服务。

Python 可以继续负责组织参数和调用原生组件，并不需要为了性能完全移除 Python。

### 10.2 Python 环境策略

- Nexora 内置 Python 是默认受控运行时；
- Blender 内部脚本使用目标 Blender 自带 Python；
- Python 组件不得污染内置 Python 的全局 site-packages；
- 每个组件使用锁定 requirements、vendor 或组件级隔离环境；
- 组件包记录 Python 和依赖版本；
- 安装、升级、卸载依赖进入任务中心并可诊断；
- 相同依赖可由内容哈希缓存复用，但组件视角仍保持隔离。

### 10.3 原生独立进程

`native-process` 是首选原生扩展形式。Nexora 使用 stdin/stdout、命名管道或本地 socket 与其通信。

优点：

- 组件崩溃不会直接结束主应用；
- 可以独立限制 CPU、内存和并发；
- 容易收集日志、超时和退出码；
- 可以按平台和架构选择不同二进制；
- 升级时不需要在主进程内卸载 DLL。

### 10.4 DLL 组件宿主

第三方 `native-library` 由 `pmc-component-host.exe` 加载：

```text
Nexora
  <-> 本地命名管道 / JSON-RPC
pmc-component-host.exe
  <-> 稳定 C ABI
第三方 DLL
```

组件宿主负责校验 DLL 哈希、签名、架构和 ABI，转发日志和进度，监控崩溃、超时与内存，并按风险决定是否一组件一宿主。组件崩溃后可以重启宿主，但不能重复提交已经完成的结果。

`builtin-rust` 只用于把当前已编译进宿主的旧实现接入统一合同，是迁移期兼容标记，不进入正式组件目录。所有正式组件包都必须支持安装和卸载，并使用独立进程、隔离 DLL、Python 或资料包运行时；无论来源是安装包、商城还是本地导入，均经过相同的权限和依赖检查。

### 10.5 Component Manifest v1

```json
{
  "schemaVersion": 1,
  "id": "media.duplicate-detector",
  "name": "媒体重复检测",
  "version": "1.0.0",
  "apiVersion": "1",
  "runtime": "native-process",
  "role": "service",
  "distribution": "marketplace",
  "uiMode": "contributed",
  "platforms": ["windows-x64"],
  "entry": "bin/windows-x64/duplicate-detector.exe",
  "capabilities": ["project.files.read"],
  "contributes": {
    "workflowNodes": [
      {
        "id": "media.find-duplicates",
        "command": "scan",
        "name": "检测重复媒体",
        "inputs": [{"name": "roots", "type": "file-list", "required": true}],
        "outputs": [{"name": "groups", "type": "json", "required": true}]
      }
    ],
    "toolActions": [
      {"id": "media.find-duplicates-now", "command": "scan", "name": "立即检测重复项"}
    ]
  },
  "resources": {
    "maxMemoryMb": 1024,
    "maxParallelism": 2
  }
}
```

模块依赖组件、Profile 显式组件和组件附属功能的统一规则见 `docs/NEXT_MAJOR_COMPONENT_DEPENDENCY_MODEL.md`。组件贡献出来的工具动作不是另一份解析实现；例如 Blender 文件解析器、渲染预检和工作流节点必须共同调用 `pmc.blendio` 的稳定命令面。

### 10.6 组件通信

建议从 JSON Lines v1 开始，大数据流使用文件、二进制流或共享内存，不把大文件编码进 JSON。

宿主发送：

```json
{"type":"initialize","apiVersion":"1","componentId":"media.duplicate-detector"}
{"type":"run","operationId":"...","command":"scan","input":{"roots":["..."]}}
{"type":"cancel","operationId":"..."}
{"type":"shutdown"}
```

组件返回：

```json
{"type":"ready"}
{"type":"progress","operationId":"...","processedItems":120,"totalItems":500}
{"type":"log","operationId":"...","level":"info","message":"..."}
{"type":"result","operationId":"...","output":{"duplicateGroups":8}}
{"type":"error","operationId":"...","code":"SOURCE_UNREADABLE","message":"..."}
```

所有写入型结果由 Nexora 核心验证并提交，组件不能通过伪造 `result` 绕过路径和权限检查。

### 10.7 文件意图路由与可替换查看器

文件管理器、媒体资料库、聊天附件、最近文件和工作流都通过平台核心 `FileIntentRouter` 请求打开、预览、编辑或解析文件。文件管理器只贡献交互入口，不保存后缀绑定真相，也不硬编码图片、文本、视频或 Blender 页面。

组件通过 `FileHandlerContribution` 声明支持的扩展名、MIME、文件类型、意图和 Surface/命令目标。绑定优先级固定为项目、Profile、软件全局、组件默认和 Windows 系统程序。

普通 `open` 在内部处理器缺失、停用或依赖不满足时可以降级系统打开；`open-internal`、`inspect`、`thumbnail`、`extract-metadata` 和渲染/工作流调用不能系统降级。处理器成功接受前不得创建工作区标签，避免组件卸载后留下空白或报错标签。

现有内置页面逐步迁移为：

```text
nexora.file-handler.image
nexora.file-handler.video
nexora.file-handler.text
nexora.file-handler.markdown
nexora.file-handler.directory
nexora.file-handler.blender-workspace -> pmc.blendio
```

这些处理器可以卸载和被第三方组件替换；`pmc.blendio` 是无头解析服务，不拥有 Blender 页面。完整合同、上下文菜单、绑定数据、系统 fallback、包关系和 R10 实施顺序见 `docs/NEXT_MAJOR_FILE_HANDLER_ROUTING.md`。

## 11. 脚本自动化模型

R11 的可执行单位是一个组件自动化命令，不是节点图。每次运行拥有 `runId`、`operationId`、不可变 Manifest/包摘要、Profile revision、项目和输入快照。`pure` 与 `idempotent` 命令可以按清单重试；`non-idempotent` 结果未知时进入 `attention`。运行、尝试、日志、权限、事件去重和 cron tick 写入 `automation_runtime.db`，统一在任务中心观察。

`WorkflowManifest`、`workflowNodes` 和 `workflowBindings` 继续作为 schema v1 兼容合同解析，但冻结且不执行。完整实现见 `docs/NEXT_MAJOR_R11_SCRIPT_AUTOMATION.md`。

## 12. 图像与媒体资料库设计

### 12.1 数据边界

建议新增独立 `media_catalog.db`，避免把高频索引、派生资源和归档历史继续堆入当前项目 `data.db`。

主要表：

```text
media_items
media_locations
media_hashes
media_collections
media_collection_members
media_tags
media_item_tags
media_annotations
media_archive_events
media_derived_assets
media_import_jobs
media_smart_groups
```

核心职责：

- `media_items`：稳定媒体 ID、类型、基础元数据和当前状态；
- `media_locations`：一个内容对应的一个或多个实际文件位置；
- `media_hashes`：BLAKE3、可选感知哈希和文件签名；
- `media_collections`：人工分组、智能分组和层级；
- `media_annotations`：备注、评分、描述和结构化字段；
- `media_archive_events`：导入、移动、复制、归档、恢复和丢失事件；
- `media_derived_assets`：缩略图、封面、代理和 OCR/AI 结果；
- `media_import_jobs`：可恢复的批量收集任务。

### 12.2 收集流程

```text
选择文件/目录/拖入/剪贴板
  -> 扫描与格式识别
  -> 计算快速签名和 BLAKE3
  -> 重复项决策
  -> 引用/复制/移动
  -> 原子提交文件
  -> 写入目录和媒体数据库
  -> 生成缩略图与派生信息
  -> 添加默认集合、标签和备注
```

长操作复用右下角任务面板，支持冲突处理、取消和失败恢复。

### 12.3 与现有集合的关系

- 现有项目集合继续服务项目文件工作区；
- 媒体集合使用稳定媒体 ID，可以跨物理目录组织；
- 第一阶段提供“从项目集合创建媒体集合”和“定位到项目文件”；
- 不直接把两套表强行合并，待使用模型稳定后再评估统一抽象；
- 序列帧识别能力作为媒体资料库的一种聚合视图复用。

## 13. 局域网模块拆分

将当前 `p2p` 大领域逐步拆成可复用内部模块：

```text
lan.identity
lan.discovery
lan.transport
lan.contacts
lan.chat
lan.transfer-manager
lan.server-bridge
lan.project-sync
lan.node-registry
```

依赖关系：

```text
lan.chat
  -> lan.contacts
  -> lan.transport

lan.project-sync
  -> lan.transport
  -> asset.manifest

render.farm-controller
  -> lan.node-registry
  -> lan.project-sync
  -> render.batch
```

已有 `LAN_TRANSFER_ARCHITECTURE.md` 继续作为文件、目录和项目同步的具体传输契约。本文只规定其在模块平台中的归属和复用方式。

## 14. Blender 场景装配与渲染农场

### 14.1 场景装配清单

创建远程任务前，控制端生成版本化 manifest：

- 源 `.blend` 的 BLAKE3、大小和修改时间；
- Blender 主/次版本要求；
- 场景、视图层、相机和渲染引擎；
- 帧范围、步长、分辨率、采样、格式和色彩管理；
- 外部贴图、缓存、字体、库链接、几何节点资源和相对路径；
- 插件和扩展的标识、版本、状态和可分发性；
- Python 前后处理组件；
- 输出结构、临时目录、提交规则和结果校验；
- 是否允许 CPU 回退或不同 GPU；
- 任务生成者、创建时间和协议版本。

### 14.2 节点能力报告

节点至少报告设备 ID、系统和 Nexora 版本、CPU/内存/磁盘、GPU/显存/驱动、Blender 版本别名、插件组件版本、当前队列和资源、缓存内容哈希、支持的渲染引擎和输出格式。

### 14.3 调度链路

```text
创建/导入渲染配置
  -> 检查源文件和场景
  -> 生成装配清单
  -> 查询可信节点能力
  -> 选择节点并锁定任务版本
  -> 比较资源哈希
  -> 同步缺失块
  -> 节点本地验证
  -> 领取帧或分片
  -> 渲染到临时输出
  -> 校验格式、尺寸和哈希
  -> 上传结果
  -> 控制端原子提交到 renders
  -> 更新帧、ETA、日志和归档状态
```

### 14.4 防重复和一致性

复用当前渲染中心的 SQLite 事务领取、Worker/节点 ID、`claimToken`、临时文件、`committing`、验证后原子替换、源文件 attention、重试/重渲染分离和“不自动开始”原则。

节点断线后在租约过期前不重复领取；旧节点结果不能覆盖新领取结果；最终文件有效且未要求覆盖时可以跳过。

### 14.5 安全与信任

聊天的明文局域网协议不能直接作为远程执行信任依据。农场必须增加：

- 设备长期身份密钥；
- 首次配对码或二维码确认；
- 控制端和节点信任列表；
- 消息签名和加密通道；
- 任务 schema 白名单；
- 禁止任意 shell、任意 Python 和任意绝对路径写入；
- 项目 staging 沙箱；
- 资源大小、路径深度、文件数和并发限制；
- 可撤销信任和操作审计。

## 15. UI 与管理中心

### 15.1 “模块与装配空间”设置页

新增全局设置页面，包含：

- 当前装配方案和空白装配空间；
- 新建、复制、重命名、修改、导入、导出和删除装配方案；
- 可选参考方案库，但参考方案和用户方案使用同一格式；
- Profile 切换影响预览；
- 已启用、已停用、阻塞、错误和需重启模块；
- 模块依赖树和“为什么不能关闭/启用”；
- 权限、后台服务、端口、数据位置和资源占用；
- 模块数据清理入口，和停用操作严格分开；
- 导入/导出 Profile、Workspace 和 Pack；
- 组件完整性检查、版本、发布者和更新回滚。

### 15.2 功能中心演进

现有功能中心继续作为“打开功能”的入口，但改为从 Module Registry 读取：

- 只显示当前 Profile 可用的工具；
- 停用模块的工具不允许通过旧 Pin 打开；
- 未满足依赖时显示阻塞原因；
- Pin 仍然只是快捷入口偏好，不代表模块启停；
- 模块贡献的新工具不要求修改 `openBuiltinTool()` 大型 switch；
- 工具打开动作由注册表中的稳定 action handler 路由。

### 15.3 装配方案可产生的首页示例

- 当前项目管理器：首页/项目标签和现有项目工作区；
- 媒体管理器：资料库、最近导入、待整理和归档状态；
- 通信端：最近消息、联系人和传输；
- 渲染控制端：队列、节点、资源装配和结果；
- 渲染节点：设备状态、当前任务、性能和日志。

这些首页也必须由装配贡献决定，不在 Shell 中按 Profile ID 写死。用户可以选择其中一个页面作为首页，也可以由组件包贡献新的首页。

项目管理器关闭后，项目根目录、创建/导入、最近项目、项目列表和项目标签都要撤下，并停止扫描及释放已打开项目资源；项目目录、最近记录和业务数据保留，重新启用后恢复。局域网、Python、任务、剪贴板、设置和其他无项目功能继续可用。默认迁移 Profile 仍选择项目主页，空白 Profile 使用最小安全主页，保证升级前体验不变且新装配可以完全脱离项目管理器。

## 16. 公开接口草案

前端核心类型：

```ts
type ModuleState =
  | 'disabled'
  | 'resolving'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'blocked'
  | 'error'
  | 'restart-required';

interface ModuleDefinition {
  id: string;
  name: string;
  version: string;
  scope: 'global' | 'project';
  requiresModules: ModuleDependency[];
  optionalModules: ModuleDependency[];
  capabilities: Capability[];
  contributions: ModuleContributions;
}

interface WorkspaceProfile {
  schemaVersion: 1;
  id: string;
  name: string;
  revision: number;
  enabledModules: ProfileModuleSelection[];
  moduleSettings: Record<string, unknown>;
  shellLayout: ProfileShellLayout;
  surfaces: ProfileSurface[];
  dataSources: ProfileDataSource[];
  commandBindings: ProfileCommandBinding[];
  workflowBindings: ProfileWorkflowBinding[];
  automationBindings: ProfileAutomationBinding[];
  variables: Record<string, string>;
}
```

建议的 Tauri 核心命令：

```text
list_modules()
get_module_state(moduleId)
enable_module(moduleId, scope?)
disable_module(moduleId, strategy)
get_module_dependencies(moduleId)
get_module_resources(moduleId)
list_workspace_profiles()
preview_profile_switch(profileId)
switch_workspace_profile(profileId, strategy)
export_workspace_profile(profileId, targetPath)
import_workspace_profile(sourcePath)
inspect_component_package(sourcePath)
install_component_package(sourcePath)
uninstall_component_package(componentId, retainData)
run_component_action(request)
cancel_component_operation(operationId)
resolve_file_intent(request)
execute_file_route(request)
list_file_handlers(request?)
get_file_association_snapshot(context)
set_file_association_binding(request)
get_file_route_diagnostics(routeId?)
```

所有命令返回稳定错误码，不让前端依赖中文错误文本判断状态。

## 17. 数据与配置位置

建议应用数据目录新增：

```text
modules/
  module-state.json
profiles/
  builtin-overrides.json
  user/*.json
components/
  installed/<component-id>/<version>/
  cache/
  staging/
component-data/<component-id>/
workflows/
trust/
  devices.db
  publishers.db
logs/components/
file-routing/
  global-bindings.json
  diagnostics.log
```

项目/资料库目录按需新增：

```text
.pm_center/
  profile.json              # 可选项目建议，不强制切换用户全局模式
  media_catalog.db          # 媒体资料库启用时
  workflows/                # 项目级工作流
  component-data/           # 项目级组件数据
  render_jobs/              # 继续保留现有渲染运行资料
```

已有 `data.db`、`tree_cache.db`、`scripts/`、`plugins/` 和 `thumbnails/` 的保护规则保持不变。

## 18. 兼容与迁移

### 18.1 默认兼容

- 升级时根据当前功能、设置和布局生成一份普通的“现有 Nexora 装配方案”，保证升级前后界面和功能一致；
- 现有功能中心 Pin 迁移到这份装配方案的布局偏好中；
- 现有插件继续由 `plugin.legacy-python` 模块加载；
- 现有插件 `plugin.json` 不需要立即升级；
- 现有 `.pm_center` 数据库和目录结构不迁移或删除；
- 现有渲染任务继续由 `render.batch` 使用；
- 现有局域网数据库继续由局域网模块使用；
- 现有项目会话恢复时，已停用模块的标签不恢复，并记录一次非破坏性提示。
- 现有图片、视频、文本、Markdown、目录和 Blender 页面先登记为 hosted FileHandler adapter，保持升级前双击行为；迁移完成后才允许卸载和第三方替换。
- 迁移后普通文件打开失败可以降级系统程序，但渲染预检、缩略图和结构化解析保持严格依赖语义。

### 18.2 插件系统迁移

当前插件 manifest 可映射为 Component Manifest 的兼容子集：

| 当前字段 | 下一代映射 |
| --- | --- |
| `id/name/version/apiVersion` | 原样保留 |
| `runtime: python` | `python-action` |
| `entry` | 组件入口 |
| `permissions` | 迁移为 Capability 请求，旧插件首次使用需审批 |
| `contributes.commands` | tool/workflow action |
| `toolbarActions` | 功能中心或工具动作 |
| `fileContextActions` | 文件上下文动作 |
| `settingsPanel` | 模块/组件设置贡献 |

设置贡献由宿主装配，不等于任意界面注入。标准组件使用版本化 schema 描述字段、校验、默认值、作用域和所需 Capability；只有宿主登记的内置实现可以引用受信 renderer ID。模块停用后，其设置导航与内容必须和工具、Surface 一样即时撤下。

设置系统分为两层：

- `core.recovery-settings` 永远可用，只负责 Profile、模块、组件、依赖诊断、恢复默认装配和退出；
- `builtin.settings-center` 负责普通设置的范围、导航、搜索与内容宿主，完成独立恢复入口后才允许被 Profile 停用。

范围选择和导航必须从当前可用 `settingsSections` 计算，不能继续维护硬编码的全局/项目列表。被选中的贡献撤下时自动回退到最近或首个可用贡献，不能留下空白页面。模块停用必须执行 `preview -> 应用内确认 -> execute`，取消为零副作用。完整迁移计划见 `NEXT_MAJOR_R7_SETTINGS_CONTRIBUTIONS.md`。

旧插件仍通过 JSON 文件和 stdout `@pmc` 消息运行。新组件优先使用统一双向协议。

### 18.3 代码迁移原则

1. 先建立 Module Manager 和 Capability Gateway，不先拆动所有领域实现。
2. 为现有领域增加 adapter，把旧命令接入模块守卫。
3. 逐步将 setup 中直接启动的服务迁入 lifecycle。
4. 前端工具和标签逐步改为 contribution registry，保持旧打开器兼容。
5. 数据迁移全部幂等，失败时旧版本数据仍可读取或回滚。

## 19. 分阶段实施路线

本节记录架构级阶段。具体开发顺序、前置依赖、每步交付物、自动/人工验收、退出门槛和产品确认模板统一见 `docs/NEXT_MAJOR_IMPLEMENTATION_ROADMAP.md`。实际开发以该执行路线为准，不允许跳过尚未 accepted 的前置里程碑。

### 阶段 0：设计冻结与测试基线

- 使用四个参考成品验证 Profile 是否能表达足够广泛的装配结果；
- 确认 Module/Component/Profile schema v1；
- 为启动、项目打开、局域网、渲染和插件建立回归测试；
- 记录现有后台线程、端口、进程、数据库和快捷键资源。

### 阶段 1：模块化宿主运行时

- 新增 `module_manager` 和 `capability_gateway`；
- 新增模块注册表、依赖解析、状态机和 ResourceRegistry；
- Tauri 命令增加模块守卫；
- 建立“隐藏/停用/卸载”区分；
- 接入应用退出和模块停用的统一清理。

验收：停用局域网后端口释放；停用剪贴板后原生线程和快捷键释放；停用渲染后不再调度新任务。

### 阶段 2：DIY Profile 与界面装配

- 实现空白装配空间以及 Profile 新建、复制、编辑、导入和导出；
- 将四种形态作为测试/示例方案验证，不在代码中注册为固定模式枚举；
- 实现装配切换预览；
- 功能中心、快捷栏、Shell 和工作区标签读取模块贡献；
- 实现 `.pmc-profile` 导入导出；
- 迁移生成的现有 Nexora 装配保持当前用户行为。

### 阶段 3：媒体资料管理器

- 建立 `media_catalog.db`；
- 完成引用/复制/移动收集；
- 完成虚拟集合、标签、备注、评分和归档历史；
- 完成重复检测、缩略图、视频封面和任务进度；
- 复用现有图片、视频和序列帧预览。

### 阶段 4：统一组件运行时

- 扩展 Python SDK 和组件 manifest；
- 增加 `python-worker` 与 `native-process`；
- 实现组件级权限、令牌、日志、取消和资源限制；
- 建立 `.pmc-pack` 安装、校验、签名和回滚；
- 实现隔离 DLL 组件宿主。

### 阶段 5：脚本自动化与装配空间

- 冻结旧 Workflow Manifest 为兼容合同，不建设 DAG 执行器；
- 实现脚本组件命令、Profile 绑定、事件和 cron 调度；
- 将任务中心作为统一执行观察面；
- 实现 `.pmc-workspace` 导入导出；
- 提供可选的渲染检查和媒体归档示例脚本组件，用户可修改或删除。

### 阶段 6：Blender 渲染农场

- 实现可信节点配对和能力报告；
- 实现场景资源 manifest 和 `.pmc-renderpack`；
- 复用目录/项目同步协议传输缺失资源；
- 实现远程帧领取、租约、结果回收和原子提交；
- 使用装配编辑器生成、导出并验证控制端和渲染节点参考方案，不增加固定 Profile 分支；
- 完成多机、断线、崩溃和源文件变化测试。

### 阶段 7：可选物理裁剪与分发

- 评估 Cargo feature、可选 sidecar 和按需组件下载；
- 评估可选装配方案/组件包目录和按需安装选择；
- 保持统一更新和数据兼容，不维护长期分叉产品。

### 阶段 8：R15 云端账户与组件商城

- 建立账户、设备会话、发布者身份和组件源；
- 建立商城目录、已购/已安装、更新渠道和版本历史；
- 复用 `.pmc-pack` 签名、依赖、Capability、原子安装和回退；
- 保持未登录、离线和服务端不可用时的完整本地能力。

### 阶段 9：R16 云设置与装配同步

- 同步非敏感软件偏好、Profile、布局和可移植 Workspace；
- 使用 revision、冲突副本、历史和恢复点，禁止静默覆盖；
- 绝对路径、密钥、项目、聊天、媒体和渲染结果保持本地；
- 团队装配库只分发配置和组件锁，组件包继续由 R15 组件源提供。

### 阶段 10：R17 公开宿主扩展 ABI

- 开放版本化 Hosted Surface、Project Context、Context Action 和组件状态接口；
- 允许组件贡献主页、导航、项目工作区、菜单和结构化 Widget/DataSource；
- 允许 Profile 选择一个外部 `project-manager` 提供者；
- 不开放内部 React、Store、TreeCache、Tauri command 或数据库表。

### 阶段 11：R18 可视化装配与受控节点编排

- 用画布组合模板插槽、Surface、导航、工具和数据源；
- 建立新的编排合同 v2，将 R11 自动化命令映射为类型化节点；
- 复用 Capability、幂等、取消、日志、attention 和版本快照；
- 输出普通 Profile/Workspace 和可审查执行计划，不生成隐藏宿主代码。

R15-R18 的完整合同与验收顺序见 `NEXT_MAJOR_R15_R18_CLOUD_ECOSYSTEM_AND_HOST_ABI.md`。

### 可选本机浏览器控制面

`builtin.local-web-console` 用于验证“完整桌面 Shell 之外的轻量 Surface”也能由模块生命周期装配。第一版固定监听 `127.0.0.1`，通过令牌和 HTTP API 白名单提供状态、部分设置、窗口显示/隐藏、重启和退出。它不等于完整 Web 版 Nexora，也不能直接开放局域网；详细边界见 `NEXT_MAJOR_R7_LOCAL_WEB_CONSOLE.md`。

## 20. 测试与验收基线

### 20.1 模块、Profile 与资源

- 依赖拓扑、循环依赖、冲突和可选依赖；
- 启动失败事务回滚；
- 停用过程中运行任务、未保存标签和端口占用；
- Profile 切换失败恢复旧模式；
- 重启后模块状态、布局和 Pin 一致；
- UDP/TCP 端口、watcher、Tokio 任务、原生线程、快捷键、SQLite 和子进程完全释放。

### 20.2 组件安全

- 未声明 Capability 的调用被拒绝；
- 路径越界、重解析点和绝对路径注入被拒绝；
- 组件签名、哈希、API 版本和平台不匹配；
- DLL 崩溃不影响 Nexora 主进程；
- 组件超时、取消、重复结果和宿主重启；
- 导出包不包含密码、密钥、聊天记录和本机绝对路径。

### 20.3 媒体资料库

- 引用、复制、移动及冲突处理；
- 重复哈希、同内容多位置和文件丢失；
- 集合、备注、标签和归档事件持久化；
- 大型目录取消和恢复；
- 原文件不被缩略图/解析操作修改；
- 数据库和派生缓存可独立备份与重建。

### 20.4 渲染农场

- 节点配对、撤销信任和错误密钥；
- Blender/插件/资源缺失预检；
- 多节点领取不重复帧；
- 控制端或节点崩溃后的租约恢复；
- 临时输出验证和原子提交；
- 网络中断、断点同步和结果重复上传；
- 源 `.blend` 变化不混用版本；
- 不允许远程任意脚本和越界输出。

### 20.5 工程验证

每个阶段至少运行：

```powershell
npm run build
cd src-tauri; cargo test --lib
cd src-tauri; cargo check
git diff --check
```

模块生命周期、Profile 切换、媒体导入、组件崩溃和真实 Blender 农场需要额外桌面端集成测试和 Windows 实机验证。

## 21. 首阶段明确不做

- 不立即维护四套独立安装包；
- R14 前不开发在线组件商城；商城固定进入 R15；
- R17 前不开放第三方宿主级页面、项目上下文和菜单 ABI；即使 R17 完成也不直接注入第三方 React；
- 不立即开放主进程内任意 DLL 加载；
- R18 前不建设复杂可视化编排；旧 Workflow 合同继续冻结且不执行；
- 不立即实现公网远程控制；
- 不把现有插件、渲染或局域网数据库一次性迁移到全新格式；
- 不因模块停用自动删除任何历史数据。

## 22. 仍需正式决策的事项

进入实现前需要形成 ADR 或在本文确定：

1. 下一超大版本的正式版本号和发布范围；
2. Module/Profile/Component manifest 的最终 schema；
3. 第三方组件签名和发布者信任机制；
4. 组件宿主使用 JSON-RPC、MessagePack 还是 Protobuf；
5. 媒体资料库数据库是否固定为独立文件；
6. Profile 切换中哪些模块允许热切换，哪些必须重启；
7. 渲染农场加密与设备配对协议；
8. 工作空间包是否允许嵌入第三方二进制组件；
9. 物理裁剪安装包采用 Cargo features、在线组件下载或两者结合；
10. R15 云端账户使用哪种认证、支付、部署区域、内容审核和退款供应商；
11. R17 Hosted Surface 使用独立 WebView、iframe 还是两级宿主，并如何分配内存与崩溃隔离；
12. R18 编排合同 v2 的图存储、类型系统、循环规则和调试协议。

## 23. 相关文档与代码入口

长期文档：

- `docs/PROJECT_CONTEXT.md`
- `docs/NEXT_MAJOR_R15_R18_CLOUD_ECOSYSTEM_AND_HOST_ABI.md`
- `docs/LAN_TRANSFER_ARCHITECTURE.md`
- `docs/PMC_SERVER_ARCHITECTURE.md`
- `examples/plugins/README.md`

前端入口：

- `src/features/builtinTools.ts`
- `src/stores/builtinToolsStore.ts`
- `src/stores/workspaceTabStore.ts`
- `src/stores/shellTabStore.ts`
- `src/components/BuiltinToolsCenter.tsx`
- `src/components/file-manager/index.tsx`
- `src/components/file-manager/ProjectWorkspace.tsx`
- `src/components/lan/LanCollaborationSurface.tsx`
- `src/components/file-manager/RenderCenterSurface.tsx`

后端入口：

- `src-tauri/src/lib.rs`
- `src-tauri/src/plugin/mod.rs`
- `src-tauri/src/task/mod.rs`
- `src-tauri/src/python/mod.rs`
- `src-tauri/src/python_env/mod.rs`
- `src-tauri/src/p2p/mod.rs`
- `src-tauri/src/p2p/transfer.rs`
- `src-tauri/src/p2p/server_client.rs`
- `src-tauri/src/render_center/mod.rs`
- `src-tauri/src/watcher/mod.rs`
- `src-tauri/src/cache_manager/mod.rs`
- `src-tauri/src/smart_clipboard/mod.rs`
- `src-tauri/resources/plugin-sdk/pmc_plugin/__init__.py`

后续任何改变模块边界、Profile 默认项、组件协议、包格式、权限模型、农场信任链或媒体数据结构的实现，都应同步更新本文件。
