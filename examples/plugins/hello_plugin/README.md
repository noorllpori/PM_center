# hello_plugin 示例说明

这是一个最小可运行的 PM Center Python 插件示例。

相关文件：

- [plugin.json](/e:/Project/PM_center/examples/plugins/hello_plugin/plugin.json)
- [main.py](/e:/Project/PM_center/examples/plugins/hello_plugin/main.py)

## 1. 这个示例做了什么

它注册了一个叫 `hello` 的命令，并把这个命令同时挂到了两个位置：

- 工具栏 `插件` 菜单
- 文件右键菜单 `插件` 分组

执行后它会：

- 读取当前选中的文件
- 弹一个成功提示
- 上报进度 `25 -> 100`
- 输出一条普通日志
- 返回一个结果对象

## 2. plugin.json 怎么看

`plugin.json` 里最关键的是三段：

- `commands`: 先声明命令 `hello`
- `toolbarActions`: 把 `hello` 挂到工具栏
- `fileContextActions`: 把 `hello` 挂到文件右键菜单

其中右键菜单动作还加了限制：

- `selectionCount: "single"`
- `targetKind: "file"`

所以它只会在“只选中一个文件”时出现。

## 3. main.py 怎么看

入口代码很短，重点只有四步：

```python
from pmc_plugin import progress, result, run, toast


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


if __name__ == "__main__":
    run(handle)
```

分别对应：

- `request`: 宿主传入的上下文
- `toast(...)`: 宿主弹提示
- `progress(...)`: 更新任务进度
- `print(...)`: 输出普通日志
- `result(...)`: 输出结构化结果
- `run(handle)`: 统一处理命令行参数和异常

## 4. 触发后大概会收到什么 request

如果你在文件区右键一个 `README.md`，插件看到的请求大概会像这样：

```json
{
  "apiVersion": "1",
  "pluginId": "hello-plugin",
  "pluginName": "Hello Plugin",
  "commandId": "hello",
  "commandTitle": "Hello Plugin",
  "trigger": "file-context",
  "projectPath": "E:/Project/PM_center",
  "currentPath": "E:/Project/PM_center/examples/plugins/hello_plugin",
  "selectedItems": [
    {
      "name": "README.md",
      "path": "E:/Project/PM_center/examples/plugins/hello_plugin/README.md",
      "isDir": false,
      "extension": "md"
    }
  ],
  "pluginScope": "global",
  "appVersion": "1.5.2",
  "permissions": []
}
```

## 5. 它会输出什么

这个插件运行时，stdout 大概会出现这些内容：

```text
@pmc {"type":"toast","title":"Hello Plugin","message":"Hello from plugin. Current selection: README.md","tone":"success"}
@pmc {"type":"progress","value":25}
Example plugin is running...
@pmc {"type":"progress","value":100}
@pmc {"type":"result","data":{"selectionCount":1,"firstItem":"README.md"}}
```

说明：

- 以 `@pmc ` 开头的是宿主可识别控制消息
- 普通文本会进入任务日志

## 6. 如何拿这个示例做自己的插件

最简单的做法：

1. 复制 `hello_plugin` 目录一份，改成你的插件名。
2. 改 `plugin.json` 里的 `id`、`name`、`commands`、`when`。
3. 改 `main.py` 里的业务逻辑。
4. 如果依赖第三方库，写到 `requirements.txt`。
5. 用 `node scripts/plugin-tool.mjs validate <你的插件目录>` 先校验。
6. 用 `node scripts/plugin-tool.mjs pack <你的插件目录>` 打包出带 `vendor/` 的分发目录。

## 7. 建议的开发顺序

建议先按这个顺序做：

1. 先让 `plugin.json` 只注册一个工具栏动作。
2. 在 `main.py` 里先 `print(request)` 或只读 `selectedItems`。
3. 确认能触发后，再加 `toast`、`progress`、`result`。
4. 最后再补依赖和复杂逻辑。

这样定位问题会快很多。
