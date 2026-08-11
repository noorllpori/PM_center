# Nexora 二次开发总览

> 适用版本：Nexora 2.8.5 开发线  
> 交付模型：闭源 Nexora 安装版 EXE + 第三方自有组件目录/签名 `.pmc-pack`  
> 目标：帮助开发者选择正确扩展方式，并从空目录完成开发、调试、装配、签名和交付

## 1. 公共模型只有三个概念

普通用户和第三方开发者只需要理解：

1. **组件**：提供服务、命令、页面、文件处理器、自动化、模板或资料。
2. **装配方案**：选择启用哪些组件，以及主页、界面模板、插槽、导航、快捷栏、文件处理和自动化绑定。
3. **项目**：用户的业务目录和项目上下文；组件通过公开接口访问，不直接连接 Nexora 数据库或 TreeCache。

历史 `Module` 字段只由 Nexora 内部兼容层读取。新第三方开发不得创建 Module Manifest、`enabledModules` 或 `moduleSettings`。

## 2. 二次开发能力地图

```text
组件目录 / .pmc-pack
  |
  +-- component.json: ID、版本、依赖、Capability、贡献
  |
  +-- 运行时
  |     Python action / Python worker
  |     Native process / Native library
  |     Data pack
  |
  +-- 公共能力
        Component Bridge
        项目上下文与组件存储
        文件操作服务
        自动化与任务中心
        Script Surface
        文件处理器
        界面模板与 UI 扩展点
```

Nexora 本体源码、内部 Rust/React 类型、Tauri command、数据库表和 Store 不属于二次开发 SDK。外部组件通过安装版提供的 Manifest、Schema、`nexora_sdk` 和宿主协议工作。

## 3. 先选择运行时

| 目标 | 运行时 | 适合情况 | 主要限制 |
| --- | --- | --- | --- |
| 一次性规则、批处理、解析 | `python-action` | 开发最快，推荐默认 | Python 是受信任代码，不是 OS 沙箱 |
| 长驻 Python 服务 | `python-worker` | 保持模型、索引或会话 | 必须处理取消、重启和状态恢复 |
| 包装现有 CLI/高性能 EXE | `native-process` | 跨语言、进程隔离 | 需要实现结构化行协议和退出清理 |
| 高性能 DLL | `native-library` | 极低开销的二进制核心 | Windows/架构/ABI 成本最高，只能在隔离宿主加载 |
| 模板、主题、静态资料 | `data-pack` | 不执行第三方代码，风险最低 | 不能申请 Capability，不能运行脚本 |

优先顺序通常是 `data-pack -> python-action -> python-worker/native-process -> native-library`。只有真实性能数据证明需要 DLL 时才选择 `native-library`。

## 4. 组件清单是唯一入口

`component.json` 声明：

- 稳定 `id`、SemVer `version`、`apiVersion`；
- 运行时、入口、平台和资源限制；
- 必需/可选组件依赖及版本范围；
- 可能申请的 Capability；
- 对宿主公开的命令、页面、文件处理器、设置区、自动化事件、模板和扩展点。

组件只声明能力不等于获得权限。运行时还会检查当前方案有效闭包、用户授权、调用链、项目上下文和实际操作范围。

权威字段与示例见 [INTERFACE_REFERENCE.md](INTERFACE_REFERENCE.md) 和 [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md)。

## 5. 组件依赖与 Component Bridge

组件通过稳定 ID 声明依赖：

```json
{
  "requiresComponents": [
    { "id": "nexora.file-operations", "versionRequirement": "^1.0.0" }
  ],
  "optionalComponents": [
    { "id": "pmc.blendio", "versionRequirement": "^1.0.0" }
  ]
}
```

目标组件使用 `automationCommands` 公开组件命令。调用方通过 `nexora_sdk.call_component()` 调用，不能猜测 Tauri 命令名：

```python
from nexora_sdk import call_component

result = call_component(
    "pmc.blendio",
    "inspect",
    {"location": {"space": "project", "path": "scene/main.blend"}},
    capability="project.files.read",
)
```

