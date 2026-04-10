# 编写 PM Center 插件前先读本文件

本文件是给以后帮你写 PM Center 插件的 AI、代理或协作者使用的执行规范。

默认要求：
- 读完本文件后，直接产出可落地插件，而不是只给思路或方案。
- 默认先把插件写到 `examples/plugins/<plugin-id>/`，方便你先审阅、再复制到真实插件目录。
- 默认输出完整可运行版本，而不是只贴几个代码片段。

如果用户没有明确要求“先只看方案”，你应当直接写插件并说明测试方式、放置路径和默认假设。

## 1. 这份文档的用途

当用户说“帮我写一个 PM Center 插件”时，你应当先遵守这里的规则，再开始产出代码。

这不是纯接口说明书，而是默认工作协议。目标是让另一位工程师或 AI 读完后，能直接开始编写插件。

## 2. 默认工作方式

当用户要求编写插件时，你应当按下面顺序工作：

1. 先判断插件用途、触发位置、目标对象和输出要求。
2. 尽量复用当前仓库已有的插件系统约定，不另造一套接口。
3. 如果信息足够，直接生成完整插件目录和代码。
4. 只有在缺少关键产品意图时才提问，例如：
   - 放工具栏还是右键菜单
   - 处理单文件、目录还是多选文件
   - 是否必须依赖第三方 Python 包
5. 如果用户没有特别说明，默认先做源码插件，不先停在方案阶段。

## 3. 默认交付物

除非用户明确指定别的目录，默认生成到：

```text
examples/plugins/<plugin-id>/
```

默认至少包含：

```text
examples/plugins/<plugin-id>/
  plugin.json
  main.py
  requirements.txt
```

按需补充：
- `README.md`
- `assets/`
- `vendor/`

约束如下：
- `plugin.json` 必须生成。
- `main.py` 必须生成。
- `requirements.txt` 默认也生成；没有依赖时可以为空文件。
- 只有在插件行为、配置方式或测试方式不够直观时，才额外生成插件自己的 `README.md`。
- `vendor/` 不是源码插件的默认输出；只有在明确做分发包时才出现。

## 4. 需求输入模板

以后用户提插件需求时，理想输入应尽量包含这些信息：

- 插件用途：要解决什么问题。
- 触发位置：工具栏插件菜单，还是文件区右键菜单。
- 目标对象：项目、当前目录、单文件、多文件、目录。
- 选择约束：空选、单选、多选是否允许。
- 文件类型：例如 `png`、`txt`、目录。
- 执行动作：重命名、导出、扫描、生成结果、调用外部工具等。
- 输出要求：日志、进度、toast、refresh、result。
- 依赖要求：是否需要第三方 Python 包。
- 分发要求：只要源码，还是还要可打包分发。

如果用户没说完整，你应当使用合理默认值并在最终说明里写出假设。

## 5. 默认假设

如果用户没有明确说明，统一采用：

- `runtime = "python"`
- `apiVersion = "1"`
- `entry = "main.py"`
- 默认输出目录为 `examples/plugins/<plugin-id>/`
- 默认交付完整可运行源码插件
- 默认优先使用标准库和现有 `pmc_plugin` SDK
- 默认不引入不必要的第三方依赖

额外规则：
- 如果用户没有给插件 `id`，使用英文 `kebab-case` 自动生成。
- 插件目录名默认与 `plugin-id` 一致。
- 明显依赖当前选中对象的动作，优先放 `fileContextActions`。
- 更像项目级工具的动作，优先放 `toolbarActions`。

## 6. 当前能力边界

当前插件系统只支持：

- Python 插件
- 本地目录加载
- 声明式 UI 挂点
- CLI 入口脚本
- 宿主通过 JSON 文件传入请求
- 插件通过 stdout 输出日志和 `@pmc {...}` 控制消息

当前不支持：

- React 组件注入
- 自定义前端页面或 webview
- JS 插件运行时
- Lua 插件运行时
- 在线安装
- 插件商店
- 权限审批弹窗
- 远程插件下载

如果用户要求超出以上边界，你应当明确说明限制，并优先提供符合现状的 Python 替代实现。

