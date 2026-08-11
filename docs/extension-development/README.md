# Nexora 扩展与装配空间开发指南

本目录面向 **已安装 Nexora EXE 的组件和装配空间开发者**。Nexora 本体是闭源产品：第三方不需要、也不会获得 Nexora 源码仓库、Cargo/Node 构建环境或内部打包 CLI。

正式扩展以开发者自己的组件目录或签名 `.pmc-pack` 交付；创建、校验、信任、调试、签名、安装与装配空间导入均通过 Nexora 安装版完成。不以散乱 `.py` 文件或直接调用 Tauri command 交付。

## 选择哪种扩展

| 目标 | 首选实现 | 入口文档 |
| --- | --- | --- |
| 项目规则、解析、批处理、自动化和小型界面 | `python-action` 或 `python-worker` + Script Surface | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md) |
| 包装已有命令行程序 | `native-process` | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md) |
| 只交付数据、模板、主题或静态资源 | `data-pack` | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md) |
| 开发工具带以下的完整界面布局和插槽 | `data-pack` 界面模板 | [INTERFACE_TEMPLATE_DEVELOPMENT_GUIDE.md](INTERFACE_TEMPLATE_DEVELOPMENT_GUIDE.md) / [离线 HTML](INTERFACE_TEMPLATE_DEVELOPMENT_GUIDE.html) |
| 高性能二进制库 | `native-library` | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md)，仅 Windows 隔离宿主 |
| 组合组件、导航、快捷栏、主页和模板 | `.pmc-profile` / `.pmc-workspace` | [WORKSPACE_AUTHORING.md](WORKSPACE_AUTHORING.md) |
| 让开发者 AI 生成合规组件 | 规范化稳定接口合同 | [AI_COMPONENT_AUTHORING_REFERENCE.md](AI_COMPONENT_AUTHORING_REFERENCE.md) |

需要先建立整体认识时，从 [SECONDARY_DEVELOPMENT_GUIDE.md](SECONDARY_DEVELOPMENT_GUIDE.md) 开始。它把组件、方案、项目、运行时、权限、Component Bridge、文件操作、自动化、UI 和分发流程放在一份独立总览中。

## 最短路径：只有安装版 Nexora

1. 在 `Alt+Q -> 脚本自动化` 打开开发者工作台，创建 Python 组件模板到你自己的开发目录。
2. 点击“VS Code”在外部编辑器中编辑 `component.json` 与入口脚本；内置编辑器仅用于临时小修改。先使用工作台的“校验”。
3. 明确“信任此开发目录”。信任是允许本机 Python 代码运行的安全决定，不是普通安装步骤。
4. 从目录安装/热重载组件，把它加入当前方案，手动调试自动化命令和 Script Surface。
5. 使用工作台生成 Ed25519 私钥并打包签名 `.pmc-pack`。私钥只保存在开发者控制的位置，不能放进组件目录、装配空间或 Git。
6. 在另一台干净用户环境中检查包、信任发布者、安装、启用、卸载、重装和方案/装配空间导入。

安装版工作台是脚本组件的唯一官方开发和发布入口，不依赖 Nexora 本体源码、Cargo、Node 或任何 Nexora 内部命令行工具。`native-process` / `native-library` 的二进制可用你选择的编译器制作，但组件目录的校验、信任、签名、安装和分发仍在 Nexora 中完成。

## 闭源交付边界

- **开发者自有内容**：`component.json`、Python/HTML/CSS/JavaScript、资源、测试、说明，以及自行编译的 EXE/DLL；可保存在你自己的目录或私有版本库。
- **Nexora 安装版提供**：开发者工作台、组件校验器、开发目录信任、运行调试、日志、签名 `.pmc-pack`、组件安装和装配空间导入/导出。
- **不向第三方开放**：Nexora 本体源码、内部 Rust/React 实现、构建脚本、内部 CLI、Tauri command、应用数据库和未文档化组件协议。

工作台创建的组件模板是学习和起步入口。本文档只承诺已列出的公开合同，不要求第三方从 Nexora 仓库复制示例或脚本。

安装版同时提供两个最小案例：项目资料与组件存储，以及依赖 BlenderIO 的文件检查。它们分别对应本仓库的 `examples/script-project-storage` 和 `examples/script-automation-blend-audit` 内容。

界面模板另提供两个互不覆盖的样例：`examples/blank-home-template` 用于理解完全空白的单插槽模板，`examples/split-interface-template` 用于理解左右 50% 布局、组件页面和当前活动页面的组合。

## 公开边界

以 [INTERFACE_REFERENCE.md](INTERFACE_REFERENCE.md) 的 Schema、SDK 与支持矩阵为准。组件不得依赖：

- 任意 Tauri command 名称或应用数据库表；
- Nexora React DOM、前端 Store、文件组件路径或本地安装目录；
- 未在 `requiresComponents`/`optionalComponents` 中声明的其他组件；
- 未申请和获批的 Capability；
- 私钥、凭据、绝对用户路径或项目源文件被自动打进包。

第三方 UI 使用沙箱 Script Surface；不能把 React、任意 JavaScript 或 CSS 注入 Nexora 宿主页面。
