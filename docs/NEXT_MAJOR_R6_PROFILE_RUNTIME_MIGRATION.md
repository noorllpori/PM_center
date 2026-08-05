# R6-1 装配方案仓库与当前配置迁移

> 状态：`accepted`
>
> 日期：2026-08-04
>
> 基线版本：2.8.4

## 1. 本轮目标

建立 R6 第一条无破坏纵向链路：把当前 Nexora 的模块期望状态和功能栏 Pin 快照为一个普通 `WorkspaceProfileV1`，但暂不让 Profile 驱动模块启停、页面关闭或布局切换。

这样可以先确认升级迁移与现有使用习惯完全一致，再在 R6-2 增加切换预览和事务切换。

## 2. 软件级仓库

应用数据目录新增：

```text
profiles/
  local.current-pm-center.json
profile-runtime.json
```

- `profiles/` 保存普通 `WorkspaceProfileV1` 文档；
- `profile-runtime.json` 只保存当前/上一个方案 ID 和迁移元数据；
- 当前迁移方案 ID 为 `local.current-pm-center`，没有系统特权，后续可以像其他方案一样复制和编辑；
- Profile 文件继续使用 R1 已冻结的 schema v1，不增加平行格式；
- 项目 `.pm_center`、聊天数据库、渲染数据库和插件目录均不参与迁移。

## 3. 首次迁移

前端在设置、Pin 和 Contribution Registry 加载完成后，将 Pin 转换为稳定 Tool 贡献 ID，再请求后端初始化。

后端快照：

- 所有非诊断模块的 `desiredEnabled` 状态；
- 对应模块当前版本的兼容约束；
- 当前功能栏 Pin 的稳定贡献 ID 和顺序；
- Nexora 来源版本、迁移时间和数量摘要。

本机首次迁移结果：

| 项目 | 数量 |
| --- | ---: |
| 装配方案 | 1 |
| 已启用正式模块 | 5 |
| 固定工具 | 6 |
| Profile revision | 1 |

诊断模块不会进入用户迁移方案；已经保存但当前不可见的 Pin 仍按原配置保留。

## 4. 数据安全与恢复

- 首次迁移只创建新文件，目标存在时不会覆盖；
- 初始化幂等，第二次启动不会用新的模块或 Pin 状态重写迁移快照；
- Profile 先完整验证并同步临时文件，再提交最终文件；
- 状态文件损坏时只返回稳定错误，原内容保持不变；
- 当前 Profile 缺失、无效、ID 不一致或路径 ID 非法时阻止读取；
- 崩溃恢复只接管保留路径中同 ID 且依赖兼容的迁移文件；
- 非当前损坏 Profile 会进入诊断列表，不阻止当前合法方案；
- R6-1 没有删除、重命名、导入、导出或切换操作。

## 5. 设置页诊断

全局设置新增“装配方案运行时”只读区，显示：

- 当前方案名称、稳定 ID、状态和 revision；
- 方案、启用模块和 Pin 数量；
- 迁移来源、版本和时间；
- `profiles/` 与运行时状态文件路径；
- 解析、兼容和文件损坏错误。

该页面明确说明本轮不会切换模块或修改 Pin，避免把迁移快照误认为已经接管运行时。

## 6. Public Interfaces

新增 Tauri 命令：

- `initialize_workspace_profile_runtime(request)`；
- `get_workspace_profile_runtime()`。

新增前端类型和 Store：

- `WorkspaceProfileRuntimeSnapshot`；
- `WorkspaceProfileSummary`；
- `WorkspaceProfileMigrationRecord`；
- `useWorkspaceProfileStore`。

## 7. 自动验证

覆盖：

- 迁移模块与 Pin，并验证重复 Pin 去重；
- 第二次初始化不重写已有迁移快照；
- 损坏运行时状态只报告、不覆盖；
- 非当前损坏 Profile 出现在诊断列表；
- 非法当前 Profile ID 在路径解析前被拒绝；
- 保留迁移路径中的不同 Profile 不会被误设为当前方案；
- TypeScript、Vite、平台合同、Rust 全量测试、编译和格式检查。

2026-08-04 已通过：

```powershell
npm run build
npm run check:platform-contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml --package nexora --check
git diff --check
```

结果：Rust 库测试 `125 passed`；TypeScript/Vite、平台合同、Rust 编译、格式和差异检查全部通过。生产构建只保留既有大 chunk 警告，Rust 只保留 4 条既有 dead-code 警告。

实机连续重启后：

- Profile SHA-256：`ED46A3218E1B284FB98554B54C2ABDBAEEDDDBC0D9BF648B406564BF3E3262B5`；
- 状态 SHA-256：`0A21E0E32762CB7E483859CF890B7891A025BBDFBE5E3B3163EF0A6A6872DA35`；
- 两次启动哈希和迁移时间完全一致。

## 8. 人工验收

- [x] 重启后原项目、工具栏 Pin、模块状态和已有页面没有变化；
- [x] 全局设置出现“装配方案运行时”；
- [x] 当前方案为“当前 Nexora 装配方案”，状态“可用”；
- [x] 显示 1 个方案、5 个启用模块、6 个固定工具、revision 1；
- [x] 方案目录和状态路径位于应用数据目录，不在项目 `.pm_center`；
- [x] 点击刷新后数量和状态不变化；
- [x] 再次重启后迁移时间不变化，证明没有重复覆盖；
- [x] 浅色、深色和窄窗口布局正常。

2026-08-04 用户确认未发现问题，R6-1 验收通过。

## 9. 下一步

R6-2 实现见 `docs/NEXT_MAJOR_R6_PROFILE_SWITCHING.md`。
