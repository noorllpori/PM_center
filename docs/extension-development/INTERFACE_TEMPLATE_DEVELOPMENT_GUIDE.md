# Nexora 界面模板开发指南

> 适用版本：Nexora 2.8.5 开发线  
> 面向对象：只有 Nexora 安装版 EXE 的界面模板作者、装配方案作者和组件开发者  
> 权威合同：`component.json` Schema、表现模板 Schema 和安装版内置校验器  
> 接口状态：模板包、静态 HTML/CSS、类型化插槽和方案绑定为 `stable-2.8.5`；模板参数可视化编辑、任意资源渲染和完整 Hosted Surface ABI 仍应按本文状态表使用

## 1. 先理解三个对象

界面模板不是一个可执行页面，也不是一个组件功能。Nexora 的界面装配由三层组成：

| 对象 | 负责什么 | 不负责什么 |
| --- | --- | --- |
| 界面模板 | 工具带以下的静态结构、区域比例、响应式布局和插槽位置 | 不读取文件、不运行 JavaScript、不调用组件命令 |
| 组件 Surface | 真正可交互的 HTML/CSS/JavaScript 页面，由组件提供并在隔离环境运行 | 不修改宿主 DOM，不直接接触 Tauri 或其他组件目录 |
| 装配方案 | 选择模板、启用组件、把宿主内容或组件 Surface 绑定到插槽 | 不复制模板源码，不把设备级凭据写进方案 |

顶部宿主工具带 `HostUtilityBar` 不属于模板。快捷栏、DEV、维护中心和 `工具 Alt+Q` 始终由 Nexora 保留。模板只接管它下面的区域。

一个完全空白模板可以没有任何已绑定内容。一个左右分栏模板可以把组件页面放在左边，把“主页 / 当前活动页面”放在右边。模板本身只声明“这里能放什么”，实际放置结果保存在当前装配方案中。

## 2. 当前支持状态

| 能力 | 状态 | 2.8.5 行为 |
| --- | --- | --- |
| `data-pack` 模板组件 | `stable-2.8.5` | 可从目录安装、复制为开发模板、重载和打包 |
| `shellTemplates` | `stable-2.8.5` | 可替换工具带以下的主窗口布局 |
| `<nexora-slot>` | `stable-2.8.5` | 支持七种类型化内容和方案绑定 |
| 每个模板独立保存绑定 | `stable-2.8.5` | 切换模板不会删除另一模板的插槽配置 |
| `fixed` / `auto-hide` 工具带 | `stable-2.8.5` | 写入装配方案，由宿主控制 |
| 复制、VS Code、手动 DEV 重载 | `experimental` | 已可用；保存文件后仍需显式重载，不是实时热更新服务器 |
| `optionsSchema` | `experimental` | Schema 可解析并进入预览合同，当前装配器尚未完整生成参数表单 |
| `assets` | `experimental` | 路径可校验；当前 Shell 渲染器不承诺显示 `img`、SVG 或任意包内图片 |
| 任意宿主 React、DOM 或 Tauri 注入 | 不支持 | 必须使用组件 Script Surface 或后续 Hosted Surface 合同 |
| 模板 JavaScript | 禁止 | 模板是纯数据包，动态行为必须来自组件 Surface |

没有明确目标版本时，只使用 `stable-2.8.5` 能力。不要让正式模板依赖 `optionsSchema` 或图片资源才能正常使用。

## 3. 最小目录结构

```text
my-interface-template/
  component.json
  README.md
  presentation/
    shell/
      my-shell/
        template.json
        base.html
        styles.css
```

根目录必须直接包含 `component.json`。安装时选择 `my-interface-template`，不要选择 `presentation`、`shell` 或 `my-shell` 子目录。

推荐约定：

- 组件 ID 使用反向域名或组织前缀，例如 `com.example.interface-template`。
- 模板 ID 与组件 ID 分开，例如 `com.example.shell.two-column`。
- 目录名可以改变，稳定 ID 不能随意改变。
- 模板版本和组件版本可以相同，修改公开结构或插槽语义时至少提升次版本。
- 一个组件可以贡献多个模板，但最小案例建议一包一个模板。

## 4. `component.json`

完整的最小模板清单：

