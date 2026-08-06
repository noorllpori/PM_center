# R11-A 脚本组件与自动化平台

> 文档状态：已实现，等待人工验收  
> 更新日期：2026-08-06  
> 适用版本：Nexora 2.8.5 之后的下一代平台开发线

## 1. 决策

R11 不建设节点式 DAG 编辑器。正式业务自动化以可安装组件为单位，Python 是首选业务实现语言，组件通过当前 Profile 决定是否生效。

旧 `WorkflowManifest`、`workflowNodes` 和 `workflowBindings` 继续解析、校验和往返保存，但属于冻结兼容合同：

- 不进入有效自动化目录；
- 不被调度器执行；
- 不提供节点画布；
- 诊断界面必须显示“旧工作流合同暂不执行”。

## 2. 分层

```text
Profile automationBindings
  -> builtin.script-automation
  -> automation_runtime.db
  -> ComponentRuntimeManager
  -> python-action / python-worker
  -> Capability Gateway
  -> Task Center / script surface
```

- 组件清单声明能力，Profile 保存实际启用的手动、事件和 cron 绑定。
- 只有当前 Profile 有效组件闭包中的组件可以运行或贡献页面。
- 模块停用只撤下入口、触发器和页面，不删除组件、绑定、状态和历史。
- Python 是受信任代码。Capability 约束 Nexora SDK 和宿主接口，不是 Python 标准库的 OS 沙箱。

## 3. 组件合同

`ComponentManifestV1.contributes` 新增：

- `automationCommands`：命令、Python 入口命令名、Schema、上下文、权限、超时、重试和执行语义。
- `automationEvents`：组件允许绑定的事件白名单。
- `scriptSurfaces`：隔离 HTML 页面、放置区域和允许调用的自身命令。

命令上下文：

- `global`：不得依赖项目。
- `project-required`：缺少有效项目时拒绝并记录诊断。
- `either`：全局或项目上下文均可。

执行语义：

- `pure`：无外部副作用，可安全重试。
- `idempotent`：组件保证相同输入和 operationId 可安全重放。
- `non-idempotent`：结果未知时进入 `attention`，宿主不得自动重放。

组件依赖继续使用 `requiresComponents`。脚本组件不能通过任意命令名绕过有效组件闭包、依赖合同或 Capability Gateway。

## 4. Profile 绑定

`WorkspaceProfileV1.automationBindings` 保存：

- 稳定绑定 ID；
- 组件 ID 和命令；
- `manual`、`event` 或五段式 `schedule`；
- 是否启用；
- `active-project`、`event-project`、`each-open-project`、`profile-variable` 或 `none`；
- 输入 JSON 和项目变量。

组件只声明“可以做什么”，不同 Profile 可以为同一组件配置不同触发方式。保存绑定不会立即执行。

## 5. 运行时

可停用模块：

```text
builtin.script-automation
```

新安装的默认装配启用该模块。R11 前已经存在的系统保留默认装配会执行一次
`scriptAutomationMigration`，补入该模块及其必需依赖；迁移标记写入后，用户后续主动停用不会被启动流程重新开启。自定义装配不参与该迁移。

依赖：

```text
builtin.automation-runtime
```

运行状态：

```text
queued -> preparing -> running -> completed
                         | -> waiting-permission
                         | -> failed
                         | -> cancelled
                         | -> attention
```

每次运行保存：

- `runId`、`operationId`、尝试编号；
- 组件 ID、版本、不可变 Manifest 和包内容摘要；
- Profile ID、revision；
- 项目、触发和输入快照；
- 每次尝试的开始/结束、日志、错误和结果；
- 权限等待和 attention 处理记录。

自动重试只允许 `pure` 或 `idempotent` 命令，并受 `maxAttempts` 限制。失败退避为 5、10、20 秒，最长 60 秒。取消和超时由 Component Runtime 终止完整进程树。

## 6. 持久化与恢复

软件级数据库：

```text
automation_runtime.db
```

