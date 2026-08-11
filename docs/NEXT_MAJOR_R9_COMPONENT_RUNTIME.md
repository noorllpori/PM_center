# R9 统一组件运行时

> 状态：`verifying`
>
> 日期：2026-08-06
>
> 前置：R3、R6、R7、R8-2 accepted

## 1. 本轮目标

R9 把组件从 Profile 中的静态目录项升级为可安装、可卸载、可调用、可取消和可诊断的真实运行单元。所有组件仍属于同一种组件模型；`service`、`feature`、`data` 只是用途，`bundled`、`local`、`marketplace` 只是来源，不代表不可卸载。

本轮支持：

- `python-action`：一次性 Nexora 内置 Python 进程；
- `python-worker`：带 `worker-ready`、JSON 行协议、心跳和复用的常驻 Python Worker；
- `native-process`：独立 EXE 进程；
- `data-pack`：只读资料目录和 JSON/文本读取；
- `builtin-rust` / 宿主适配器：为现有内置实现提供迁移兼容入口；
- ShellTemplate、PageTemplate、ThemePreset 目录。

R10-1 已加入安全 `.pmc-pack` 安装、内容摘要与 Ed25519 发布者信任；R10-2 已加入隔离 `native-library` 宿主、PE 架构检查和 ABI 握手。许可证会在安装检查中显示；更细粒度供应链审计属于后续增强。第三方 DLL 继续禁止进入 Nexora 主进程。

## 2. 安装目录与动态目录

应用数据目录新增：

```text
components/
  packages/<component-id>/
  logs/
  runtime-state.json
  .staging/
  .backup/
  .trash/
```

- 本地安装选择包含 `component.json` 的目录。
- 安装先扫描文件数量、总大小、符号链接、入口路径和组件图，再复制到 staging。
- 提交使用同卷目录重命名；已有版本先进入 backup，失败恢复旧目录。
- 随安装包组件卸载时只记录 `removedBundled`，不删除安装程序资源；可在恢复设置中重新安装。
- 本地组件卸载先撤到 trash，再清理目录。
- 仍被其他已安装组件必需依赖的组件禁止卸载；模块和 Profile 引用不会被删除，缺失组件会进入可解释的 blocked 状态。
- 每次安装、卸载或重新安装后，同步刷新 Profile 组件目录和 CapabilityGateway 注册表。

## 3. 统一执行监督

每次调用有稳定 `operationId`，运行时统一记录：

- 组件、版本、命令、运行时；
- 启动/完成时间、耗时、PID；
- `starting/running/completed/failed/cancelled/timed-out`；
- 日志路径和错误信息。

约束：

- 同一 `operationId` 不能并发复用；
- `maxParallelism` 限制组件活动操作数；
- `timeoutMs` 到期终止完整进程树；
- `maxMemoryMb` 按进程树采样，超限终止；
- 取消会设置取消状态并终止完整进程树；
- 日志限制捕获大小并清理旧文件；
- 应用退出或重启先取消操作并关闭 Worker；
- Python Worker 失效、无心跳或返回错误时撤下旧 Worker，并在未取消的情况下重启重试一次。

全局事件：`nexora:component-operation`。

## 4. 权限边界

公开 `invoke_component_command` 不允许带权限组件直接执行。调用方必须：

1. 使用 R3 CapabilityGateway 为模块和组件申请能力；
2. 得到一次性 token；
3. 在组件调用中提交完全相同的 module、component、capability、operation 和 scope；
4. 由网关消费 token 后才进入组件运行时。

动态安装组件使用 `moduleId: "*"` 注册到权限目录，但实际调用仍必须挂在一个正在运行、且声明相同 Capability 的模块下。组件不能自己读取 Profile 私有本机绑定文件。

## 5. BlendIO 兼容迁移

`pmc.blendio` 是随安装包提供、但可卸载和重新安装的标准服务组件。它贡献：

- `blender.inspect-file` 工具动作；
- `blender.inspect` 工作流节点；
- `blender.file-summary` 数据源。

R9 使用受信任宿主适配器承接现有 Rust BlendIO，避免在本轮重复引入未签名 EXE。R10 已将当前随附版本迁出 Nexora 主进程，改由 `pmc-blendio-service` 独立进程运行；文件详情、外部依赖、预览和渲染设置写入继续在入口处检查组件安装状态；卸载后明确提示重新安装，不再静默继续调用内置解析器。

