# 组件包开发与分发

## 1. 组件目录

每个组件目录根部必须有 `component.json`。入口、UI 与资源都应位于这个根目录下，且 Manifest 中路径只能是相对路径。

```text
my-component/
  component.json
  main.py                    # python-action / python-worker 入口
  ui/
    index.html               # 可选 Script Surface
    app.js
    style.css
  assets/
  README.md
```

EXE/DLL 组件通常使用 `bin/windows-x64/`，但路径本身由 `entry` 明确声明。不要假设 Nexora 的安装目录、应用数据目录或 Python 安装位置。

## 2. 最小 Python 组件

```json
{
  "schemaVersion": 1,
  "id": "com.example.asset-audit",
  "name": "Asset Audit",
  "version": "0.1.0",
  "apiVersion": "1",
  "runtime": "python-action",
  "role": "feature",
  "distribution": "local",
  "uiMode": "contributed",
  "platforms": ["windows-x64"],
  "entry": "main.py",
  "capabilities": ["project.files.read"],
  "contributes": {
    "automationCommands": [{
      "id": "com.example.asset-audit.scan",
      "command": "scan",
      "name": "检查项目资源",
      "contextRequirement": "project-required",
      "executionSemantics": "idempotent",
      "requiredCapability": "project.files.read",
      "capabilityOperation": "read",
      "inputSchema": {"type": "object"},
      "outputSchema": {"type": "object"},
      "maxAttempts": 2,
      "maxParallelism": 1,
      "timeoutMs": 120000
    }]
  },
  "resources": {"maxMemoryMb": 256, "maxParallelism": 1, "timeoutMs": 120000},
  "publisher": "Example Studio"
}
```

```python
from nexora_sdk import (
    is_cancelled,
    log,
    progress,
    read_request,
    write_error,
    write_result,
)

def main():
    request = read_request()
    if not request.context.project_path:
        write_error("需要项目上下文")
        return
    progress(0.1, "正在检查")
    if is_cancelled(request.context):
        write_error("操作已取消")
        return
    log(f"正在检查 {request.context.project_path}")
    progress(1.0, "完成")
    write_result({"checked": True, "projectPath": request.context.project_path})

if __name__ == "__main__":
    main()
```

完整字段定义以 `shared/pmc-platform/schemas/v1/component-manifest.schema.json` 为准。稳定 ID 应使用开发者命名空间，例如 `com.example.*`；第三方不得使用 `nexora.*` 伪装为内置发布者。

## 3. 运行时选择

| 运行时 | 适用情况 | 进程协议与限制 |
| --- | --- | --- |
| `python-action` | 一次性操作、脚本自动化 | 每次调用启动 Nexora 准备的 Python；stdin 输入 JSON，stdout 返回结果 JSON。 |
| `python-worker` | 连续短任务或需要复用加载成本 | 首行输出 `{"type":"worker-ready"}`；之后使用 JSON 行协议，必须响应取消/心跳。 |
| `native-process` | 已有 CLI、强隔离计算 | 接收 `nexora.component.v1` JSON 请求；最后一个 JSON 结果行应为 `{"ok":true,"result":...}` 或 `{"ok":false,"error":"..."}`。 |
| `native-library` | 性能关键的 DLL | 仅 Windows；Nexora 只通过隔离 `pmc-component-host` 加载，必须满足 ABI 和 PE 架构检查。 |
| `data-pack` | 只读资料、模板、静态资源 | 无可执行代码；宿主提供受限的 `describe`、`list`、`read-text`、`read-json`。 |

运行时会给可执行组件传入的请求包含 `protocol`、`operationId`、`componentId`、`componentVersion`、`command`、`input` 和 `cancellationFile`。当取消文件出现时，组件应尽快清理并退出；宿主超时、内存超限或用户取消时仍会终止完整进程树。

## 4. 依赖、Capability 与自动化

- 用 `requiresComponents` 声明不可缺少的能力，用 `optionalComponents` 表示可降级功能；不要探测未声明的组件。
- `capabilities` 只声明组件可能请求的能力。真正调用仍需宿主获得一次性授权令牌。
- 自动化命令需要声明项目上下文、执行语义、输入/输出 Schema、重试次数、并行数与超时。
- `pure` 和 `idempotent` 才允许在崩溃后自动重试；副作用无法安全重放时使用 `non-idempotent`。
- Profile 绑定决定何时运行：手动、白名单事件或五段式 cron。保存绑定不会自动运行。

可调用的事件白名单与完整行为见 `docs/NEXT_MAJOR_R11_SCRIPT_AUTOMATION.md`。脚本组件可通过 `nexora_sdk.call_component()` 调用其 Manifest 已声明依赖的命令，不能调用任意 Tauri command。

## 5. Script Surface

Script Surface 是第三方界面的公开入口：组件清单声明 HTML 入口和可以调用的自身命令。当前它会从功能中心打开为沙箱对话框，也可在开发者工作台预览。`placements` 是保留的兼容放置描述，当前不会仅因声明了 `shell`、`workspace`、`dialog` 或 `widget` 就自动把页面嵌进对应宿主位置。页面运行于 `iframe sandbox="allow-scripts"`，没有 Tauri IPC、宿主 DOM、本机 URL 或任意网络访问权；它通过每次会话 nonce 消息桥调用自己 Manifest 中允许的自动化命令。

优先用 Script Surface 做界面。`widgets`、`dataSources`、任意宿主菜单和任意 React 渲染器目前没有独立第三方 ABI，不能仅靠声明字段假设可用。

## 6. 签名与安装

开发阶段可从目录安装；包含 Python/EXE/DLL 的目录必须由用户显式信任后才能运行。分发阶段应使用签名包：

```powershell
npm run component:keygen -- D:\Keys\example-publisher.json
npm run component:pack -- D:\Work\asset-audit D:\Release\asset-audit.pmc-pack --key D:\Keys\example-publisher.json --publisher-id com.example --publisher-name "Example Studio" --license MIT
```

包安装会检查 BLAKE3 摘要、签名、发布者信任、ZIP 条目、路径穿越、大小、平台、入口、依赖、DLL 架构和表现模板。签名有效但发布者未知时，需要用户先审阅并信任发布者；不要把“安装成功”解释为“已获所有 Capability”。

## 7. 发布前检查

1. 用 Schema/工作台校验 Manifest；
2. 以最小权限和最小 `resources` 运行；
3. 测试无项目、有项目、拒绝权限、取消、超时、重复调用与异常退出；
4. 测试缺失依赖、版本不兼容、卸载和重装；
5. 用未信任发布者与受损 `.pmc-pack` 测试安装拒绝；
6. 在干净 Windows 用户或第二台设备执行安装、加入 Profile、卸载和回滚；
7. 发布包附带许可证、版本说明、支持的 Nexora/API 版本和已知限制。
