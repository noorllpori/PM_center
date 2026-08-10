# Nexora 扩展接口参考

适用版本：Nexora `2.8.5`。本文是面向只持有 Nexora 安装版 EXE 的组件开发者的权威公开合同。Nexora 的 Rust 函数、Tauri command、React Store、DOM、CSS 类名、SQLite 表和内部目录结构均不是公开 API。

## 1. 接口状态

| 状态 | 含义 |
| --- | --- |
| `stable-2.8.5` | 已在 2.8.5 实现并纳入兼容检查，第三方可以依赖。 |
| `experimental` | 已实现但可能在同一主版本调整，必须提供降级行为。 |
| `planned-r14.5` | 合同已规划但当前安装版不保证可用。 |
| `reserved-r17` | 只保留方向和命名，禁止生成可执行依赖。 |

没有明确目标版本时，只能使用 `stable-2.8.5`。组件不能通过猜测内部命令来绕过状态限制。

## 2. 用户模型与兼容层

用户侧只有三个概念：组件、方案、项目。

- 组件：可安装、卸载、启停和依赖的功能单元，运行时可以是 Python、EXE、隔离 DLL、资料包或宿主服务。
- 方案：选择组件、界面、文件处理、自动化和组件设置的版本化装配文档。
- 项目：用户业务目录及其 `.pm_center` 管理数据。

旧方案中的 `enabledModules`、`moduleSettings` 和 Module Manifest 由 Nexora 内部适配器继续读取。它们是 `stable-2.8.5` 兼容输入，不是新第三方组件可使用的开发合同。新组件只使用 `component.json` 和 `enabledComponents`。

## 3. Component Manifest v1

状态：`stable-2.8.5`。根文件名固定为 `component.json`，`schemaVersion` 和 `apiVersion` 当前均为 `1`。

```json
{
  "schemaVersion": 1,
  "id": "com.example.project-audit",
  "name": "项目检查",
  "description": "读取项目文件并生成检查结果。",
  "version": "1.0.0",
  "apiVersion": "1",
  "runtime": "python-action",
  "role": "feature",
  "category": "automation",
  "tags": ["audit", "project"],
  "distribution": "local",
  "uiMode": "headless",
  "platforms": ["windows-x64"],
  "entry": "main.py",
  "capabilities": ["project.files.read"],
  "requiresComponents": [],
  "optionalComponents": [],
  "contributes": {
    "automationCommands": []
  },
  "resources": {
    "maxMemoryMb": 256,
    "maxParallelism": 1,
    "timeoutMs": 120000
  }
}
```

核心字段：

| 字段 | 状态 | 规则 |
| --- | --- | --- |
| `id`、`version`、`apiVersion` | `stable-2.8.5` | ID 必须稳定；版本和依赖使用 SemVer。 |
| `runtime`、`entry`、`platforms` | `stable-2.8.5` | 入口只能是组件包内相对路径。 |
| `role`、`distribution`、`uiMode` | `stable-2.8.5` | 只描述用途，不授予权限。 |
| `category` | `stable-2.8.5` | `workspace`、`file-handler`、`service`、`automation`、`appearance`、`integration`、`data`。 |
| `tags` | `stable-2.8.5` | 最多 32 个稳定标签，用于检索与展示。 |
| `capabilities` | `stable-2.8.5` | 声明可能请求的权限；运行时仍需授权。 |
| `requiresComponents` | `stable-2.8.5` | 缺失或版本不兼容时组件被阻止。 |
| `optionalComponents` | `stable-2.8.5` | 可选依赖缺失时组件必须自行降级。 |
| `contributes` | `stable-2.8.5` | 公开命令、页面、文件处理器、设置和模板。 |

组件分类只影响展示，不改变 Capability、生命周期或卸载规则。

## 4. 组件命令与 Component Bridge v1

状态：`stable-2.8.5`。`automationCommands` 同时是自动化命令和公开组件命令目录，不存在第二套 `exports` 协议。