真正把所有缩略图和渲染预检内部调用都改为统一 operation/token，及将可重分发的 BlendIO 交付为独立签名 `.pmc-pack`，已具备 R10 发布链路；真实第三方发布包与双机导入仍作为人工发行验收项。自包含 `.pmc-workspace` 只嵌入已验证且可再分发的原始归档，不会从已安装目录重建无签名包。

R9 的组件可用性守卫不定义最终双击行为。R10-0 增加 `FileIntentRouter` 后，普通 `.blend` 双击在 BlenderIO/Blender 工作区不可用时必须降级到 Windows 默认程序，并且不能先创建失效标签；渲染预检、结构化解析和缩略图仍返回依赖缺失，不允许用系统打开冒充成功。完整规则见 `NEXT_MAJOR_FILE_HANDLER_ROUTING.md`。

## 6. 表现模板目录

新增可卸载的 `nexora.presentation.templates` 资料组件，登记：

- `nexora.shell.top-bar`
- `nexora.shell.side-bar`
- `nexora.shell.minimal`
- `nexora.shell.blank-home`（默认空白，可显式装配启动主页）
- 三个内置 PageTemplate
- system/light/dark ThemePreset

R9 只提供目录、所有者、版本、适配器和诊断。第三方 `base.html`、CSS 净化、资源协议和整壳原子替换属于 R10-P。

## 7. 公共接口

- `get_component_runtime_overview()`
- `install_component_from_directory(request)`
- `uninstall_component(componentId)`
- `reinstall_bundled_component(componentId)`
- `invoke_component_command(request)`
- `cancel_component_operation(operationId)`

恢复设置新增“组件运行时”页，显示安装来源、运行时、贡献数、包路径、Worker/PID、活动操作、最近操作、模板数量和随附组件恢复入口。卸载必须通过应用内确认框确认，取消不会执行后台动作。

## 8. R8-3 衔接

R8-3 同轮增加 `.pmc-workspace`：

- `manifest.json` + `workspace.json`；
- 携带可移植 Profile、普通变量、按顺序排列的本地 Surface ID 和活动 Surface ID；
- 使用 BLAKE3、条目白名单、大小限制、路径穿越检查和敏感数据扫描；
- 导入创建新 Profile，不自动应用；本机工具/路径映射仍写在 Profile 外；
- 映射写入失败时删除刚导入的 Profile，避免半导入。

装配空间仍不携带项目文件、项目路径、聊天、联系人、缓存或凭据；R10 后可以在用户选择 `self-contained` 时携带可重新分发、已验证的组件二进制和模板归档。默认 `references-only` 继续只携带逻辑依赖。

## 9. 验收

1. 在“恢复设置 -> 组件运行时”看到 `pmc.blendio` 和表现模板组件。
2. 卸载表现模板组件时先取消，确认列表不变；再次确认后进入“可重新安装”，重新安装后恢复。
3. 卸载 `pmc.blendio` 后打开 `.blend` 文件详情，确认显示组件缺失提示；重新安装后恢复解析。
4. 从本地目录安装合法 `python-action`、`python-worker`、`native-process` 或 `data-pack` 测试组件，确认目录和 Profile 组件列表同步。
5. 测试组件超时、超内存、崩溃和取消，确认进程树退出且最近操作有稳定状态。
6. Python Worker 连续调用时 PID 保持；终止 Worker 后下一次调用自动建立新 PID。
7. 导出 `.pmc-workspace`，导入预览显示页面引用与变量数量；导入后新增 Profile 但不切换当前方案。
8. 损坏摘要、未知条目、绝对路径或敏感字段的装配空间被拒绝，现有 Profile 不变化。

## 10. 未完成边界

- R10：DLL 隔离宿主、签名、架构检查、`.pmc-pack` 安全安装与升级回滚；
- R10-0/R10-4：文件意图路由、后缀绑定、系统 fallback 和内置查看器组件化；
- R10-P：第三方 HTML/CSS 表现模板；
- R11：脚本自动化运行时消费组件自动化命令；旧工作流节点只兼容解析；
- R13：远程设备调度组件和渲染 Worker。
