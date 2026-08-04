# R4 任务、Python 与旧插件生命周期接入

> 状态：`verifying`
>
> 日期：2026-08-04
>
> 基线版本：2.8.4
>
> 模块 ID：`builtin.automation-runtime`

## 1. 本次范围

本次是 R4 第四个独立验收领域，管理以下普通自动化进程：

- 任务中心的 Python 内联任务和旧插件任务；
- Python 脚本、Python 文件和普通 pip 命令；
- 系统 Python、PMC 内置 Python 和 venv 探测；
- venv 创建、删除、包安装、卸载和包列表；
- 旧 Python 插件发现、设置、依赖管理和动作执行。

渲染中心的调度器、Blender Worker、性能采样、前后置渲染脚本和任务恢复不属于本模块，按固定顺序留到 R4 最后一个分项。

## 2. 模块声明

模块为软件级全局内置模块，首次升级默认启用。声明的主要 Capability：

- `task.run`、`task.cancel`；
- `python.execute`、`python.packages.manage`；
- `process.spawn`；
- 项目/外部文件读写；
- 应用设置读写和插件依赖可能使用的 HTTP 请求。

组件 `builtin.automation-runtime.service` 已登记到 CapabilityGateway。现有静态命令先使用模块状态守卫和统一进程注册表；R9 统一组件运行时再把每次文件、网络和进程操作切换为短期 token。

## 3. 统一进程注册表

所有受管长短进程在启动后登记：

- 全局唯一登记 ID；
- PID；
- 进程类型，如 Python 任务、pip、venv 或插件动作；
- 用于诊断的操作名称与启动时间。

进程等待结束、启动失败、取消或异常返回时由 RAII lease 自动撤销登记。设置页健康检查显示活动任务数、活动进程数、类型、PID 样本和最长运行时间。

Windows 停止进程统一使用 `taskkill /T /F` 终止整个进程树，避免 Python 或插件派生的子进程残留。渲染中心也复用同一个 `process_utils::terminate_pid_tree`，不再维护第二份 Windows 终止实现。

## 4. 生命周期

### 启用

1. Module Manager 将模块进入 `starting`；
2. 开放任务、Python、venv、pip 和旧插件命令；
3. 向 ResourceRegistry 登记一个 `child-process` 聚合资源；
4. 模块进入 `running`，新进程可以登记。

应用启动恢复期间，命令最多等待 5 秒，不会因为 Module Manager 的异步恢复顺序偶发返回空 Python 或插件列表。

### 停用

1. 原子切换到 `stopping`，立即拒绝新进程登记；
2. 将全部运行中普通任务标记为取消；
3. 终止任务、Python、pip、venv 和旧插件进程树；
4. 等待进程等待者收尾、清理临时脚本和插件请求文件；
5. 确认进程登记归零；
6. ResourceRegistry 清理聚合资源，模块进入 `disabled`。

普通停止最长等待 12 秒。8 秒后仍有进程登记会明确报错并进入可诊断状态，不会无提示显示为已停用。

## 5. 命令守卫与任务状态

稳定错误前缀：

```text
AUTOMATION_RUNTIME_MODULE_DISABLED
AUTOMATION_RUNTIME_MODULE_STARTING
AUTOMATION_RUNTIME_MODULE_STOPPING
```

任务取消的旧竞态已修复：进程等待期间不再提前从任务表移除。用户取消和模块停用都能找到真实 PID；任务中心通过 `task-cancelled` 事件显示“已取消”，不会把模块停用造成的终止误记为普通失败。

用户取消等待 5 秒后进程仍未退出时返回明确错误，前端不能提前假装取消成功。任务超时仍保持“失败/超时”语义，不与用户取消混淆。

## 6. 数据安全

- 停用不删除任务历史、任务日志、全局脚本或项目脚本；
- 停用不删除 venv、已安装包、插件目录、插件设置或插件依赖；
- 任务临时 `.py` 和插件请求 JSON 在正常、失败和取消路径清理；
- 插件依赖先安装到同盘暂存目录，成功后再替换 `vendor/`；安装失败或被停用时保留原依赖；
- 重新启用后继续读取原 Python 环境、venv、插件状态和脚本；
- 旧入口即使仍显示，也不能绕过后端模块守卫启动进程；入口自动撤下属于 R5 Contribution Registry。

普通 venv 内的 pip 不提供事务回滚。安装中途被终止时保留环境供用户检查或重试，不自动删除整个 venv。旧插件的 `vendor/` 依赖目录使用额外暂存和回退保护。

## 7. 自动验证

已覆盖：

- 停用状态拒绝新进程登记；
- 进程 lease 结束后诊断登记归零；
- 启动真实长时间子进程后停用模块，进程树被终止；
- 停用完成后活动进程为 0，命令守卫恢复为 disabled；
- 插件依赖暂存提交成功时替换旧目录，提交失败时恢复旧依赖；
- Rust 编译检查通过；
- 原任务输出、插件控制消息和超时路径保持兼容。

自动命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml --package pm_center --check
npm run build
npm run check:platform-contracts
git diff --check
```

2026-08-04 自动结果：Rust 库测试 `114 passed`；`cargo check`、`cargo fmt --check`、前端 TypeScript/Vite 构建、平台合同检查和 `git diff --check` 全部通过。

## 8. 人工验收

- [ ] 启用模块后，Python 环境页能识别 PMC 内置 Python 和现有 venv；
- [ ] 运行一个持续 20 秒以上的 Python 任务，任务中心能持续收到输出；
- [ ] 点击取消后任务显示“已取消”，任务管理器中没有对应 Python 子进程；
- [ ] 再运行一个长任务，在“模块与权限”中停用“任务、Python 与旧插件”；
- [ ] 停用完成后任务显示“已取消”，模块资源数为 0；
- [ ] 停用后新建任务、运行脚本、pip/venv 操作和插件动作返回模块停用提示；
- [ ] 停用后任务历史、脚本、venv、插件设置和插件目录仍存在；
- [ ] 重新启用后 Python 环境、任务中心和旧插件恢复使用；
- [ ] 连续启停 5 次，无残留 Python、pip 或插件子进程，资源计数不增长；
- [ ] 原有渲染队列和 Blender Worker 不受本模块停用影响。

## 9. 已知边界

- R5 完成前，任务、Python 和插件入口不会随模块停用自动消失，后端会拒绝调用；
- 当前任务队列状态仍由前端 Store 持久化，R6/R9 再统一到 Profile 和组件运行时；
- 旧插件静态 Tauri 命令已经封堵，但细粒度文件/网络操作尚未逐次申请 Capability token；
- 渲染中心使用的 Blender Worker 和 PMC Python 前后置脚本由下一分项统一迁移，避免两个模块同时争夺渲染进程所有权。