数据库启用 WAL 和幂等迁移，保存运行、尝试、事件去重、cron tick、组件状态及 attention 记录。

恢复规则：

- 中断的 `pure/idempotent` 运行重新排队；
- 中断的 `non-idempotent` 运行进入 `attention`；
- 重启前的权限请求令牌失效，运行重新排队并重新申请；
- 旧尝试标记为 `interrupted`，不会覆盖新尝试；
- 开发目录发生变化后，旧运行仍保留原 Manifest 和摘要，不拿新代码静默恢复。

## 7. 触发器

首版白名单：

- `app.started`
- `project.opened`
- `project.closed`
- `file.changed`
- `lan.file-received`
- `render.batch-created`
- `render.batch-completed`
- `render.job-completed`
- `task.completed`
- `task.failed`

事件使用稳定 `eventId` 和去重键。没有来源去重键时，运行时生成唯一键，不会把内容相同但时间不同的合法事件永久去重。

`app.started` 在 Profile、模块和会话恢复完成后只发一次。定时器每 30 秒检查五段式 cron，并以 Profile、绑定、分钟和项目上下文联合去重。

## 8. Python SDK

随 Nexora 发布：

```text
src-tauri/resources/script-sdk/nexora_sdk
```

当前稳定协议提供：

- 读取命令、输入、Profile、项目、触发、组件、runId 和 operationId；
- 结构化结果与错误；
- 标准错误日志和结构化进度；
- 每次操作的取消标记与主动取消检查；
- 通过仅绑定本机回环地址、每次运行随机 nonce 的宿主桥调用清单中声明的依赖组件；
- 依赖调用必须同时满足当前 Profile 有效闭包、组件依赖合同和本次运行已授权 Capability，不能传入任意 Tauri 命令；
- 读写组件自己的全局或项目级状态；
- 向自身声明的 `scriptSurface` 发送受控事件；
- `python-action` 与 `python-worker` 的 SDK 路径注入。

宿主调用只接受组件清单声明的自动化命令。依赖桥不开放 Tauri 命令名，只允许调用 `requiresComponents` 或 `optionalComponents` 中已经进入有效闭包的组件；根运行取消时，桥接产生的子组件操作一并取消。

## 9. UI 沙箱

`scriptSurfaces` 在 `iframe sandbox="allow-scripts"` 中运行：

- CSP 默认拒绝所有来源；
- 禁止远程脚本、网络连接、对象、frame、表单和 base URL；
- 本地 JS/CSS 读取后内联，脚本使用每次会话 nonce；
- 页面不能访问宿主 DOM、Tauri IPC 或本机 URL；
- 页面通过 nonce 和 `allowedCommands` 白名单调用自身自动化命令；
- 页面只接收与自身组件和 surface 相关的运行事件；
- 页面失败只显示该页面错误，不影响 Nexora Shell。

当前 Profile 有效组件的页面会进入 `Alt+Q` 功能中心，并可在开发者工作台预览。进入通用 Shell、工作区、Widget 和设置目录时仍必须通过受控贡献注册表，不能加载第三方 React 代码。

## 10. 开发者工作台

功能中心 `Alt+Q` 中的“脚本自动化”打开工作台，支持：

- 创建 Python 组件模板；
- 选择、校验、信任和解除信任开发目录；
- 安装或热重载开发组件；
- 加入当前 Profile；
- 编辑 JSON、Python、HTML、CSS 和 JavaScript；
- 使用内容摘要阻止覆盖外部修改；
- 手动调试运行；
- 新建、编辑和删除 Profile 自动化绑定；
- 预览隔离页面。
- 在工作台中创建或选择 Ed25519 私钥，并直接生成签名 `.pmc-pack`；该入口复用命令行打包器的同一 Rust 实现，MSI 环境不依赖 Cargo。

正式分发继续使用 `nexora-component-pack` Ed25519 打包签名链路，工作台与 CLI 共用实现。私钥不得进入组件目录或源代码仓库。