## 7. `plugin.json` 编写规则

插件必须包含 `plugin.json`。

当前清单可用字段：

- `id`
- `name`
- `version`
- `apiVersion`
- `runtime`
- `entry`
- `description`
- `minAppVersion`
- `enabledByDefault`
- `settingsPanel`
- `contributes`
- `permissions`

至少正确填写：

- `id`
- `name`
- `version`
- `apiVersion`
- `runtime`
- `entry`
- `contributes`

### 7.1 `commands`

所有动作都必须先在 `contributes.commands` 中声明。

每个 command 至少包含：
- `id`
- `title`

建议同时补上：
- `description`

### 7.2 `toolbarActions`

当动作应出现在工具栏 `插件` 菜单中时，使用：

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

### 7.3 `fileContextActions`

当动作应出现在文件区右键菜单中时，使用：

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

默认情况下，`fileContextActions` 会出现在右键菜单底部的 `插件` 分组中。

### 7.3.1 右键菜单插入规则

右键菜单额外支持 `menu` 配置，用来控制动作是直接插入主菜单，还是放到二级菜单里：

```json
"fileContextActions": [
  {
    "command": "quick-run",
    "when": {
      "selectionCount": "single",
      "targetKind": "file"
    },
    "menu": {
      "placement": "inline"
    }
  },
  {
    "command": "batch-report",
    "when": {
      "selectionCount": "multiple",
      "targetKind": "file"
    },
    "menu": {
      "placement": "inline",
      "submenu": "批量工具"
    }
  }
]
```

规则如下：

- `menu` 只对 `fileContextActions` 生效。
- `menu.placement` 只支持 `section` 或 `inline`。
- 不写 `menu` 时，等同于 `placement = "section"`。
- `placement = "section"`：动作仍显示在 `插件` 分组中。
- `placement = "inline"`：动作直接插入右键主菜单。
- `submenu` 为非空字符串时，该动作进入一个二级菜单。
- 当前只支持一级二级菜单，不支持更深层级嵌套。

建议：

- 高频动作用 `inline`。
- 同类批处理动作用同一个 `submenu` 收拢。
- 不要把所有动作都塞进主菜单，避免右键菜单过长。

### 7.4 `when`

当前只支持这些条件：

- `projectOpen`: `true | false`
- `selectionCount`: `any | none | single | multiple`
- `targetKind`: `any | file | directory | mixed`
- `extensions`: 扩展名数组，例如 `["png", ".jpg"]`

不要使用任意表达式，也不要假设存在别的条件语法。

### 7.5 `settingsPanel`

插件现在可以声明设置面板，用来提供介绍说明、参数表单，以及文件选择类设置。

最小示例：

```json
"settingsPanel": {
  "summary": "这段文字会显示在插件介绍里。",
  "sections": [
    {
      "title": "用途",
      "content": "解释这个插件适合做什么。",
      "tone": "info"
    }
  ],
  "settings": {
    "title": "运行参数",
    "description": "这些参数会保存下来，并在插件执行时传给 request.settingsValues。",
    "storage": "pluginDir",
    "fields": [
      {
        "key": "brightnessFactor",
        "label": "亮度倍数",
        "type": "number",
        "defaultValue": 1.3,
        "min": 0.1,
        "max": 4,
        "step": 0.05,
        "unit": "x"
      },
      {
        "key": "sampleImage",
        "label": "样本图片",
        "type": "file",
        "accept": ["png", "jpg", "jpeg"],
        "fileStoreMode": "copy",
        "picker": "file"
      }
    ]
  }
}
```

当前支持的 `settings.fields[].type`：

- `text`
- `textarea`
- `number`
- `boolean`
- `select`
- `file`

文件字段补充规则：

- `picker` 只支持 `file` 或 `directory`
- `fileStoreMode` 只支持 `path` 或 `copy`
- `picker = "directory"` 时只能用 `path`

设置存储位置：

- `storage = "appData"`：保存到应用本地数据目录
- `storage = "pluginDir"`：保存到插件目录下的 `.pmc`

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
- 普通日志统一用 `print(..., flush=True)`。

