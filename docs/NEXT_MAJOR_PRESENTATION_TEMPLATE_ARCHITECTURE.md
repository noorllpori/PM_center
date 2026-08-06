# Nexora 界面表现模板与可替换 Shell

> 状态：`planned`
>
> 日期：2026-08-06
>
> 前置：R7 装配编辑器、R8 Profile 包合同

## 1. 目标

Nexora 的 Profile 不只决定启用哪些模块，也应决定软件启动后采用什么外壳、主页和页面结构。现有“顶部 / 侧边 / 紧凑”只是三个内置兼容模板，不再作为界面系统的固定边界。

用户可以安装模板包，从预设开始调整复杂布局，并将模板选择、变量、插槽映射和主题一并保存在 Profile 中。模板负责结构和表现，模块、组件、命令和数据源继续负责功能。

目标不是给功能代码套一层换色皮肤，而是允许同一套平台能力装配成明显不同的产品：

- 项目管理器；
- 图像与媒体资料管理器；
- 局域网通信端；
- Blender 外部渲染控制台；
- 以后安装的其他专业工作台。

## 2. 核心原则

1. **主页就是被选择的 Surface**：`shellLayout.home` 指向哪个 Surface，哪个 Surface 就占据 Shell 的主页宿主，不再额外打开一个同名标签。
2. **不存在具有业务特权的“原始主页”**：现有项目主页降级为 `builtin.project-manager` 提供的普通 Surface；只有不可撤下的恢复 Surface 保留宿主特权。
3. **HTML 负责结构，不负责越权执行**：模板可以用受控 `base.html` 重绘布局，但不能携带任意 JavaScript、React 代码或直接调用 Tauri。
4. **功能通过稳定插槽进入模板**：导航、工具栏、Surface、Widget、命令、状态和数据源由宿主注册后挂载。
5. **模板与功能解耦**：停用模块时，其贡献从模板插槽撤下；模板本身不能让已停用功能重新出现。
6. **所有正式模板可安装、卸载和升级**：内置模板也登记为模板包，只是默认随 Nexora 分发。
7. **配置可移植**：Profile 保存模板引用和参数；装配空间可以连同模板包及资源一起导出。
8. **永远可恢复**：模板缺失、损坏、版本不兼容或无法渲染时回退最小安全 Shell，不能白屏。

## 3. 分层模型

### 3.1 ShellTemplate

控制整个应用框架，包括：

- 品牌区和窗口控制；
- 顶部栏、侧栏或其他导航结构；
- 功能中心、搜索和快捷工具；
- 主 Surface 宿主；
- 标签、任务状态、通知和覆盖层；
- 无边框窗口拖动区域；
- 普通和窄窗口的结构变化。

现有模式映射为：

| 兼容字段 | 内置模板 ID |
| --- | --- |
| `top-bar` | `builtin.shell.top-bar` |
| `side-bar` | `builtin.shell.side-bar` |
| `minimal` | `builtin.shell.compact` |

`navigationKind` 在模板运行时完成前继续作为兼容回退。新编辑的 Profile 同时写入 `shellTemplate`；模板运行时稳定后停止新增对三枚举的业务分支，但仍长期读取旧 Profile。

### 3.2 PageTemplate

控制单个 Surface 的复杂页面结构，例如：

- 仪表盘网格；
- 目录树、内容区和详情栏；
- 聊天联系人、会话和文件面板；
- 渲染队列、帧列表、日志和性能面板；
- 图像瀑布流、分类树、预览和备注栏。

Widget 通过既有 `region` 字段进入模板声明的命名区域。模板可以改变区域结构和响应式规则，但不能伪造未注册 Widget。

### 3.3 ThemePreset

只描述视觉令牌，不决定页面功能：

- 颜色和语义状态色；
- 字体、字号和行高；
- 间距与密度；
- 边框、圆角和阴影；
- 图标集；
- 动画时长及“减少动态效果”策略。

Shell 和单个 Surface 均可引用主题。Surface 主题覆盖只允许修改声明为可覆盖的令牌。

### 3.4 TemplatePackage

模板包是可安装组件包的一种表现资源，建议使用 `data-pack` 运行时并声明模板贡献。它没有更高等级，也不会因为能控制外观而自动获得文件、网络或进程权限。

建议结构：

```text
manifest.json
presentation/
  shell/
    studio-workbench/
      template.json
      base.html
      styles.css
      preview.png
  surfaces/
    media-library/
      template.json
      base.html
      styles.css
  themes/
    graphite/
      theme.json
      preview.png
  assets/
    icons/
    images/
```

实际 `base.html`、CSS 和资源属于模板包。Profile 保存模板 ID、版本约束、变体、参数和插槽配置；需要形成自包含交付时，由 `.pmc-workspace` 携带经过检查的模板包，不把大段 HTML 重复内联到每个 Profile。

## 4. Profile 合同

schema v1 以向后兼容可选字段增加模板绑定：

