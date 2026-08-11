> **历史归档（2026-08-12）**：本文件仅保留为实现与验收追溯资料，不代表当前公开能力或后续开发依据。请先阅读 docs/README.md。
# R14：稳定性、升级与发布准备

状态：`in progress`。本文件是发布候选的证据清单，不把开发机上的单元测试描述成 MSI、双机或真实 Blender 的验收结果。

最近一次本机自验收：2026-08-07。`cargo fmt --check`、平台合同、TypeScript、Vite 生产构建、`git diff --check`、普通 `cargo check` 和 222 项 Rust 库测试均已通过。构建中仍有既有的 dead-code 警告，不是发布放行结论。R12/R13/R14 的逐项边界和外部阻塞项见 `docs/NEXT_MAJOR_R12_R13_R14_ACCEPTANCE_AUDIT.md`。

## 已有本机基线

| 领域 | 已验证的实现 | 证据 |
| --- | --- | --- |
| Profile 迁移 | 旧配置迁移只执行一次，用户修改的 Widget 与自定义 Profile 不会被后续迁移覆盖。 | `profile_runtime.rs` 的迁移、Shell Layout、设置归属与品牌迁移测试。 |
| Profile 切换恢复 | 切换事务在提交前回滚，在提交后完成前端收尾；错误 JSON 或过期 journal 只报告，不覆盖原状态。 | `interrupted_switch_*`、`corrupt_*`、`stale_switch_*` 测试。 |
| 组件安装 | 组件包先在 staging 校验，使用同卷重命名提交；失败时回滚旧目录，包路径、签名、平台和入口会先验证。 | `component_runtime.rs` 的路径、签名、解压与安装事务代码/测试。 |
| 项目与渲染恢复 | 项目关闭只释放 watcher、数据库和 TreeCache 句柄；渲染帧使用临时文件、令牌和原子提交，崩溃可恢复。 | `PROJECT_CONTEXT.md`、`render_center/mod.rs` 的领取与恢复测试。 |
| 媒体资料安全 | 复制/移动先原子提交，再写索引；重复 BLAKE3 内容只建立关联，不删除来源。 | `media_library.rs` 的目录与重复来源测试。 |
| 农场控制面 | 信任、租约、控制端 staging 和加密用途绑定信封均可本机回环验证。 | `render_farm.rs` 的配对、租约、提交和加密信封测试。 |

## 本机自动验收命令

所有 Cargo 输出应放到系统临时目录，避免占用用户正在运行的开发 target：

```powershell
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --target-dir $env:TEMP\nexora-r14-verify --manifest-path src-tauri/Cargo.toml --lib --no-default-features
cargo check --target-dir $env:TEMP\nexora-r14-verify --manifest-path src-tauri/Cargo.toml --no-default-features
npm run check:platform-contracts
npx tsc --noEmit
npx vite build --outDir $env:TEMP\nexora-r14-dist
git diff --check
```

这些检查必须全部通过，且只允许既有的非阻断编译警告。它们不生成或覆盖用户项目、用户组件或正式安装目录。

## 仍需独立环境验收

以下项目不能在当前开发目录自证完成：

1. 从真实 `2.8.x` MSI 升级到候选版：比较 Profile、组件清单、局域网资料、媒体资料库和项目 `.pm_center` 的迁移前后哈希；再以保留用户数据的方式卸载、重装和回滚。
2. 全新 Windows 用户与第二台设备：组件缺失、导入 `.pmc-profile/.pmc-workspace`、组件包安装/卸载、受损包拒绝、内置 Python 资源和第三方许可显示。
3. 两台 Windows 设备的局域网和 R13 农场：配对、撤销、资源分块、十帧真实 Blender、断网、节点崩溃、控制端重启和结果哈希复核。R13 当前还未开始真正的网络调度，故此项现在明确为阻塞项。
4. 实际媒体库：大型目录、各种图片/视频/音频格式、预览、复制/移动冲突和磁盘不足。
5. 性能：冷启动、空闲 CPU/内存、10,000 项以上目录和媒体资料库的分页、退出后 Worker/端口/监听器为零。
6. 发行物：MSI 签名、版本说明、已知限制、GPL-2.0-only 与所有随包第三方许可/NOTICE 的人工审计，以及分阶段更新和回滚演练。

## 发布阻断规则

- 任何迁移错误、文件覆盖、结果哈希不一致、未经授权的能力调用、无法退出的 Worker/端口或回滚失败，均阻断发布。
- 未完成的双机和 MSI 检查不允许被标记为“已验收”；它们只能随发布候选测试记录关闭。
- R13 在完成加密传输、分块同步、节点 staging、实际渲染调度与双机验收以前，不可宣传为可用渲染农场。
- 正式安装包中加入或升级任何组件包、Python 运行时、DLL 或许可证时，必须重新执行上述审计并更新第三方 NOTICE。

## 发布候选记录模板

```markdown
版本：
安装包哈希：
迁移前/后数据哈希：
升级：通过 / 失败
回滚：通过 / 失败
卸载后用户数据：保留 / 清除（按预期）
双机通信：通过 / 失败
双机农场：通过 / 未实施
真实 Blender 十帧：通过 / 失败
第三方许可审计：通过 / 失败
已知限制：
负责人和日期：
```
