# 公开接口参考

状态：Nexora 2.8.5 开发线。本文只列可以让外部组件或装配空间依赖的接口；Rust 内部函数、Tauri command、React Store 和 SQLite 表不是第三方兼容合同。

## 1. 合同版本

| 合同 | 稳定文件 | 用途 |
| --- | --- | --- |
| Component Manifest v1 | `shared/pmc-platform/schemas/v1/component-manifest.schema.json` | `component.json` 的结构、运行时、依赖、贡献与资源限制。 |
| Workspace Profile v1 | `shared/pmc-platform/schemas/v1/workspace-profile.schema.json` | 装配方案的 Module/Component、Surface、导航、工具、设置和自动化绑定。 |
| Workspace Package v1 | `shared/pmc-platform/schemas/v1/workspace-package.schema.json` | `.pmc-workspace` 的页面骨架与 Profile。 |
| Package Header v1 | `shared/pmc-platform/schemas/v1/package-header.schema.json` | `.pmc-profile`、`.pmc-workspace`、`.pmc-pack`、`.pmc-renderpack` 的 ZIP 包头。 |
| Presentation Template v1 | `shared/pmc-platform/schemas/v1/presentation-template.schema.json` | 安全 Shell/Page 模板描述。 |
| Python SDK v1 | `src-tauri/resources/script-sdk/nexora_sdk/__init__.py` | 脚本输入、进度、取消、依赖调用、组件状态与 Surface 事件。 |

`schemaVersion: 1` 与 `apiVersion: "1"` 是当前兼容边界。新字段必须向后兼容，破坏性变化必须新建版本，而不是复用 v1 字段含义。

## 2. Component Manifest 核心字段

| 字段 | 作用 |
| --- | --- |
| `id`、`name`、`version`、`apiVersion` | 稳定组件身份和 SemVer 兼容性。 |
| `runtime`、`entry`、`platforms` | 执行方式、包内相对入口和支持平台。 |
| `role`、`distribution`、`uiMode` | 组件用途/来源/界面模式描述，不授予权限。 |
| `capabilities` | 可能使用的 Capability 声明；实际调用仍需要授权。 |
| `requiresComponents`、`optionalComponents` | 组件依赖图，禁止环和自依赖。 |
| `contributes` | 自动化、脚本页面、文件处理器、设置、模板等声明。 |
| `resources` | `maxMemoryMb`、`maxParallelism`、`timeoutMs`。 |
| `publisher` | 显示性发布者信息；包签名的发布者身份位于包头。 |

允许的 Capability 名称由 `shared/pmc-platform/src/capability.rs` 的 `Capability::ALL` 定义，例如 `project.files.read`、`project.files.write`、`filesystem.external.read`、`network.http.request`、`process.spawn` 和 `render.queue.write`。选择最小集合。

## 3. Contribution 支持矩阵

| `contributes` 项 | 状态 | 正确用途 |
| --- | --- | --- |
| `automationCommands` | 公开可用 | 用于手动、事件和 cron 运行；Python SDK/Task Center 可追踪。 |
| `automationEvents` | 公开可用 | 只声明组件允许绑定的白名单事件。 |
| `scriptSurfaces` | 公开可用，当前入口固定 | 沙箱 HTML/CSS/JS 页面，从功能中心打开为对话框或在工作台预览；`placements` 不会自动生成任意 Shell/工作区/Widget 宿主。 |
| `fileHandlers` | 公开可用 | 让文件意图路由器选择处理器，支持扩展名、MIME、意图、优先级和目标。 |
| `settingsSections` | 公开可用 | 结构化设置字段，由 Nexora 统一渲染与保存。 |
| `shellTemplates`、`pageTemplates`、`themePresets` | 公开可用 | 静态资源模板，经净化、预览和恢复 Shell 处理。 |
| `toolActions` | 可声明/诊断 | 不等于自动获得任意宿主工具栏按钮；使用 Script Surface 或由未来受控宿主入口承载。 |
| `widgets`、`dataSources` | 可保留引用 | 当前没有第三方 React 渲染器 ABI，不能作为独立 UI 扩展承诺。 |
| `workflowNodes` | 冻结兼容 | 可往返保存，但不执行、不提供 DAG 画布。 |

## 4. Python SDK v1

脚本应首先调用 `read_request()`，它返回命令、用户输入和不可变执行上下文。SDK 当前提供：

