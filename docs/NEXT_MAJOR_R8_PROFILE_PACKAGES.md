# R8 Profile 与装配空间包

> 状态：`accepted`
>
> 日期：2026-08-05
>
> 前置：R6、R7 accepted

## 1. 目标

让用户可以把 Nexora 的 DIY 装配结果安全发送到其他设备，并在导入前明确看到内容、依赖和风险。包导入不得隐式切换运行方案、安装组件或修改现有方案。

## 2. 分阶段实施

### R8-1：安全 Profile 包

- `.pmc-profile` 使用 ZIP 容器，只允许根目录 `manifest.json` 和 `profile.json`。
- `manifest.json` 使用共享 `PackageHeaderV1`，内容摘要固定为 BLAKE3。
- 导出内容移除本地 Profile ID、修订和迁移/编辑时间元数据；相同方案产生字节一致的包。
- 导出和导入均递归扫描密码、token、私钥、凭据、Cookie、聊天、联系人、头像缓存和本机绝对路径。
- 检查条目数量、单条目大小、总解压大小、重复条目、未知条目、路径穿越、包类型、合同版本、内容大小和摘要。
- 导入先显示名称、说明、生产版本、模块/组件/页面/Widget/Pin 数量和依赖问题。
- 导入始终生成新的 `local.profile-*` ID 和修订 1；名称冲突时要求改名。
- 导入完成后当前 Profile、模块运行状态和快捷栏均不变化。
- 用户创建或导入的非当前 `local.profile-*` 方案可在应用内确认后删除。
- 当前方案及默认、空白恢复方案受保护；切换离开后，升级生成的旧迁移方案允许删除。

### R8-2：路径和工具映射

- 为 Blender、FFmpeg、Python 和其他外部工具增加稳定别名，不在 Profile 中保存绝对路径。
- 导入预览显示未解析变量和工具别名，用户明确选择本机映射后才能导入。
- 增加组件/模块版本差异、可替代依赖和贡献冲突的分组处理界面。
- 仍不自动下载或安装依赖；安装动作由 R9/R10 的组件管理器显式执行。

#### R8-2 实现记录

- Profile schema v1 增加 `toolAliases` 和 `pathVariables`。工具使用稳定 ID，例如 `nexora.tool.blender`、`nexora.tool.ffmpeg`、`nexora.tool.ffprobe` 和 `nexora.tool.python`，Profile 内只保存逻辑别名、版本要求、是否必需和说明。
- 导出时根据启用模块幂等补齐内置需求：渲染中心声明 Blender 和可选 FFmpeg，自动化运行时声明 Python，外部工具模块声明可选 FFprobe；用户显式声明的同名别名优先。
- 导入预览返回工具别名和路径变量。前端优先列出设置中的 Blender 版本、FFmpeg/FFprobe 路径和 Python 环境，也允许选择“系统自动检测”或手动选择本机文件/目录。
- 必需映射未完成时导入按钮不可用；后端仍会重新检查重复、未知别名、绝对路径、文件/目录类型和实际存在性，不能只信任前端。
- 本机绝对路径写入应用数据目录 `profiles/local-bindings/<profile-id>.json`，不进入 Profile 和 `.pmc-profile`。绑定写入失败时回滚刚创建的 Profile；删除 Profile 时同步清理绑定文件。
- 导入时可沿用其他方案中相同稳定工具或同名、同类型路径变量的本机映射。沿用操作复制解析后的值，不保存跨 Profile 引用，源方案后续修改或删除不会影响新方案。
- `automatic` 只表示用户明确允许运行时使用系统或 Nexora 的自动检测结果。组件实际通过别名解析绑定进入 R9 ComponentGateway，不允许组件自行读取 Profile 私有映射文件。

### R8-3：装配空间骨架

- 增加可选 `.pmc-workspace`，组合 Profile、变量声明和可移植的工作区骨架。
- 第一版不携带项目源文件、聊天记录、缓存、密钥或用户私人资料。
- 支持跨机器检查、升级预览和失败时全量回滚，不允许半导入状态。

#### R8-3 实现记录

- `.pmc-workspace` 只允许根目录 `manifest.json` 和 `workspace.json`，继续使用 `PackageHeaderV1`、BLAKE3 和确定性 JSON。
- `WorkspacePackageV1` 携带可移植 Profile、普通字符串变量、按顺序排列的 `openSurfaceIds` 和必须属于该列表的 `activeSurfaceId`。
- 导出界面的“空间”操作使用 Profile Surface 顺序形成第一版页面骨架；不读取当前项目文件、动态项目标签或业务数据。
- 导入复用 R8-2 本机工具和路径映射；Profile 创建后绑定写入失败会回滚 Profile。
- 导入结果返回变量和页面引用供后续 Shell/会话装配使用，但不会自动切换当前 Profile 或打开页面。
- R8-3 的初始实现不携带组件二进制、模板 HTML/CSS 或项目内容；R10 包安全完成后，装配空间已支持可选 `self-contained` 模式，仅嵌入已验证、允许重新分发的原始 `.pmc-pack`。项目内容仍不进入装配空间。