当前可直接使用的 SDK 方法：

- `load_request(path)`
- `get_settings(request)`
- `get_setting(request, key, default=None)`
- `get_interaction_responses(request)`
- `get_interaction_response(request, request_id)`
- `is_confirmed(request, request_id)`
- `emit(event_type, **payload)`
- `progress(value)`
- `toast(message, title=None, tone="info")`
- `refresh(scope="project", path=None)`
- `result(data)`
- `error(message)`
- `confirm(message, request_id, title=None, confirm_text="确认", cancel_text="取消", data=None)`
- `run(handler)`

默认编码要求：

- Python 文件使用 UTF-8。
- 输出中文日志时也保持 UTF-8。
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
- `interactionResponses`
- `settingsValues`
- `settingsStoragePath`
- `settingsFilesDir`

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
@pmc {"type":"confirm","requestId":"delete-files","title":"确认删除","message":"确认继续吗？","confirmText":"删除","cancelText":"取消","data":{"items":["a.blend1","b.blend2"]}}
@pmc {"type":"refresh","scope":"project"}
@pmc {"type":"result","data":{"count":3}}
@pmc {"type":"error","message":"Something went wrong"}
```

宿主当前识别这些 `type`：

- `progress`
- `toast`
- `confirm`
- `refresh`
- `result`
- `error`

兼容说明：

- 旧格式 `/***50*/` 仍可识别。
- 新插件优先用 `progress(...)`。

## 11. 依赖与打包规则

当前插件系统的目标是：

- 最终用户不需要单独安装 Python。
- 最终用户不需要自己执行 `pip install`。

因此你在写插件时应当遵守：

- 源码插件阶段，把依赖写进 `requirements.txt`。
- 不要默认要求用户手工安装依赖。
- 只有在用户明确要求分发包时，再走 `plugin-tool pack`。

可用命令：

```bash
node scripts/plugin-tool.mjs init <targetDir> [pluginId]
node scripts/plugin-tool.mjs validate <pluginDir>
node scripts/plugin-tool.mjs pack <pluginDir> [outputDir]
```

## 12. 输出规范

以后生成插件时，你的回复至少应同时说明：

- 插件目录结构
- 关键文件内容或关键行为
- 动作会出现在哪个挂点
- 依赖策略
- 如何验证
- 采用了哪些默认假设

如果任务允许直接改仓库，就直接把文件落到仓库里，不要只贴草稿代码。

## 13. 验收清单

生成后的插件至少应满足：

- 目录结构完整
- `plugin.json` 存在且字段合法
- `entry` 指向的文件存在
- command 和 action 引用关系正确
- `when` 条件只使用当前支持的字段和值
- `fileContextActions.menu` 只使用当前支持的规则
- Python 入口能通过 `run(handle)` 正常读取请求
- 普通日志使用 `print(..., flush=True)`
- `progress`、`toast`、`result`、`refresh`、`error` 使用方式清晰
- `requirements.txt` 与实际依赖一致
- 可以通过 `plugin-tool validate`

如果用户要求可分发插件，还应补充：

- `plugin-tool pack` 的执行说明
- 是否会产出 `vendor/`

## 14. 本地测试模板

默认验证命令：

```bash
node scripts/plugin-tool.mjs validate examples/plugins/<plugin-id>
```

如果需要打包分发，再执行：

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
- [demo_plugin/plugin.json](./demo_plugin/plugin.json)
- [demo_plugin/main.py](./demo_plugin/main.py)
- [blender-backup-cleaner/plugin.json](./blender-backup-cleaner/plugin.json)
- [blender-backup-cleaner/main.py](./blender-backup-cleaner/main.py)

说明：

- `hello_plugin` 只是最小代码范例。
- 它现在同时演示了默认插件分组、主菜单直插按钮和二级菜单按钮。
- `demo_plugin` 演示了介绍面板、参数表单、文件选择和设置持久化。
- `blender-backup-cleaner` 演示了工具栏动作、项目级递归扫描、确认弹窗和批量删除文件。
- 不再为它单独维护 README。