```json
{
  "schemaVersion": 1,
  "id": "com.example.interface-template",
  "name": "示例界面模板",
  "description": "提供一个可装配的双区域界面。",
  "version": "1.0.0",
  "apiVersion": "1",
  "runtime": "data-pack",
  "role": "data",
  "category": "appearance",
  "tags": ["interface-template", "layout"],
  "distribution": "local",
  "uiMode": "none",
  "platforms": ["any"],
  "entry": "presentation",
  "capabilities": [],
  "contributes": {
    "shellTemplates": [
      {
        "id": "com.example.shell.two-column",
        "name": "双区域布局",
        "version": "1.0.0",
        "templatePath": "presentation/shell/two-column/template.json"
      }
    ]
  }
}
```

关键规则：

| 字段 | 要求 |
| --- | --- |
| `runtime` | 外部界面模板固定为 `data-pack` |
| `entry` | 包内相对路径，可以指向 `presentation` 目录 |
| `capabilities` | 必须为空；模板不能申请文件、网络、进程或项目权限 |
| `shellTemplates[].id` | 稳定全局 ID，必须与 `template.json.id` 完全一致 |
| `shellTemplates[].version` | 必须与 `template.json.version` 完全一致 |
| `templatePath` | 从组件根目录开始的安全相对路径 |

模板组件可以依赖普通组件，但模板本身不会因此获得调用能力。需要交互时，应让被插入的组件 Surface 自己声明命令、依赖和 Capability。

## 5. `template.json`

一个左右对半模板：

```json
{
  "schemaVersion": 1,
  "id": "com.example.shell.two-column",
  "kind": "shell",
  "version": "1.0.0",
  "semanticVersion": "1.0.0",
  "baseHtml": "presentation/shell/two-column/base.html",
  "styles": "presentation/shell/two-column/styles.css",
  "slots": [
    {
      "id": "left",
      "name": "左侧区域",
      "accepts": ["component-surface"],
      "multiplicity": "one",
      "layout": "single",
      "required": false,
      "collapseWhenEmpty": false
    },
    {
      "id": "right",
      "name": "右侧区域",
      "accepts": ["active-surface", "component-surface"],
      "multiplicity": "one",
      "layout": "single",
      "required": false,
      "collapseWhenEmpty": false
    }
  ],
  "regions": ["left", "right"]
}
```

### 5.1 顶层字段

| 字段 | 说明 |
| --- | --- |
| `schemaVersion` | 当前为 `1` |
| `id` | 与组件贡献 ID 一致 |
| `kind` | 主窗口模板使用 `shell`；另有兼容的 `page` 和 `theme` 合同 |
| `version` | SemVer，必须与贡献版本一致 |
| `semanticVersion` | 模板语义版本；当前建议与 `version` 相同 |
| `baseHtml` | 静态 HTML 文件的包内相对路径 |
| `styles` | 可选 CSS 文件的包内相对路径 |
| `slots` | 类型化插槽定义 |
| `regions` | 兼容字段；新模板以 `slots` 为准 |
| `optionsSchema` | 已解析但当前参数编辑支持不完整，不要作为必需功能 |
| `assets` | 可声明包内资源，但当前 Shell 运行时不承诺渲染图片标签 |

Shell 模板只要声明了 `slots`，至少有一个插槽必须接受 `active-surface`。它不一定要叫 `primary`，左右模板中的 `right` 就可以承担主 Surface。

### 5.2 插槽字段

| 字段 | 取值 | 说明 |
| --- | --- | --- |
| `id` | 小写本地 ID | 例如 `left`、`primary`、`status`；必须在 HTML 中存在同名 `<nexora-slot>` |
| `name` | 用户可读名称 | 显示在装配编辑器中 |
| `accepts` | 类型数组 | 决定可插入的宿主内容或组件页面 |
| `multiplicity` | `one` / `many` | `one` 只允许一个有效绑定 |
| `layout` | `single` / `stack` / `tabs` / `flow` | `one` 必须配合 `single` |
| `required` | 布尔值 | 为 `true` 且没有有效绑定时会阻止保存并应用 |
| `collapseWhenEmpty` | 布尔值 | 空插槽是否从渲染树中折叠；要保留空白区域时设为 `false` |
| `minWidth` 等 | 正整数 | 可选最小/最大宽高，最小值不能大于最大值 |

### 5.3 `accepts` 类型

