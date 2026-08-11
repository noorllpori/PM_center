> **历史归档（2026-08-12）**：本文件仅保留为实现与验收追溯资料，不代表当前公开能力或后续开发依据。请先阅读 docs/README.md。
# Nexora 文件意图路由与可替换文件处理器

> 状态：`verifying`（R10-0 至 R10-4 和 R10-P1/P2 已实现，正在做完整集成验收）
>
> 里程碑：R10-0 至 R10-4
>
> 日期：2026-08-06
>
> 前置：R3 CapabilityGateway、R5 Contribution Registry、R9 组件运行时

当前实现范围：组件清单支持 `fileHandlers` 贡献和 `fileKinds`（`file`/`directory`）；文件双击、右键打开和目录进入先经过 Tauri `route_file_intent`。图片、视频、文本、目录和 Blender 工作区均由可卸载的 Nexora 组件贡献，组件缺失时普通 `open` 降级系统程序，`inspect` 等严格意图不会伪装成功。`pmc.blendio` 只提供无头 BlendIO 服务，`nexora.blender.workspace` 单独提供内部 Blender 标签页且依赖 BlendIO。组件运行时支持 `.pmc-pack` ZIP 安全检查、暂存解压、BLAKE3 完整性摘要、Ed25519 发布者签名和本机发布者信任库；可执行组件包必须签名且发布者已被显式信任。`native-library` 先检查平台与 PE 架构，再由隔离 `pmc-component-host` 执行 ABI 握手和 JSON 调用，DLL 不进入 Nexora 主进程。BlenderIO 的当前随附版本由独立 `pmc-blendio-service` 进程执行，替换为同 ID 的签名组件包不会改变调用方。

## 1. 目标

把当前散落在文件列表、工作区标签、详情面板和右键菜单中的扩展名判断，收敛为统一的 `FileIntentRouter`。图片、视频、文本、Markdown、Blender、PDF 等查看、编辑、预览和解析能力都由可安装组件贡献，用户可以卸载 Nexora 自带实现并绑定第三方实现。

本设计解决以下固定问题：

- 卸载 BlenderIO 或 Blender 工作区后，双击 `.blend` 不创建失效标签页，而是继续降级到系统默认程序；
- “系统打开”和“Nexora 内部打开”是不同意图，不再由某个页面临时判断；
- 文件管理器不拥有图片、文本或 Blender 的具体实现，只负责发起打开请求和展示候选菜单；
- 同一格式可以同时安装多个处理器，用户按全局、Profile 或项目选择默认项；
- 组件卸载、停用、版本不兼容或依赖缺失时，路由结果立即更新；
- 路由诊断可以解释最终选择、跳过原因和系统降级原因，便于后期开发调试。

## 2. 固定架构边界

### 2.1 `FileIntentRouter` 属于平台核心

`FileIntentRouter` 是不可撤下的薄路由服务，属于 Nexora 平台合同和 Contribution Registry，不属于项目文件管理器模块。

原因：

- 项目文件管理器可以被 Profile 停用，但聊天附件、最近文件、搜索结果、媒体资料库和工作流仍可能需要打开文件；
- 如果后缀绑定由文件管理器持久化，关闭文件管理器后其他模块会失去相同语义；
- 路由器只解析候选和绑定，不读取大文件、不创建业务页面、不持有项目数据库，因此不会演变成新的业务模块。

文件管理器负责：

- 双击、Enter、右键菜单和“打开方式”设置入口；
- 把 `filePath`、项目上下文和用户意图提交给路由器；
- 根据路由结果调用 Shell 打开 Surface 或调用系统程序；
- 展示诊断和错误，不在自身代码中硬编码格式组件。

路由器负责：

- 读取当前有效组件与 `FileHandlerContribution`；
- 合并项目、Profile 和全局绑定；
- 检查组件状态、依赖、平台、版本、Capability 和候选匹配；
- 返回确定性路由计划；
- 在执行成功前不创建标签页或窗口；
- 记录稳定诊断轨迹。

### 2.2 服务组件与界面处理器分离

