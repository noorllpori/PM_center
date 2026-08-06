# Nexora 隔离原生组件示例

`npm run tauri dev` 和 `npm run build` 会自动构建本目录的 DLL 并复制到 `bin/windows-x64/`。该二进制不提交到 Git。

在 Nexora 的“恢复设置 -> 组件运行时”中选择“从目录安装”，选择本目录。安装后点击“健康调用”，应返回 `native library isolated`。DLL 仅由 `pmc-component-host.exe` 加载，Nexora 主进程不加载它。