| 类型 | 实际内容 | 使用建议 |
| --- | --- | --- |
| `active-surface` | 当前活动项目、启动主页或当前 Shell 页面 | 主内容区通常需要一个；同一模板一般只绑定一次 |
| `component-surface` | 当前方案有效组件提供的隔离页面 | 用于音乐播放器、工具面板、信息面板等 |
| `navigation` | 当前方案导航 | 需要导航的模板显式提供 |
| `tabs` | Nexora Shell 标签栏 | 多页面工作流使用 |
| `toolbar` | 当前项目的项目工具栏 | 无活动项目时为空 |
| `status` | 当前项目状态条 | 无活动项目时为空 |
| `widget` | Widget 内容类型 | 字段已保留；第三方 Widget 宿主能力仍受当前 Hosted Surface 边界限制 |

`active-surface` 与 `component-surface` 不相同：

- `active-surface` 跟随用户当前页面。打开项目后它显示项目工作区；切换到一个 Shell 页面后它显示该页面。
- `component-surface` 固定挂载指定组件的指定 Surface，不会因为用户切换主页而改变。
- 如果同一个单例组件页面既被设为启动主页，又绑定到包含 `active-surface` 的同一模板中，宿主会避免重复挂载同一实例。

## 6. `base.html`

```html
<main class="split-template">
  <section class="split-template__pane split-template__pane--left">
    <nexora-slot name="left"></nexora-slot>
  </section>
  <section class="split-template__pane split-template__pane--right">
    <nexora-slot name="right"></nexora-slot>
  </section>
</main>
```

### 6.1 当前允许的元素

实时 Shell 渲染器只保留以下结构元素：

```text
main header aside section footer div nav article
ul ol li p span h1 h2 h3 h4 h5 h6 hr figure figcaption
```

`<nexora-slot>` 是特殊宿主节点。以下内容即使通过普通浏览器能显示，也不要放进模板：

- `button`、`input`、`select`、`textarea`、`a`；
- `img`、`svg`、`canvas`、`video`、`audio`；
- `script`、`iframe`、`object`、`embed`、`link`、`base`；
- 任意内联事件，例如 `onclick`；
- 任意 `javascript:`、`file:`、`data:`、远程 HTTP 资源；
- `window.__TAURI__`、宿主对象、动态 `import()` 或 `eval()`。

当前保留的普通属性只有：`class`、`id`、`title`、`role`、`aria-*`、`data-*` 和合法的 `tabindex`。内联 `style` 会被丢弃，因此样式全部写入 `styles.css`。

### 6.2 旧模板兼容节点

历史模板可以包含：

```text
<pm-navigation>  -> navigation
<pm-tabs>        -> tabs
<pm-toolbar>     -> toolbar
<pm-surface-host>-> primary
<pm-task-status> -> status
```

新模板一律使用 `<nexora-slot name="...">`。旧的窗口控制、拖动区和恢复入口不再由新模板声明，它们已经归宿主工具带和系统标题栏管理。

## 7. `styles.css`

```css
.split-template {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  min-height: 100%;
  height: 100%;
  min-width: 0;
  background: var(--nexora-surface, #ffffff);
}

.split-template__pane {
  display: flex;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}

.split-template__pane--left {
  border-right: 1px solid var(--nexora-border, #e5e7eb);
}

.split-template__pane > div {
  min-height: 0;
  min-width: 0;
  flex: 1;
}

@media (max-width: 720px) {
  .split-template {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
  }

  .split-template__pane--left {
    border-right: 0;
    border-bottom: 1px solid var(--nexora-border, #e5e7eb);
  }
}
```

Nexora 会把普通 CSS 规则自动限定到当前模板根容器，模板不能修改宿主工具带、对话框或其他模板。条件规则会递归编译。

CSS 禁止：

- `@import`；
- `expression()`；
- `javascript:`、`vbscript:`、`file:`、`data:`；
- `http://`、`https://` 或协议相对 URL；
- 依赖宿主内部 Tailwind 类名或 React DOM 结构。

布局建议：

1. 根节点同时设置 `min-height: 100%`、`min-width: 0`。
2. 所有 Grid/Flex 子项设置 `min-width: 0`，可滚动区域设置 `min-height: 0`。
3. 固定区域使用 `minmax()` 或明确的最小/最大值，避免窄窗口横向溢出。
4. 颜色变量必须带回退值，例如 `var(--nexora-border, #e5e7eb)`。
5. 不使用视口宽度缩放字体；组件页面负责自己的排版。
6. 模板只做布局，不给组件页面叠加大面积装饰或强制字体。

当前宿主会给 `component-surface` 外层添加边框、背景和圆角。这是 2.8.5 的 Hosted Surface 容器行为，模板 CSS 不应依赖移除它。

## 8. 方案如何保存模板状态

装配方案内部会保存类似结构：

