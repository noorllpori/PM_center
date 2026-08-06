# R5-3 Widget、DataSource 与工作流节点贡献目录

> 状态：`accepted`
>
> 日期：2026-08-04
>
> 基线版本：2.8.4

## 1. 本轮目标

完成 R5 的第三条纵向链路：模块声明工具、工作区、Surface、Widget、DataSource 和 WorkflowNode 后，前端能够按稳定 ID 查询、挂载、撤下和恢复，而不在主应用打开逻辑中增加该模块的专用分支。

本轮新增一个默认停用的隔离诊断模块 `diagnostic.contribution-sample`。它不承载业务数据，只用于验收 Contribution Registry。

## 2. 新增贡献目录

前端新增三类强类型定义和按 ID 查询入口：

- `WidgetContributionDefinition`：标题、说明、默认数据源及最小布局尺寸；
- `DataSourceContributionDefinition`：作用域、值类型和只读数据读取器；
- `WorkflowNodeContributionDefinition`：分类、命令、输入端口、输出端口和当前是否可执行。

诊断样本声明：

| 类型 | 稳定 ID |
| --- | --- |
| Tool | `diagnostic.contribution-sample.tool` |
| WorkspaceTab | `diagnostic.contribution-sample.workspace-tab` |
| Surface | `diagnostic.contribution-sample.surface` |
| Widget | `diagnostic.contribution-sample.registry-widget` |
| DataSource | `diagnostic.contribution-sample.registry-data-source` |
| WorkflowNode | `diagnostic.contribution-sample.echo-node` |

Rust manifest 与前端目录使用同一组稳定 ID，现有唯一性测试覆盖全部六类声明。

## 3. 通用挂载行为

- 工作区新增通用 `contribution` 标签类型，不再要求每个贡献页面扩展固定业务联合类型；
- 功能中心继续使用既有 `workspaceTab` 打开目标，诊断工具没有专用打开分支；
- `ContributedWorkspaceSurface` 通过 Surface ID 查找渲染器；
- `ContributedWidget` 通过 Widget ID 查找定义、校验模块贡献、连接 DataSource 并选择渲染器；
- DataSource 每次读取当前 Contribution Registry 快照，样本 Widget 会实时显示模块、运行模块、有效贡献和冲突数量；
- 通用贡献标签继续参与项目会话保存与恢复；旧 v3 会话不需要迁移；
- 模块停用后，工具、Pin、标签、Widget、DataSource 和节点定义同时变为不可用，已打开标签自动关闭；
- 重新启用后入口和原 Pin 恢复，但标签不会自动打开。

## 4. 工作流边界

诊断回显节点已经登记稳定命令和字符串输入/输出端口，用于证明节点目录能跟随模块启停和冲突规则变化。

本轮不执行节点。`executable: false` 是明确状态；后续 R11 决策已冻结 DAG 执行，重试、幂等、取消和恢复由脚本组件自动化运行时承担，前端不能伪造执行结果。

## 5. 数据与兼容性

- 不增加数据库迁移；
- 不改变现有 Pin 文件格式，新增诊断 Pin 使用原有版本化 Store；
- 不改变旧缓存、渲染、局域网和文件标签 ID；
- 通用 `contribution` 标签只有携带合法 `contributionId` 时才允许从会话恢复；
- 未安装、冲突或已停用的贡献继续被安全忽略；
- 诊断模块默认停用，不改变普通用户启动后的界面。

## 6. 自动验证

最低验证命令：

```powershell
npm run build
npm run check:platform-contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml --package nexora --check
git diff --check
```

Rust 定向测试额外验证诊断 manifest 六类贡献完整，并与全部内置贡献做同类型唯一性检查。

## 7. 人工验收

2026-08-04 已完成人工验收，诊断页面显示 10 个模块、10 个运行模块、29 个有效贡献、0 个冲突，三类目录状态正常：

- [x] 打开一个项目，在设置的“后台模块”中找到默认停用的“贡献隔离样本”；
- [x] 启用后确认模块显示 6 项贡献，`Alt+Q` 出现“贡献隔离样本”；
- [x] 点击工具后在当前项目打开单例工作区标签，不产生重复标签；
- [x] 页面 Widget 显示模块、运行中、有效贡献和冲突数量，三项目录状态均为可用；
- [x] 工作流节点显示 `value:string` 输入和输出，并明确为尚未接入执行器；
- [x] 将该工具 Pin 到快捷栏，确认 Pin 可正常显示和排序；
- [x] 保持标签打开并停用诊断模块，确认工具、Pin 和标签立即撤下；
- [x] 重新启用后 Pin 恢复，但标签不自动打开；
- [x] 启用状态下重启，确认已打开标签可以恢复；停用状态下旧会话被安全忽略；
- [x] 检查浅色、深色、正常宽度和窄窗口布局。

## 8. 未包含

- 不提供 Profile 布局编辑或 Widget 拖拽网格；
- R5 不允许第三方任意 React、脚本或未经净化的 HTML 注入；后续受控 `base.html` 表现模板按 `NEXT_MAJOR_PRESENTATION_TEMPLATE_ARCHITECTURE.md` 进入独立模板运行时，不属于本轮 Widget 渲染器；
- 不实现 DataSource 网络轮询、缓存策略或跨进程订阅协议；
- 不执行 WorkflowNode；
- 不改变静态 Tauri 命令注册；
- 不升级版本号。

## 9. 下一步

R5-3 已通过人工验收。下一项进入 R5-4 收口：增加前端目录一致性诊断、缺失渲染器状态、订阅释放验证，并完成 R5 最终验收。通过后将 R5 转为 `accepted`，进入 R6 装配方案运行时。