### R8-P：界面表现模板引用

- Profile 可以保存 `shellTemplate`、Surface `template` 和 `themePreset` 的稳定 ID、版本约束、变体与普通设置。
- 现有 `navigationKind` 继续作为顶部、侧边和紧凑三个内置模板的兼容回退，不再增加新的固定布局枚举。
- `.pmc-profile` 只携带模板引用和参数，不内联未检查 HTML/CSS，也不自动安装缺失模板。
- 导入预览必须列出模板名称、来源、版本要求、缺失状态、受影响 Surface 和可用替代项。
- R10 包安全完成后，`.pmc-workspace` 可在 `self-contained` 模式携带锁定模板包、主题和包内资源，实现自包含外观复现；默认 `references-only` 模式继续只保存依赖引用。
- 模板包安装、签名、净化和运行时切换不属于 R8-1，详细顺序见 `NEXT_MAJOR_PRESENTATION_TEMPLATE_ARCHITECTURE.md`。

## 3. R8-1 公共接口

- `export_workspace_profile_package(request)`：导出确定性 `.pmc-profile`。
- `inspect_workspace_profile_package(packagePath)`：只读检查并返回导入预览。
- `import_workspace_profile_package(request)`：重新检查后原子创建新方案。
- `delete_workspace_profile(profileId)`：删除非当前的用户方案或旧迁移方案，当前、默认与空白恢复方案拒绝删除。

R8-2 扩展：

- `WorkspaceProfileV1.toolAliases[]`：逻辑工具别名、稳定工具 ID、版本要求和必需状态。
- `WorkspaceProfileV1.pathVariables[]`：本机文件/目录变量及必需状态。
- `ProfilePackageImportPreview.toolAliases/pathVariables`：导入前展示需要映射的内容。
- `ImportWorkspaceProfilePackageRequest.toolMappings/pathMappings`：提交用户明确选择的本机绑定。
- `ProfileLocalBindingInput.mode`：`automatic` 或 `path`。

R8-3 扩展：

- `WorkspacePackageV1`：可移植 Profile、普通变量和页面骨架合同。
- `export_workspace_package(request)`：导出 `.pmc-workspace`。
- `inspect_workspace_package(packagePath)`：只读检查装配空间。
- `import_workspace_package(request)`：原子导入 Profile 和本机映射，并返回页面骨架。

导入提交使用 Profile 运行时与切换流程共享的锁。检查失败、名称冲突或依赖缺失时，现有方案目录和当前运行状态保持不变。

## 4. 自动验收

1. 相同 Profile 连续导出两次，包字节和 payload digest 一致。
2. 有效包往返导入后生成新 ID、修订为 1，当前 Profile ID 不变。
3. 密码、API Key、token、私钥、聊天/联系人字段和 Windows、UNC、Unix、`file://` 绝对路径被拒绝。
4. 损坏 ZIP、摘要不匹配、路径穿越、重复/未知条目和超限包被拒绝。
5. schema、format、kind 或摘要算法不支持时返回稳定错误码。
6. 缺失或版本不兼容模块/组件时只显示预览问题，不写入新方案。
7. 名称冲突不覆盖现有方案，用户改名后才可导入。
8. 必需工具或路径未映射时不创建 Profile；映射成功后 Profile JSON 不含本机绝对路径，独立绑定文件包含规范化后的本机路径。
9. 绑定文件写入失败会回滚导入，删除 Profile 后对应绑定文件消失。

## 5. 人工验收

1. 在“全局设置 -> 恢复 -> 装配方案”导出一个可用方案，确认生成 `.pmc-profile`。
2. 选择该文件导入，确认先显示包信息、数量、模板引用和安全/依赖检查结果。
3. 修改导入名称并点击“导入为新方案”，确认列表新增方案但当前方案没有切换。
4. 再次导入同一个包，确认自动建议“副本”名称，使用已存在名称时明确阻止。
5. 在方案设置中加入本机 Blender 或 FFmpeg 绝对路径，确认 R8-1 拒绝导出并指出 JSON 路径。
6. 新建或导入一个方案，点击“删除”后先取消，确认方案仍存在；再次确认删除后列表移除该方案，当前与系统恢复方案不受影响。
7. 导出一个启用了渲染中心和自动化运行时的方案，再导入该包，确认出现 Blender、Python 以及可选 FFmpeg 映射；未完成必需映射时不能导入。
8. 选择设置中已有 Blender/Python，或使用系统自动检测后导入，确认当前运行方案没有切换；重新导出该方案时包内不包含本机绝对路径。

R8-3 已于 2026-08-06 完成人工验收，R8 完成；组件消费本机别名绑定由 R9 ComponentGateway 和统一组件运行时负责。