宿主负责：

- 依赖是否已安装、版本兼容并进入当前方案有效闭包；
- 目标命令是否公开；
- Capability 是否由调用方和目标同时声明并获批；
- `operationId`、超时、取消、调用深度和递归保护；
- 项目上下文、日志、错误结构和进程崩溃隔离。

依赖缺失时组件应显示可读诊断或降级，不应白屏。

## 6. Capability 权限

常见权限族：

| 权限 | 典型操作 |
| --- | --- |
| `project.files.read/write` | 读取或修改项目内文件 |
| `filesystem.external.read/write` | 访问用户明确授权的项目外目录 |
| `filesystem.dialog.open` | 打开文件/目录选择器，不代表拥有读取权限 |
| `project.metadata.read/write` | 读写公开项目元数据 |
| `project.storage.read/write/direct` | 组件自己的项目状态、Blob、缓存或专属目录 |
| `cache.inspect/maintain` | 查询、失效或重建目录缓存 |
| 网络、进程、通知等 | 只按 Manifest 和当前操作范围授权 |

授权可按一次、会话或持久范围保存。外部路径授权绑定组件和根目录，不能跨组件复用，也不能用 `..`、符号链接或 Junction 越界。

Python/EXE 是受信任代码模型。Capability 约束 Nexora Bridge，不声称能阻止受信任进程直接使用当前 Windows 用户权限。因此签名、发布者信任和开发目录信任是安全边界的一部分。

## 7. 文件操作组件

公共文件服务 ID 为 `nexora.file-operations`。其他组件不直接操作 TreeCache、Watcher 或项目数据库，而是使用结构化位置：

```json
{"space":"project","projectId":"optional-project-id","path":"assets/a.png"}
```

```json
{"space":"external","grantId":"grant-id","path":"D:\\Media\\a.png"}
```

公开能力包括：目录列举和搜索、文件读写和哈希、创建/复制/移动/重命名/删除、批处理、流式大文件、外部授权、项目导入导出、缓存状态/刷新/重建和 Watcher 状态。

项目写操作由宿主统一处理冲突、临时文件、原子替换、Watcher、TreeCache 失效和审计。组件不能伪造项目根目录，也不能访问 `.pm_center` 内部文件。

## 8. 项目上下文与组件存储

组件可以获得公开项目快照、活动选择、分页文件列表和元数据。项目级业务数据应写入组件自己的存储命名空间：

```text
组件状态
  global/state     需要保留的全局 JSON/Blob
  global/cache     可清理的全局缓存
  project/state    需要随项目保留和备份的数据
  project/cache    可重建的项目缓存
```

默认使用 `get_state()`、`set_state()`、`put_blob()`、`open_blob()` 等 SDK。只有确实需要数据库或大型索引文件时才申请 `project.storage.direct`，并通过宿主获取本组件专属目录。不得自行拼接 `.pm_center/components/<other-id>`。

停用、卸载和清理缓存默认不删除 `state`。删除持久数据必须是单独、明确的用户操作。

## 9. 自动化组件

组件通过 `automationCommands` 声明可运行命令，通过 Profile `automationBindings` 决定触发规则。支持：

- 手动执行；
- 五段式 cron；
- `app.started`；
- 项目打开/关闭和文件变化；
- 局域网文件接收；
- 渲染批次/作业完成；
- 任务完成/失败。

每次运行保存组件版本、包摘要、Profile revision、项目上下文、输入快照、尝试、日志和结果。只有声明幂等且允许重试的运行可以自动恢复；非幂等运行结果未知时进入 `attention`。

开发时必须覆盖取消、超时、并发上限、权限等待、Profile 切换、组件停用和进程树清理。

## 10. UI 开发分三种

### 10.1 Script Surface

适合完整交互页面。组件可携带 HTML/CSS/JavaScript，但运行在隔离页面中：