```json
{
  "id": "com.example.audit.inspect",
  "command": "inspect",
  "name": "检查文件",
  "contextRequirement": "project-required",
  "executionSemantics": "pure",
  "requiredCapabilities": ["project.files.read"],
  "capabilityOperation": "read",
  "inputSchema": {"type": "object"},
  "outputSchema": {"type": "object"},
  "maxAttempts": 1,
  "maxParallelism": 1,
  "timeoutMs": 30000
}
```

调用方使用：

```python
result = call_component(
    "pmc.blendio",
    "inspect",
    {"path": resolved_path},
    capability="project.files.read",
    timeout_ms=120000,
)
```

`call_component(..., capability="...")` 用于单项权限；跨空间调用可使用 `capabilities=["...", "..."]`。调用方必须传入目标本次操作实际需要的完整集合，宿主会检查每一项是否同时由调用方、目标组件和本次运行批准。

宿主依次验证：

1. 目标在调用方 `requiresComponents` 或 `optionalComponents` 中声明。
2. 依赖已安装、SemVer 兼容并进入当前方案有效闭包。
3. 目标 Manifest 的 `automationCommands` 公开了该 `command`。
4. 当前运行获得目标命令要求的 Capability。
5. 项目上下文、超时、取消和调用链合法。

`requiredCapability` 是兼容字段，等价于单元素 `requiredCapabilities`。跨项目/外部空间的命令可声明多项 Capability；运行时只会针对本次源、目标与操作实际需要的范围申请授权，调用方和目标组件都必须在 Manifest 中声明它们。

自调用、未声明依赖、失效依赖和递归调用会被拒绝。通用组件发布/订阅总线是 `reserved-r17`；R14.5 继续使用宿主事件与自动化绑定。

### 4.1 统一错误

状态：`stable-2.8.5`。

```json
{
  "code": "dependency_not_active",
  "message": "依赖组件未进入当前方案有效闭包",
  "details": {"componentId": "pmc.blendio"},
  "retryable": false
}
```

Python SDK 抛出 `NexoraBridgeError`，字段为 `code`、`details` 和 `retryable`。调用方必须把错误当作业务结果处理，不能假设依赖永远存在。

## 5. BlenderIO 公共命令

状态：`stable-2.8.5`。

- 组件 ID：`pmc.blendio`
- 命令 ID：`pmc.blendio.inspect`
- 运行命令：`inspect`
- 依赖版本建议：`^1.0`
- Capability：`project.files.read`
- 输入：`{"path":"C:\\absolute\\project\\file.blend"}`
- 输出：结构化 JSON 对象；字段随 BlendIO 1.x 向后兼容扩展，调用方应忽略未知字段。

项目内文件应先通过 `resolve_project_file()` 得到受控绝对路径。外部文件不是项目接口的一部分，需要单独的外部文件 Capability 和用户选择流程。

## 6. 文件操作公共服务

状态：`stable-2.8.5`。

- 组件 ID：`nexora.file-operations`
- 依赖版本建议：`^1.0`
- 不依赖 BlenderIO。需要解析 `.blend` 的组件应同时声明 `pmc.blendio`。
- 所有调用都使用 `call_component()`；调用方不得读取项目数据库、TreeCache 或拼接 Nexora 内部目录。

### 6.1 路径与授权

```python
from nexora_sdk import call_file_operation, external_location, project_location

project_file = project_location("assets/a.png")
external_file = external_location(r"D:\\Media\\a.png", grant_id)
```

`project` 路径必须为相对路径，默认使用运行、页面或事件携带的项目。多项目组件可以通过 `project.describe` 获取稳定 `projectId`，再在 `project_location(..., project_id=...)` 中指定一个仍处于打开状态的项目。

`external` 路径必须同时提供同一组件获得的 `grantId`。`external.select` 创建单次授权；`external.grant-directory` 创建 `once`、`session` 或 `persistent` 目录授权；`external.list-grants` 与 `external.revoke-grant` 管理当前组件自己的授权。授权属于设备级数据，不随 Profile、项目或 `.pmc-workspace` 导出。`access` 只能为 `read`、`write` 或 `read-write`。

`external.select` 会打开 Windows 目录选择器，并强制签发 `once` 授权；`external.grant-directory` 也会打开选择器，并按请求创建 `once`、`session` 或 `persistent` 授权。组件不能通过提交一个任意 `rootPath` 绕过选择器。每个 `grantId` 只能由取得它的组件使用，撤销和过期后所有对应位置立即失效。

