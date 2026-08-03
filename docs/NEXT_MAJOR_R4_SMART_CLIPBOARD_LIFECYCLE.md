# R4 智能剪贴板生命周期接入

> 状态：`verifying`  
> 日期：2026-08-03  
> 基线版本：2.8.4  
> 模块 ID：`builtin.smart-clipboard`

## 1. 本次范围

R4 按领域独立迁移和验收。本次只接入智能剪贴板，不包含局域网、项目资源、Python/插件、任务或渲染中心。

保持原有行为：

- Windows 原生 Win32/GDI 窗口；
- `AddClipboardFormatListener` 后台监听；
- `Ctrl+\`` 全局快捷键；
- 文本、图像和文件历史；
- 最多 500 条、保留 30 天；
- 停用不删除历史数据库和图像载荷。

## 2. 模块声明

模块为软件级全局内置模块，声明：

- Capability：`clipboard.read`、`clipboard.write`；
- 后台服务：`clipboard-listener`、`native-window`、`global-hotkey`；
- 数据策略：停用保留数据，删除必须显式操作；
- 首次出现时在 Windows 默认启用；
- 用户一旦手动停用，后续启动尊重持久化的停用选择。

原生组件使用稳定 ID `builtin.smart-clipboard.native` 注册到 CapabilityGateway，为后续组件化调用保留明确所有者和权限边界。本次不改变现有剪贴板内部实现去直接调用 Gateway；该组件登记用于冻结所有权，细粒度宿主调用在后续统一组件运行时接入。

## 3. 生命周期

### 启用

1. Module Manager 进入 `starting`；
2. 在阻塞线程池启动 Win32 消息线程；
3. 等待原生窗口就绪；
4. 向 ResourceRegistry 登记一个 `native-thread` 聚合资源；
5. 健康检查确认线程和窗口仍运行；
6. 模块进入 `running`。

### 停用

1. Module Manager 进入 `stopping`；
2. ResourceRegistry 请求原生窗口销毁；
3. `WM_DESTROY` 注销剪贴板监听和 `Ctrl+\`` 快捷键；
4. 消息循环退出并注销窗口类；
5. 等待原生线程完成并回收 JoinHandle；
6. 登记资源归零，模块进入 `disabled`。

控制器由不可清空的单值 `OnceLock` 改为可替换的受锁 `Option<Controller>`，因此同一应用进程内支持多次启停。线程异常退出时，下次启用会先回收旧句柄再重新初始化。

## 4. 入口守卫

`open_smart_clipboard` 在调用原生 `show()` 前读取 Module Manager 状态。模块不是 `running` 时返回稳定前缀：

```text
SMART_CLIPBOARD_MODULE_DISABLED
```

因此旧功能中心和固定快捷栏即使暂时仍显示入口，也不能绕过生命周期。R5 Contribution Registry 会再负责根据模块状态自动撤下入口。

## 5. 设置页

设置中的模块页分为：

- 已接入后台模块：当前显示智能剪贴板，可启用、停用、重启和健康检查；
- 隔离诊断模块：保留 R2/R3 故障注入与 100 次泄漏检查。

资源详情会显示智能剪贴板拥有剪贴板监听、原生窗口和快捷键。100 次泄漏检查仍只运行隔离诊断模块，避免自动测试反复抢占用户系统剪贴板和快捷键。

## 6. 自动验证

已覆盖：

- 新模块首次注册读取 `defaultEnabled=true`；
- 已持久化 `desiredEnabled=false` 时不被默认值覆盖；
- 同一进程启动、停用、再次启动和应用退出；
- 运行时 ResourceRegistry 有 1 个聚合资源，停用后为 0；
- 停用后命令守卫拒绝打开，重新启用后守卫放行；
- 停用和退出后 `clipboard_history.db` 保留；
- 热键已被其他进程占用时只记录警告，模块仍正常启动。

自动命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm run build
npm run check:platform-contracts
git diff --check
```

2026-08-03 自动结果：Rust 单元测试 `105 passed`，`cargo check`、`cargo fmt --check`、前端 `npm run build`、平台合同检查和 `git diff --check` 全部通过。

## 7. 人工验收

- [ ] 启动软件后 `Ctrl+\`` 可以打开智能剪贴板；
- [ ] 功能中心点击智能剪贴板可以打开同一个原生窗口；
- [ ] 设置 -> 模块与权限中停用智能剪贴板；
- [ ] 停用后窗口关闭，`Ctrl+\`` 不再响应；
- [ ] 停用后从旧入口打开会显示模块已停用错误；
- [ ] 再次启用后快捷键和入口恢复；
- [ ] 停用前的文本、图像和文件历史仍能检索与恢复；
- [ ] 连续重启模块 5 次，无重复窗口、残留线程或资源计数增长；
- [ ] 退出 PM Center 后没有残留 `pm-center-smart-clipboard` 线程所属进程。

## 8. 回滚与已知限制

- 可在设置页停用模块，历史数据不会删除。
- R5 完成前，功能中心入口不会随模块停用自动消失，而是由后端命令守卫拒绝调用。
- CapabilityGateway 已登记模块和原生组件，但现有 Win32 内部读写剪贴板尚未改造成逐次 token 调用；这属于 R9 统一组件运行时的边界，不在本次迁移中扩大范围。
- 当前使用一个 ResourceRegistry 聚合资源代表同一原生线程拥有的监听、窗口和快捷键，避免对不可独立释放的 Win32 资源伪造三个清理句柄。
