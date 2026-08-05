# R7-2 最小 Shell、主页与启动装配

> 状态：`verifying`（R7-2B；R7-2A accepted）
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

### R7-2A 实现记录

- `shellLayout.home` 通过 `ProfileHomeSurface` 和纯解析器定位 Profile Surface、Surface 贡献、模块可用性与 Shell 渲染器。
- 迁移生成的默认 Profile 使用本地 Surface `pm-center-project-home`，绑定稳定贡献 `builtin.project-manager.home-surface`，继续渲染现有项目主页。
- 已存在的默认迁移 Profile 在无切换日志时幂等补齐主页，修订只增加一次；未完成的 R6 切换恢复优先，不在事务中改写 Profile。
- 空白 Profile 保持不配置主页，进入不可撤下的最小安全主页；工具中心、全局设置和 Profile 诊断仍可打开。
- 主页引用缺失、贡献未知、宿主类型不符、模块不可用或渲染器缺失时统一回退并显示稳定诊断码对应的说明，不产生空白 WebView。
- 本轮尚未把项目主页归属切换到实际 `builtin.project-manager` 模块；贡献暂由宿主目录提供，R7-2B 接入模块所有权和项目标签生命周期。

2026-08-05 已完成默认主页、空白装配空间、安全入口、窄窗口、往返切换和重启持久化人工验收，R7-2A 转为 `accepted`。

### R7-2B 实现记录

- 新增默认启用的全局模块 `builtin.project-manager`，要求 `builtin.project-resources ^1.0`，但资源模块不反向依赖项目管理器。
- 项目管理器正式声明 `builtin.project-manager.home-surface`、`builtin.project-manager.project-workspace-surface` 和 `builtin.project-manager.project-shell-tab`；项目主页不再是无所有者的宿主贡献。
- 每个动态项目标签记录项目 Shell 贡献和模块所有者；停用项目管理器或切换到不含该模块的 Profile 时，前端统一撤下项目标签、会话订阅和对应项目资源句柄。
- 项目管理器不可用时，手动打开项目、最近项目自动打开和项目会话恢复都会被阻止；局域网 Shell、全局设置和其他有效贡献继续工作。
- 默认迁移 Profile 在自身已选择项目资源时幂等补入项目管理器，不依赖当前正在运行的 Profile；因此从空白方案升级后仍可正常切回默认方案。
- `builtin.project-resources` 继续拥有数据库、TreeCache、Watcher、缓存管理、MDT 和集合命令；渲染中心仍只依赖资源层，不依赖项目管理器。

## 7. 验收

### R7-2A 验收结果

- 默认“当前 PM Center 装配方案”打开后仍显示原项目主页，项目目录、最近项目和打开项目行为不变。
- 切换到“空白装配空间”后显示“最小安全主页”，不显示项目列表；`Alt+Q`、页面内“打开功能中心”和“全局设置”均可用。
- 从空白方案切回默认方案后项目主页恢复，重启后仍保持所选方案主页。
- 桌面宽度和窄窗口下安全主页无横向遮挡，浅色和深色主题文字可读。

以上项目已于 2026-08-05 验收通过。

### R7-2B 当前验收

- 设置的模块诊断中显示“项目管理器”运行中，并明确依赖“项目资源”。
- 默认方案的主页、最近项目、打开项目、项目标签和文件工作区行为与升级前一致。
- 打开一个项目后停用“项目管理器”，项目标签自动撤下并回到最小安全主页；全局设置和 `Alt+Q` 仍可使用。
- 只停用“项目管理器”时，“项目资源”不会被自动停用；停用“项目资源”时依赖检查会提示项目管理器依赖它。
- 复制默认方案并仅移除项目管理器、保留项目资源，应用后重启仍不恢复项目标签，也不自动打开最近项目；项目资源保持运行。
- 重新启用项目管理器后项目主页和最近记录恢复，重新打开项目可以正常读取原 `.pm_center` 数据。
- 从空白方案切回“当前 PM Center 装配方案”后，项目管理器与项目主页正常恢复，不出现缺失模块或无效 Profile。

通过以上项目后，R7-2B 标记为 `accepted` 并进入 R7-2C。

- 默认 Profile 的主页和当前版本视觉、项目列表及操作一致。
- 空白 Profile 不显示项目内容，但工具中心、全局设置和 Profile 管理可用。
- 停用项目管理器后不再扫描项目，项目标签关闭，数据库和 Watcher 释放。
- 只启用局域网、Python、任务或剪贴板时可以正常启动、重启和使用。
- 重新启用项目管理器后，项目根目录、最近记录和项目数据恢复。
- 无效主页引用和模块卸载均回退安全主页，不白屏。
- 会话恢复与未来 `app.started` 工作流不会重复打开同一页面或窗口。
