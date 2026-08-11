> **历史归档（2026-08-12）**：本文件仅保留为实现与验收追溯资料，不代表当前公开能力或后续开发依据。请先阅读 docs/README.md。
# R12/R13/R14 本机验收审计

审计日期：2026-08-07  
结论：本机可证明的实现和自动化回归已通过；R12、R13、R14 均不能标记为 `accepted`。下表将“代码与测试已经证明”“必须在独立环境验证”“尚未实现”分开，避免用本机测试替代产品或发布验收。

## 本次执行结果

| 检查 | 结果 | 范围 |
| --- | --- | --- |
| `cargo fmt --check --manifest-path src-tauri/Cargo.toml` | 通过 | Rust 格式 |
| `cargo test --target-dir $env:TEMP\\nexora-r14-tests --manifest-path src-tauri/Cargo.toml --lib --no-default-features` | 通过，222/222 | Rust 单元与本地集成夹具 |
| `cargo check --target-dir $env:TEMP\\nexora-r14-check --manifest-path src-tauri/Cargo.toml --no-default-features` | 通过 | 无默认特性编译 |
| `npm run check:platform-contracts` | 已通过 | 平台合同 |
| `npx tsc --noEmit` | 已通过 | 前端类型 |
| `npx vite build --outDir $env:TEMP\\nexora-r14-dist` | 已通过 | 前端生产构建；仅有既有 chunk 大小提示 |
| `git diff --check` | 通过 | 补丁空白与冲突检查 |

本机命令不会写入项目 `.pm_center`、正式组件目录或安装目录。Cargo 使用系统临时 target，避免干扰开发中的 target。

## R12：参考装配纵向验收

| 项目 | 状态 | 本机证据或后续门槛 |
| --- | --- | --- |
| 四套参考装配均由同一 Profile/Module/Component/Contribution 合同组成 | 已本机证明 | `reference_assemblies` 测试覆盖项目管理器、媒体、局域网、外部 Blender 渲染器和失效贡献泄漏。 |
| 项目管理器最小装配与当前默认装配兼容 | 已本机证明 | `current_default_profile_preserves_project_manager_reference_contract`。 |
| 媒体库独立数据库、分页索引、重复内容关联、复制/移动提交顺序 | 已本机证明 | `media_library` 测试覆盖 10,000 项索引和失败前清理；移动来源仅在 catalog 提交后删除。 |
| 通信端不隐式依赖项目资源 | 已本机证明 | `lan_communications_reference_profile_is_project_free_and_complete`。 |
| 外部渲染器使用私有应用数据，不写项目 `.pm_center` | 已本机证明 | `external_blender_renderer_reference_profile_is_project_free_and_complete` 与渲染中心测试。 |
| 大型真实媒体目录及多种图片、视频、音频预览 | 外部验收 | 需在真实素材与磁盘压力下检查。 |
| 局域网双机发现、聊天、文件和目录传输 | 外部验收 | 需两台 Windows 设备和防火墙/多网卡条件。 |
| 真实 Blender 至少十帧、插件兼容和输出预览 | 外部验收 | 需真实 `.blend`、Blender 和日志核验。 |
| 新安装、缺组件、导入、切换、停用/卸载及宽窄窗口 | 外部验收 | 需 MSI 或干净用户环境。 |

## R13：Blender 远程渲染农场

| 项目 | 状态 | 本机证据或后续门槛 |
| --- | --- | --- |
| 默认关闭、模块生命周期和 Profile 贡献边界 | 已本机证明 | `render_farm_manifest_is_opt_in_and_does_not_start_a_transport`。 |
| 可信设备身份、签名配对、撤销和 nonce 绑定 | 已本机证明 | `pairing_is_signed_and_nonce_bound`、`revoked_or_stale_nodes_cannot_commit_or_overwrite`。 |
| 渲染包路径安全与资源摘要 | 已本机证明 | `package_rejects_traversal_and_keeps_payload_hashes`。 |
| 节点能力预检 | 已本机证明 | 未信任、未报告、Blender/组件缺失、磁盘不足均被拒绝；符合报告的可信节点才能通过。 |
| 用途限定、过期、收发者绑定、防篡改和防重放加密信封 | 已本机证明 | X25519、Ed25519、XChaCha20-Poly1305 和持久化 replay guard 测试。 |
| 实际加密 socket/传输适配器 | 尚未实现 | 当前只实现本机可验证的安全控制面和加密信封，不监听或连接远程农场端口。 |
| 资源分块、断点续传、远程 staging 与远程 Worker | 尚未实现 | 没有远程执行，不允许宣传为可用农场。 |
| 控制端调度、双机十帧真实渲染、断网/崩溃恢复 | 尚未实现且需外部验收 | 先实现执行链路，再在两机与真实 Blender 环境验证。 |

## R14：稳定性、升级与发布准备

| 项目 | 状态 | 本机证据或后续门槛 |
| --- | --- | --- |
| Profile 迁移、切换 journal 和损坏状态恢复 | 已本机证明 | `profile_runtime` 迁移、回滚、陈旧 journal 与修订冲突测试。 |
| 组件 staging、签名/路径校验、同卷提交和失败回滚 | 已本机证明 | `component_runtime` 与 `profile_package` 测试。 |
| 项目资源释放和渲染帧原子提交/崩溃恢复 | 已本机证明 | `project_resources`、`render_center` 测试。 |
| 格式、编译、Rust 回归、平台合同和前端构建 | 已本机证明 | 本文“本次执行结果”。 |
| MSI 从 2.8.x 升级、回滚、卸载和用户数据保留 | 外部验收 | 需干净快照、真实安装包和升级前后哈希。 |
| 新 Windows 用户与组件缺失/损坏包安装流程 | 外部验收 | 需独立用户或虚拟机。 |
| 冷启动、空闲 CPU/内存、端口/Worker/句柄为零 | 外部验收 | 需运行中的桌面应用和系统采样。 |
| 签名、第三方许可/NOTICE、分阶段发布与回滚 | 外部验收 | 需候选发行物和人工法律/发布审计。 |

## 放行规则

- 本文中的“已本机证明”只表示实现已被本机自动测试覆盖，不能替代用户操作、真实工具、MSI 或双机结果。
- R12 需要关闭所有“外部验收”项；R13 还必须先完成明确列出的执行链路；R14 需要通过升级、卸载、性能、许可和发行候选审计。
- 任何文件覆盖、迁移错误、权限绕过、哈希不一致、残留 Worker/端口或回滚失败都会阻断发布。
