# PM Center 2.8.4 当前行为与资源基线

> 对应里程碑：R0  
> 状态：verifying，静态与自动验证已完成，运行时与产品流程待验收  
> 基线日期：2026-08-03  
> 接口快照：`docs/baselines/pm-center-2.8.4-interfaces.json`  
> 生成命令：`npm run audit:baseline:write`  
> 漂移检查：`npm run audit:baseline:check`

## 1. 目的

本文件冻结下一超大版本重构前的实际结构和行为。后续模块化、装配方案、组件运行时和节点工作流开发都要以此判断：

- 哪些行为是当前兼容要求；
- 哪些资源目前无条件启动；
- 哪些资源已有释放入口；
- 哪些是当前已知缺陷，不能误判为重构引入；
- 哪些静态接口变化需要明确审查。

R0 不修改现有产品行为，只建立可重复的观察和验收基线。

## 2. 自动接口快照

### 2.1 统计

| 项目 | 2.8.4 基线 |
| --- | ---: |
| Rust 顶层模块 | 18 |
| 已注册 Tauri 命令 | 152 |
| `#[tauri::command]` 定义 | 152 |
| 前端字面量 `invoke` 目标 | 117 |
| Rust `pm-center:*` 事件字符串 | 19 |
| 前端字面量事件订阅 | 15 |
| 内置工具 | 10 |
| 工作区标签类型 | 10 |
| Shell 标签类型 | 3 |

一致性结果：

- 已注册但找不到定义：0；
- 已定义但未注册：0；
- 前端字面量调用但未注册：0；
- 已注册但没有前端字面量调用：35。

最后一项不能直接视为死代码。部分命令可能通过动态 wrapper、兼容入口或尚未被当前 UI 触发。删除前必须逐个追踪调用路径和兼容用途。

### 2.2 快照限制

- 只识别命令/事件名称为直接字符串字面量的前端调用；
- Rust 事件统计基于 `pm-center:*` 字符串，不能证明每个字符串运行时一定发出；
- 动态拼接的事件和命令需要人工审计；
- 快照用于发现接口漂移，不替代 Rust/TypeScript 编译和运行时测试。

## 3. 当前顶层代码结构

Rust 顶层模块：

```text
cache_manager
db
file_details
fs
icon_extractor
link_preview
p2p
plugin
process_utils
python
python_env
render_center
smart_clipboard
task
thumbnail_cache
tools
tree_cache
watcher
```

前端主路径：

```text
main.tsx
  -> App.tsx
    -> FileManager
      -> Shell tabs
      -> Project sessions
      -> Workspace tabs
      -> Settings / task / tools dialogs
    -> WindowManager
    -> FileOperationPanel
```

## 4. 当前前端入口基线

### 4.1 Shell 标签

```text
home
project
lan
```

### 4.2 项目工作区标签

```text
files
cache
render
p2p
directory
image
text
video
blend
collection
```

### 4.3 内置工具

```text
render-center
cache-manager
p2p-chat
p2p-project
python-environments
task-center
settings
mdt-overview
blender-file-parser
smart-clipboard
```

当前事实：

- 工具定义集中在 `src/features/builtinTools.ts`；
- Pin 偏好保存在 `builtin-tools.json`；
- 打开工具仍由 `src/components/file-manager/index.tsx` 中的 `openBuiltinTool()` switch 路由；
- 工作区和 Shell 标签都是固定 TypeScript 联合类型；
- Pin 只控制入口，不控制后台模块状态。

## 5. 应用启动行为

### 5.1 Rust `run()` 启动

主进程启动时注册：

- Tauri autostart、opener、dialog、fs、store 和 notification 插件；
- `Ctrl+Alt+S` 全局唤醒快捷键；
- single-instance 插件；
- 全局项目数据库连接注册表；
- 系统托盘和显示/隐藏/退出菜单；
- 主窗口关闭时隐藏而不是退出。

Rust `setup` 当前无条件执行：

1. 保存 watcher 的 AppHandle；
2. 初始化 Windows 智能剪贴板；
3. 启动每 2 秒一次的脏目录修复循环；
4. 创建托盘；
5. 显示并聚焦主窗口。

### 5.2 React `App` 启动

非独立窗口启动时当前无条件执行：

1. 加载任务持久化状态；
2. 注册任务事件监听；
3. 注册渲染事件监听；
4. 初始化局域网协作；
5. 设置当前浅色主题状态。

重要基线：即使用户没有打开局域网页面，`useLanCollaborationStore.initialize()` 也会在主 App 挂载时调用；后端初始化成功后会尝试启动发现服务。

## 6. 长期资源与所有者

