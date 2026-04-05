# 编写 PM Center 插件前先读本文件

本文件是给 AI 助手、代理和协作者使用的执行规范。

默认要求：

- 读完本文件后，按用户需求直接产出可落地插件。
- 默认不是只给方案，而是直接生成完整插件目录和代码。
- 默认先把插件写到 `examples/plugins/<plugin-id>/`，方便审阅和二次调整。

如果用户没有明确要求先只看方案，你应当直接写插件，并在回复里说明生成了什么、怎么测试、有哪些假设。

## 1. 本文件的用途

当用户说“帮我写一个 PM Center 插件”时，你应当先读取本文件，并按这里的规则执行。

这份文档的目标不是单纯解释接口，而是统一未来写插件时的默认工作方式，尽量减少来回确认。

你应当把它理解为：

- 插件编写规范
- 默认交付规范
- 实现边界说明
- 最低验收标准

## 2. 默认工作方式

当用户要求编写插件时，你应当按下面顺序工作：

1. 先判断插件用途、触发方式和目标对象。
2. 如果有现成代码可复用，优先复用当前仓库的插件系统约定，不另造接口。
3. 如果用户需求不影响核心行为，可以直接采用本文件中的默认值。
4. 只有在缺少关键产品意图时才提问，例如：
   - 到底放工具栏还是右键菜单
   - 是处理单个文件还是批量文件
   - 是否必须依赖第三方库
5. 如果信息足够，就直接生成完整插件，而不是先停在设计说明。

默认情况下，你的产出应当是“可以继续验证和打包的源插件”，而不是只有代码片段。

## 3. 默认交付物

除非用户明确要求别的目录，默认生成到：

```text
examples/plugins/<plugin-id>/
```

默认至少包含这些文件：

```text
examples/plugins/<plugin-id>/
  plugin.json
  main.py
  requirements.txt
```

按需补充这些文件或目录：

- `README.md`
- `assets/`
- `vendor/`

规则如下：

- `plugin.json` 必须生成。
- `main.py` 必须生成。
- `requirements.txt` 默认也要生成；如果没有依赖，可以保留为空文件。
- 只有在插件行为、配置方式或测试方式不够直观时，才默认补充插件自己的 `README.md`。
- `vendor/` 不作为源插件的默认输出；只有在用户明确要求打包分发，或你已经执行打包流程时才出现。

## 4. 用户需求输入模板

以后当用户提插件需求时，理想输入应尽量包含这些信息：

- 插件用途：这个插件要解决什么问题
- 触发位置：工具栏插件菜单，还是文件区右键菜单
- 目标对象：项目、当前目录、单个文件、多个文件、目录
- 选择约束：单选、多选、是否允许空选择
- 文件类型：例如 `png`、`blend`、目录
- 执行动作：重命名、导出、扫描、生成报告、调用外部工具等
- 输出方式：日志、进度、toast、refresh、result
- 依赖要求：是否需要第三方 Python 包
- 分发要求：只要源码，还是要可打包分发

如果用户没有按这个模板说完整，你应当尽量自己补齐合理默认值，并在最终说明里写出假设。

## 5. 默认假设

如果用户没有明确说明，统一采用以下默认值：

- `runtime = "python"`
- `apiVersion = "1"`
- `entry = "main.py"`
- 默认输出目录为 `examples/plugins/<plugin-id>/`
- 默认交付为完整可运行源插件
- 默认不引入不必要的第三方依赖
- 默认优先使用标准库和现有 `pmc_plugin` SDK

额外默认规则：

- 如果用户没给插件 `id`，使用英文 kebab-case 生成，例如 `batch-rename`、`image-report`。
- 插件目录名默认与 `plugin-id` 一致。
- 如果动作明显依赖当前选择对象，优先做 `fileContextActions`。
- 如果动作更像项目级工具，优先做 `toolbarActions`。
- 如果用户没有要求复杂文档，回复里给出简明说明即可；插件目录内是否生成 README 取决于复杂度。

## 6. 当前能力边界

当前实现只能支持这些能力：

- Python 插件
- 本地目录加载
- 声明式 UI 挂点
- CLI 入口脚本
- 请求 JSON 输入
- stdout 普通日志
- stdout `@pmc {...}` 控制消息

当前不支持这些能力：

- React 组件注入
- 自定义前端页面或插件 webview
- JS 插件运行时
- Lua 插件运行时
- 在线安装
- 插件商店
- 权限审批弹窗
- 远程插件下载

如果用户要求当前未支持的能力，你应当明确说明限制，并优先提供一个符合现状的 Python 版本替代方案。

## 7. 清单编写规则

插件必须包含 `plugin.json`。

当前可用字段：

- `id`
- `name`
- `version`
- `apiVersion`
- `runtime`
- `entry`
- `description`
- `minAppVersion`
- `enabledByDefault`
- `contributes`
- `permissions`

你应当至少正确填写这些关键项：

- `id`
- `name`
- `version`
- `apiVersion`
- `runtime`
- `entry`
- `contributes`

### 7.1 commands

所有动作必须先在 `contributes.commands` 里声明。

每个 command 至少包含：

- `id`
- `title`

建议同时补上：

- `description`

### 7.2 toolbarActions

当动作应出现在工具栏 `插件` 菜单时，使用：

```json
"toolbarActions": [
  {
    "command": "your-command-id",
    "when": {
      "projectOpen": true
    }
  }
]
```

### 7.3 fileContextActions

当动作应出现在文件区右键菜单 `插件` 分组时，使用：

```json
"fileContextActions": [
  {
    "command": "your-command-id",
    "when": {
      "selectionCount": "single",
      "targetKind": "file"
    }
  }
]
```

### 7.4 when 条件

当前只允许这些条件：

