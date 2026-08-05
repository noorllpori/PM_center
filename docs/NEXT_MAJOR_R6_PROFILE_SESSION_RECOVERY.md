# R6-3 Profile 会话归属与中断恢复

> 状态：`accepted`
>
> 日期：2026-08-04
>
> 基线版本：2.8.5

## 1. 本轮目标

让 Profile 成为应用重启后的全局运行事实源，并明确它与项目/页面会话的边界：

- Profile 决定正式模块集合和功能栏 Pin；
- 应用会话只记录保存时使用的 Profile，不允许项目会话反向切换全局 Profile；
- 切换进程在任意阶段退出后，可以按最后成功提交的 Profile 自动恢复；
- 模块期望状态与当前 Profile 漂移时，在启动初始化阶段统一收敛。

本轮不增加 Profile 创建、复制、编辑、布局设计或版本历史，这些仍属于 R7。

## 2. 持久切换事务

应用数据目录新增：

```text
profile-switch-journal.json
```

切换阶段为：

1. `prepared`：已记录来源、目标、目标 revision 和事务 ID，尚未修改模块；
2. `modulesApplied`：目标模块集合已完整应用；
3. `profileCommitted`：`profile-runtime.json` 已提交目标 Profile，等待前端 Pin 落盘；
4. 前端严格写入 Pin 后调用完成命令，删除切换日志。

日志在模块改动前原子创建。模块切换失败会恢复旧运行集合并清理日志；Profile 状态提交失败也恢复旧模块集合。Pin 写入失败通过同一事务专用回滚命令恢复来源 Profile，不再发起第二个普通切换。

## 3. 启动恢复规则

启动恢复只相信 `profile-runtime.json` 中最后成功提交的全局 Profile：

- 无切换日志：按当前 Profile 启动模块，并修正 `module-runtime.json` 漂移；
- 日志存在且当前 Profile 仍为来源：说明提交前中断，恢复来源模块集合并删除日志；
- 日志存在且当前 Profile 已为目标：说明提交后中断，完成目标模块集合和 Pin，再删除日志；
- 日志损坏、ID 冲突或目标 revision 改变：停止自动恢复并保留原文件，避免猜测性覆盖。

旧的“先按 module-runtime 恢复、稍后再读 Profile”启动路径已移除，避免错误模块先短暂启动再被关闭。

最近一次中断恢复或漂移收敛会记录在运行时状态，并在设置诊断页显示时间和结果。

## 4. 会话归属

`app-session.json` schema 从 v3 升级到 v4，新增：

```json
{
  "profile": {
    "id": "local.current-pm-center",
    "revision": 1
  }
}
```

- v3 及更早会话按兼容会话加载，下一次保存自动升级；
- Profile ID/revision 一致时正常恢复；
- 不一致时仍恢复项目、文件页和独立窗口，但只恢复当前 Contribution Registry 可用的功能页；
- 界面提示“已按当前装配方案恢复”，不会调用 Profile 切换命令；
- Profile 切换导致贡献撤下后，现有 Store 会关闭对应标签并重新保存 v4 会话。

因此任何项目目录、项目标签或项目会话文件都不能静默改变软件级全局 Profile。

## 5. Public Interfaces

新增 Tauri 命令：

- `finalize_workspace_profile_switch(transactionId)`；
- `rollback_workspace_profile_switch(transactionId)`。

扩展运行时返回：

- `WorkspaceProfileSwitchResult.transactionId`；
- `WorkspaceProfileRuntimeSnapshot.journalPath`；
- `WorkspaceProfileRuntimeSnapshot.pendingSwitch`；
- `WorkspaceProfileRuntimeSnapshot.lastRecovery`。

新增主要类型：

- `WorkspaceProfilePendingSwitch`；
- `WorkspaceProfileSwitchPhase`；
- `WorkspaceProfileRecoveryRecord`；
- `PersistedWorkspaceProfileReference`。

## 6. 自动验证

已覆盖：

- `prepared/modulesApplied` 阶段中断后回到最后提交的来源 Profile；
- Profile 已提交但 Pin 未完成时继续完成目标 Profile；
- 模块期望状态漂移记录与收敛；
- 损坏切换日志只报告且不覆盖；
- 切换日志与当前状态冲突时拒绝猜测性恢复；
- 旧会话字段可选迁移、Profile 匹配判断和不可用贡献过滤通过 TypeScript 构建；
- 全量 Rust、平台合同、编译、格式与差异检查。

2026-08-04 实际结果：

- `npm run build`：通过；
- `npm run check:platform-contracts`：通过；
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`：133 项通过，0 项失败；
- Profile 中断恢复与漂移收敛定向测试：12 项通过；
- `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features`：通过，仅保留 4 个既有 dead-code warning；
- `cargo fmt --manifest-path src-tauri/Cargo.toml --package pm_center --check`：通过；
- `git diff --check`：通过。

## 7. 人工验收

- [x] 切到空白 Profile，退出并重启后仍为 0 个正式模块和 1 个 Pin；
- [x] 切回当前 PM Center Profile，退出并重启后恢复 5 个正式模块和原 Pin 顺序；
- [x] 在不同 Profile 保存的项目会话只提示兼容恢复，不会改变当前全局 Profile；
- [x] 不可用的局域网、渲染等贡献标签不会被旧会话重新打开；缓存标签在项目资源模块可用时正常恢复；
- [x] 设置页可看到切换日志路径和最近恢复结果；
- [x] 窄窗口下恢复提示和 Profile 诊断区无重叠；当前版本在 `App.tsx` 固定浅色主题，深色验收不适用。

本轮桌面实测执行了完整往返：

```text
local.current-pm-center
  -> local.blank-workspace
  -> 重启
  -> local.current-pm-center
  -> 重新打开项目并重启
```

最终状态为 `local.current-pm-center`，5 个正式模块运行，Pin 顺序恢复为：

```text
p2p-project
render-center
settings
mdt-overview
cache-manager
python-environments
```

`app-session.json` 为 v4，记录 Profile `local.current-pm-center` revision 1，并恢复 `D:\Project\20260630@打印机` 的项目文件页；切换完成和各次重启后均无残留 `profile-switch-journal.json`。

跨 Profile 兼容验收额外使用仅启用 `builtin.project-resources` 的临时 Profile：旧会话中的渲染、局域网工作区和 Shell 局域网页均被过滤，缓存管理因其贡献仍可用而正确恢复；当前全局 Profile 始终保持测试方案，没有被旧会话反向切换。2026-08-05 完成人工验收，未发现问题；验收后已删除临时 Profile 并恢复正式方案、5 个正式模块、6 个固定工具和项目文件页会话。

## 8. 下一步

R6 已通过人工验收，可以作为 R7 的稳定依赖。R7 从普通空白 Profile 开始实现 DIY 装配编辑器，包括模块选择、Pin/导航/Surface 布局、预览、撤销重做和版本历史。