| 资源 | 当前所有者 | 启动时机 | 当前停止方式 | 下一代迁移关注点 |
| --- | --- | --- | --- | --- |
| `Ctrl+Alt+S` | Tauri global shortcut | Rust `run()` | 应用退出 | Kernel 资源，通常常驻 |
| 系统托盘 | `lib.rs` | Rust `setup` | 应用退出 | Kernel 资源 |
| 智能剪贴板线程/窗口 | `smart_clipboard/windows.rs` | Rust `setup` 无条件 | `smart_clipboard::shutdown()` | 迁入 `clipboard.windows` 模块 |
| `Ctrl+\`` | 智能剪贴板 Win32 窗口 | 剪贴板初始化 | `UnregisterHotKey` | 模块停用必须注销 |
| Clipboard listener | 智能剪贴板 Win32 窗口 | 剪贴板初始化 | 窗口销毁 | 模块停用必须移除 |
| 脏目录修复循环 | `lib.rs` + `tree_cache` | Rust `setup` 无条件 | 当前仅随进程退出 | 需要取消令牌和资源登记 |
| 项目 watcher | `watcher` | `init_project/activate_project` | `release_project_resources` 或切换活动项目 | 当前只监听活动项目 |
| watcher worker 线程 | `watcher` | 激活项目 | drop watcher 后 join | 已有相对完整释放入口 |
| 项目 `Database` | `DbState` | 首次项目命令 | `release_project_resources` | 纳入项目模块资源 |
| TreeCache 连接 | `tree_cache` registry | 打开/激活项目 | `release_project_cache` | 纳入项目模块资源 |
| LAN UDP 31523 | `p2p` | App 初始化 LAN | `stop_lan_discovery` 仅切状态 | 需确认 listener 是否真正释放 |
| LAN TCP 31524 | `p2p` | App 初始化 LAN | `stop_lan_discovery` 仅切状态 | 当前 server task/listener 生命周期需重构 |
| LAN 发现循环 | `p2p` | `start_lan_discovery` | `is_running=false` | 需统一取消和 join |
| PMC Server 连接 | `p2p/server_client` | 设置启用/连接 | disconnect/cancel sender | 迁入 `lan.server-bridge` |
| LAN 文件传输任务 | `p2p/transfer` | 发送/接受 | transfer cancellation | 已有取消基础，需统一资源登记 |
| 任务子进程 | `task` | 运行任务/插件 | `cancel_task` | 迁入 ComponentRuntime |
| 渲染调度状态 | `render_center::RUNTIME` | 继续队列/任务 | `shutdown_all` 或任务暂停/取消 | 迁入 `render.batch` 生命周期 |
| Blender Worker/进程树 | `render_center` | 任务开始 | pause/cancel/shutdown | 已有 PID 树终止基础 |
| FFmpeg 打包进程 | `render_center` | 视频打包 | 操作取消/进程结束 | 作为任务/组件资源登记 |

## 7. 数据位置与所有权

### 7.1 项目目录

| 路径 | 当前所有者 | 保护规则 |
| --- | --- | --- |
| `.pm_center/data.db` | db、render_center | 用户/项目状态，不自动删除 |
| `.pm_center/tree_cache.db` | tree_cache、file_details | 索引可重建，解析缓存可清理 |
| `.pm_center/thumbnails/` | thumbnail_cache | 可清理并按需生成 |
| `.pm_center/scripts/` | 项目脚本 | 用户数据，受保护 |
| `.pm_center/plugins/` | 项目插件 | 用户数据，受保护 |
| `.pm_center/render_jobs/` | render_center | Worker、日志和运行资料，受保护 |
| `renders/` | render_center/用户 | 业务输出，绝不当缓存清理 |

### 7.2 应用数据目录

| 数据 | 当前文件/目录 |
| --- | --- |
| 应用设置 | `settings.json` |
| 应用会话 | `app-session.json` |
| 功能 Pin | `builtin-tools.json` |
| 任务状态 | `tasks.json` |
| Python 环境偏好 | `python-envs.json` |
| 旧启动器数据 | `launcher.json` |
| 旧 P2P 设置 | `p2p-settings.json` |
| LAN 数据 | `lan_collaboration.db`、`lan_collaboration/` |
| 智能剪贴板 | `smart_clipboard/clipboard_history.db` 及载荷 |
| 渲染调度设置 | `render-scheduler.json` |

## 8. 关键流程基线

### 8.1 打开项目

```text
选择/恢复项目
  -> init_project 或 activate_project
  -> 创建 .pm_center 支持目录
  -> 初始化渲染存储
  -> 获取/创建项目 Database
  -> 获取/创建 TreeCache
  -> 停止旧活动 watcher
  -> 对新项目递归监听
  -> 恢复项目工作区会话
```

