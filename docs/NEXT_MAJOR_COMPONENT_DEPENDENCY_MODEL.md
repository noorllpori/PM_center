# 下一代统一组件依赖模型与 BlenderIO 定位

> 状态：`r7-0-accepted`
>
> 日期：2026-08-05
>
> 适用里程碑：R7、R9、R11、R13

## 1. 核心结论

`BlenderIO` 不属于“Blender 文件解析器页面”，也不应继续作为渲染中心内部的私有函数。它是一个可安装、卸载、升级、版本化并可被多个模块复用的组件。

组件模型不区分“内核组件”和“普通组件”，也不因为组件随安装包提供就赋予额外权限。所有组件使用相同的安装、卸载、依赖、Capability、健康检查和版本管理规则。

三层边界固定为：

```text
组件：pmc.blendio
  提供 .blend 读取、预览、外部依赖、场景摘要和受控编辑能力

模块：builtin.render-center / builtin.project-resources
  通过组件服务接口消费 BlenderIO，不直接依赖具体 Rust crate

功能贡献：Blender 文件解析器 / 工作流节点 / 数据源
  是 BlenderIO 对用户和工作流暴露的附属入口，不拥有解析实现
```

因此：

- 批渲染启用时会自动解析并激活 BlenderIO 依赖；
- 不启用批渲染时，也可以单独激活 BlenderIO 并保留文件解析器；
- BlenderIO 默认可随 Nexora 安装包提供，但用户仍可卸载、重装或升级；
- Profile 导出时可以准确说明依赖的组件及版本；
- 未来可以换用独立 EXE、隔离 DLL、Python 或其他兼容实现，上层模块不需要改业务接口。

## 2. Module、Component 与 Feature 的区别

### Module

模块拥有产品级生命周期、后台资源和页面集合，例如渲染中心、项目资源、局域网协同。模块可以依赖其他模块和组件。

### Component

组件提供可调用能力，类似受 Nexora 管理的库或服务。它有稳定 ID、版本、运行时、命令、Capability、资源限制和健康状态，但不必拥有完整页面。

组件只使用描述维度，不使用权限等级分类：

```text
role: service | feature | data
distribution: bundled | marketplace | local
uiMode: none | hosted | contributed
```

- `role` 描述主要用途，不决定权限；
- `distribution` 只说明来源，`bundled` 仍允许卸载和重装；
- `uiMode=hosted` 表示组件返回结构化数据，由 Nexora 通用界面显示；
- `uiMode=contributed` 才允许注册受控 Widget、工具动作等贡献；
- Capability 始终按实际操作审批，任何角色和来源都不能绕过网关。

组件运行时继续使用既有分类：

```text
python-action
python-worker
native-process
native-library
data-pack
builtin-rust
```

`builtin-rust` 标记为迁移期兼容运行时：它用于让已编译进宿主的旧实现暂时经过统一调用合同，但不进入可安装组件目录，也不代表“内核组件”、不可卸载等级或更高权限。所有正式组件包都必须可安装和卸载；BlenderIO、HDA、PPT、PDF 等格式组件优先使用 `native-process`，必要时使用隔离 `native-library` 或 Python 运行时。

### Feature Contribution

功能贡献是组件或模块暴露给用户的入口，例如工具动作、Widget、DataSource 和工作流节点。入口撤下不等于删除组件数据或卸载组件。

## 3. 依赖合同

`ModuleManifestV1` 增加可选字段：

```json
{
  "requiresComponents": [
    {"id": "pmc.blendio", "versionRequirement": "^1.0"}
  ],
  "optionalComponents": []
}
```

语义：

- `requiresComponents` 缺失或版本不兼容时，模块状态为 `blocked`；
- `optionalComponents` 不可用时模块仍可启动，但对应贡献或增强能力不可用；
- 组件依赖参与 Profile 预检、导入预检、版本升级和诊断；
- 组件依赖不能通过前端隐藏入口绕过，必须由运行时解析；
- 循环依赖只允许组件依赖组件时形成 DAG，任何环都必须拒绝。

`WorkspaceProfileV1` 增加可选 `enabledComponents`：

```json
{
  "enabledComponents": [
    {"id": "pmc.blendio", "versionRequirement": "^1.0"}
  ]
}
```

显式激活用于“只使用组件附属功能但不启用依赖它的业务模块”。由模块传递激活的组件不必重复写入；保存时保留用户显式选择，运行时另外计算有效组件集合。

## 4. BlenderIO 组件

首个正式无头服务组件定义：

```json
{
  "schemaVersion": 1,
  "id": "pmc.blendio",
  "name": "BlenderIO",
  "version": "1.0.0",
  "apiVersion": "1",
  "runtime": "native-process",
  "platforms": ["windows-x64"],
  "entry": "bin/windows-x64/blendio.exe",
  "role": "service",
  "distribution": "bundled",
  "uiMode": "hosted",
  "capabilities": ["project.files.read"],
  "contributes": {
    "toolActions": [
      {
        "id": "blender.inspect-file",
        "command": "inspect",
        "name": "解析 Blender 文件"
      }
    ],
    "workflowNodes": [
      {
        "id": "blender.inspect",
        "command": "inspect",
        "name": "读取 Blender 场景",
        "inputs": [{"name": "file", "type": "file", "required": true}],
        "outputs": [{"name": "summary", "type": "json", "required": true}]
      }
    ],
    "dataSources": ["blender.file-summary"]
  }
}
```