无头解析服务和文件页面不是同一个组件角色。

以 Blender 为例：

```text
pmc.blendio
  角色：无头文件服务
  能力：inspect / preview / external-data / render-settings
  不创建页面

nexora.file-handler.blender-workspace
  角色：文件处理器与 Surface 提供者
  能力：打开 .blend 工作区、详情、右键动作
  依赖：pmc.blendio
```

这样渲染中心可以只依赖 `pmc.blendio`，不被迫启用 Blender 文件页面；用户也可以替换 Blender 工作区 UI，而继续使用同一个解析服务。

### 2.3 内置组件没有不可替换特权

Nexora 自带文件处理器使用统一组件模型：

- 显示名称以 `Nexora` 开头；
- 稳定 ID 使用 `nexora.*` 命名空间；
- 可以卸载、重装、升级或被第三方实现替换；
- 仍通过 CapabilityGateway 获取文件读取、文件写入、进程启动等权限；
- `bundled` 只表示安装器初始携带，不表示不可卸载。

已有稳定 ID `pmc.blendio` 不强制改名，避免破坏现有 Profile 和任务记录；其显示名称改为“Nexora BlenderIO 文件服务”。新组件使用 `nexora.*`。

第三方组件必须使用自己的发布者命名空间，不能伪装成 `nexora.*`。

## 3. 文件意图

第一版固定意图：

```ts
type FileIntent =
  | 'open'
  | 'open-internal'
  | 'open-system'
  | 'preview'
  | 'edit'
  | 'inspect'
  | 'thumbnail'
  | 'extract-metadata';
```

语义：

| 意图 | 语义 | 无内部处理器时 |
| --- | --- | --- |
| `open` | 用户默认打开行为 | 降级到系统程序 |
| `open-internal` | 明确要求 Nexora 内部页面 | 返回无可用处理器，不调用系统 |
| `open-system` | 明确绕过 Nexora | 直接调用系统默认程序 |
| `preview` | 轻量只读预览 | 可回退通用详情，不启动系统程序 |
| `edit` | 可修改内容的编辑器 | 默认不降级，除非用户明确选择系统编辑 |
| `inspect` | 结构化解析 | 必须返回结构化结果或稳定错误 |
| `thumbnail` | 生成缩略图 | 返回无处理器，不弹窗口 |
| `extract-metadata` | 提取元数据 | 返回无处理器，不弹窗口 |

交互式 `open` 可以系统降级；渲染预检、缩略图、工作流节点和结构化解析不能把“系统打开”当成功结果。

## 4. FileHandler Contribution 合同

R10-0 在组件贡献中增加可选 `fileHandlers`：

```ts
interface FileHandlerContributionV1 {
  id: string;
  name: string;
  description?: string;
  matches: FileHandlerMatchV1;
  actions: FileHandlerActionV1[];
  priority?: number;
  supportsMultiple?: boolean;
  requiresComponents?: ComponentDependency[];
}

interface FileHandlerMatchV1 {
  extensions?: string[];
  mimeTypes?: string[];
  fileKinds?: Array<'file' | 'directory'>;
  probeCommand?: string;
}

interface FileHandlerActionV1 {
  intent: FileIntent;
  kind: 'surface' | 'component-command' | 'external-program';
  target: string;
  openMode?: 'workspace-tab' | 'independent-window' | 'dialog' | 'headless';
  readOnly?: boolean;
}
```

约束：

- 扩展名使用不带点的小写值，例如 `txt`、`blend`；
- 扩展名只是候选条件，不能替代 MIME、签名或组件探测；
- `probeCommand` 必须是同一组件登记的只读命令，并受超时和内存限制；
- `surface` 目标必须属于当前有效 Contribution Registry；
- `component-command` 必须属于声明该处理器的组件；
- `external-program` 仍需进程 Capability，不能携带任意 Shell 字符串；
- 一个处理器可支持多个意图，不要求为预览、编辑、详情重复安装组件；
- 处理器 ID 全局稳定，用户绑定保存处理器 ID，不保存前端组件名或函数名。