### 6.2 命令目录

| 命令 | 上下文 | Capability | 输出 |
| --- | --- | --- | --- |
| `project.describe` | 项目 | `project.files.read` | 项目 ID、名称和当前根目录快照。 |
| `directory.list`、`entry.stat`、`entry.exists`、`entry.search` | 项目或外部 | 对应读取 Capability | 分页条目、元数据、存在状态或搜索结果。 |
| `file.read`、`file.hash` | 项目或外部 | 对应读取 Capability | UTF-8/Base64 数据或 BLAKE3 摘要。 |
| `file.write`、`directory.create` | 项目或外部 | 对应写入 Capability | 写入/目录的元数据。 |
| `entry.copy`、`entry.move`、`entry.rename`、`entry.delete`、`batch.execute` | 项目、外部或跨空间 | 源读取及目标写入 | 完成项与逐项结果。 |
| `stream.open-read`、`stream.read`、`stream.open-write`、`stream.write`、`stream.commit`、`stream.abort` | 项目或外部 | 对应读写 Capability | 绑定组件、授权与调用链的短期流句柄。 |
| `external.import`、`external.export` | 项目与外部 | 项目和外部的对应读写 | 复制后的目标元数据。 |
| `cache.status`、`cache.query`、`cache.invalidate`、`cache.refresh-directory`、`cache.rebuild-project`、`watcher.status` | 项目 | `cache.inspect` 或 `cache.maintain` | 缓存快照、维护结果或监听状态。 |

`file.write` 会先写入同目录临时文件再替换目标；`expectedHash` 可拒绝并发覆盖。所有变更命令均支持冲突策略 `error`、`overwrite`、`rename`、`skip`。默认删除请求回收站，只有 `permanent=true` 才会永久删除。项目和外部空间的移动跨卷时按复制、校验、删除执行。

```python
page = call_file_operation(
    "directory.list",
    {"location": project_location("assets"), "source": "auto", "limit": 100},
    capability="project.files.read",
)
```

项目导出到已授权外部目录的最小调用：

```python
result = call_file_operation(
    "external.export",
    {
        "source": project_location("deliver/final.mp4"),
        "target": external_location(r"D:\\Exports\\final.mp4", grant_id),
        "conflict": "rename",
    },
    capabilities=["project.files.read", "filesystem.external.write"],
)
```

## 7. Python SDK v1

状态：`stable-2.8.5`。SDK 位于 Nexora 随组件运行时提供的 `nexora_sdk` 包中。

### 6.1 基础运行

```python
request = read_request()
log("starting")
progress(0.5, "half way")
raise_if_cancelled(request.context)
write_result({"ok": True})
```

| 函数 | 状态 | 返回/作用 |
| --- | --- | --- |
| `read_request()` | `stable-2.8.5` | 返回命令、输入和不可变运行上下文。 |
| `log(message)` | `stable-2.8.5` | 写入任务中心结构化日志。 |
| `progress(value, message)` | `stable-2.8.5` | 上报 0 到 1 的进度。 |
| `is_cancelled(context)`、`raise_if_cancelled(context)` | `stable-2.8.5` | 响应取消。 |
| `write_result(value)`、`write_error(message)` | `stable-2.8.5` | 输出最终协议结果。 |
| `call_component(...)` | `stable-2.8.5` | 调用已声明依赖的公开命令。 |
| `project_location(...)`、`external_location(...)`、`call_file_operation(...)` | `stable-2.8.5` | 创建结构化文件位置并调用文件操作服务。 |
| `emit_surface_event(...)` | `stable-2.8.5` | 向组件自己的沙箱页面发送受控事件。 |

### 6.2 项目接口