```json
{
  "shellLayout": {
    "home": "media-home",
    "navigation": ["media-home", "lan-main"],
    "pinnedTools": ["builtin.render-center.tool"],
    "navigationKind": "top-bar",
    "shellTemplate": {
      "id": "vendor.shell.studio-workbench",
      "versionRequirement": "^1.0",
      "variant": "wide",
      "settings": {
        "sidebarWidth": 264,
        "density": "compact"
      }
    },
    "themePreset": {
      "id": "vendor.theme.graphite",
      "versionRequirement": "^2.1"
    }
  },
  "surfaces": [
    {
      "id": "media-home",
      "kind": "dashboard",
      "layout": "contribution-defined",
      "template": {
        "id": "vendor.surface.media-library",
        "versionRequirement": "^1.4",
        "variant": "catalog"
      },
      "widgets": [
        {"id": "folders", "widget": "media.folder-tree", "region": "sidebar"},
        {"id": "assets", "widget": "media.asset-grid", "region": "content"},
        {"id": "details", "widget": "media.inspector", "region": "inspector"}
      ]
    }
  ]
}
```

`ProfilePresentationBinding` 统一包含：

- `id`：模板或主题的稳定 ID；
- `versionRequirement`：SemVer 约束；
- `variant`：包内预设变体；
- `settings`：通过模板 schema 校验的普通 JSON 参数。

模板选择是 Profile 的一部分。模板实现和资源是组件目录的一部分，二者不能混为未经检查的任意 HTML 字符串。

## 5. 受控 HTML 与宿主插槽

### 5.1 Shell 强制节点

每个 Shell 模板必须提供：

- `<pm-surface-host>`：当前主页或 Shell 页面；
- `<pm-overlay-host>`：对话框、菜单和 HelpAssistant；
- `<pm-window-controls>`：最小化、最大化和关闭；
- 至少一个 `<pm-window-drag-region>`；
- `<pm-recovery-entry>`：恢复入口，允许在正常视觉中隐藏到系统菜单，但不能删除；
- 可选 `<pm-navigation>`、`<pm-toolbar>`、`<pm-tabs>`、`<pm-task-status>` 和 `<pm-notification-center>`。

缺少强制节点的包在安装和应用前被拒绝。

### 5.2 Surface 插槽

PageTemplate 在 `template.json` 中声明命名区域、允许的内容种类、最小/最大实例数、响应式行为和空状态。宿主只把当前 Profile 已声明且所属模块可用的 Widget/Contribution 放入对应区域。

### 5.3 允许的表达能力

- 白名单 HTML 元素和 `pm-*` 宿主元素；
- CSS 自定义属性和经过检查的样式表；
- 包内相对资源；
- 受控条件、循环和文本绑定；
- 注册命令 ID 的事件绑定；
- 主题令牌和模板参数；
- 宿主提供的可访问性、焦点和键盘导航属性。

### 5.4 明确禁止

- `<script>`、内联事件属性、`eval` 和动态模块加载；
- 任意 React 组件注入；
- 未经登记的 WebView、iframe 或远程页面；
- `file://`、任意绝对路径和包外资源；
- 从 CSS 发起任意外网请求；
- 直接调用 `window.__TAURI__`、原始 invoke、文件系统、网络和进程 API；
- 用覆盖层遮挡恢复入口、权限确认和系统窗口控制；
- 模板自定义代码绕过 CapabilityGateway。

## 6. 渲染流程

```text
读取 Profile
  -> 解析模板引用和版本
  -> 检查组件已安装且已启用
  -> 校验 manifest、哈希、签名和模板 schema
  -> 解析并净化 base.html / CSS
  -> 编译为受控渲染树
  -> 注入宿主插槽和主题令牌
  -> 绑定当前 Surface、Widget、命令和数据源
  -> 挂载
```

模板必须在 staging 中完成解析和预览。只有整棵渲染树通过检查后才能替换当前 Shell；失败继续保留旧 Shell，不能先卸载旧界面再尝试新模板。

模板缓存以包摘要、模板 ID、版本和宿主 API 版本为键。升级模板时旧编译缓存失效，但 Profile 参数和 Widget 数据不被删除。

## 7. 主页和导航规则

- `shellLayout.home` 是唯一静态主页来源。
- 主页 Surface 直接渲染进 `<pm-surface-host>`，不是先进入固定主页再打开标签。
- 导航点击当前主页只聚焦主页，不创建副本。
- 导航点击其他单例 Surface 只打开或聚焦一个实例。
- 模板可以不显示传统导航，但必须保留功能中心或恢复入口中的可达路径。
- `shellLayout.home` 缺失、模板缺失、Surface 所属模块停用或渲染器失败时进入最小安全 Shell。
- 现有项目主页只是项目管理器提供的一个可选 Surface；停用项目管理器后不会保留空壳主页。

## 8. 装配编辑器

编辑器后续增加“界面表现”层级：