`bundled` 表示安装器默认携带或首次运行时可直接安装，不表示不可卸载。Blender 文件解析器由 Nexora 的宿主界面承载；BlenderIO 自身不创建窗口，只提供结构化命令和结果。

稳定命令面建议为：

```text
inspect
extract-preview
collect-external-data
read-render-settings
edit-render-settings
```

读取命令默认只需要文件读取权限。写回渲染设置必须单独请求写权限，使用临时文件、验证和原子替换，不能因为组件已获读取权限而自动获得写权限。

同类格式能力全部沿用这一模型，例如：

```text
pmc.hda-reader   -> Houdini HDA 结构、依赖与缩略图
pmc.ppt-reader   -> PPT/PPTX 页面、备注、媒体与缩略图
pmc.pdf-reader   -> PDF 文本、页面、元数据与预览
pmc.psd-reader   -> PSD 图层、画布、字体与链接资源
```

它们都是可安装组件，不需要为每种格式增加新的 Nexora 固定模块。

## 5. 当前代码迁移

现状：

- `src-tauri/crates/blendio` 已经同时提供 Rust 库和 CLI；
- `get_file_details`、缩略图、Blender 文件标签和解析器弹窗直接消费 BlendIO；
- R7-0 已登记 `pmc.blendio` 的正式 Component Manifest；
- 渲染中心已通过 `requiresComponents` 声明必需依赖，项目资源已通过 `optionalComponents` 声明可选依赖；
- Profile Runtime 已计算显式组件、模块传递组件和组件传递依赖，并在缺失、版本不兼容或循环时阻止应用；
- 当前执行仍由宿主内 `blendio` crate adapter 完成，尚未切换到可卸载 EXE 的 ComponentGateway 调用。

迁移顺序：

1. [已完成] 保留现有 crate 和 Tauri 命令，避免当前功能回归；
2. [R9 verifying] 将现有 BlendIO 能力登记为可卸载、可重装的 `pmc.blendio` 组件，并通过受信任宿主适配器迁移现有 Rust 实现；独立签名二进制留到 R10；
3. [R9 verifying] 已实现动态目录、本地目录安装、随附组件卸载/重装、依赖图校验和 Profile/Capability 目录同步；归档包签名与升级回滚进入 R10；
4. 让旧 `get_file_details` 等命令成为 ComponentGateway 的兼容 adapter；
5. 渲染中心把场景预检、外部资源和渲染设置读取改为组件调用；
6. [已完成合同] 项目资源模块把 `.blend` 预览和详情声明为可选组件能力；
7. R9 已在 BlendIO 业务入口增加组件可用性守卫；R10 独立宿主完成后移除业务模块对 `blendio` crate 的直接引用；
8. R11 将相同命令注册为工作流节点，R13 远程节点按组件版本报告能力。

## 6. R7 编辑器中的表现

R7 装配编辑器不能只显示模块开关，还要显示组件影响：

- 选择渲染中心时，依赖摘要显示“将自动启用 BlenderIO”；
- 取消渲染中心时，如果解析器仍被 Pin 或显式激活，BlenderIO 保持启用；
- 取消 BlenderIO 时，编辑器列出被阻塞的模块、工具、Widget、数据源和工作流节点；
- 组件卡片区分“显式启用”和“由模块依赖”；
- 已卸载组件显示来源和安装入口，随包组件与商城组件使用相同安装状态；
- 保存预检显示版本不兼容、缺失组件、权限和平台问题；
- 组件本身不被误画成完整产品模块，也不重复生成一套设置页。

R7 只实现组件目录、依赖解析、草稿编辑和预检。R9 已实现 Python/EXE/资料包监督；DLL 隔离仍属于 R10。

## 7. 验收标准

- Profile 只启用渲染中心时，有效组件集合包含 BlenderIO；
- Profile 不启用渲染中心但显式启用 BlenderIO 时，解析器仍可使用；
- 缺失或版本不兼容的 BlenderIO 会阻止渲染中心应用，而不是运行后才报错；
- 卸载 BlenderIO 后依赖项进入缺失状态，重装兼容版本后可恢复，不需要修改 Profile；
- 关闭解析器入口不会停止正在使用 BlenderIO 的渲染模块；
- 同一个 `.blend` 检查结果可以被文件详情、解析器、渲染预检和工作流节点复用；
- Capability 审计能区分调用模块、调用组件、命令和目标路径；
- 替换为兼容的外部组件实现后，上层模块合同和 Profile 不变化。
- BlenderIO、HDA、PPT、PDF 等组件使用同一注册、安装、权限和宿主界面机制。