示例：

```json
{
  "id": "nexora.file-handler.text",
  "name": "Nexora 文本编辑器",
  "matches": {
    "extensions": ["txt", "log", "json", "xml", "yaml", "yml"]
  },
  "actions": [
    {
      "intent": "open",
      "kind": "surface",
      "target": "nexora.surface.text-editor",
      "openMode": "workspace-tab"
    },
    {
      "intent": "edit",
      "kind": "surface",
      "target": "nexora.surface.text-editor",
      "openMode": "workspace-tab"
    }
  ],
  "priority": 100
}
```

## 5. 文件绑定模型

### 5.1 绑定作用域

```ts
interface FileAssociationBindingV1 {
  id: string;
  scope: 'global' | 'profile' | 'project';
  match: {
    extension?: string;
    mimeType?: string;
  };
  intent: FileIntent;
  handler: string | 'system';
  behavior?: 'fallback' | 'strict';
}
```

优先级从高到低：

```text
当前项目绑定
  -> 当前 Profile 绑定
  -> 软件全局绑定
  -> 可用处理器优先级
  -> 系统默认程序
```

绑定规则：

- 项目绑定只影响该项目，不写入可分享项目文件，除非用户明确导出项目建议；
- Profile 绑定只保存处理器稳定 ID，不保存本机程序路径；
- 全局绑定属于本机用户偏好；
- 处理器卸载后不删除绑定，状态显示为“目标缺失”，重装兼容组件后自动恢复；
- 默认 `behavior=fallback`，缺失时继续选择其他处理器或系统程序；
- `strict` 只用于用户明确要求固定内部处理器或工作流合同，不适用于普通双击默认值；
- 系统程序路径由 Windows 关联或本机工具别名解决，不进入可分享 Profile。

### 5.2 数据位置

```text
应用数据目录/
  file-routing/
    global-bindings.json
    diagnostics.log

  profiles/local-bindings/<profile-id>.json
    # Profile 对本机处理器、外部程序或工具别名的解析结果

项目 .pm_center/
  file-associations.json
    # 可选项目级覆盖；只保存逻辑处理器 ID，不保存其他设备绝对路径
```

如果不希望项目目录产生新的配置文件，第一版项目绑定可以继续存入项目 `data.db`；两种实现只能选择一种，不能同时维护两份真相。

## 6. 确定性解析算法

路由输入：

```ts
interface ResolveFileIntentRequest {
  path: string;
  intent: FileIntent;
  projectPath?: string;
  profileId: string;
  preferredHandlerId?: string;
}
```

解析步骤固定为：

1. 规范化路径并确认文件或目录存在；
2. 获取扩展名、基础 MIME 和轻量文件签名；
3. 读取当前 Profile 的有效组件闭包；
4. 从 Contribution Registry 收集支持该意图的处理器；
5. 删除未安装、停用、被依赖阻止、平台不兼容或版本不兼容的候选；
6. 应用项目、Profile 和全局绑定；
7. 对需要探测的候选执行有界 `probeCommand`；
8. 按显式绑定、匹配精度、用户优先级、组件优先级和稳定 ID 排序；
9. 返回一个路由计划，不立即创建 UI；
10. Shell 执行计划，目标确认接受后才创建标签页或窗口；
11. 执行失败且允许 fallback 时继续下一个候选；
12. 普通 `open` 最终可以返回 `system`，其他意图返回稳定失败。

禁止行为：

- 先创建工作区标签，再检查组件是否存在；
- `.blend` 解析失败后把“启动 Blender”伪装成解析成功；
- 图片查看器失败后自动用文本编辑器打开二进制数据；
- 前端根据中文错误文本选择 fallback；
- 多个页面各自维护扩展名 `switch`。

## 7. 路由计划与执行结果