### 8.2 关闭项目标签

```text
关闭 Shell project tab
  -> 取消会话持久化订阅
  -> 从前端 session registry 移除
  -> invoke release_project_resources
  -> 若是活动 watcher 则停止并 join worker
  -> 移除项目 Database
  -> 释放 TreeCache
```

当前限制：渲染中心可能仍持有独立运行状态，项目资源释放和渲染任务生命周期尚未由统一所有者协调。

### 8.3 局域网初始化

```text
主 App 挂载
  -> initialize_lan_collaboration
  -> 创建/迁移 lan_collaboration.db
  -> 恢复传输状态并清理 staging
  -> 加载/创建个人资料
  -> 初始化 PMC Server client
  -> start_lan_discovery
  -> 绑定 UDP 31523 和 TCP 31524
```

当前限制：这是 App 级无条件初始化，不取决于局域网页面或模块开关。

### 8.4 渲染任务

```text
创建批次/任务（保持等待）
  -> 用户开始/继续队列
  -> 调度器事务领取作业
  -> 按全局进程上限分配 Worker
  -> persistent 或 isolated Blender
  -> claimToken 领取帧
  -> 临时输出
  -> 校验并原子提交
  -> 更新帧、作业、批次、ETA 和性能
```

兼容规则：创建、排序、重试、重渲染和编辑设置都不自动开始；只有明确开始/继续动作启动执行。

### 8.5 Python 插件动作

```text
扫描 plugin.json
  -> 检查 enabled 和动作
  -> 解析 PMC 内置 Python
  -> 写入临时 request JSON
  -> 启动 Python entry
  -> 读取 stdout/stderr
  -> 解析 @pmc 控制消息
  -> 任务完成并清理 request 文件
```

当前限制：manifest `permissions` 会传给插件请求，但后端没有基于这些权限限制实际文件/网络/进程访问。

### 8.6 智能剪贴板

```text
Rust setup
  -> 启动 Win32 消息线程
  -> 注册窗口类和隐藏窗口
  -> AddClipboardFormatListener
  -> RegisterHotKey Ctrl+`
  -> 写入全局历史数据库
  -> 功能中心/快捷键显示原生窗口
