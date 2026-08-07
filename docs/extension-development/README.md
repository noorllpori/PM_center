# Nexora 扩展与装配空间开发指南

本目录给两类开发者使用：

1. 只有 Nexora 安装版 EXE，希望在本机制作、测试、签名和分发组件或装配空间的人；
2. 使用 Nexora 源码仓库，希望通过 CLI、示例和自动检查开发组件的人。

它不要求修改 Nexora 主程序。正式扩展以组件目录或签名 `.pmc-pack` 交付，不以散乱 `.py` 文件或直接调用 Tauri command 交付。

## 选择哪种扩展

| 目标 | 首选实现 | 入口文档 |
| --- | --- | --- |
| 项目规则、解析、批处理、自动化和小型界面 | `python-action` 或 `python-worker` + Script Surface | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md) |
| 包装已有命令行程序 | `native-process` | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md) |
| 只交付数据、模板、主题或静态资源 | `data-pack` | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md) |
| 高性能二进制库 | `native-library` | [COMPONENT_PACK_GUIDE.md](COMPONENT_PACK_GUIDE.md)，仅 Windows 隔离宿主 |
| 组合模块、组件、导航、快捷栏、主页和模板 | `.pmc-profile` / `.pmc-workspace` | [WORKSPACE_AUTHORING.md](WORKSPACE_AUTHORING.md) |

## 最短路径：只有安装版 Nexora

1. 在 `Alt+Q -> 脚本自动化` 打开开发者工作台，创建 Python 组件模板到你自己的开发目录。
2. 编辑 `component.json` 与入口脚本；先使用工作台的“校验”。
3. 明确“信任此开发目录”。信任是允许本机 Python 代码运行的安全决定，不是普通安装步骤。
4. 从目录安装/热重载组件，把它加入当前 Profile，手动调试自动化命令和 Script Surface。
5. 使用工作台生成 Ed25519 私钥并打包签名 `.pmc-pack`。私钥只保存在开发者控制的位置，不能放进组件目录、装配空间或 Git。
6. 在另一台干净用户环境中检查包、信任发布者、安装、启用、卸载、重装和 Profile/装配空间导入。

安装版的工作台是脚本组件的首选开发工具；它不依赖安装 Cargo。Native Process/Library 仍需要你自己的编译工具链。

## 源码仓库开发

仓库中提供同一条打包链路和真实示例：

```powershell
# 生成发布者私钥，必须放在仓库外
npm run component:keygen -- D:\Keys\my-publisher.json

# 校验平台合同
npm run check:platform-contracts

# 生成签名组件包
npm run component:pack -- D:\Work\my-component D:\Release\my-component.pmc-pack --key D:\Keys\my-publisher.json --publisher-id com.example --publisher-name "Example Studio" --license MIT
```

可先阅读：

- `examples/script-automation-blend-audit/`：Python、BlenderIO 依赖、事件、组件状态和 Script Surface；
- `examples/script-automation-acceptance-suite/`：自动化验收夹具；
- `examples/presentation-template-studio/`：Shell 模板资料包；
- `examples/native-library-echo/`：隔离 DLL ABI 示例。

## 公开边界

以 [INTERFACE_REFERENCE.md](INTERFACE_REFERENCE.md) 的 Schema、SDK 与支持矩阵为准。组件不得依赖：

- 任意 Tauri command 名称或应用数据库表；
- Nexora React DOM、前端 Store、文件组件路径或本地安装目录；
- 未在 `requiresComponents`/`optionalComponents` 中声明的其他组件；
- 未申请和获批的 Capability；
- 私钥、凭据、绝对用户路径或项目源文件被自动打进包。

第三方 UI 使用沙箱 Script Surface；不能把 React、任意 JavaScript 或 CSS 注入 Nexora 宿主页面。