```ts
interface FileRoutePlan {
  routeId: string;
  path: string;
  intent: FileIntent;
  selected: FileRouteCandidate | null;
  candidates: FileRouteCandidate[];
  fallback: 'system' | 'none';
  trace: FileRouteTraceEntry[];
}

interface FileRouteCandidate {
  handlerId: string;
  componentId: string;
  actionKind: 'surface' | 'component-command' | 'external-program';
  target: string;
  openMode?: string;
  score: number;
}

interface FileRouteExecutionResult {
  routeId: string;
  status: 'opened' | 'handled' | 'system-opened' | 'cancelled' | 'failed';
  handlerId?: string;
  surfaceId?: string;
  errorCode?: string;
}
```

`routeId` 用于把一次双击、候选解析、Surface 创建和组件操作串进同一诊断链。标签页会话只在执行成功后记录。

## 8. 双击和右键菜单

### 8.1 双击

- 双击发起 `intent=open`；
- 有有效默认内部处理器时打开对应 Surface；
- 内部处理器缺失或执行失败且允许 fallback 时调用 Windows 默认程序；
- 没有任何系统关联时显示“选择打开方式”，不创建空标签；
- 双击目录继续由目录工作区处理器接管，也必须经过同一路由合同。

### 8.2 右键菜单

固定结构：

```text
打开
使用 Nexora 打开 >
  Nexora 图片查看器
  第三方图片查看器
使用系统打开
设置此格式的默认打开方式...
```

规则：

- “打开”执行当前默认绑定；
- “使用 Nexora 打开”只列出当前有效内部候选；没有候选时整个菜单项隐藏；
- “使用系统打开”永远绕过内部处理器；
- 设置默认打开方式打开文件关联管理弹窗，不直接修改 Windows 系统关联；
- 组件可以贡献与格式相关的附加动作，例如“读取 Blender 外部依赖”，但必须与通用打开命令分组；
- 菜单在每次打开时从路由快照生成，不能缓存已经卸载的组件入口。

## 9. 组件卸载与生命周期

卸载处理器组件前显示影响：

- 当前绑定到该处理器的后缀；
- 当前打开且由其拥有的页面；
- 未保存编辑内容；
- 依赖它的模块、组件、工作流和 Profile；
- 卸载后的默认 fallback。

执行规则：

- 有未保存编辑内容时阻止卸载，先保存、丢弃或取消；
- 只读页面可以在确认后关闭；
- 组件从 Registry 撤下后，新请求不能再选择它；
- 已开始的无头操作按组件运行时取消或等待策略处理；
- 未来双击自动重新解析，不保留失效缓存；
- 不把已经打开的文件自动交给系统程序，避免突然启动外部应用；
- 绑定记录保留为缺失状态，重装同一稳定 ID 后恢复；
- 卸载服务组件会让依赖它的 UI 处理器变为不可用，但不会删除 UI 组件数据。

## 10. 内置文件组件拆分

建议组件：

| 组件 | 主要格式 | 意图 | 依赖 |
| --- | --- | --- | --- |
| `nexora.file-handler.directory` | 目录 | open、inspect | 项目资源可选 |
| `nexora.file-handler.image` | PNG/JPEG/WebP/GIF/TIFF 等 | open、preview、thumbnail | 图像解码服务 |
| `nexora.file-handler.video` | MP4/MOV/WebM 等 | open、preview、thumbnail、metadata | FFprobe 可选 |
| `nexora.file-handler.text` | TXT/LOG/JSON/XML/YAML 等 | open、edit、preview | 无 |
| `nexora.file-handler.markdown` | MD | open、edit、preview | 文本组件可选 |
| `nexora.file-handler.blender-workspace` | BLEND | open、inspect、preview | `pmc.blendio` |
| `pmc.blendio` | BLEND | inspect、thumbnail、metadata | 无头服务 |

迁移期可以先把现有 React 页面登记为 `hosted` Surface adapter，再逐步拆成真正可安装包，不要求一次重写所有查看器。

文本组件替换示例：

```text
卸载 Nexora 文本编辑器
  -> .txt 的 nexora.file-handler.text 候选消失
  -> 用户绑定显示缺失
  -> 普通双击回退 Windows 默认编辑器
  -> 安装 vendor.text-pro 后可绑定 .txt
  -> Nexora 文件管理器代码不变化
```