| 函数 | Capability | 状态 |
| --- | --- | --- |
| `get_project_context()` | 项目读类 Capability | `stable-2.8.5` |
| `list_project_files(relative_path="", cursor=0, limit=100)` | `project.files.read` | `stable-2.8.5` |
| `stat_project_file(relative_path)` | `project.files.read` | `stable-2.8.5` |
| `resolve_project_file(relative_path, access="read")` | `project.files.read/write` | `stable-2.8.5` |
| `mutate_project_files(mutations)` | `project.files.write` | `stable-2.8.5` |
| `get_project_metadata(key, default=None)` | `project.metadata.read` | `stable-2.8.5` |
| `set_project_metadata(key, value)` | `project.metadata.write` | `stable-2.8.5` |

文件列表每页最多 500 项。相对路径不得是绝对路径、包含 `..` 或进入 `.pm_center`。批量变更最多 100 项，支持：

```json
{"kind":"create-directory","path":"output"}
{"kind":"copy","path":"a.txt","destination":"backup/a.txt","overwrite":false}
{"kind":"move","path":"a.txt","destination":"archive/a.txt","overwrite":false}
{"kind":"rename","path":"a.txt","destination":"b.txt","overwrite":false}
{"kind":"delete","path":"tmp.txt"}
```

写操作由 Nexora 负责冲突检查、Watcher 通知、TreeCache 失效和审计。组件禁止直接连接项目数据库或修改 TreeCache。

### 6.3 状态与 Blob

| 函数 | Capability | 状态 |
| --- | --- | --- |
| `get_state`、`set_state`、`delete_state` | 组件自身状态 | `stable-2.8.5` |
| `put_blob(name, data, scope, kind)` | `project.storage.write` | `stable-2.8.5` |
| `open_blob(name, scope, kind)` | `project.storage.read` | `stable-2.8.5` |
| `list_blobs(scope, kind)` | `project.storage.read` | `stable-2.8.5` |
| `delete_blob(name, scope, kind)` | `project.storage.write` | `stable-2.8.5` |
| `get_storage_directory(scope, kind)` | `project.storage.direct` | `stable-2.8.5` |

`scope` 为 `global` 或 `project`，`kind` 为 `state` 或 `cache`。Bridge Blob 单次写入上限 512 KiB。`open_blob()` 返回包含 Base64 数据和元数据的对象。

- `state`：停用、删除组件安装副本和普通缓存清理时保留；删除持久状态需要单独确认。
- `cache`：可被缓存中心删除，组件必须能够重建。
- `direct`：只返回本组件自己的专属目录，禁止拼接或探测其他组件目录。

项目存储当前物理上位于 `.pm_center/components/<component-id>/`，但该路径不是公开合同；只通过 SDK 获取句柄或目录。

## 8. Capability

状态：全部 `stable-2.8.5`。Manifest 只能声明实际使用的最小集合。

```text
app.profile.read              app.profile.write
app.settings.read             app.settings.write
notification.send             clipboard.read
clipboard.write               filesystem.dialog.open
filesystem.external.read      filesystem.external.write
project.open                  project.files.read
project.files.write           project.metadata.read
project.metadata.write        project.storage.read
project.storage.write         project.storage.direct
cache.inspect                 cache.maintain
task.run                      task.cancel
python.execute                python.packages.manage
process.spawn                 network.http.request
network.lan.discover          network.lan.message
network.lan.transfer          network.server.connect
render.inspect                render.queue.read
render.queue.write            render.worker.execute
render.result.commit
```

Python 是受信任代码模型。Capability 约束 Nexora Bridge 与宿主接口，不承诺限制 Python 标准库在当前 Windows 用户权限下能做的事情。生产环境只能运行可信签名包；开发目录必须由用户显式信任。

批准方式为 `allowOnce`、`allowSession` 和 `allowAlways`。多 Capability 命令会在同一自动化运行中按需逐项等待批准；任一项被拒绝则整个运行不执行。

## 9. 页面、文件处理和设置贡献