1. 选择 Shell 模板和变体；
2. 选择全局主题；
3. 选择启动主页 Surface；
4. 将导航、工具、标签和状态区放入 Shell 插槽；
5. 为每个 Surface 选择 PageTemplate；
6. 将 Widget 拖入命名区域；
7. 编辑模板参数、主题令牌和响应式规则；
8. 在桌面、窄窗口、浅色和深色模式实时预览；
9. 保存前检查缺失模板、未知插槽、不可达页面、溢出和恢复入口；
10. 应用失败时保留原 Profile 和原 Shell。

高级用户可以编辑受控 HTML/CSS 文件，但编辑器必须显示允许语法、实时诊断和净化后的真实预览。普通用户主要使用模板、预设和可视化布局，不要求理解 HTML。

## 9. 安装、导出与卸载

- `.pmc-profile` 保存模板引用、版本约束和参数，不携带二进制或未检查资源。
- 导入预览列出缺失模板、版本冲突、主题和受影响 Surface；缺失依赖时只允许保存为阻塞草稿，不能应用。
- `.pmc-workspace` 可以携带锁定的模板包和包摘要，导入时仍经过 R10 包检查，不直接解压到运行目录。
- `.pmc-pack` 可以单独分发模板、主题、图标和素材。
- 卸载正在被 Profile 引用的模板前显示引用方案；当前 Profile 正在使用时必须先切换到安全模板或其他有效模板。
- 卸载模板不删除 Profile；Profile 保留引用并进入可解释的 blocked 状态，重新安装兼容版本后恢复。

## 10. 分阶段实施顺序

### R8-P：合同与可移植引用

- 增加 `ProfilePresentationBinding`；
- `shellLayout` 增加 `shellTemplate`、`themePreset`；
- Surface 增加 `template`、`themePreset`；
- `navigationKind` 标记为兼容字段；
- Profile 包检查模板 ID、版本约束和本机路径；
- 文档冻结模板与主页语义。

### R9-P：模板目录和内置适配器

状态：`verifying`（2026-08-06 已实现合同、动态目录和内置适配器登记）。

- 增加 ShellTemplate、PageTemplate、ThemePreset 贡献目录；
- 把顶部、侧边和紧凑登记为三个内置模板；
- 现有运行时先通过适配器继续渲染旧 React Shell；
- 贡献诊断显示模板实现、所有者、版本和引用关系。

### R10-P：模板包与受控渲染器

- 定义 `template.json` 和白名单；
- 实现 HTML/CSS 解析、净化和受控渲染树；
- 实现包内资源协议、缓存、签名和升级；
- 支持安装、卸载、缺失恢复和原子 Shell 切换；
- 加入静态安全分析和恶意模板测试。

### R10-P2：可视化界面装配

- 模板市场/本地包选择；
- 插槽和 Widget 拖放；
- 主题变量编辑；
- 响应式预览；
- HTML/CSS 高级编辑与诊断；
- 导出自包含装配空间。

R12 四个参考装配必须至少使用两种明显不同的 ShellTemplate 和三种 PageTemplate，证明产品差异来自 Profile 与模板，而不是新增固定模式分支。

## 11. 验收

### 自动验收

- 旧 Profile 仅含 `navigationKind` 时保持原界面；
- 新 Profile 的模板绑定 JSON/Rust/TypeScript/Schema 往返不丢字段；
- 非法模板 ID、版本约束和变体被稳定错误码拒绝；
- 缺少强制宿主节点、含脚本、绝对路径、远程资源或越权绑定的模板被拒绝；
- 模板安装、升级、卸载和应用失败不会产生半写入状态；
- 模板缺失或渲染崩溃回退安全 Shell；
- 主页导航不会创建重复 Surface；
- 停用模块后相关 Widget、命令和数据源立即从模板撤下。

### 产品验收

1. 将同一 Profile 在顶部、侧边和紧凑内置模板间切换，功能和会话保持不变。
2. 安装一个工作台模板，确认顶部栏、导航、内容区域和任务状态结构明显改变。
3. 把局域网页面设置为主页，重启后直接进入该页面，不先显示项目主页，也不生成重复标签。
4. 使用媒体模板装配目录树、瀑布流和详情栏，窄窗口切换为单栏浏览。
5. 删除或破坏当前模板包，重启后进入恢复 Shell，并能重新安装或切换模板。
6. 导出装配空间到另一台设备，安装依赖后恢复相同 Shell、页面模板、主题、顺序和参数。
7. 检查浅色、深色、125%/150% 缩放、键盘导航和减少动态效果模式。

## 12. 当前立即落地边界

R8 已把模板绑定加入 Profile 公共合同；R9-P 已增加 ShellTemplate、PageTemplate、ThemePreset 贡献目录，并把三种导航登记为可卸载资料组件的内置适配器。真正读取第三方 `base.html`、安装签名模板包和替换整个 Shell 必须按 R10-P 实施，不能在缺少净化、恢复和包安全机制时直接开放。
