# R7 本机网页控制台

更新时间：2026-08-05

## 1. 定位

`builtin.local-web-console` 是一个默认停用的软件级模块。它让用户通过真实浏览器访问 Nexora 的轻量控制面，用于：

- 查看版本、当前装配方案、模块健康状态和活动资源；
- 显示或隐藏主窗口；
- 修改少量常规设置；
- 受确认保护地重启或退出 Nexora。

它不是完整桌面 UI 的 Web 版本，也不是远程管理服务。项目文件、渲染编辑、局域网聊天、Shell、插件命令和任意 Tauri `invoke` 不会通过该服务开放。

## 2. 安全边界

- 监听地址固定为 `127.0.0.1`，配置只能修改端口，不能改成 `0.0.0.0` 或局域网地址；
- 默认端口为 `31530`，有效范围为 `1024-65535`；
- 首次读取配置时生成持久 UUID 访问令牌，保存在应用数据目录的 `local-web-console.json`；
- 令牌通过 URL fragment 交给浏览器，随后保存到 `sessionStorage` 并从地址栏移除；
- 所有 `/api/*` 请求必须携带 Bearer token；
- 后端只注册固定白名单路由，没有通用命令转发、文件系统、Shell、Python 或插件桥；
- 页面设置 CSP、`no-store`、`nosniff` 和 `no-referrer` 等响应头；
- 设置写入、程序重启和程序退出可分别在桌面设置中关闭。

第一版不支持局域网访问。未来若要增加其他设备访问，必须另立设计，增加设备配对、TLS、权限分级、撤销信任和审计，不能直接放宽当前监听地址。

## 3. 生命周期与持久化

- 模块由 Module Manager 启停，HTTP listener 登记到 ResourceRegistry；
- 停用模块、切换装配方案、退出和重启都会等待 listener 优雅停止；
- 端口或权限修改后需要重启该模块；
- 启停入口会同步更新当前 Profile 的模块选择，保证浏览器触发应用重启后服务仍能按原端口恢复；
- 访问令牌不会在模块重启或应用重启时变化，浏览器可以使用同一会话重新连接；
- 服务启动失败时模块进入错误状态，并在模块恢复/诊断入口中显示端口占用等原因。
- 详细设置作为 `builtin.local-web-console` 的运行时 `settingsSections` 贡献挂载；模块停用后该设置区立即撤下，重新启用通过恢复内核或模块诊断完成。

## 4. 第一版网页 API

| 路由 | 方法 | 说明 |
| --- | --- | --- |
| `/health` | GET | 无敏感信息的服务存活检查 |
| `/api/bootstrap` | GET | 状态、模块、Profile、资源和可编辑设置 |
| `/api/settings` | PUT | 写入允许公开的四项常规设置 |
| `/api/window/show` | POST | 显示并聚焦主窗口 |
| `/api/window/hide` | POST | 隐藏主窗口 |
| `/api/app/restart` | POST | 确认后清理模块并重启应用 |
| `/api/app/exit` | POST | 确认后清理模块并退出应用 |

网页可编辑设置固定为：

- 启动后恢复上次会话；
- 关闭项目标签页时确认；
- 关闭文件标签页时确认；
- 项目根目录。

后端更新 `settings.json` 时保留其他未知字段，并通过 `pm-center:local-web-settings-changed` 通知桌面端 Store 同步内存状态。

## 5. 代码入口

- 后端服务：`src-tauri/src/local_web_console.rs`
- 内嵌网页：`src-tauri/src/local_web_console/page.html`
- 模块生命周期：`src-tauri/src/platform/builtin_modules.rs`
- Profile 同步：`src-tauri/src/platform/profile_runtime.rs`
- 前端 API：`src/api/localWebConsole.ts`
- 桌面设置贡献：`src/components/settings/LocalWebConsoleSettingsSection.tsx`
- 设置贡献宿主：`src/components/settings/settingsContributionImplementationRegistry.ts`
- 功能中心：`src/features/builtinTools.ts`

## 6. 验收

自动验收：

- Rust 单元测试、`cargo check --no-default-features`；
- TypeScript 与 Vite build；
- 平台合同检查和贡献 ID 唯一性；
- `git diff --check`；
- 配置写入保留未知设置字段；
- 模块默认关闭并只声明 localhost 暴露范围。

桌面与浏览器验收：

1. 从装配与权限/恢复入口启用模块，确认“网页控制台”设置贡献和功能中心工具同时出现；
2. 点击“打开浏览器”，确认地址为 `127.0.0.1:31530` 且 fragment 很快从地址栏消失；
3. 验证状态、设置页、显示/隐藏主窗口；
4. 关闭某项网页权限，重启服务，确认对应动作变为不可用；
5. 修改端口并重启服务，确认旧端口释放、新端口可访问；
6. 在浏览器确认重启 Nexora，确认页面可自动重新连接；
7. 停用模块，确认端口释放，功能中心入口和详细设置贡献同时撤下；
8. 使用无令牌的新浏览器会话直接访问地址，确认无法读取 API 数据；
9. 在窄窗口和系统深色主题下检查布局与文字无重叠。