| Contribution | 状态 | 边界 |
| --- | --- | --- |
| `scriptSurfaces` | `stable-2.8.5` | 沙箱 HTML/CSS/JS；只能调用 `allowedCommands`。 |
| `fileHandlers` | `stable-2.8.5` | 声明扩展名、MIME、意图、优先级和工作区目标。 |
| `settingsSections` | `stable-2.8.5` | 由 Nexora 按 Schema 渲染；停用后表单撤下，数据保留。 |
| `uiExtensionPoints`、`uiExtensions`、`uiExtensionBindings` | `stable-2.8.5` | 目标组件发布命名插槽/整页替换点；扩展必须声明目标依赖与自身隔离页面，Profile 决定启用和顺序。 |
| `shellTemplates`、`pageTemplates`、`themePresets` | `stable-2.8.5` | 静态模板，经净化和恢复 Shell 承载。 |
| `toolActions` | `experimental` | 可登记和诊断，不保证任意宿主工具栏位置。 |
| `widgets`、`dataSources` | `experimental` | 无第三方 React ABI。 |
| `workflowNodes`、`workflowBindings` | 兼容冻结 | 可往返保存，但不执行。 |
| Hosted Surface、项目管理器级 ABI | `reserved-r17` | 当前不得生成或伪造。 |

Script Surface 使用 CSP 和会话 nonce，不能访问宿主 DOM、Tauri IPC、本机 URL 或任意远程脚本。插槽页面会收到受控 `ui-extension-context` 事件（项目 ID、项目路径、相对选择、主题、语言和尺寸）；页面崩溃只撤下自身，整页替换会回退默认项目工作区。

首批稳定项目管理器插槽：`nexora.project-manager.project-toolbar`、`project-sidebar`、`file-details`、`file-context-menu`、`project-home-widgets`、`project-status-bar` 和 `project-workspace`。最后一个是 `surface` 类型，且只接受一项 `mode: "replace"` 绑定；其他插槽按 Profile `order` 排列。

## 10. 开发目录与安全重载

状态：`experimental`。

受信任开发目录出现后，顶部显示 `DEV` 按钮。单击扫描并重载发生变化的组件；菜单可重新扫描、重载全部、打开日志和进入开发者工作台。

重载会拒绝有不可安全取消的非幂等运行或活动原生操作的组件，并返回运行/操作 ID。安装采用暂存与备份替换，校验或安装失败时保留旧版本。Python Worker、EXE 和隔离 DLL 由运行时重新创建，贡献目录重新同步。

## 11. 包、签名和方案

| 文件 | 状态 | 内容 |
| --- | --- | --- |
| `.pmc-pack` | `stable-2.8.5` | 单个签名组件包。 |
| `.pmc-profile` | `stable-2.8.5` | 方案逻辑，不携带组件本体、凭据和绝对路径。 |
| `.pmc-workspace` | `stable-2.8.5` | 方案、界面骨架和可选可分发组件原包。 |

正式可执行组件需要受信任发布者签名。使用安装版开发者工作台创建模板、校验、信任开发目录、调试、生成密钥并打包；不要手工伪造 ZIP 或包头。

## 12. 兼容与禁止事项

- 未安装或未进入当前方案有效闭包的组件不贡献功能，也不能被调用。
- 禁用只撤下运行时、页面和贡献，保留组件安装文件、方案引用、`state` 与 `cache`，之后可直接重新启用。
- 随安装包组件只能禁用，不能从 MSI 安装内容中物理删除；本地目录和 `.pmc-pack` 组件可以执行“删除”，移除 Nexora 安装副本及缓存归档。
- 删除第三方组件会从所有方案中原子清理组件选择、页面、Pin、模板、自动化和文件处理绑定；任一步失败时恢复原方案，不能留下半失效 Profile。
- 删除组件默认仍保留组件 `state`；清理缓存只删除 `cache`。删除持久状态必须使用单独的显式确认流程。重新安装同一组件不会自动恢复已清理的方案入口，需要用户重新加入当前方案。
- 新组件不得写 `enabledModules`、`moduleSettings` 或 Module Manifest。
- 不得调用任意 Tauri command、React Store、内部事件名、数据库表或 TreeCache。
- 不得依赖 `.pm_center`、应用数据目录和安装目录的物理结构。
- 不得把 `planned-r14.5` 或 `reserved-r17` 写成已实现能力。

端到端样例：

- `examples/script-project-storage`：项目资料、JSON 状态、Blob 和专属目录。
- `examples/script-automation-blend-audit`：声明依赖并调用 `pmc.blendio/inspect`。
