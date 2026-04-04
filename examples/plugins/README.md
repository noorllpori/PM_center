# PM Center 插件接口说明（当前实现）

这份文档描述的是仓库里已经实现的插件接口，不是未来规划稿。

当前版本的插件系统有几个边界：

- 只支持 `Python` 插件。
- 插件通过本地目录加载，不支持在线安装。
- 插件入口是命令行脚本，不是前端组件。
- UI 挂点目前只有工具栏 `插件` 菜单和文件区右键菜单 `插件` 分组。
- 插件和宿主之间的通信方式是：
  - 宿主写入一个请求 JSON 文件。
  - 插件读取这个 JSON。
  - 插件通过标准输出打印普通日志，或者打印 `@pmc {...}` 控制消息。

## 1. 插件目录结构

最小结构：

```text
my_plugin/
  plugin.json
  main.py
```

常见完整结构：

```text
my_plugin/
  plugin.json
  main.py
  vendor/
  assets/
  README.md
  requirements.txt
```

说明：

- `plugin.json` 是清单文件。
- `main.py` 是入口脚本。
- `vendor/` 用来放打包好的第三方依赖。
- `requirements.txt` 只在打包时使用，不会在最终用户机器上自动 `pip install`。

## 2. 插件放到哪里

插件支持两个作用域：

- 全局插件目录：应用数据目录下的 `plugins/`
- 项目插件目录：`<project>/.pm_center/plugins/`

规则：

- 项目插件和全局插件可以同时存在。
- 如果 `id` 相同，项目插件覆盖全局插件。

## 3. plugin.json 清单字段

当前可用字段：

- `id`: 插件唯一标识。
- `name`: 插件显示名。
- `version`: 插件版本。
- `apiVersion`: 当前固定写 `1`。
- `runtime`: 当前固定写 `python`。
- `entry`: 入口文件，通常是 `main.py`。
- `description`: 描述。
- `minAppVersion`: 最低宿主版本，可选。
- `enabledByDefault`: 是否默认启用。
- `contributes`: 动作声明。
- `permissions`: 预留字段，v1 先记录，不做权限弹窗。

一个最小可运行示例：

```json
{
  "id": "hello-plugin",
  "name": "Hello Plugin",
  "version": "0.1.0",
  "apiVersion": "1",
  "runtime": "python",
  "entry": "main.py",
  "description": "A sample PM Center plugin.",
  "enabledByDefault": true,
  "contributes": {
    "commands": [
      {
        "id": "hello",
        "title": "Hello Plugin",
        "description": "Show a toast and return a small result payload."
      }
    ],
    "toolbarActions": [
      {
        "command": "hello",
        "when": {
          "projectOpen": true
        }
      }
    ],
    "fileContextActions": [
      {
        "command": "hello",
        "when": {
          "selectionCount": "single",
          "targetKind": "file"
        }
      }
    ]
  },
  "permissions": []
}
```

可以直接参考：

- [hello_plugin/plugin.json](/e:/Project/PM_center/examples/plugins/hello_plugin/plugin.json)

## 4. contributes 的可用挂点

当前只支持这三类：

- `commands`: 先声明动作本身。
- `toolbarActions`: 把某个 command 挂到工具栏 `插件` 菜单。
- `fileContextActions`: 把某个 command 挂到文件区右键菜单 `插件` 分组。

动作条件 `when` 当前只支持：

- `projectOpen`: `true | false`
- `selectionCount`: `any | none | single | multiple`
- `targetKind`: `any | file | directory | mixed`
- `extensions`: 文件扩展名数组，例如 `["png", ".jpg"]`

说明：

- `extensions` 只对文件选择有效。
- 选中了目录时，`extensions` 不会匹配。
- 目前不支持任意表达式，不支持复杂逻辑语法。

## 5. Python 入口协议

宿主启动插件时使用：

```bash
main.py --pmc-request <json-path>
```

推荐写法：

```python
from pmc_plugin import run


def handle(request):
    print("hello plugin", flush=True)


if __name__ == "__main__":
    run(handle)
```

`run(handler)` 会自动：

- 解析 `--pmc-request`
- 读取 JSON 请求
- 把请求对象传给 `handler(request)`
- 如果抛异常，自动发出一条 `error` 控制消息并继续抛出异常

## 6. 宿主传给插件的 request JSON

插件会收到一个 JSON 对象，当前字段如下：

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

其中 `selectedItems` 的每一项结构如下：

- `name`
- `path`
- `isDir`
- `extension`

一个示例：

```json
{
  "apiVersion": "1",
  "pluginId": "hello-plugin",
  "pluginName": "Hello Plugin",
  "commandId": "hello",
  "commandTitle": "Hello Plugin",
  "trigger": "file-context",
  "projectPath": "E:/Project/Demo",
  "currentPath": "E:/Project/Demo/assets",
  "selectedItems": [
    {
      "name": "demo.png",
      "path": "E:/Project/Demo/assets/demo.png",
      "isDir": false,
      "extension": "png"
    }
  ],
  "pluginScope": "project",
  "appVersion": "1.5.2",
  "permissions": []
}
```

