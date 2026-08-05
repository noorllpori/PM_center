# Nexora 下一代平台合同 schema v1

> 里程碑：R1  
> 状态：`verifying`，合同候选已实现并通过自动检查，等待产品确认后冻结  
> 日期：2026-08-03  
> Rust 实现：`shared/pmc-platform`  
> TypeScript 类型：`src/types/platform.ts`  
> JSON Schema：`shared/pmc-platform/schemas/v1/`

## 1. 本阶段边界

R1 只冻结“模块、装配方案、组件、工作流和包如何表达”，不启用模块开关，不改变当前页面，不迁移用户数据，也不启动新的后台服务。

这份合同是后续 Module Manager、Capability Gateway、Contribution Registry、装配编辑器和节点执行器的共同输入。后续实现不得再创建一套平行配置格式。

## 2. 已实现合同

| 合同 | Rust 类型 | 文件/入口 |
| --- | --- | --- |
| 模块清单 | `ModuleManifestV1` | `module-manifest.schema.json` |
| 组件清单 | `ComponentManifestV1` | `component-manifest.schema.json` |
| 装配方案 | `WorkspaceProfileV1` | `workspace-profile.schema.json` |
| 工作流 | `WorkflowManifestV1` | `workflow-manifest.schema.json` |
| 包头 | `PackageHeaderV1` | `package-header.schema.json` |
| 稳定错误 | `ContractError` | `contract-error.schema.json`，字段为 `code/path/message` |

所有合同均使用 `schemaVersion: 1` 和 camelCase JSON 字段。未来版本必须先按版本分派，再反序列化；当前读取器不会猜测或静默降级未来 schema。

## 3. 稳定标识和版本

- 模块、组件、贡献、命令、工作流和 Capability 使用小写点分命名，例如 `render.batch`、`media.find-files`。
- 每段只能以小写字母开始，其余字符只允许小写字母、数字和 `-`。
- Profile 内部页面、节点和部件实例使用局部 ID，例如 `render-dashboard`、`queue-summary`。
- 模块和组件版本使用 SemVer；依赖与 Profile 模块选择使用 SemVer requirement，例如 `^1.0` 或 `*`。
- `apiVersion` 第一版使用整数字符串。实现版本与宿主 API 版本分开比较。
- 同一作用域内的重复 ID、模块自身依赖、必需依赖缺失和依赖环均使用稳定错误码拒绝。

## 4. Capability 目录

首版 Capability 已形成闭合集合，未知名称不会被当作普通字符串放行。当前覆盖：

- Profile、设置、通知和剪贴板；
- 外部文件选择及读写；
- 项目文件、元数据和缓存；
- 任务、Python 和进程；
- HTTP、局域网发现、消息、传输和服务器连接；
- 渲染检查、队列、Worker 和结果提交。

每项 Capability 同时映射为 `normal`、`sensitive` 或 `critical` 风险级别。R3 会在此目录上增加授权范围、路径边界、短期 token 和审计，不会让组件直接获得原始 Tauri 命令。

## 5. Module Manifest v1

模块清单声明：

- 全局或项目作用域；
- 必需、可选和冲突模块及版本约束；
- 所需 Capability；
- 后台服务 ID；
- Shell、工作区、工具、Surface、Widget、DataSource、设置、右键命令和工作流节点贡献；
- 停用后保留数据、删除必须显式操作的数据策略。

依赖使用对象而不是只有字符串：

```json
{
  "id": "render.batch",
  "versionRequirement": "^1.0"
}
```

这样 Profile 导入和组件升级前可以准确预检版本，不把“不存在”和“版本不兼容”混为一类。

## 6. Workspace Profile v1

“装配方案”是普通、可复制、可编辑、可删除的用户数据，不存在四种写死系统模式，也没有系统 Profile 特权字段。

Profile 可以声明：

- 启用模块及版本约束；
- 显式启用组件及版本约束；模块传递依赖由运行时另外解析；
- 模块普通设置；
- 逻辑工具别名、版本要求和本机路径变量声明；
- 首页、导航页面和 Pin 工具；
- ShellTemplate、PageTemplate 和 ThemePreset 的版本化引用、变体和参数；
- Surface 及受控布局；
- Widget 实例、响应式网格位置、数据源和可见条件；
- DataSource；
- 工具栏、功能中心、右键、快捷键和页面动作绑定；
- 工作流触发绑定和字符串变量。

布局和逻辑保持分离：Profile 只绑定工作流，不在布局字段内存放节点图。可见条件是受控结构。schema v1 允许引用模板，但不允许把任意 React、JavaScript 或未经净化的 HTML 当作功能实现注入主 WebView。

`ProfilePresentationBinding` 使用稳定 ID、SemVer requirement、可选变体和普通 JSON 设置。`shellLayout.shellTemplate` 控制应用外壳，`shellLayout.themePreset` 控制全局主题，Surface 的 `template` / `themePreset` 控制页面结构和局部主题。旧 `navigationKind` 长期读取并映射到三个内置 ShellTemplate，只作为兼容字段。

模板的实际 `base.html`、CSS 和资源不直接存入 `.pmc-profile`，而由可安装模板组件包提供；自包含交付由 `.pmc-workspace` 携带经过安全检查的包。完整安全边界见 `docs/NEXT_MAJOR_PRESENTATION_TEMPLATE_ARCHITECTURE.md`。

`toolAliases` 只保存逻辑别名和稳定工具 ID，不保存本机绝对路径；`pathVariables` 只声明需要映射的文件或目录。导入产生的绝对路径进入应用数据目录的 Profile 本机绑定文件，并在 R9 由 ComponentGateway 解析。可分享 Profile 中继续拒绝 Windows、UNC、Unix 和 `file://` 绝对路径。