```json
{
  "shellLayout": {
    "hostToolbar": { "mode": "fixed" },
    "shellTemplate": {
      "id": "com.example.shell.two-column",
      "versionRequirement": "^1.0.0"
    },
    "interfaceTemplateStates": [
      {
        "templateId": "com.example.shell.two-column",
        "slotBindings": [
          {
            "id": "slot-left-player",
            "slotId": "left",
            "kind": "component-surface",
            "componentId": "com.example.music-player",
            "surfaceId": "com.example.music-player.surface",
            "enabled": true,
            "order": 10
          },
          {
            "id": "slot-right-active-surface",
            "slotId": "right",
            "kind": "active-surface",
            "enabled": true,
            "order": 10
          }
        ]
      }
    ]
  }
}
```

不要手工修改用户的 `.pmc-profile`。装配编辑器负责生成和校验绑定。理解这段结构主要用于排查：

- 模板文件决定有哪些插槽；
- Profile 决定每个插槽放什么；
- 切换模板时，每个 `templateId` 的绑定分别保留；
- 修改插槽 ID 等于修改公开合同，旧绑定需要迁移或移除；
- “保存并应用”保存 Profile 草稿，不会写回 HTML/CSS 文件。

## 9. 推荐开发流程

### 9.1 从现有模板复制

1. 打开“编辑装配方案 -> 界面装配”。
2. 选择一个内置或已安装模板。
3. 点击“复制为开发模板”。
4. 选择一个父目录。Nexora 会在其中创建新的组件目录、生成新 ID、信任目录并安装开发副本。
5. 在新模板旁点击“VS Code”。

不要把已安装组件目录当作源码目录直接修改。安装副本是只读运行副本，开发源目录才是你应编辑的位置。

### 9.2 修改和刷新

1. 在 VS Code 保存 `component.json`、`template.json`、`base.html` 或 `styles.css`。
2. 回到 Nexora，在模板开发区点击“重载”，或点击顶部 `DEV` 扫描并重载已变化组件。
3. 等待校验和安装副本替换完成。
4. 检查界面装配预览和主窗口实际效果。
5. 只有插槽绑定、主页、工具带模式等方案内容改变时，才点击“保存并应用”。

当前不是浏览器式实时热更新。文件保存后不自动执行代码，也不自动替换运行模板。这个显式重载步骤用于确保校验失败时可以保留旧版本。

### 9.3 从零目录安装

1. 创建本指南第 3 节的目录结构。
2. 打开 `工具 Alt+Q -> 维护中心 -> 组件运行时`。
3. 选择“从目录安装”，选中包含 `component.json` 的根目录。
4. 在当前装配方案中启用该组件。
5. 在“界面装配 -> 安装的界面模板”选择它。
6. 为插槽添加宿主内容或组件页面，然后“保存并应用”。

从目录安装适合首次验证；长期开发建议使用“复制为开发模板”建立可追踪的受信任开发目录。

## 10. 打包与分发

开发完成后：

1. 在开发者工作台选择模板组件根目录。
2. 重新校验，确认目录处于信任状态。
3. 创建或选择保存在组件目录之外的 Ed25519 私钥。
4. 填写发布者 ID、名称和许可证。
5. 生成签名 `.pmc-pack`。
6. 在另一台干净的 Nexora 安装环境中测试安装、启用、模板选择、切换、卸载和重装。

交付物区别：

| 文件 | 内容 |
| --- | --- |
| `.pmc-pack` | 一个模板组件本体 |
| `.pmc-profile` | 模板 ID、版本要求、插槽绑定和方案配置，不携带组件包 |
| `.pmc-workspace` | 可选择引用组件，或在自包含模式中携带允许再分发的签名组件包 |

私钥、开发目录绝对路径、设备授权和用户项目文件都不能进入模板包或装配空间。

## 11. 版本与兼容策略

| 修改 | 建议版本 | 兼容要求 |
| --- | --- | --- |
| 只调整 CSS 颜色、间距、响应式断点 | Patch | 不改变插槽 ID 和语义 |
| 增加一个可选插槽 | Minor | `required: false`，旧方案仍可应用 |
| 修改插槽接受类型或从 `many` 改为 `one` | Minor 或 Major | 旧绑定可能失效，必须写迁移说明 |
| 删除/重命名插槽 | Major | 会产生旧 `slotId` 引用，必须提供替代关系 |
| 改变模板 ID | 新模板 | 视为另一个模板，旧状态不会自动继承 |
| 改变组件 ID | 新组件 | 依赖、安装记录和方案引用全部视为新对象 |