- 不能访问宿主 DOM、Tauri API、本机任意 URL；
- 只能通过 nonce 和白名单消息桥调用自身 `allowedCommands`；
- 文件、网络和进程操作转交组件命令，再经过 Capability Gateway；
- Surface 崩溃只撤下自身，不应拖垮 Nexora 主界面。

`placements` 可声明 `shell`、`workspace`、`dialog`、`widget`、`independent-window`。当前安装版已经支持 Shell 主页、模板 `component-surface` 和对话框等路径，但完整项目管理器级 Hosted Surface ABI 仍需按接口状态使用。

### 10.2 UI 扩展点

目标组件可以公开命名插槽或整页替换点。扩展组件声明目标依赖、自己的 Surface 和插入/替换方式，由 Profile 决定启用和排序。

扩展只能插入目标明确开放的位置。它不能任意修改任何组件 DOM。目标组件停用、扩展崩溃或版本不兼容时，宿主撤下扩展并保留默认界面。

### 10.3 界面模板

界面模板是 `data-pack`，只组织工具带以下的静态结构和类型化插槽。完整流程见 [INTERFACE_TEMPLATE_DEVELOPMENT_GUIDE.md](INTERFACE_TEMPLATE_DEVELOPMENT_GUIDE.md)。

模板与组件页面是互补关系：模板决定“放在哪里”，Surface 决定“显示和执行什么”。

## 11. 文件处理器

组件通过 `fileHandlers` 声明扩展名、MIME、文件类型、意图、优先级和工作区目标。当前装配方案决定哪些处理器有效。

典型意图：打开、预览、编辑、检查。没有匹配的 Nexora 处理器时，普通打开回退系统程序。文件夹工作区、文本编辑器、图像预览、视频播放器和 Blender 工作区都可以由不同组件提供并替换。

文件管理器负责路由和授权，实际处理器负责页面或命令。不要在普通右键菜单里硬编码另一个组件 ID。

## 12. 设置与装配方案

组件可声明由宿主渲染的 `settingsSections`。设置值属于当前组件和声明的作用域。组件停用后设置 UI 撤下，数据保留。

一个 Profile 原子保存：

- 启用组件和版本要求；
- 启动主页、导航、快捷栏；
- 界面模板、工具带模式和插槽绑定；
- 文件处理器选择；
- 自动化绑定；
- 组件设置和 UI 扩展绑定。

只有“另存为”创建副本。编辑当前方案并“保存并应用”应更新原方案；取消或关闭不改变运行状态。

## 13. 开发目录、安装副本和正式包

| 形态 | 用途 | 是否可直接编辑 |
| --- | --- | --- |
| 开发目录 | 开发者自己的源码和资源 | 是，使用 VS Code |
| Nexora 安装副本 | 运行时校验后复制的组件 | 否，不作为源码目录 |
| `.pmc-pack` | 已签名正式分发包 | 否，通过升级发布新版本 |

推荐循环：

```text
创建/选择开发目录
  -> 校验 Manifest 和文件
  -> 明确信任开发目录
  -> 安装或 DEV 重载
  -> 加入当前方案
  -> 调试命令、页面和权限
  -> 生成 Ed25519 签名 .pmc-pack
  -> 干净环境安装验收
```

保存开发文件不会自动替换运行副本。显式重载会先停止新调用、取消安全操作、释放页面/进程、重新校验并替换安装副本；失败时保留旧版本。

## 14. 分发文件

| 文件 | 解决的问题 | 携带组件 |
| --- | --- | --- |
| `.pmc-pack` | 安装单个组件 | 是 |
| `.pmc-profile` | 分享组件选择和装配逻辑 | 否，只记录依赖 |
| `.pmc-workspace` | 分享可复现装配空间 | `references-only` 不携带；`self-contained` 可携带允许再分发的签名包 |
| 项目目录 | 分享业务文件和 `.pm_center` 数据 | 不应混入组件私钥或开发目录 |