## 11. BlenderIO 外部包验收

Blender 外置分两包：

```text
Nexora-BlenderIO.pmc-pack
  无头 native-process 服务
  提供解析和受控写入

Nexora-Blender-Workspace.pmc-pack
  提供 FileHandler、Surface、右键动作和 HelpAssistant
  requiresComponents: pmc.blendio ^1.0
```

最终验收：

1. 安装一个没有 BlenderIO 的 Nexora；
2. 双击 `.blend`，使用 Windows 默认 Blender 打开，不出现 Nexora 空标签；
3. “使用 Nexora 打开”中不显示 Blender 工作区；
4. 安装 BlenderIO 包但不安装工作区包，渲染预检可使用解析服务，双击仍系统打开；
5. 安装 Blender 工作区包后，内部打开候选出现；
6. 绑定 `.blend` 到 Nexora Blender 工作区后，双击打开内部标签；
7. 卸载 BlenderIO，工作区候选进入依赖缺失并从菜单隐藏，双击回退系统；
8. 重装兼容 BlenderIO 后原绑定自动恢复；
9. 安装第三方 Blender 工作区后可以切换绑定，不影响渲染中心对解析服务的依赖；
10. 损坏包、错误架构、签名不可信或版本不兼容时不改变当前可用处理器。

## 12. `.pmc-pack` 与 `.pmc-workspace`

`.pmc-profile` 只保存逻辑绑定和组件依赖，不包含组件文件。

`.pmc-workspace` 的 `references-only` 与 `self-contained` 均已可用。后者使用下列包目录约定；导入端会在用户确认后先完成全部签名、依赖、平台和冲突预检，再将缺失组件作为一个导入操作安装，任何常规失败都会回滚本次新增组件及新 Profile：

```text
manifest.json
workspace.json
dependency-lock.json
packages/
  Nexora-BlenderIO.pmc-pack
  Nexora-Blender-Workspace.pmc-pack
  vendor-text-editor.pmc-pack
```

导出模式：

- `references-only`：只保存依赖 ID 和版本，体积小；
- `self-contained`：携带用户有权重新分发的组件包、模板和资料包；
- 自包含导出只嵌入“曾以已验证 `.pmc-pack` 安装、已缓存原归档、声明允许再分发”的组件。随 Nexora 安装但没有独立归档、由目录安装、来源或信任状态失效、或明确不可分发的组件只写引用，并在导出预览列出原因；导出器不会从已安装目录重新拼装包，以免丢失原发布者签名；
- `dependency-lock.json` 固定组件 ID、版本要求、已解析版本、嵌入路径和未嵌入原因。导入时会拒绝锁文件与 `packages/` 不一致、重复包、路径穿越、加密或符号链接条目、超限压缩内容、摘要/签名/平台不符和依赖图无效的归档；
- 导入预览统一展示每个依赖的嵌入状态、发布者、许可证、版本和阻塞原因。有效但尚未受信任的签名发布者必须逐个明确勾选信任；未签名或签名无效的可执行包始终拒绝；
- 若本机已有同 ID 但版本不同的组件，导入会停止，不会用空间内版本静默替换。版本相同则复用本机组件；仅缺失组件会被安装；
- 用户确认前不安装任何包、不切换 Profile、不修改绑定；
- 任一必需包安装、组件目录同步、Profile 写入或本机映射写入失败时，回滚本次新装组件和 Profile，不能留下半装配状态。异常断电时最多留下未被 Profile 引用的已安装组件，绝不会切换到半失效 Profile；恢复设置可安全卸载该组件。

组件开发者可用仓库内的确定性签名工具生成可分发归档：

```powershell
# 生成一次并妥善保管的 Ed25519 私钥；不要提交到源码或包内
npm run component:keygen -- D:\Keys\nexora-publisher.json

# 打包包含 component.json 的组件目录
npm run component:pack -- D:\MyComponent D:\Release\my-component.pmc-pack --key D:\Keys\nexora-publisher.json --publisher-id com.example --publisher-name "Example Studio" --license MIT
```

