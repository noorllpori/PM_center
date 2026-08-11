> **历史归档（2026-08-12）**：本文件仅保留为实现与验收追溯资料，不代表当前公开能力或后续开发依据。请先阅读 docs/README.md。
# R4 渲染中心生命周期接入

> 状态：`accepted`
>
> 日期：2026-08-04
>
> 基线版本：2.8.4
>
> 模块 ID：`builtin.render-center`

## 1. 本次范围

本次是 R4 最后一个独立验收领域，统一管理：

- 渲染作业与跨项目调度；
- 常驻 Blender Worker、逐帧兼容进程和渐进多开；
- Worker 性能采样与状态事件；
- Blender 场景检查进程；
- 渲染前后置 Nexora Python 脚本；
- FFmpeg 序列帧视频打包；
- 应用退出与异常中断后的帧状态恢复。

渲染数据库、预设、日志、输出图像和完成帧属于持久数据，停用模块时全部保留。

## 2. 模块声明

模块为软件级全局内置模块，首次升级默认启用，并要求 `builtin.project-resources` 处于运行状态。主要 Capability：

- `render.inspect`；
- `render.queue.read`、`render.queue.write`；
- `render.worker.execute`、`render.result.commit`；
- `process.spawn`、`python.execute`；
- 项目/外部文件读写与应用设置读写。

组件 `builtin.render-center.service` 已登记到 CapabilityGateway。当前静态 Tauri 命令使用模块状态守卫；R9 再将每次 Worker 启动、结果提交和外部路径操作切换为短期 token。

## 3. 运行资源

渲染中心维护独立进程登记表，记录 PID、类型、操作名称和启动时间。以下进程均通过 RAII lease 登记并在退出后撤销：

- `blender-worker`：常驻 Worker；
- `blender-isolated`：逐帧兼容进程；
- `blender-inspect`：场景与帧范围检查；
- `render-hook`：前后置 Python 脚本；
- `ffmpeg-package`：视频打包。

Windows 统一调用 `process_utils::terminate_pid_tree` 终止整个进程树。ResourceRegistry 登记一个 `child-process` 聚合资源，设置页健康检查显示活动作业、活动打包和进程摘要。

## 4. 生命周期

### 启用

1. Module Manager 先确保项目资源模块可用；
2. 渲染中心进入 `running`，开放命令和调度闸门；
3. 登记调度/Worker/打包聚合资源；
4. 恢复数据库读取与诊断能力；
5. 不调用 `kick_scheduler`，所有暂停作业仍需用户手动开始或继续。

### 停用

1. 原子切换到 `stopping`，立即拒绝新命令、新作业领取和新进程登记；
2. 唤醒等待进程预算、渐进启动和重试退避的 Worker；
3. 活动作业设置暂停标记，终止全部 Blender、Python 与 FFmpeg 进程树；
4. `running/committing` 帧恢复为 `pending`，领取令牌、Worker、临时输出和运行性能归零；
5. 运行尝试记录为 `aborted`，不增加帧 `attempts` 和失败数；
6. 等待作业任务、性能采样和视频打包收尾；
7. 模块进入 `disabled`。

用户已经明确取消的作业继续收敛为 `cancelled`；`attention` 作业保持源文件待确认状态。已完成帧与最终输出不回退。

## 5. 命令与调度守卫

稳定错误前缀：

```text
RENDER_CENTER_MODULE_DISABLED
RENDER_CENTER_MODULE_STARTING
RENDER_CENTER_MODULE_STOPPING
```

所有渲染中心 Tauri 命令在应用启动恢复期间最多等待 5 秒。调度器在启动、数据库领取前和 Worker 创建前重复检查模块状态，避免停用竞态启动新 Blender。5/15 秒帧重试和 Worker 启动退避改为可唤醒等待，停用不再被固定睡眠拖住。

新建、重试、重新渲染、编辑和排序仍遵守既有核心语义：只修改等待状态，不会自动开始或暂停。模块重新启用也不自动续跑。

## 6. 数据安全

- 不删除 `render_center.db`、作业、批次、帧、尝试历史、预设或日志；
- 不删除已完成图像、视频和渲染归档；
- 仅删除未提交的 `.pm-part`、打包清单和临时黑帧目录；
- 停用不改变已完成/已跳过帧；
- 源文件 `attention` 状态和指纹继续保留；
- 重新启用读取原数据，但必须由用户点击开始/继续。

## 7. 自动验证

已覆盖：

- 停用状态拒绝进程登记，启用后的进程 lease 能归零；
- 生命周期暂停将运行作业改为 `paused`；
- `running` 帧恢复 `pending`，尝试改为 `aborted`，失败次数不增加；
- 临时输出被清理，完成输出与持久数据不受影响；
- 原有常驻 Worker、渐进多开、驱动崩溃降级、领取令牌、提交恢复、ETA、性能与打包测试继续通过；
- Rust 库测试、编译检查和格式检查通过。

自动命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml --package nexora --check
npm run build
npm run check:platform-contracts
git diff --check
```

2026-08-04 自动结果：Rust 库测试 `117 passed`；`cargo check`、`cargo fmt --check`、前端 TypeScript/Vite 构建、平台合同检查和 `git diff --check` 全部通过。

## 8. 人工验收

2026-08-04 已完成真实任务验收，渲染中心生命周期、作业并发和任务级帧多开行为符合当前设计，R4 最后一个分项转为 `accepted`。

- [x] 设置页显示 `builtin.render-center` 为运行中，资源数为 1；
- [x] 启动一个常驻 Worker、多开为 2 的真实任务，确认渐进启动、性能图和帧进度正常；
- [x] 渲染中停用“渲染中心”，确认 Blender 进程树退出；
- [x] 停用后作业为“已暂停”，运行帧回到等待，失败数和尝试次数没有增加；
- [x] 已完成图像、预设、日志和任务历史仍存在且可读取；
- [x] 停用后渲染命令返回模块停用提示；
- [x] 重新启用后队列保持暂停，不会自行启动 Blender；
- [x] 手动点击继续后从未完成帧继续，不重复覆盖有效完成帧；
- [x] 多开数量受任务设置和全局 Blender 进程上限共同约束；
- [x] 应用退出与模块停用不会遗留仍领取帧的 Worker。

## 9. 已知边界

- R5 完成前，渲染入口不会随模块停用自动撤下，后端会拒绝调用；
- 当前 ResourceRegistry 使用一个聚合资源表示动态 Worker，单个 PID 通过渲染进程登记表诊断；
- 强制终止发生在 Blender 写临时输出期间时，最终输出仍由领取令牌与原子提交规则保护；
- R9 前现有静态命令尚未逐次申请 Capability token。