正式发布至少附带：组件 ID/版本、目标 Nexora/API 版本、平台、依赖、Capability、许可证、发布者、公钥/签名信息、升级与回退说明。

## 15. 接口状态和 AI 生成规则

| 状态 | 使用规则 |
| --- | --- |
| `stable-2.8.5` | 第三方可在目标 2.8.5 上依赖 |
| `experimental` | 已实现但可能调整，必须提供降级行为 |
| `planned-r14.5` | 旧规划标签，不应当作当前可执行保证 |
| `reserved-r17` | 只保留方向和命名，不能生成运行依赖 |

开发者 AI 默认只能使用 `stable-2.8.5`。AI 不得伪造 Tauri、React Store、数据库、内部事件或宿主 DOM 接口。规范化生成合同见 [AI_COMPONENT_AUTHORING_REFERENCE.md](AI_COMPONENT_AUTHORING_REFERENCE.md)。

## 16. 从零完成一个扩展

1. 写清目标：服务、自动化、页面、文件处理器还是模板。
2. 选择最低成本运行时。
3. 分配稳定组件 ID、命令 ID、Surface ID 和版本。
4. 只声明实际依赖和最小 Capability。
5. 在安装版开发者工作台创建或选择开发目录。
6. 用 VS Code 编写清单、代码、页面和 README。
7. 校验、信任、安装/重载并加入测试 Profile。
8. 测试无项目/有项目、依赖缺失、权限拒绝、取消、超时和卸载。
9. 生成签名 `.pmc-pack`，保留包摘要和发布说明。
10. 在第二台或干净用户环境验收后再交付 Profile/Workspace。

## 17. 当前不能做什么

- 直接加载第三方 React 组件到 Nexora 主进程。
- 读取内部 Store、数据库表、TreeCache 对象或任意 Tauri command。
- 让普通模板 HTML 运行 JavaScript、显示任意交互控件或访问远程资源。
- 在没有目标扩展点的情况下任意修改另一个组件 UI。
- 把 Python/EXE/DLL 描述成完整 OS 沙箱。
- 把 Profile 当组件包，或把 Workspace 当项目归档。
- 在 R17 完整 Hosted Surface/项目 Provider ABI 尚未稳定前完整复刻所有本体项目管理器内部能力。

## 18. 权威文档入口

| 文档 | 用途 |
| --- | --- |
| [INTERFACE_REFERENCE.md](INTERFACE_REFERENCE.md) | Manifest、SDK、Capability、Bridge 和接口状态 |
| [AI_COMPONENT_AUTHORING_REFERENCE.md](AI_COMPONENT_AUTHORING_REFERENCE.md) | 给开发者 AI 的硬性生成规范 |
| [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md) | 组件目录、运行时、打包和签名 |
| [WORKSPACE_AUTHORING.md](WORKSPACE_AUTHORING.md) | Profile/Workspace 制作和交付 |
| [INTERFACE_TEMPLATE_DEVELOPMENT_GUIDE.md](INTERFACE_TEMPLATE_DEVELOPMENT_GUIDE.md) | 界面模板、插槽和开发重载 |
| [BUILTIN_COMPONENT_CATALOG.html](BUILTIN_COMPONENT_CATALOG.html) | 内置组件 ID、依赖、命令和能力目录 |

## 19. 发布验收

- Manifest/Schema/版本和依赖校验通过。
- 未声明依赖的组件命令无法调用。
- 允许一次、允许会话、始终允许和拒绝权限均有预期行为。
- 项目路径、外部授权、路径穿越和跨组件存储被正确限制。
- 取消、超时、并发、崩溃、Profile 切换和卸载没有残留进程或句柄。
- Surface 在窄窗口、浅色、深色和缩放下无白屏、重叠或文本溢出。
- 缺失组件、缺失模板和页面崩溃有稳定回退。
- 停用不删除状态；缓存清理不删除持久数据。
- 签名包可在干净环境安装、升级、回退、禁用、删除和重装。
- 文档明确列出已知限制，不把实验或规划接口写成稳定能力。