打包器按稳定路径顺序写入 ZIP，生成 BLAKE3 内容摘要和 Ed25519 包头签名。它不会把私钥或原始来源目录写进归档。首次安装来自该发布者的可执行包时，用户仍须在 Nexora 中查看并确认该发布者。

## 13. 公共接口草案

Tauri 命令：

```text
resolve_file_intent(request) -> FileRoutePlan
execute_file_route(request) -> FileRouteExecutionResult
list_file_handlers(request?) -> FileHandlerDescriptor[]
get_file_association_snapshot(context) -> FileAssociationSnapshot
set_file_association_binding(request) -> FileAssociationSnapshot
remove_file_association_binding(bindingId) -> FileAssociationSnapshot
get_file_route_diagnostics(routeId?) -> FileRouteDiagnostic[]
```

事件：

```text
nexora:file-handlers-changed
nexora:file-associations-changed
nexora:file-route-completed
```

稳定错误码：

```text
FILE_NOT_FOUND
FILE_HANDLER_NOT_FOUND
FILE_HANDLER_UNAVAILABLE
FILE_HANDLER_DEPENDENCY_MISSING
FILE_HANDLER_VERSION_MISMATCH
FILE_HANDLER_PROBE_FAILED
FILE_HANDLER_REJECTED_FILE
FILE_HANDLER_OPEN_FAILED
FILE_SYSTEM_ASSOCIATION_MISSING
FILE_ROUTE_CANCELLED
FILE_ROUTE_STALE
```

## 14. 安全边界

- 扩展名不可信，处理器必须验证实际格式；
- 文件读取、写入、外部进程和项目外路径继续经过 CapabilityGateway；
- 路由器不把任意路径作为组件命令参数放行，先做规范化和 scope 检查；
- 预览和探测默认只读，有严格超时、内存和输出大小限制；
- 编辑器写入使用临时文件、格式验证和原子替换；
- 外部程序参数使用数组，不经过 Shell 拼接；
- 第三方处理器不能注册 `nexora.*` ID；
- 组件卸载不能删除用户文件，组件数据清理是独立确认操作；
- `.pmc-workspace` 携带包时必须复用 R10 签名、哈希、架构、路径穿越和压缩炸弹检查。

## 15. 文件关联管理与诊断 UI

“文件关联”设置页建议放在可停用的文件体验/文件管理器设置贡献中，但数据和解析命令由核心路由提供。文件管理器关闭后，恢复设置中的只读诊断仍可查看和重置错误绑定。

页面包含：

- 按扩展名搜索；
- 当前全局、Profile 和项目绑定；
- 可用和不可用候选；
- 默认系统程序状态；
- 组件、版本、发布者和支持意图；
- “恢复自动选择”“使用系统程序”和选择指定处理器；
- 当前路由诊断。

单次诊断显示：

```text
路径：D:/Project/test.blend
意图：open
项目绑定：无
Profile 绑定：nexora.file-handler.blender-workspace
候选 1：Blender 工作区 -> 跳过，缺少 pmc.blendio
候选 2：无
最终：Windows 系统默认程序
```

调试要求：

- 每次路由有 `routeId`；
- trace 使用稳定阶段和错误码；
- 记录候选 ID，不记录文件内容；
- 日志路径和保留周期可诊断；
- 前端不能只显示“打开失败”，必须能展开实际跳过原因。

## 16. 实施顺序

### R10-0：合同和路由骨架

- 增加 `FileIntent`、`FileHandlerContributionV1`、绑定和诊断合同；
- 建立 `FileIntentRouter` 与确定性候选解析；
- 建立路由诊断页；
- 现有文件双击和右键先经过路由器，但继续调用现有内置页面 adapter；
- 修正“先创建标签再检查组件”的路径。

退出门槛：现有图片、视频、文本、Markdown、目录和 Blender 打开行为无回归；卸载 BlenderIO 后 `.blend` 双击系统打开且不创建标签。