模板切换不会删除旧模板状态。用户切回旧模板时，原插槽绑定会恢复。只有明确删除方案绑定或删除组件时才清理相关引用。

## 12. 左右 50% 完整样例

仓库/安装版样例名为 `split-interface-template`。它与“完全空白模板”分开，避免一个案例同时承担两种语义。

结构：

```text
split-interface-template/
  component.json
  README.md
  presentation/shell/split-50/
    template.json
    base.html
    styles.css
```

推荐装配：

- 左侧：Ninniku 音乐播放器的 `component-surface`。
- 右侧：`active-surface`。
- 启动主页：选择一个 Shell 页面；没有选择时由当前 Profile 主页规则决定。
- 宽度低于 720px：自动变成上下各 50%。

## 13. 常见问题

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| 从目录安装一直转圈 | 选择了子目录、清单无效、旧运行正在占用或安装异常 | 选择包含 `component.json` 的根目录；先在开发者工作台校验并查看日志 |
| 修改 HTML/CSS 没变化 | 修改的是安装副本，或尚未执行 DEV 重载 | 确认编辑的是受信任开发目录；保存后点击模板旁“重载” |
| “保存并应用”不可用 | 当前草稿存在模板、依赖、插槽或版本诊断 | 查看底部阻止原因；优先处理缺失模板、无主 Surface、旧插槽绑定和单例重复 |
| 点击“+ 主页”没有反应 | `multiplicity: one` 已有内容，或模板状态仍有旧绑定 | 移除现有绑定；确认编辑器已迁移/清理旧 `slotId` |
| 提示插槽不存在 | 修改了 `template.json.slots[].id`，Profile 仍引用旧 ID | 把 ID 改回兼容值，或在装配编辑器重新绑定后保存 |
| 模板要求主插槽 | Shell 声明了 slots，但没有任何插槽接受 `active-surface` | 至少一个插槽的 `accepts` 加入 `active-surface` |
| 空区域消失 | `collapseWhenEmpty` 默认为 `true` | 显式设为 `false` |
| 组件页面不在可选列表 | 组件未启用、依赖闭包无效、Surface 不允许 Shell 放置或版本不兼容 | 启用组件并检查 `scriptSurfaces.placements`、依赖和版本 |
| 同一页面不能放两次 | Surface 默认为 `singleton` | 组件作者必须声明 `instanceMode: multiple`，并保证实例状态隔离 |
| 图片或按钮不显示 | 当前模板宿主只允许静态结构标签 | 把交互和媒体放进组件 Script Surface，而不是模板 HTML |
| 切换模板后原布局仍在 | 每个模板会保存自己的状态，旧绑定不会跨语义盲目映射 | 在目标模板的插槽中重新选择内容；切回后原状态仍会保留 |
| 版本不满足 `^1.0.0` | 方案记录的版本要求与已安装版本不兼容 | 更新方案版本要求，或安装匹配版本；不要只改目录名 |

## 14. 发布验收清单

- 根目录选择、安装、启用和模板选择均成功。
- `component.json`、贡献和 `template.json` 的 ID/版本完全一致。
- 至少一个插槽接受 `active-surface`。
- 空插槽、必需插槽和折叠行为符合设计。
- 普通宽度、窄窗口、浅色、深色和 125%/150% 缩放无横向溢出。
- 有项目、无项目、切换 Shell 页面、打开/关闭标签时主 Surface 正常。
- 组件 Surface 缺失、停用或崩溃时模板仍可使用或明确回退。
- 修改开发目录后需要显式重载；重载失败保留旧运行版本。
- 切换到另一模板再切回，原绑定和参数保持。
- `.pmc-pack` 在干净环境可安装、卸载和重装。
- `.pmc-profile` 缺包时给出依赖诊断，`.pmc-workspace` 自包含导入遵守签名检查。

## 15. 参考与许可说明

本指南及离线 HTML 的信息架构参考了 [Just the Docs](https://github.com/just-the-docs/just-the-docs) 的“固定导航、内容优先、移动端适配、少依赖”原则。Just the Docs 使用 MIT 许可证。

Nexora 文档没有引入 Jekyll、Ruby Gem、CDN 或 Just the Docs 源码；离线 HTML 是为 Nexora 重新编写的单文件实现。该参考只影响文档组织方式，不改变 Nexora 模板包合同或产品许可证。
