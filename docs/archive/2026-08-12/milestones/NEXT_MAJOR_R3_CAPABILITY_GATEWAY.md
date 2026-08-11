> **历史归档（2026-08-12）**：本文件仅保留为实现与验收追溯资料，不代表当前公开能力或后续开发依据。请先阅读 docs/README.md。
# R3 Capability 权限网关验收记录

> 日期：2026-08-03  
> 基线版本：2.8.4  
> 状态：accepted
> 范围：权限内核、隔离诊断组件和静态 Tauri adapter，不迁移现有业务模块

2026-08-03 产品人工验收通过，权限提示、审批流程和安全自检未发现问题。

## 1. 本阶段交付

R3 在 `src-tauri/src/platform/capability_gateway.rs` 建立统一能力网关：

- 模块运行状态守卫；
- Module manifest 与 Component manifest 能力声明检查；
- 组件归属和版本检查；
- `read/write/delete/execute/connect/notify` 操作分级；
- `project/library/cache/receive/staging/external` 路径范围；
- 首次使用审批、仅本次授权、长期授权和撤销；
- 60 秒有效且只能使用一次的短期 token；
- SQLite 授权与审计记录；
- 隔离诊断 adapter 和权限安全自检。

现有局域网、渲染、剪贴板、项目文件、Python 和插件命令仍保持原实现。R4 会按既定顺序把真实领域入口接到同一个网关，避免在生命周期迁移前形成半托管状态。

## 2. 授权模型

权限采用首次使用审批：

```text
组件请求
  -> 模块必须 running
  -> 模块 manifest 必须声明 Capability
  -> 组件必须注册、属于该模块并声明 Capability
  -> Capability 必须允许目标操作
  -> 路径必须位于宿主给定范围
  -> 匹配长期授权，或请求用户批准
  -> 签发 60 秒单次 token
  -> adapter 再次校验并消费 token
```

用户决定：

- `allowOnce`：仅签发本次 token，不保存授权；
- `allowAlways`：保存当前模块版本、组件版本、Capability、操作和路径范围；
- `deny`：不签发 token，不执行目标操作。

长期授权不跨组件或范围复用。模块或组件版本变化后，旧授权保留用于审计展示，但标记为失效并要求重新批准。

R6 Profile Runtime 完成前，当前 Module Manager 的运行状态暂时承担“当前装配已启用”守卫；R6 会在网关前增加正式 Profile 选择检查。

## 3. 路径边界

组件只能提交相对路径，宿主 adapter 提供绝对根目录。网关拒绝：

- 空路径；
- 组件提交的绝对路径；
- `..`、根目录和 Windows 路径前缀；
- 不存在或不可规范化的授权根目录；
- 通过符号链接或 Windows 重解析点越过根目录；
- Capability 与路径范围类型不匹配；
- 使用其他路径范围签发的 token。

对尚不存在的写入目标，网关向上查找最近的现有父目录并规范化，防止父目录符号链接逃逸。被拒绝的读取、写入和删除不会改变目标文件。

## 4. 数据与审计

软件数据目录新增 `capability-gateway.db`，启用 WAL，schema version 为 1。

数据库保存：

- 长期授权及其模块/组件版本；
- Capability、操作和路径范围哈希；
- 请求审批、批准、拒绝、token 消费、失败和撤销记录；
- 稳定拒绝原因码。

审计最多保留最近 5000 条。token 和待审批请求只保存在当前进程内存中，应用重启后自动失效，不写入磁盘。数据库或审计写入失败时，网关不签发 token。

## 5. 公开命令

```text
get_capability_gateway_overview
request_platform_capability
decide_platform_capability
revoke_platform_capability_grant
run_platform_capability_operation
run_platform_capability_diagnostic
```

`run_platform_capability_operation` 当前仅操作应用数据目录中的权限诊断沙箱，或模拟无副作用的执行/网络/通知检查。它证明静态 Tauri 命令可以保留注册，但 adapter 必须消费网关 token 后才能进入实现。

相对冻结的 R0 接口快照，本阶段静态增量为：Tauri 命令 `+6`、前端字面量 invoke 目标 `+6`；Rust 顶层模块、事件、标签和工具注册表没有新增。R0 快照仍不覆盖，接口检查会继续报告经过审查的已知漂移。

## 6. 自动验收

R3 专项测试覆盖：

- 模块未运行时拒绝；
- 模块或组件未声明 Capability 时拒绝；
- token 绑定模块、组件、能力、操作和路径；
- token 单次使用与过期拒绝；
- 长期授权复用和组件版本变化失效；
- 相对路径穿越和绝对组件路径拒绝；
- 符号链接/重解析点逃逸拒绝；
- 只读 Capability 不能写入或删除；
- 拒绝操作不改变受保护文件；
- 审批、消费、拒绝和撤销写入审计。

专项命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml platform:: --lib
```

设置页中的“运行权限安全自检”使用独立临时 Module Manager、数据库和诊断目录，不改变主运行时授权和模块状态。

## 7. 人工验收

打开“全局设置”，在“模块诊断”下方找到“Capability 权限网关”：

1. 确保对应诊断模块处于运行中；
2. 选择“项目文件读取”，点击“申请并运行”；
3. 确认审批区域能看出组件、所属模块、能力、原因、风险和访问范围；
4. 选择“仅本次”，确认操作成功但长期授权仍为空；
5. 再次申请并选择“始终允许”，确认长期授权出现；
6. 再次运行，确认不再询问，但每次仍签发并消费单次 token；
7. 撤销授权，确认审计出现 `revoked`，下次重新询问；
8. 分别检查普通、敏感和关键场景；
9. 运行权限安全自检，确认全部通过；
10. 在正常宽度和窄窗口检查文字、路径和按钮不溢出。

## 8. 已知限制与下一步

- R3 只开放隔离诊断 adapter；真实领域命令在 R4 迁移。
- Profile 正式授权守卫要等 R6 Profile Runtime，当前使用模块运行状态作为临时守卫。
- 第一版 token 仅限本机当前进程，不支持远程委托；远程执行 token 属于 R13。
- R4 文件 adapter 必须直接使用网关解析后的路径或安全文件句柄，不能在业务层重新拼接未经校验的路径。
- 现有旧插件仍能调用其原静态命令，必须在 R4 的“任务、Python 和旧插件”迁移项中封堵。
