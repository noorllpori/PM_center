# Nexora 示例目录

`examples` 按用途分类。每个可安装示例都以自身目录中的 `component.json` 为入口。

## `components/`

新版 `.pmc-pack` / Python 组件示例：

- `ninniku-music-player`：依赖文件操作组件的音乐播放器。
- `script-project-storage`：项目上下文、Blob 和组件存储。
- `script-automation-blend-audit`：依赖 BlenderIO 的 `.blend` 检查自动化。
- `script-automation-acceptance-suite`：R11 自动化运行、取消、重试和 attention 验收夹具。

## `templates/`

静态 HTML/CSS 界面模板：

- `blank-home-template`：空白主页和单个主页插槽。
- `split-interface-template`：左右分栏、响应式上下布局和宿主拖拽分割。
- `presentation-template-studio`：最小模板合同与安全校验样例。

## `runtime/`

宿主运行时和 ABI 示例：

- `native-library-echo`：Windows x64 原生 DLL 隔离组件。当前仍可用，但只在 Windows x64 上构建和安装；运行 `npm run prepare:native-library-example:dev` 会重新构建 DLL，安装后由 `pmc-component-host` 执行健康调用。

## `plugins/`

旧版 Python 插件系统样例。它们使用 `plugin.json` 和旧插件 API，不是新版组件合同。
