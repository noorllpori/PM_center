# Nexora 项目现状与开发约定

> 当前基线：Nexora 2.8.5
>
> 记录日期：2026-08-12
>
> 基线提交：80def58f2f94

## 产品与边界

Nexora 是 Windows 优先的本地制作项目工作台。项目本体始终是普通目录；Nexora 的项目级状态位于 .pm_center，不修改项目业务文件。

用户侧只需要理解三个对象：组件、装配方案和项目。

- 组件提供页面、工具、文件处理器、自动化和服务能力。
- 装配方案决定启用组件、界面模板、主页、快捷栏和插槽绑定。
- 项目提供文件和业务上下文；组件通过公开接口访问，不能直接读取内部数据库、Store 或 TreeCache。

历史 Module 字段仅由兼容层读取。新开发、设置和装配均以组件为唯一用户概念。

## 当前可用能力

- Profile 可保存、复制、切换、导入导出，并按方案启停组件与恢复会话。
- 组件运行时支持 Python、原生进程、隔离 DLL、资料包和内置适配器；组件可安装、停用、卸载、重装与版本校验。
- Capability Gateway、Component Bridge、项目上下文和组件存储为公开接口提供权限、依赖、取消、日志与生命周期边界。
- 文件操作组件提供受控项目路径与已授权外部路径的读取、写入、复制、移动、缓存和 Watcher 维护能力。
- 脚本自动化支持手动、事件和 cron 触发，包含运行记录、权限等待、取消、安全重试、attention 与隔离 Script Surface。
- 界面模板支持静态 HTML/CSS、类型化插槽、组件页面、主页绑定、固定或自动隐藏的宿主工具带、开发副本和 DEV 重载。
- 项目浏览、媒体预览、局域网、任务、Python、Blender 渲染等既有业务能力仍由内置组件和服务提供。

## 里程碑状态

| ID | 状态 | 当前结论 |
| --- | --- | --- |
| D0 | accepted | 架构目标与资料边界已确立。 |
| R0 | verifying | 当前资源与真实恢复流程仍需补充实机证据。 |
| R1 | accepted | Schema 与兼容合同已建立。 |
| R2 | accepted | 生命周期与资源登记已验收。 |
| R3 | accepted | Capability 权限网关已验收。 |
| R4 | accepted | 现有后台能力已接入生命周期。 |
| R5 | accepted | 前端贡献目录与诊断已验收。 |
| R6 | accepted | 装配方案运行时、迁移和会话恢复已验收。 |
| R7 | accepted | DIY 装配编辑器 MVP 已验收。 |
| R8 | accepted | Profile 与 Workspace 导入导出已验收。 |
| R9 | verifying | 统一组件运行时已实现，仍待完整人工环境验收。 |
| R10 | verifying | 文件路由、隔离宿主、签名包和组件安全已实现，仍待第三方与双机验收。 |
| R11 | accepted | 脚本组件与自动化平台已完成自验收。 |
| R12 | in progress | 参考装配的真实媒体、Blender、双机和安装环境验收待完成。 |
| R13 | in progress | Blender 远程渲染农场仍待真实传输、节点和双机验收。 |
| R14 | in progress | 发布稳定性、安装、回滚和发布候选仍待外部验收。 |
| R15 | pending | 云账户、组件商城、组件源、更新与回退尚未实现。 |
| R16 | pending | 云设置、Profile/Workspace 同步和团队库尚未实现。 |
| R17 | in progress | Hosted Surface 与界面模板子路线在验证；完整 Project Provider ABI 未完成。 |
| R18 | in progress | 可视化界面装配在验证；自动化节点编排未开始。 |

## 当前限制

- R15/R16 不存在安装版可用实现：组件仍通过本地目录、签名包或随附资源交付，方案同步仍是本地导入导出。
- 第三方不能直接接入 Nexora React、Tauri command、数据库、TreeCache 或 Watcher。
- 完整第三方项目管理器级 ABI、通用 Widget/DataSource ABI、任意右键菜单和独立窗口 Surface 管理仍属于 R17 后续。
- 旧 Workflow/DAG 只兼容解析，不执行；新的节点目录、图编辑器和调试能力仍属于 R18 后续。

## 工作规则

每次改动先确认组件归属、Profile 生命周期、Capability 和公开合同。更新接口时同步修改 Schema、实现、测试、安装版开发文档和 AI 合同；不得新增绕过 Component Bridge 的平行入口。

当前实施顺序与验收条件见 [实施路线](NEXT_MAJOR_IMPLEMENTATION_ROADMAP.md)。组件和模板开发请从 [扩展开发入口](extension-development/README.md) 开始。