## 11. Task Center

任务中心新增“自动化运行”页：

- 查看全部或当前项目运行；
- 查看触发来源、Profile revision、耗时和执行语义；
- 查看输入、输出、权限、attention、尝试历史和结构化日志；
- 取消活动运行；
- 重试安全运行；
- 允许一次、始终允许或拒绝权限；
- 处理结果不确定的非幂等运行。

## 12. 生命周期

切换 Profile、停用模块或卸载组件时：

- 先停止新触发和新领取；
- `pure/idempotent` 活动运行被取消并等待组件进程退出；
- 活动的 `non-idempotent` 运行阻止切换或卸载；
- 停用后调度器、事件监听和进程资源释放；
- 重新启用后保留历史和绑定，并恢复安全的排队运行。

## 13. 接口

主要 Tauri 命令：

```text
validate_script_component
trust_script_development_directory
untrust_script_development_directory
reload_script_component
create_script_component_template
list/read/save_script_development_file
get_script_surface_document

list_automation_bindings
save_automation_binding
remove_automation_binding

start_automation_run
cancel_automation_run
retry_automation_run
resolve_automation_attention
list_automation_runs
get_automation_run
get_automation_runtime_snapshot
emit_automation_event
```

事件：

```text
nexora:automation-run-changed
nexora:automation-log
nexora:automation-permission-required
nexora:automation-attention
nexora:script-surface-event
```

## 14. 实施状态

- R11-A0：合同、Profile 绑定、Schema 和数据库完成。
- R11-A1：SDK、手动运行、上下文、日志、取消完成。
- R11-A2：持久运行、尝试记录、Task Center、权限、重试和 attention 完成。
- R11-A3：事件、cron、去重和项目上下文完成。
- R11-A4：CSP、nonce、受控消息桥和页面预览完成。
- R11-A5：模板、目录信任、编辑冲突、热重载和现有签名链路入口完成。
- R11-A6：Profile/模块/卸载保护、恢复与资源清理完成，等待人工验收。

2026-08-06 已完成原生桌面冒烟：旧默认装配自动补入脚本自动化，`Alt+Q` 可见入口，脚本开发者工作台、签名打包区和 Task Center 自动化运行页均可正常打开且无白屏。真实第三方组件的权限交互、崩溃恢复和长时间资源验收仍保持待确认。

## 15. 验收

1. 从工作台打开 `examples/script-automation-blend-audit`，确认合同有效但未信任时不能安装。
2. 解除或卸载 `pmc.blendio`，确认示例依赖校验失败；恢复 BlenderIO 后再安装示例。
3. 信任、安装、加入当前 Profile，手动运行并在任务中心查看输入、输出、日志和尝试记录。
4. 新建 `file.changed` 和 cron 绑定，确认同一来源事件不重复、不同时间的同内容事件仍能执行。
5. 测试 global、project-required 和 either 上下文；缺少项目时 project-required 必须拒绝。
6. 测试权限允许一次、始终允许和拒绝；重启后旧等待令牌不得继续使用。
7. 取消长脚本并检查无残留 Python 进程；模拟崩溃，确认幂等重排和非幂等 attention。
8. 预览脚本页面，确认自身白名单命令可调用，远程网络、Tauri IPC 和本机 URL 不可用。
9. 运行中停用模块或切换 Profile，确认安全运行取消，非幂等运行明确阻止。
10. 连续启停和运行 100 次，确认没有残留 Worker、定时器和事件订阅。

自动验证：

```powershell
npm run check:platform-contracts
cargo test --manifest-path shared/pmc-platform/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
npm run build
git diff --check
```

## 16. 明确不做

- 节点 DAG、节点画布和远程执行；
- 散乱 `.py` 作为正式装配功能；
- 第三方 JavaScript 注入 Nexora 宿主页面；
- 对 Python 标准库承诺完整 OS 沙箱；
- 绕过组件依赖、有效组件闭包或 Capability Gateway 的宿主调用。
