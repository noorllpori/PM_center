# R5-4 Contribution Registry 收口与运行时诊断

> 状态：`accepted`
>
> 日期：2026-08-04
>
> 基线版本：2.8.4

## 1. 本轮目标

完成 R5 最后一轮加固，使前端贡献目录、模块 manifest、Surface/Widget/DataSource 实现和运行时订阅之间可以自检，并确保缺失实现时显示可解释错误，而不是空白页面。

本轮不执行 WorkflowNode，不引入 Profile 布局，也不加载任意第三方 React 代码。工作流执行仍属于 R11。

## 2. 统一 Surface 目录

- `SURFACE_CONTRIBUTIONS` 统一登记全部内置和诊断 Surface；
- Surface 定义包含稳定 ID、所属模块、标题和 `workspace/shell/dialog/event/native` 宿主类型；
- WorkspaceTab 和 ShellTab 通过目录中的 Surface ID 建立所有权关系；
- 工作区与 Shell 渲染器集中登记在实现注册表，不再由主应用为局域网页面保留专用渲染分支；
- 实现清单同时暴露 Workspace Surface、Shell Surface、Widget 渲染器和 DataSource 读取器 ID，供诊断使用。

## 3. 目录一致性检查

诊断页面会核对：

- 前端贡献 ID 格式、同类型重复 ID 和定义类型；
- 前端定义所属模块与 Rust manifest 声明是否一致；
- 后端声明是否缺少前端定义；
- 工作区/Shell 标签引用的 Surface 是否存在、宿主和所有者是否正确；
- Surface、Widget 和 DataSource 是否存在对应实现；
- Widget 引用的 DataSource 是否存在且属于同一模块；
- WorkflowNode 输入输出端口是否重复；
- 工具打开目标是否指向已登记的 WorkspaceTab 或 ShellTab；
- 运行时贡献冲突和没有定义的多余实现。

当前完整目录的预期结果：

| 项目 | 数量 |
| --- | ---: |
| 前端定义 | 31 |
| 其中模块定义 | 29 |
| Manifest 声明 | 29 |
| 实现注册 | 7 |
| 一致性问题 | 0 |

核心设置和 Blender 文件解析器属于主程序工具，因此计入前端定义但不由模块 manifest 声明。

## 4. 缺失实现状态

- 工作区标签缺少 `contributionId`、定义、模块、manifest 声明或 Surface 渲染器时显示统一错误页；
- Shell 标签使用相同规则，不再因未知或失效贡献留下空白主区域；
- Widget 缺少定义、DataSource、读取器或渲染器时显示包含稳定 ID 的错误块；
- 错误状态提供刷新注册表操作，但不会擅自启用模块或修改用户配置；
- 正常停用模块仍由既有撤下逻辑关闭标签，错误页主要保护旧会话、开发错误和运行时竞态。

## 5. 订阅释放

- Contribution Registry 的运行时事件监听改为引用计数；
- 第一个消费者安装全局监听，最后一个消费者释放时移除监听；
- 初始化返回幂等释放函数，React Strict Mode 重复挂载不会产生负计数；
- 初次刷新失败时自动回滚订阅；
- 设置或其他并行初始化失败时，已经取得的释放函数仍会登记并在卸载时执行；
- 诊断页面执行一次 acquire/release 自检，确认消费者数量恢复、监听状态不漂移。

## 6. 自动验证

2026-08-04 已通过：

```powershell
npx tsc --noEmit
npm run build
npm run check:platform-contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml --package nexora --check
git diff --check
```

结果：Rust 库测试 `120 passed`；TypeScript、Vite、平台合同、Rust 编译和格式检查全部通过。Vite 仅保留既有大 chunk 警告，Rust 仅保留 4 条既有 dead-code 警告。

## 7. 人工验收

- [x] 启用“贡献隔离样本”并打开诊断工作区；
- [x] “目录一致性”显示 `31 / 29 / 29 / 7`，检查通过且没有问题；
- [x] “订阅释放”显示当前消费者 1、事件监听已安装、引用计数自检通过；
- [x] 设备协作 Shell 标签仍可正常打开、切换、关闭和重新打开；
- [x] 停用诊断模块后，工具、Pin 和工作区标签同时撤下；
- [x] 重新启用后入口和 Pin 恢复，但标签不自动打开；
- [x] 浅色、深色、正常宽度和窄窗口没有遮挡或空白区域；
- [x] 缺失实现路径由统一错误状态和生产构建确认，不会返回空白页面。

最后一项由统一错误状态的代码路径和生产构建覆盖；若不希望手工修改代码，可以只核对正常页面与自动检查结果。

## 8. 验收后动作

2026-08-04 人工确认通过，R5-4 与整个 R5 已转为 `accepted`：

- R5 成为 R6 的稳定前置依赖；
- 下一项进入 R6 装配方案运行时与当前配置兼容迁移。