### R10-1：安全 `.pmc-pack` 安装器

- 包索引、BLAKE3、签名、发布者、许可证和架构检查；
- staging、原子安装、升级、回滚和卸载；
- 先用 `data-pack`、`python-action`、`python-worker` 和 `native-process` 测试；
- `.pmc-workspace` 增加依赖锁和可选自包含包目录。

已实现：ZIP 路径/符号链接/加密/大小限制，原子 staging/替换/回滚，`contentDigest`（除 `manifest.json` 外所有内容的 BLAKE3），以及 Ed25519 发布者签名。包头扩展固定为：

```json
{
  "contentDigest": "<blake3 of non-manifest entries>",
  "publisher": {"id":"vendor.example","displayName":"Example","publicKey":"<base64 32-byte key>"},
  "license": "MIT",
  "signature": {"algorithm":"ed25519","value":"<base64 64-byte signature>"}
}
```

签名材料是 `nexora.component-pack.v1\0`、移除 `signature` 字段后的确定性包头 JSON、零字节和 `contentDigest`。`native-process`、`native-library`、Python 运行时等含可执行代码的归档包必须签名且发布者位于本机信任库；无代码 `data-pack` 可以经明确确认以完整性模式安装。信任库位于组件运行时根目录 `trusted-publishers.json`，仅保存发布者 ID、显示名、公钥和信任时间，不保存私钥。DLL 安装期读取 PE 机器类型，拒绝本机架构不匹配的二进制。

退出门槛：损坏包、路径穿越、压缩炸弹、错误架构、内容摘要不符和不可信签名不改变已安装状态。

当前完成：`.pmc-workspace` 已支持 `references-only` 和 `self-contained`。自包含包使用 `dependency-lock.json`、嵌入原始已验证归档和导入期统一预览；缺失组件安装、Profile 写入和本机映射写入任一常规失败都会回滚。`nexora-component-pack` 负责可重复的 Ed25519 签名包生成，避免手工构造 ZIP 造成摘要或签名不一致。

### R10-2：隔离原生宿主

- `pmc-component-host.exe`、命名管道/本地 RPC、ABI 协商；
- DLL 每组件或每信任组隔离；
- 崩溃、死循环、内存增长和宿主重启诊断；
- DLL 不能进入 Nexora 主进程。

当前完成：`pmc-component-host` 独立 Cargo binary；Windows `LoadLibraryW`、`nexora_component_abi_v1` 握手、`nexora_component_invoke_v1` JSON ABI、请求/响应大小限制、崩溃/超时错误码和发布资源自动准备。正式打包前由 `prepare:component-host` 编译并放入 Tauri resources；开发环境优先使用环境变量、应用旁路或 `target/debug` 宿主。调用导出约定为 `extern "system" fn(requestPtr, requestLen, responsePtr, responseCapacity) -> i64`，返回写入响应缓冲区的 JSON 字节数，负数表示组件错误。仓库提供 `examples/native-library-echo`，开发/构建前自动生成 DLL；在组件运行时页面安装目录后可以直接点“健康调用”验证整个隔离链路。

### R10-3：BlenderIO 与 Blender 工作区外置

- 把宿主内 BlendIO adapter 替换为独立签名 `native-process` 包；
- 把 Blender 页面、菜单和 Surface 登记为独立文件处理器组件；
- 完成第 11 节的无包、安装、卸载、重装和第三方替换验收。

当前完成：BlendIO 服务和 Blender 工作区已拆为两个组件；工作区默认不自动安装，可从“可重新安装/安装器随附”列表显式安装。`pmc.blendio` 目前由随安装包的 `pmc-blendio-service` 独立进程执行，不再在 Nexora 主进程通过 Rust adapter 直接读取 `.blend`。只安装 BlendIO 时解析/渲染预检仍可用，但 `.blend` 普通打开回退系统；安装工作区且 BlendIO 可用时才创建内部 Blender 标签。后续外部发布包以同 ID 接管该进程入口。

### R10-4：内置查看器组件化

