# R6-2 装配方案预检与事务切换

> 状态：`verifying`
>
> 日期：2026-08-04
>
> 基线版本：2.8.5

## 1. 本轮目标

让 R6-1 创建的普通 `WorkspaceProfileV1` 第一次真正驱动正式模块集合和功能栏 Pin，同时把切换风险完整展示在确认之前。

本轮不提供 Profile 编辑器，不接管完整 Shell 布局，也不允许项目静默切换全局 Profile。

## 2. 空白装配空间

软件级 Profile 仓库会幂等创建 `local.blank-workspace.json`：

- 不启用任何非核心模块；
- 只固定核心设置入口；
- 与迁移方案使用相同 schema、校验和切换路径；
- 它只是普通的最小起点，不是写死的产品工作模式。

已有迁移 Profile 不会被重写，原迁移时间和内容保持不变。

## 3. 只读切换预览

设置中心的“装配方案运行时”会列出当前仓库中的 Profile。点击“查看影响”只读取状态并报告：

- 将启用和停止的模块；
- 模块版本、必需依赖和冲突；
- 正在切换状态的忙碌模块；
- 目标 Pin 是否有前端实现，以及提供模块是否在目标中启用；
- 将撤下的 Shell/工作区/Surface 贡献；
- 将释放的登记资源数量和名称；
- 当前模块期望状态与 Profile 的漂移警告。

预览不会修改 `profile-runtime.json`、`module-runtime.json`、Pin、页面或后台服务。

## 4. 事务切换与恢复

确认切换后：

1. 后端重新生成预览，拒绝过期或存在阻塞的请求；
2. Module Manager 按反向拓扑停止目标之外的正式模块；
3. 按拓扑顺序启动目标模块并执行现有健康检查；
4. 全部成功后统一更新模块 `desiredEnabled`；
5. 最后提交当前/上一个 Profile ID；
6. 前端严格持久化 Profile 的 Pin 顺序并刷新 Contribution Registry。

模块启动、停止或健康检查失败时，Module Manager 恢复切换前的运行集合和期望状态。Profile 状态提交失败时也恢复旧模块集合。Pin 持久化失败时，前端请求切回原 Profile。

切换只停用模块，不删除聊天记录、渲染数据库、项目缓存、Python 环境、插件或其他模块数据。

## 5. Public Interfaces

新增 Tauri 命令：

- `preview_workspace_profile_switch(request)`；
- `switch_workspace_profile(request)`。

新增主要类型：

- `WorkspaceProfileSwitchPreview`；
- `WorkspaceProfileModuleChange`；
- `WorkspaceProfileSwitchIssue`；
- `WorkspaceProfileSwitchResult`。

`builtinToolsStore` 新增基于稳定 Tool 贡献 ID 的严格 Pin 替换入口。

## 6. 自动验证

- 初始化幂等创建普通空白 Profile；
- 预览报告模块和 Pin 影响且不改运行时状态文件；
- 模块启动失败后恢复原运行模块、期望状态和登记资源；
- TypeScript/Vite 构建；
- Rust 单元测试、`cargo check` 与格式检查；
- 平台合同与差异检查。

2026-08-04 实测结果：

- `npm run build` 通过；
- `npm run check:platform-contracts` 通过，校验 6 个 schema、6 个 fixture 和 Rust 语义规则；
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` 通过，128 个测试全部成功；
- `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features` 通过，仅保留 4 条既有 dead-code 警告；
- `cargo fmt --manifest-path src-tauri/Cargo.toml --package pm_center --check` 通过；
- `git diff --check` 通过。

## 7. 人工验收

- [x] 全局设置显示“当前 PM Center 装配方案”和“空白装配空间”；
- [x] 点击“查看影响”后只出现预览，工具栏和页面不变化；
- [x] 空白方案预览显示将停止 5 个正式模块，并提示释放 6 个后台资源；
- [x] 确认切换后局域网、渲染、缓存、Python、任务、智能剪贴板等模块贡献撤下；
- [x] 项目管理器核心界面和设置中心仍可使用；
- [x] 切回“当前 PM Center 装配方案”后 5 个正式模块、10 个登记资源和原 6 个 Pin 按原顺序恢复；
- [ ] 重启后仍停留在最后成功切换的 Profile；
- [ ] 浅色、深色和窄窗口布局无重叠。

本轮使用 Tauri 开发版完成真实桌面往返：空白方案状态为 0 个正式模块、1 个固定工具；切回后 5 个正式模块均显示“运行中”，局域网监听、项目 watcher、渲染调度和智能剪贴板线程健康检查正常。应用最终停留在“当前 PM Center 装配方案”。

## 8. 后续

R6-3 继续处理 Profile 与项目会话/页面布局的关系、当前方案漂移收敛和崩溃中断恢复。Profile 新建、复制、编辑、拖拽布局和版本历史进入 R7 装配编辑器。
