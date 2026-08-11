> **历史归档（2026-08-12）**：本文件仅保留为实现与验收追溯资料，不代表当前公开能力或后续开发依据。请先阅读 docs/README.md。
# R2 Module Manager 与 ResourceRegistry 验收记录

> 日期：2026-08-03  
> 基线版本：2.8.4  
> 状态：verifying  
> 范围：模块生命周期内核和隔离诊断模块，不迁移现有业务模块

## 1. 本阶段交付

R2 在 `src-tauri/src/platform/` 建立第一版运行时内核：

- `module_manager.rs`：模块注册、八态状态机、依赖拓扑、冲突检查、事务启动、回滚、停用策略、健康检查、重启和状态持久化；
- `resource_registry.rs`：记录资源 ID、所有者、类型、标签、创建时间和诊断信息，按注册逆序执行异步清理；
- `builtin_modules.rs`：四个隔离诊断模块和启动/健康/停止故障注入；
- `mod.rs`：Tauri 命令、运行时初始化和 100 次启停诊断入口；
- `ModuleDiagnosticsSection.tsx`：设置页中的模块状态、依赖、资源、最近错误和人工测试入口。

现有局域网、渲染、智能剪贴板、Watcher、数据库和任务模块仍使用原生命周期。应用退出时先调用 Module Manager，再调用现有 `render_center::shutdown_all()` 与 `smart_clipboard::shutdown()` 作为兼容清理。R4 才逐项迁移真实模块，避免在没有 Capability Gateway 的情况下扩大改动范围。

## 2. 状态与持久化

状态固定为：

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

应用数据目录新增 `module-runtime.json`，保存：

- 用户期望启用状态；
- 最后运行状态；
- 最近稳定错误；
- 最后一次退出是否完成清理。

运行状态不会直接跨进程恢复为 `running`。检测到未清理退出时，模块先恢复为 `disabled` 并记录 `MODULE_FORCED_STOP` 诊断，再按期望状态重新启动。停用只释放资源并保留数据。

## 3. 启停语义

启用：

1. 解析必需依赖拓扑和版本；
2. 检查正向及反向冲突；
3. 依赖优先启动；
4. 执行启动和健康检查；
5. 中途失败时反向停止本事务已启动模块并清理资源。

缺失或版本不兼容的必需依赖只将受影响模块标记为 `blocked`，不会阻止整个 Module Manager 初始化，也不会妨碍其他健康模块启停。模块 `start()` 成功但首次健康检查失败时，会先调用该模块 `stop()`，再释放 ResourceRegistry 中的资源。

停用策略：

- `graceful`：存在运行中依赖方时拒绝；
- `cascade`：按反向拓扑先停依赖方，再停目标；
- `force`：停止超时或失败时仍释放登记资源，并保留可解释错误。

依赖模块如果不是用户显式启用，且最后一个依赖方停止，会作为孤立依赖自动停止。可选依赖只参与可用性和版本诊断，不会被强制启动。

## 4. ResourceRegistry

当前注册表支持诊断以下资源类型：

```text
tokio-task
native-thread
child-process
network-listener
database
watcher
global-shortcut
event-subscription
temporary-path
other
```

每个资源包含运行时清理句柄和可序列化诊断元数据。模块停用、重启和应用退出均按资源注册顺序的逆序释放。清理超时和失败会进入模块最近错误，不静默忽略。

## 5. 公开命令

```text
list_platform_modules
get_platform_module
preview_disable_platform_module
enable_platform_module
disable_platform_module
restart_platform_module
run_platform_module_health_check
configure_platform_module_failure
get_platform_module_failure_injections
run_platform_module_diagnostic
```

命令错误返回结构化 `code/moduleId/message/details`，前端不需要解析中文字符串判断错误类型。

## 6. 自动验收

R2 专项测试覆盖：

- 三级依赖按拓扑启动、反向停止；
- 循环依赖在运行前拒绝；
- 中途启动失败回滚已启动依赖；
- 冲突与缺失可选依赖诊断；
- 缺失必需依赖时仅阻塞对应模块；
- 首次健康检查失败时执行生命周期停止并释放资源；
- 停止超时进入可解释错误，强制停止释放资源；
- 未清理退出恢复到安全状态；
- 连续启停 100 次，登记资源和活动诊断任务均为 0；
- ResourceRegistry 严格按注册逆序清理并报告清理超时。

专项命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml platform:: --lib
```

相对冻结的 R0 接口快照，本阶段经过审查的静态增量为：Rust 顶层模块 `+1`、Tauri 命令 `+10`、前端字面量 invoke 目标 `+10`，事件和前端标签/工具注册表均未变化。R0 快照用于保留重构前证据，因此不覆盖；`audit:baseline:check` 会按设计报告这组已知漂移。

设置页还提供“运行 100 次泄漏检查”，使用独立临时 Module Manager，不改变主运行时状态。

## 7. 人工验收

打开“全局设置 -> 模块诊断”：

1. 启用“依赖工作模块”，确认基础模块先进入运行中，两个模块各有一个登记资源；
2. 停用基础模块，确认预检提示依赖方，并在确认后级联停止；
3. 打开“启动失败”故障注入后启用故障模块，确认模块进入错误且基础依赖被回滚；
4. 打开“停止超时”，启用并停用慢停止模块，确认显示 `MODULE_STOP_TIMEOUT`；再点“强制释放”，确认状态变为已停用且资源为 0；
5. 运行健康检查和 100 次泄漏检查；
6. 保持一个诊断模块启用后正常退出并重启，确认它按期望状态恢复；
7. 在浅色、深色、正常宽度和窄窗口检查文字与按钮不溢出。

## 8. 已知限制与下一步

- R2 诊断模块编译在当前应用中，但不会提供业务入口；仅用于验证运行时内核。
- 当前 Tauri 命令仍是静态注册，R3 增加 Capability Gateway 和模块状态守卫。
- 现有长期任务、端口、线程、数据库和子进程尚未登记到 ResourceRegistry；R4 逐模块迁移并做真实资源验收。
- R0 的真实渲染和双机局域网样本仍需补齐，最迟在对应真实模块进入 R4 前完成。