- 图片、视频、文本、Markdown 和目录处理器全部登记贡献；
- 允许卸载和第三方替换；
- Profile 编辑器增加逻辑文件绑定；
- 完成系统 fallback、会话恢复、未保存编辑和窄窗口验收。

当前完成：图片、视频、文本和目录组件已登记并参与路由；目录处理器使用 `fileKinds: ["directory"]`，双击和右键均通过同一入口。卸载后保留绑定但候选变为缺失，普通打开回退系统，重装后自动恢复。

### R10-P：表现模板包

R10-P1 已在 R10-1 包安全之上加入 `template.json` 合同、`base.html`/CSS 静态净化和资源路径检查，不与文件处理器另建安装系统。外部模板必须使用无权限 `data-pack`；真实渲染、预览和 Shell 原子切换归 R10-P2。

## 17. 自动测试

- 扩展名、MIME、签名、目录和无扩展名文件的候选排序；
- 项目、Profile、全局、默认优先级和 `strict/fallback`；
- 处理器缺失、停用、依赖缺失、版本不兼容和探测失败；
- 同分候选按稳定 ID 产生确定结果；
- 路由计划生成后组件目录变化时返回 `FILE_ROUTE_STALE` 并重新解析；
- Surface 创建失败不写入会话、不留下空标签；
- 普通 `open` 系统降级，`inspect/thumbnail` 不系统降级；
- 卸载和重装后绑定保留并恢复；
- 未保存编辑阻止卸载；
- 第三方组件不能注册 `nexora.*`；
- `.pmc-workspace` 自包含安装全成功或全回滚。

## 18. 人工验收

1. 分别双击图片、视频、文本、Markdown、目录和 `.blend`，确认打开结果与迁移前一致；
2. 右键菜单只显示可用内部处理器，系统打开始终可用；
3. 卸载 Nexora 文本编辑器后 `.txt` 使用系统程序；安装第三方文本处理器并绑定后使用第三方页面；
4. 卸载 BlenderIO 后 `.blend` 不创建 Nexora 标签，系统 Blender 正常启动；
5. 只安装 BlenderIO 时渲染预检可用，但内部 Blender 页面不出现；
6. 安装 Blender 工作区后内部候选出现，绑定后双击进入单例工作区标签；
7. 有未保存文本时卸载编辑器被阻止；
8. 项目绑定覆盖 Profile，离开项目后恢复 Profile 绑定；
9. 打开路由诊断，能够解释每个候选的选择或跳过原因；
10. 导出自包含 `.pmc-workspace`，在无相关组件的第二台设备预览、确认、安装并复现绑定；取消导入时零副作用。

## 19. 非目标

- 不直接修改 Windows 系统文件关联；
- 不允许组件通过任意 Shell 命令接管所有文件；
- 不在 R10-0 重写全部现有查看器 UI；
- 不把文件处理器等同于格式解析服务；
- 不让系统 fallback 满足渲染、工作流或结构化解析依赖；
- 不因组件卸载删除用户文件、项目数据或绑定历史；
- 不把外部包安装动作隐藏在双击文件之后，安装必须显式预览和确认。

## 20. 已冻结结论

1. 文件管理器提供交互入口，但不拥有文件处理器注册表和绑定真相；
2. 双击默认可以降级到系统打开，内部解析与后台任务不能；
3. 标签页只能在处理器解析和接受成功后创建；
4. 图片、视频、文本、Markdown、目录和 Blender 页面都是可替换组件；
5. BlenderIO 是无头服务，Blender 工作区是依赖它的 UI 文件处理器；
6. 组件卸载后绑定保留为缺失状态，兼容重装后自动恢复；
7. 内置组件显示名使用 Nexora 前缀，第三方使用自己的发布者命名空间；
8. `.pmc-pack` 是单组件安装包，`.pmc-workspace` 可以在 R10 后选择携带多个可分发包；
9. 路由必须有稳定 trace 和错误码，禁止继续扩散扩展名 `switch`；
10. 本文是 R10 文件处理器、外部包和内置查看器拆分的统一依据。