| 函数 | 作用 |
| --- | --- |
| `read_request()` | 读取一次 JSON 请求和 `Context`。 |
| `log(message)` | 写入结构化运行日志。 |
| `progress(value, message)` | 上报 0 到 1 的进度。 |
| `is_cancelled(context)` / `raise_if_cancelled(context)` | 检查宿主取消标志。 |
| `write_result(value)` / `write_error(message)` | 输出最终协议结果。 |
| `call_component(component_id, command, payload, ...)` | 调用已声明依赖的组件命令。 |
| `get_state` / `set_state` / `delete_state` | 读写组件自己的 `global` 或 `project` 状态。 |
| `emit_surface_event(surface_id, event, payload)` | 向自身已声明 Script Surface 发送受控事件。 |

SDK 桥仅监听本机回环地址，且每次运行使用随机 nonce。依赖调用仍需有效 Profile 闭包、Manifest 依赖和本次已授权 Capability；SDK 不提供任意 Tauri IPC。

## 5. 当前随附组件可依赖项

| 组件 ID | 可公开依赖的用途 | 当前公开边界 |
| --- | --- | --- |
| `pmc.blendio` | Blender 文件结构读取 | 在 Manifest 中声明 `requiresComponents` 后，可通过 SDK 调用 `inspect`，输入为 `{"path":"..."}`；示例见 `examples/script-automation-blend-audit/`。其他 BlenderIO 命令在形成独立 Schema 前不应被第三方包视为稳定接口。 |
| `nexora.blender.workspace` | `.blend` 内部文件工作区 | 文件路由宿主功能，不是给第三方脚本直接调用的服务。 |
| `nexora.file-handler.image`、`video`、`text`、`directory` | 图片、视频、文本和目录的内置处理器 | 它们提供宿主页面；第三方若要替换，应声明自己的 `fileHandlers`，不能调用内置 React 页面。 |
| `nexora.presentation.templates` | 内置 Shell、页面与主题模板 | 可被 Profile 引用；模板 ID 与版本受组件依赖/卸载检查保护。 |

随附组件可卸载，不代表会永远存在。依赖组件必须处理“未安装、版本不兼容或被当前 Profile 排除”的状态。

## 6. 可执行组件行协议

`python-action`、`python-worker` 和 `native-process` 使用 JSON。请求至少包含：

```json
{
  "protocol": "nexora.component.v1",
  "operationId": "uuid",
  "componentId": "com.example.asset-audit",
  "componentVersion": "0.1.0",
  "command": "scan",
  "input": {},
  "cancellationFile": "..."
}
```

最终响应：

```json
{"ok": true, "result": {}}
```

或：

```json
{"ok": false, "error": "readable failure"}
```

常驻 Worker 在启动时先输出 `{"type":"worker-ready"}`，之后逐行接收请求。长任务可输出 `{"type":"progress","operationId":"...","progress":0.5,"message":"..."}`、`heartbeat` 或 `log`；最终结果必须关联当前 `operationId`。

## 7. 包与导入接口

| 扩展名 | 内容 | 重要限制 |
| --- | --- | --- |
| `.pmc-pack` | 一个可安装组件的签名 ZIP | 可执行包需可信发布者；保留原始验证归档后才能被自包含空间重新分发。 |
| `.pmc-profile` | 可移植 Profile | 不携带组件文件、绝对路径、凭据或用户业务数据。 |
| `.pmc-workspace` | Profile、变量、Surface 骨架，且可选嵌入组件包 | 导入为新 Profile，不自动切换；不携带项目源文件或聊天资料。 |
| `.pmc-renderpack` | 受控 Blender 渲染资源包 | 农场实际传输仍属于 R13 后续实现。 |

所有包使用 `PMC_PACKAGE`、Package Header、BLAKE3 和严格 ZIP 条目检查。不要自行构造 ZIP；用工作台或 `nexora-component-pack` 生成 `.pmc-pack`，用 Nexora 的导出功能生成 Profile/装配空间。

## 8. 非 API 清单

以下内容即使当前可从源码调用，也不能被第三方包依赖：

- `src-tauri/src/platform/*` 的 Rust 实现与 Tauri command 名；
- React 组件、Zustand Store、CSS class、前端 `contributionRegistry.ts` 私有对象；
- 应用数据内部路径、SQLite 表、运行时状态 JSON；
- 内置 Component/Module 的未文档化命令和 `extensions` 中的私有字段；
- 当前局域网聊天协议与未来农场传输的内部帧格式。

需要新能力时，应先提出 Manifest/SDK/Schema 合同，再实现宿主支持；不能用“临时直接调用”变成事实 API。