- `projectOpen`: `true | false`
- `selectionCount`: `any | none | single | multiple`
- `targetKind`: `any | file | directory | mixed`
- `extensions`: 扩展名数组，例如 `["png", ".jpg"]`

不要使用任意表达式，不要假设还存在别的条件语法。

## 8. Python 编码规范

默认按下面方式写 Python 插件：

```python
from pmc_plugin import run


def handle(request):
    print("plugin started", flush=True)


if __name__ == "__main__":
    run(handle)
```

要求如下：

- 默认使用 `pmc_plugin.run(...)` 包装入口。
- 需要进度时优先用 `progress(...)`。
- 需要宿主提示时优先用 `toast(...)`。
- 需要结构化结果时优先用 `result(...)`。
- 需要刷新当前项目视图时用 `refresh(...)`。
- 需要显式错误消息时用 `error(...)`。
- 普通日志一律用 `print(..., flush=True)`。

当前可直接使用的 SDK 方法：

- `load_request(path)`
- `emit(event_type, **payload)`
- `progress(value)`
- `toast(message, title=None, tone="info")`
- `refresh(scope="project", path=None)`
- `result(data)`
- `error(message)`
- `run(handler)`

默认编码要求：

- Python 文件使用 UTF-8。
- 如果输出中文日志，也要保持 UTF-8。
- 尽量避免复杂全局状态和隐式副作用。

## 9. 宿主会传给插件什么

宿主会以命令行方式启动入口：

```bash
main.py --pmc-request <json-path>
```

请求 JSON 当前包含这些关键字段：

- `apiVersion`
- `pluginId`
- `pluginName`
- `commandId`
- `commandTitle`
- `trigger`
- `projectPath`
- `currentPath`
- `selectedItems`
- `pluginScope`
- `appVersion`
- `permissions`

`selectedItems` 中每项包含：

- `name`
- `path`
- `isDir`
- `extension`

写插件时不要假设还能直接拿到宿主内部状态对象，也不要假设前端能被直接注入。

## 10. stdout 控制消息规范

插件可以输出普通日志，也可以输出控制消息。

控制消息格式：

```text
@pmc {"type":"progress","value":50}
@pmc {"type":"toast","title":"Done","message":"Finished","tone":"success"}
@pmc {"type":"refresh","scope":"project"}
@pmc {"type":"result","data":{"count":3}}
@pmc {"type":"error","message":"Something went wrong"}
```

当前宿主识别这些 `type`：

- `progress`
- `toast`
- `refresh`
- `result`
- `error`

兼容说明：

- 旧格式 `/***50*/` 仍可识别
- 新插件优先用 `progress(...)`

## 11. 依赖和打包规则

当前插件系统的目标是：

- 最终用户不需要单独安装 Python
- 最终用户不需要自己 `pip install`

因此你在写插件时应当遵守：

- 源插件阶段把依赖写进 `requirements.txt`
- 不要默认要求用户手工装依赖
- 如果用户明确需要分发包，再走 `plugin-tool pack`

可用命令：

```bash
node scripts/plugin-tool.mjs init <targetDir> [pluginId]
node scripts/plugin-tool.mjs validate <pluginDir>
node scripts/plugin-tool.mjs pack <pluginDir> [outputDir]
```

默认说明策略：

- 如果你只生成源插件，回复里说明如何 `validate`
- 如果用户要求可分发版本，额外说明如何 `pack`

## 12. 你最终应当如何交付

以后当你根据用户需求编写插件时，默认同时完成这些事情：

1. 直接创建插件目录和文件。
2. 让目录结构符合 PM Center 当前插件规范。
3. 让 `plugin.json`、`main.py`、`requirements.txt` 能互相对应。
4. 在回复里写明：
   - 插件目录放在哪里
   - 这个插件做什么
   - 有哪些动作和触发点
   - 是否有依赖
   - 如何本地验证
   - 有哪些默认假设

如果任务允许修改仓库，你应当直接落文件，不要只贴代码草稿。

默认回复里至少要包含：

- 生成的插件目录
- 关键行为说明
- 测试步骤
- 未明确需求时采用的假设

## 13. 最低验收清单

生成后的插件至少应满足：

- 插件目录结构完整
- `plugin.json` 存在且字段合法
- `entry` 指向的文件存在
- command 和 action 引用关系正确
- `when` 条件只使用当前支持的字段和值
- Python 入口可通过 `run(handle)` 正常读取请求
- 日志输出使用 `print(..., flush=True)`
- 进度、toast、result、refresh、error 的使用符合当前协议
- `requirements.txt` 与实际依赖一致
- 回复中提供了测试和放置说明

如果用户要求的是“可分发插件”，还应满足：

- `plugin-tool validate` 可通过
- `plugin-tool pack` 路径和使用方式说明清楚

## 14. 本地测试说明模板

默认可以按下面方式说明测试：

```bash
node scripts/plugin-tool.mjs validate examples/plugins/<plugin-id>
```

如果需要打包分发，再补：

```bash
node scripts/plugin-tool.mjs pack examples/plugins/<plugin-id> dist/plugin-packages
```

然后把插件目录手动复制到以下任一目录进行验证：

- 全局插件目录
- 项目目录下的 `.pm_center/plugins/`

最后在 PM Center 中：

- 打开项目
- 刷新插件列表
- 在工具栏插件菜单或文件右键菜单中触发动作
- 检查日志、进度、toast、refresh、result 是否符合预期

## 15. 范例引用

最小代码范例请参考：

- [hello_plugin/plugin.json](./hello_plugin/plugin.json)
- [hello_plugin/main.py](./hello_plugin/main.py)

说明：

- `hello_plugin` 只是最小代码范例
- 不再为它单独维护 README
- 以后以本文件作为唯一主说明入口