```

## 9. 当前已知问题，作为重构前基线

以下问题在下一代改造前已经存在，验收时需要区分“保持”“修复”或“另立任务”：

- 大尺寸文件、批量导入和首张预览可能有明显等待；
- 局域网在部分双机环境仍需要继续验证发现、消息和传输稳定性；
- 局域网协作目前随主 App 自动初始化，不符合未来按装配启停目标；
- `stop_lan_discovery` 主要切换状态，长期 socket/task 的完整释放需要专门验收；
- 智能剪贴板和脏目录循环在 Rust setup 中无条件启动；
- 插件 `permissions` 尚未构成强制权限边界；
- 152 个 Tauri 命令全部静态暴露，没有模块状态守卫；
- 工具打开、Shell 标签和工作区标签仍有硬编码注册点；
- 多 Blender Worker 受显存和驱动稳定性影响，需要真实项目验证；
- 35 个已注册命令没有被静态审计发现前端字面量调用，必须分类后才能精简。

## 10. R0 运行时测量表

下面数据必须使用同一台测试机、同一构建配置重复测量。尚未测量项不得填写估计值。

| 场景 | 启动耗时 | PM Center 内存 | 空闲 CPU | 端口 | 子进程 | 状态 |
| --- | ---: | ---: | ---: | --- | --- | --- |
| 主 App，无项目 | 待测 | 待测 | 待测 | 31523/31524 待实测 | 待测 | pending |
| 打开普通项目 | 待测 | 待测 | 待测 | 同上 | 待测 | pending |
| 打开局域网页面 | 待测 | 待测 | 待测 | 同上 | 待测 | pending |
| 渲染队列空闲 | 待测 | 待测 | 待测 | 同上 | 待测 | pending |
| 单 Worker 渲染 | 待测 | 待测 | 待测 | 同上 | Blender | pending |
| 关闭项目后 | 待测 | 待测 | 待测 | 同上 | 待测 | pending |

### 10.1 实测样本：恢复会话

2026-08-03 使用 `npm run tauri dev` 启动 2.8.4 开发构建。当前 `app-session.json` 恢复 3 个项目标签，活动 Shell 页面为局域网页面。稳定 8 秒采样结果：

| 项目 | 实测值 |
| --- | ---: |
| PM Center 进程树 | 7 个进程（1 个主进程、6 个 WebView2） |
| 进程树 Working Set | 483.97 MiB |
| 进程树 Private Memory | 385.55 MiB |
| 主进程 Working Set | 62.12 MiB |
| 进程树线程 | 296 |
| 进程树句柄 | 3809 |
| 32 逻辑 CPU 下 8 秒平均 CPU | 0.036% |
| UDP | `0.0.0.0:31523` |
| TCP 监听 | `0.0.0.0:31524` |
| 服务器连接 | 到配置 PMC Server 的 2 条已建立 TCP 连接 |
| PM Center 启动的 Blender/Python/FFmpeg | 0 |

采样后终止开发实例，`pm_center` 进程、31523/31524 和 Vite 1420 监听均已消失。机器上另有用户自行启动的 Blender/Python 进程，本表通过进程树归属排除，没有计入 PM Center。

这组数据不能替代“无项目”“单 Worker 渲染”和“关闭项目后”的独立样本。尝试通过临时 `APPDATA` 隔离会话时，Tauri 仍使用 Windows Known Folder，因此没有搬动真实应用数据强行制造无项目状态；其余样本保留为后续人工/专项验收，不填估计值。

## 11. R0 产品流程验收清单

### 应用与项目

- [ ] 新启动主窗口、托盘和 `Ctrl+Alt+S` 正常；
- [ ] 打开项目、切换项目、关闭项目和会话恢复正常；
- [ ] 关闭项目后文件句柄和 watcher 符合当前设计；
- [ ] 文件树、列表、搜索、集合、标签和预览可用。

### 局域网

- [ ] 主 App 启动后当前自动发现行为已实测；
- [ ] 两台设备互相发现；
- [ ] 大厅和私聊双向可见；
- [ ] 文件和目录传输、拒绝、取消和重新接收可用；
- [ ] Windows 通知、提示音和接收目录可用。

### Python 与插件

- [ ] PMC 内置 Python 可检测；
- [ ] 现有示例插件可以扫描和执行；
- [ ] 日志、进度、确认、失败和取消可用；
- [ ] 插件依赖安装/卸载行为已记录。

### Blender 渲染

- [ ] 创建任务后保持等待；
- [ ] 开始、暂停、恢复和取消符合当前规则；
- [ ] persistent Worker 连续帧只加载一次文件；
- [ ] 结果、性能、ETA、预览和打包可用；
- [ ] GPU 多开已知限制记录准确。

### 智能剪贴板

- [ ] `Ctrl+\`` 可以打开；
- [ ] 文本、图片、文件历史可恢复；
- [ ] 搜索、删除、Esc 和自动粘贴符合当前规则；
- [ ] 退出应用后快捷键和窗口资源释放。

## 12. R0 退出门槛

R0 转为 `verifying` 前必须完成：

- [x] 可重复的接口审计脚本；
- [x] 2.8.4 接口快照；
- [x] 命令定义、注册和前端字面量调用一致性检查；
- [x] 静态资源所有者清单；
- [x] 关键流程和已知问题清单；
- [x] 前端 build；
- [x] Rust unit tests；
- [x] Rust cargo check；
- [x] 主 App 运行时资源测量（恢复 3 项目标签并打开 LAN 页的一组真实样本）；
- [ ] 产品流程人工验收；
- [ ] 产品确认基线无重要遗漏。

完成自动验证后可以把 R0 更新为 `verifying`。只有运行时测量和产品流程确认完成后，R0 才能标记为 `accepted` 并进入 R1。

## 13. 2026-08-03 自动验收证据

| 检查 | 结果 | 说明 |
| --- | --- | --- |
| `npm run audit:baseline:check` | 通过 | 2.8.4 接口快照与源码一致 |
| `npm run build` | 通过 | TypeScript 和 Vite 构建成功 |
| `cargo test --lib` | 通过 | 80 passed，0 failed |
| `cargo check` | 通过 | 编译检查成功 |
| `git diff --check` | 通过 | 无空白错误 |
| 恢复会话资源采样 | 通过 | 记录完整进程树、CPU、内存、线程、句柄、端口和退出后释放 |

当前非阻断基线警告：

- Vite 提示 `MarkdownEditorSurface` 和主入口等产物存在超过 500 kB 的 chunk，需要在后续性能工作中评估按页面/模块拆包；
- Rust 有 5 组 dead-code 警告，涉及 `ArchivedChange`、部分归档变更方法、`ProjectInfo`、`TreeCacheDb.project_path` 和旧 `process_dirty_dirs`；
- 自动验收时没有运行中的 PM Center 进程，也没有 31523/31524 监听，因此运行时内存、CPU、端口和进程数据保持待测，不使用估计值。