## 7. 当前可调用的 Python SDK 方法

`src-tauri/resources/plugin-sdk/pmc_plugin/__init__.py` 里目前有这些方法：

```python
from pmc_plugin import load_request, emit, progress, toast, refresh, result, error, run
```

说明：

- `load_request(path)`: 读取请求 JSON。
- `emit(event_type, **payload)`: 底层方法，向 stdout 输出 `@pmc {...}`。
- `progress(value)`: 上报进度，自动夹到 `0-100`。
- `toast(message, title=None, tone="info")`: 让宿主弹提示。
- `refresh(scope="project", path=None)`: 请求宿主刷新当前项目视图。
- `result(data)`: 输出结构化结果。
- `error(message)`: 输出错误控制消息。
- `run(handler)`: 推荐入口包装器。

一个完整例子：

```python
from pmc_plugin import progress, refresh, result, run, toast


def handle(request):
    selected = request.get("selectedItems", [])
    first_name = selected[0]["name"] if selected else "nothing"

    toast(
        f"Hello from plugin. Current selection: {first_name}",
        title="Hello Plugin",
        tone="success",
    )
    progress(25)
    print("Example plugin is running...", flush=True)
    progress(100)
    result({
        "selectionCount": len(selected),
        "firstItem": first_name,
    })
    refresh()


if __name__ == "__main__":
    run(handle)
```

## 8. stdout 控制消息协议

除了普通日志，插件还可以输出控制消息：

```text
@pmc {"type":"progress","value":50}
@pmc {"type":"toast","title":"Done","message":"Finished","tone":"success"}
@pmc {"type":"refresh","scope":"project"}
@pmc {"type":"result","data":{"count":3}}
@pmc {"type":"error","message":"Something went wrong"}
```

当前宿主识别的 `type`：

- `progress`
- `toast`
- `refresh`
- `result`
- `error`

当前行为：

- `progress` 会更新任务面板进度。
- `toast` 会弹出宿主提示。
- `refresh` 会刷新当前项目视图。
- `result` 会记录到任务日志里。
- `error` 会记录错误；如果插件最终退出码还是 `0`，宿主也会把任务视为失败。

兼容格式：

- 旧的进度格式 `/***50*/` 仍然有效。
- 新插件建议优先用 `progress(50)`。

## 9. 运行时环境

插件运行时目前由宿主自动准备：

- 内置 `Windows x64 embeddable CPython 3.11`
- 仅供插件系统使用
- 不替换现有任务系统、Blender、用户自管 Python 环境

宿主启动插件时会额外设置这些环境变量：

- `PYTHONIOENCODING=utf-8`
- `PYTHONUTF8=1`
- `PMC_PLUGIN_DIR`
- `PMC_PLUGIN_ID`
- `PMC_PLUGIN_SCOPE`
- `PYTHONPATH`

其中 `PYTHONPATH` 当前会包含：

- `plugin-sdk/`
- 当前插件目录
- 如果存在，再加上 `vendor/`

这意味着插件里可以直接：

- `import pmc_plugin`
- 导入自身目录下的模块
- 导入 `vendor/` 里的第三方依赖

## 10. 打包和校验

仓库里有一个插件工具：

- `node scripts/plugin-tool.mjs init <targetDir> [pluginId]`
- `node scripts/plugin-tool.mjs validate <pluginDir>`
- `node scripts/plugin-tool.mjs pack <pluginDir> [outputDir]`

含义：

- `init`: 初始化一个 Python 插件骨架。
- `validate`: 校验 `plugin.json` 和入口文件。
- `pack`: 把插件复制到输出目录，并把 `requirements.txt` 里的依赖装进 `vendor/`。

示例：

```bash
node scripts/plugin-tool.mjs validate examples/plugins/hello_plugin
node scripts/plugin-tool.mjs pack examples/plugins/hello_plugin dist/plugin-packages
```

## 11. 当前没有开放的能力

从当前实现来看，这些能力还没有开放：

- 直接注入 React 组件
- 自定义前端页面或插件 webview
- JS / Lua 运行时
- 插件主动调用宿主内部 Tauri command 的 RPC SDK
- 热重载
- 在线安装、插件商店、签名校验、权限弹窗

也就是说，当前插件接口本质上是：

- 声明式动作注册
- Python CLI 执行
- 请求 JSON 输入
- stdout 控制消息输出

## 12. 现成示例

可以直接看这个插件：

- [hello_plugin/plugin.json](/e:/Project/PM_center/examples/plugins/hello_plugin/plugin.json)
- [hello_plugin/main.py](/e:/Project/PM_center/examples/plugins/hello_plugin/main.py)
- [hello_plugin/README.md](/e:/Project/PM_center/examples/plugins/hello_plugin/README.md)
