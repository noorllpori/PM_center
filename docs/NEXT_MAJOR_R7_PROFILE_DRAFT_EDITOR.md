# R7-1 装配方案草稿编辑器

> 状态：`verifying`
>
> 日期：2026-08-05
>
> 前置：R6 accepted、R7-0 accepted

## 1. 本轮目标

让用户不编辑 JSON 就能创建和复制普通 `WorkspaceProfileV1`，并编辑第一层装配内容：

- 名称和说明；
- 启用模块；
- 显式启用组件；
- 查看模块依赖、组件传递来源和保存阻塞项；
- 使用 revision 乐观锁安全保存。

本轮不实现删除、历史版本、导航/Pin 排序、Surface、Widget、属性面板、撤销重做和布局拖拽。这些继续按 R7 后续小步交付。

## 2. 编辑规则

- 当前运行 Profile 不能原地修改，界面只提供“复制编辑”。这样不会出现文档已变化、模块运行时却仍是旧集合的分裂状态。
- 非当前且 JSON 可读取的 Profile 可以直接编辑；依赖被阻止的方案仍可打开并移除已卸载模块或组件，只有文档本身无效时禁用编辑。
- 新建从普通空白 Profile 创建；复制会生成新的 `local.profile-<uuid>`，不会覆盖或修改来源文件。
- 创建、复制和保存都不自动启动、停止或切换模块。保存后仍需“查看影响”并明确应用。
- 待完成的 Profile 切换日志存在时，仓库编辑被阻止，必须先完成恢复。
- 名称最多 80 字符，说明最多 500 字符。

## 3. 依赖行为

- 选择模块时递归补齐 `requiresModules`，并沿用依赖声明的版本要求。
- 取消模块时，递归取消所有仍选中且依赖它的模块。
- 被取消模块声明的工具贡献会从该草稿的 `shellLayout.pinnedTools` 中移除；核心工具和其他模块的 Pin 不受影响。
- `enabledComponents` 只记录用户显式选择。模块或其他组件传递启用的组件显示为“生效”，但不会重复写入显式列表。
- 后端使用完整模块与组件目录持续预检；缺失、版本不兼容、冲突或依赖未选择都会阻止保存。
- 所有正式组件仍是同一种可安装组件。`bundled` 只表示初始分发来源，不表示不可卸载；组件安装器属于 R9/R10，不在编辑器中伪造安装状态。

## 4. 数据与并发安全

- `create_workspace_profile` 只使用新文件写入，目标已存在时失败。
- `save_workspace_profile` 要求 `expectedRevision` 与磁盘文档当前 revision 同时匹配。
- 保存成功后 revision 加一并原子替换 JSON。
- 草稿校验失败、revision 冲突或当前方案直改时，原文件保持不变。
- 多窗口同时编辑同一方案时，后保存的旧修订被拒绝，用户可重新读取后再编辑。

## 5. 新增接口

- `get_workspace_profile_document(profileId)`
- `validate_workspace_profile_draft(profile)`
- `create_workspace_profile(request)`
- `save_workspace_profile(request)`

前端通过 `WorkspaceProfileEditorDialog` 编辑，通过 `useWorkspaceProfileStore` 统一刷新仓库快照。后端错误使用稳定代码 `ProfileEditBlocked` 和 `ProfileRevisionConflict`。

## 6. 自动验收

- [x] 复制产生新 ID，来源文件字节不变；
- [x] 保存成功 revision 递增；
- [x] 旧 revision 保存被拒绝；
- [x] 当前运行 Profile 直改被拒绝；
- [x] 依赖无效时磁盘文件不变；
- [x] TypeScript 与 Vite production build 通过；
- [x] 完整平台测试、共享合同测试和格式检查通过。

## 7. 人工验收

1. 在全局设置的“装配方案运行时”点击“新建方案”，确认创建后直接进入编辑器，当前运行方案没有变化。
2. 勾选“渲染中心”，确认“项目资源”自动被选中，组件区的 BlenderIO 显示生效和依赖来源。
3. 取消“项目资源”，确认渲染中心一并取消，相关模块工具 Pin 不再保留在草稿中。
4. 单独显式勾选 BlenderIO，保存后确认显示为显式组件；取消渲染模块后它仍保持生效。
5. 保存方案，确认修订从 r1 变为 r2；关闭重开后内容保持。
6. 对当前运行方案只看到“复制编辑”，复制后修改名称和模块，确认原方案未变化。
7. 点击新方案的“查看影响”，确认只有明确点击“应用此方案”后模块和页面才切换。
8. 在窄窗口检查编辑器模块、组件和校验区可滚动，不重叠且按钮文字完整。

以上通过后，将 R7-1 标记为 `accepted`，进入 `NEXT_MAJOR_R7_SHELL_HOME_COMPOSITION.md` 定义的 R7-2 最小 Shell、项目管理器、快捷栏、导航与首页装配。
