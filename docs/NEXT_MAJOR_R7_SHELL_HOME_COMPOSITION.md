# R7-2 最小 Shell、主页与启动装配

> 状态：`planned`
>
> 日期：2026-08-05
>
> 前置：R7-1 accepted

## 1. 目标

让 PM Center 可以在没有项目管理器的情况下正常运行。项目管理器、媒体管理器、局域网通信端和渲染端都是可装配结果，不再把项目列表主页当成宿主固定界面。

## 2. 固定边界

最小 Shell 始终提供：

- 窗口、主题和故障提示；
- 工具中心 `Alt+Q`；
- 全局设置；
- Profile、模块和组件管理；
- Profile 切换恢复与最小安全主页。

这些能力用于恢复错误装配，不作为“不可卸载组件”分类，也不赋予组件额外权限。

## 3. 项目模块拆分

- 新增 `builtin.project-manager`：项目目录、创建/导入、最近项目、项目列表、项目 Shell 标签和文件工作区。
- 保留 `builtin.project-resources`：项目数据库、TreeCache、Watcher、脏目录修复和项目关闭释放。
- `builtin.project-manager` 依赖 `builtin.project-resources`。
- 渲染、媒体等模块可以只依赖项目资源层，不强制显示项目主页。
- 停用项目管理器前关闭或阻止仍在使用的项目标签，切换预览必须列出将释放的项目资源。

## 4. 主页贡献

把现有 `WelcomeScreen` 拆成：

- 项目主页 Surface；
- 项目目录 Widget；
- 快速操作 Widget；
- 最近打开 Widget；
- 项目列表 Widget。

Profile 的 `shellLayout.home` 指向主页 Surface。默认迁移 Profile 继续使用完整项目主页；空白 Profile 使用最小安全主页。主页未配置、贡献缺失或模块不可用时必须回退，不能出现空白 WebView。

## 5. 启动行为

静态默认入口由 `shellLayout.home` 决定。R11 完成后，Profile 可通过 `workflowBindings` 将 `app.started` 绑定到启动工作流，受控打开 Shell Surface、工作区贡献、工具弹窗或独立窗口。

优先级固定为：

1. 崩溃和未完成 Profile 切换恢复；
2. 用户会话恢复策略；
3. `app.started` 启动工作流；
4. Profile 默认主页；
5. 最小安全主页。

工作流只能调用登记过的宿主命令，不能直接注入 UI、创建任意窗口或绕过模块和 Capability 检查。

## 6. 实施顺序

1. R7-2A：实现 Profile Home Resolver 和最小安全主页。
2. R7-2B：新增 `builtin.project-manager`，接管项目主页和项目 Shell 可用性。
3. R7-2C：拆分项目主页 Surface、Widget、DataSource 和命令贡献。
4. R7-2D：装配编辑器增加主页、导航、快捷栏和 Widget 选择排序。
5. R7-2E：处理项目会话关闭、资源释放、旧 Profile 迁移和窄窗口布局。

## 7. 验收

- 默认 Profile 的主页和当前版本视觉、项目列表及操作一致。
- 空白 Profile 不显示项目内容，但工具中心、全局设置和 Profile 管理可用。
- 停用项目管理器后不再扫描项目，项目标签关闭，数据库和 Watcher 释放。
- 只启用局域网、Python、任务或剪贴板时可以正常启动、重启和使用。
- 重新启用项目管理器后，项目根目录、最近记录和项目数据恢复。
- 无效主页引用和模块卸载均回退安全主页，不白屏。
- 会话恢复与未来 `app.started` 工作流不会重复打开同一页面或窗口。
