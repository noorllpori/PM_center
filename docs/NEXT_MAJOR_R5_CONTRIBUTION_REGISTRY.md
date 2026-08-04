# R5 前端 Contribution Registry

> 状态：`in-progress`
>
> 日期：2026-08-04
>
> 基线版本：2.8.4

## 1. 本轮范围

本轮建立前端贡献注册表的第一条完整纵向链路，先迁移已有内置功能：

- 功能中心与可固定快捷栏；
- 局域网主 Shell 标签；
- 缓存管理、渲染中心和局域网项目工作区标签；
- Python、任务、MDT 和智能剪贴板入口的模块可见性；
- Blender 文件页“加入渲染”直接入口；
- 工作区会话保存、恢复和旧格式兼容；
- 模块停用后的已挂载页面关闭。

设置区、右键命令、Widget、DataSource 和工作流节点将在后续 R5 分项继续迁移。

## 2. 注册表与稳定 ID

Rust 内置模块清单现在声明真实贡献 ID，前端从 `list_platform_modules` 构建运行时注册表。注册表按贡献类型分别登记所有权，重复 ID 不会静默覆盖，冲突项会被撤下并显示在模块诊断页。

当前已使用的贡献类型包括：

- `tools`；
- `shellTabs`；
- `workspaceTabs`；
- `surfaces`；
- `settingsSections`。

稳定 ID 使用小写点分命名，例如 `builtin.render-center.tool`、`builtin.render-center.workspace-tab` 和 `builtin.render-center.surface`。工作区标签只有在标签贡献和对应 Surface 都有效时才允许打开或恢复。

## 3. 打开与兼容规则

- `BuiltinToolId`、旧 Pin ID 和原有 `openCacheManagerTab()`、`openRenderCenterTab()`、`openP2PTab()`、`openLanTab()` 继续保留；
- 新入口通过贡献 ID 调用 `openWorkspaceContributionTab()` 或 `openShellContributionTab()`；
- 功能中心的打开行为由声明式 `openTarget` 分发，不再按工具 ID维护大型 switch；
- 缓存、渲染和局域网标签保持原来的固定单例 ID；
- 会话格式升级为 v3，可保存可选 `contributionId`；
- v2 及更早的 `cache/render/p2p` 记录仍会映射到新贡献；
- 未知、缺失、冲突或已停用的旧贡献会被忽略，不阻止其余会话恢复。

## 4. 停用与重新启用

模块从 `running` 离开后，前端收到统一运行时变更事件并刷新注册表：

- 功能中心入口隐藏；
- 已 Pin 的快捷项隐藏，但 Pin 顺序和配置不删除；
- 已打开的 Shell/工作区标签关闭；
- Python、任务和 MDT 等已挂载面板关闭；
- Blender 文件页的“加入渲染”入口隐藏；
- 后端 R4 命令守卫继续拒绝竞态调用。

重新启用模块后入口和 Pin 自动恢复，但旧标签不会自行打开，渲染任务也不会自行开始。模块数据、任务历史和会话配置保持不变。

## 5. 诊断

设置页“后台模块”现在同时显示 R4 生命周期与 R5 贡献信息：

- 每个模块声明的贡献数量；
- 当前有效贡献总数；
- 重复贡献冲突数量和具体所有者；
- 注册表刷新错误。

Rust 测试使用稳定常量约束内置贡献 ID，并验证同一贡献类型内不重复。

## 6. 自动验证

2026-08-04 已通过：

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml --package pm_center --check
npm run check:platform-contracts
git diff --check
```

结果：Rust 库测试 `119 passed`；TypeScript/Vite 构建、Rust 编译、格式、平台合同和差异检查全部通过。

## 7. 本轮人工验收

以渲染中心为主要纵向样本：

- [ ] 打开项目、渲染标签，并确认渲染中心已 Pin；
- [ ] 在设置中停用 `builtin.render-center`；
- [ ] 功能中心的渲染入口消失；
- [ ] 顶部已 Pin 的渲染图标消失；
- [ ] 所有项目中已打开的渲染工作区标签关闭；
- [ ] Blender 文件页的“加入渲染”按钮消失；
- [ ] 任务、完成帧、输出和 Pin 配置没有被删除；
- [ ] 重新启用后功能中心和 Pin 恢复，但渲染标签不自动打开、任务不自动开始；
- [ ] 对局域网、项目资源、任务/Python 和智能剪贴板各做一次相同的入口撤下检查；
- [ ] 重启软件，确认旧会话中的已停用贡献被安全忽略。

## 8. 后续 R5 分项

1. 将设置分区注册为 `settingsSections` 并随模块动态撤下；
2. 将文件右键动作注册为 `contextCommands`；
3. 建立 Widget、DataSource 和工作流节点的前端目录；
4. 增加隔离测试贡献，验证新工具和页面无需修改主应用 switch；
5. 完成桌面、窄窗口、深浅主题和订阅释放验收后，将 R5 转为 `accepted`。