`shellLayout.home` 是静态默认主页，只能引用当前 Profile 中声明的 Surface。没有配置、引用失效或所属模块不可用时，宿主回退到不可撤下的最小安全主页。复杂启动行为不增加平行的脚本字段，而是通过 `workflowBindings` 将受控 `app.started` 事件绑定到 Workflow；真正执行进入 R11。

启动工作流只能调用注册过的宿主命令和贡献目标，并继续接受模块状态、项目状态与 Capability 检查。崩溃恢复、未完成 Profile 切换和用户会话恢复优先于启动工作流，防止脚本覆盖恢复状态或重复打开窗口。以上均使用现有 v1 字段语义，不需要新增 schema 字段。

## 7. Component Manifest v1

运行时固定为：

```text
python-action
python-worker
native-process
native-library
data-pack
builtin-rust
```

除 `builtin-rust` 外均必须声明使用 `/` 的包内相对入口；绝对路径、反斜杠和 `..` 会被拒绝。`builtin-rust` 是迁移期兼容标记，仅用于让已编译进宿主的旧实现经过统一合同，不进入正式可安装组件目录，不能伪造外部入口，也不代表组件等级或额外权限。所有正式组件包必须可安装和卸载。

组件统一使用同一安装、卸载、升级、权限和依赖模型，不划分“内核组件”和“普通组件”。可选 `role`、`distribution`、`uiMode` 只描述组件用途、分发来源和 UI 承载方式。组件可贡献工具动作、Widget、DataSource 和带类型输入/输出端口的工作流节点；资源限制目前包含最大内存、最大并行和超时，真正的进程监督在 R9 实现。

模块与组件之间使用 `requiresComponents` / `optionalComponents`，Profile 使用可选 `enabledComponents`。这些字段为 schema v1 的向后兼容可选扩展；依赖缺失、版本不兼容和循环必须在带注册表的验证阶段拒绝。`pmc.blendio` 是首个默认随包分发但可卸载的 `native-process` 服务组件；HDA、PPT、PDF 等格式组件沿用相同合同，详细规则见 `docs/NEXT_MAJOR_COMPONENT_DEPENDENCY_MODEL.md`。

## 8. Workflow Manifest v1

工作流是结构化 DAG：

- 触发器为手动、事件或定时；
- 节点引用稳定的节点类型贡献；
- 字面量和 Profile 变量作为输入，节点间数据通过显式端口连线；
- 执行位置为本地、优先远端或必须远端；
- 每个节点声明重试、退避、超时和启用状态。

校验会拒绝节点环、重复连线、一个输入多条连线、缺失必需端口和不兼容端口类型。`integer -> number`、`file/directory -> path` 是首版仅有的安全隐式兼容。

claimToken、staging、持久化恢复和远程租约属于 R11 执行状态，不写入静态 Workflow Manifest。

## 9. 包格式

`.pmc-profile`、`.pmc-workspace`、`.pmc-pack` 和 `.pmc-renderpack` 统一视为归档容器。顶层 `manifest.json` 使用 `PackageHeaderV1`：

- 固定 `magic: PMC_PACKAGE`；
- `schemaVersion` 和 `formatVersion` 分离；
- 包类型、稳定包 ID、创建时间和生产者版本；
- 指向主 payload 索引文件的相对路径、大小和 BLAKE3/SHA-256 摘要。

主 payload 可以继续引用归档内其他文件。Profile 包的 payload 是 Profile JSON；Workspace、Component Pack 和 Render Pack 的 payload 是各自索引。路径穿越、解压上限、签名和敏感数据扫描在 R8/R10 实现。

## 10. 向前兼容规则

- 未知 `schemaVersion`：拒绝并返回 `UNSUPPORTED_SCHEMA_VERSION`。
- 已知 schema 中的未知可选顶层字段：保留并在 JSON round-trip 后原样存在。
- 未知 Capability：拒绝并返回 `UNKNOWN_CAPABILITY`。
- 未知模块、贡献、节点类型：在脱离注册表的结构解析阶段允许，在带注册表的装配/工作流验证阶段返回 `INVALID_REFERENCE` 或 `MISSING_DEPENDENCY`。
- 删除或改变已有字段语义必须升级 schema；只增加可选字段可以保持 v1。

## 11. 自动检查

```powershell
npm run check:platform-contracts
npm run build
cd src-tauri
cargo test --lib
cargo check
```

`check:platform-contracts` 会：

1. 使用 AJV 验证六份 JSON Schema 和有效 fixture；
2. 确认未知 Capability 在 Schema 层被拒绝；
3. 运行 `pmc-platform` Rust 测试；
4. 确认提交的 Schema 与 Rust 类型重新生成结果完全一致；
5. 验证依赖环、越界入口、重复页面、Workflow 环、端口和未来版本错误码。

## 12. 本次需确认

R1 冻结前需要确认以下产品合同：

1. UI 统一使用“装配方案”，英文内部名继续为 `WorkspaceProfile`；
2. Profile 的模块列表使用 `{ id, versionRequirement }`，不退回纯字符串；
3. 四种 `.pmc-*` 格式统一使用带 `manifest.json` 的归档容器；
4. `.pmc-profile` 不含二进制，`.pmc-workspace` 默认不含业务文件，只有 `.pmc-pack`/`.pmc-renderpack` 在后续安全检查后允许携带受控资源或二进制；
5. 工作流 schema 现在保留 `schedule` 和远端执行位置，但直到 R11/R13 才真正开放运行。

确认后本合同从 `verifying` 转为 `accepted`，R2 才可以基于它实现 Module Manager 与 ResourceRegistry。
