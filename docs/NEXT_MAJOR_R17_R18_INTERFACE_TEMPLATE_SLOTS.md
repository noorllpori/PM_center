# Nexora R17-R18 界面模板与插槽装配

> 状态：`verifying`
>
> 更新日期：2026-08-12；验证基线：80def58f2f94。
>
> 范围：R17 Hosted Surface 的界面合同与 R18 可视化界面装配子路线。它不代表 R15 云端生态、R16 同步、完整 Project Provider ABI 或自动化节点编排已经完成。

## 用户模型

普通用户只需要理解三个对象：组件、装配方案和项目。

- 组件公开页面、工具、文件处理器和服务能力。
- 装配方案选择启用组件、界面模板、主页、快捷栏和插槽绑定。
- 项目提供业务上下文；模板不能直接读取项目文件或数据库。

主窗口分为两层。Nexora 固定管理顶部 `HostUtilityBar`，其中始终包含快捷栏、条件显示的 DEV 重载、维护中心和 `工具 Alt+Q`。工具带以下完全由当前界面模板组织。模板不能删除维护入口、功能中心或系统标题栏按钮。

工具带模式写入 `shellLayout.hostToolbar.mode`：

- `fixed`：一直显示。
- `auto-hide`：窗口顶部感应区、键盘焦点和 `Alt+Q` 均可展开；鼠标离开工具带后立即收起，键盘焦点仍在工具带内时保持展开。

## 模板包

界面模板是 `data-pack` 组件中的 Shell 模板贡献，使用正常 `.pmc-pack` 安装、签名、信任、卸载和工作区导入链路，不存在第二套模板安装格式。

模板由静态 HTML、CSS、包内资源和 `template.json` 组成。它不能包含 JavaScript、Tauri API、远程 URL、本机路径、`iframe`、内联事件或动态模块加载。Nexora 在安装和加载时验证模板，并把 CSS 作用域限定在本模板根节点，不能改写工具带、对话框或其他模板。

```html
<main class="workspace">
  <aside><nexora-slot name="navigation"></nexora-slot></aside>
  <header><nexora-slot name="project-tools"></nexora-slot></header>
  <section><nexora-slot name="primary"></nexora-slot></section>
  <footer><nexora-slot name="status"></nexora-slot></footer>
</main>
```

模板必须至少声明一个接受 `active-surface` 的 `primary` 插槽。旧 `pm-navigation`、`pm-toolbar`、`pm-tabs`、`pm-surface-host` 和 `pm-task-status` 仍可读取，并映射到新插槽。旧 `navigationKind` 与旧 Shell ID 也继续兼容。

## 插槽合同

`TemplateSlotDefinition` 定义：

- `accepts`：`active-surface`、`component-surface`、`widget`、`navigation`、`tabs`、`toolbar` 或 `status`。
- `multiplicity`：`one` 或 `many`。
- `layout`：`single`、`stack`、`tabs`、`flow`。
- `required`、`collapseWhenEmpty` 和可选尺寸上下限。

标签、导航、项目工具、状态和当前活动页面由宿主供应。`component-surface` 由方案绑定已启用组件的隔离页面。模板约束优先于组件推荐尺寸；组件最小尺寸高于插槽最大尺寸时，保存与应用被阻止。

空插槽不会自动注入内容。模板显式装配 `active-surface` 后，如果方案没有指定启动主页，则显示轻量的 Nexora Logo 欢迎页；用户选择具体主页后由该页面替换。这样“完全空白模板”和“已有主页区域但尚未选业务页面”是两个可区分的状态。

每个模板的参数和绑定独立保存在 `shellLayout.interfaceTemplateStates`。切换模板不会删除旧模板的绑定，切回即可恢复。只有显式“另存为”会复制装配方案。

## 组件页面

`scriptSurfaces` 可声明：

- `defaultSurface`：组件默认主界面，每个组件至多一个。
- `instanceMode`：`singleton` 或 `multiple`。
- `sizeHints`：最小、推荐和最大尺寸，以及可选紧凑 Surface。
- `placements`：Shell、工作区、Widget、对话框或独立窗口。

装配器只显示当前方案有效组件闭包中的兼容 Surface。单例 Surface 不能重复装配；多实例 Surface 生成独立 `instanceId` 和页面生命周期。模板编辑预览中的隔离页面不能启动自动化、文件写入、网络请求或后台任务。

## 开发、重载与回退

已安装模板只读。选择“复制为开发模板”时，Nexora 复制组件目录，生成新的 `local.template-copy-*` 组件 ID 与模板 ID，随后将该目录作为受信任开发目录安装。开发者可用 VS Code 编辑副本，并通过 DEV 重载完成重新校验和注册。

重载会先检查活动操作和非幂等自动化运行；存在不可安全取消的运行时拒绝重载并保留旧版本。校验或挂载失败时，当前模板仍继续使用，主窗口回退到内置安全布局，固定工具带不会消失。

`list_interface_templates`、`get_interface_template_preview`、`validate_interface_layout`、`duplicate_interface_template_for_development`、`reload_development_interface_template` 和 `export_interface_template_package` 都复用组件运行时。导出仍需要开发者签名密钥和发布者资料，不授权模板直接访问宿主资源。

## 当前验收范围

- 固定与自动隐藏工具带始终能访问维护中心与 `Alt+Q`。
- 内置顶部、侧边、紧凑模板和外部静态模板均可保留标签、导航、项目工具、主内容与状态区。
- 插槽可添加、移除和排序宿主内容与组件页面；空插槽合法并保持空白，保存前只检查插槽类型、数量、单例、组件归属和尺寸。
- 切换模板后分别保留各自的插槽配置。
- 静态模板拒绝脚本、远程资源、越界资源、未知插槽和全局 CSS 根选择器。
- 模板或页面失败时仍可进入维护中心并回退安全布局。

## 未完成范围

- 完整 Project Provider、第三方项目会话、TreeCache/Watcher 生命周期 ABI 仍属于 R17 后续。
- 通用 Widget/DataSource 渲染 ABI、任意右键菜单贡献和独立窗口 Surface 管理仍需后续 Hosted Surface 扩展。
- R18 自动化节点目录、图编辑器、执行计划与断点调试不在本子路线内；旧 Workflow 合同继续冻结且不执行。
- R15 组件商城与 R16 云同步的编号、范围和前置关系不变。
