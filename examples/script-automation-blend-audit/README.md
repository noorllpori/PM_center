# Blender 文件检查自动化示例

此目录用于 R11-A 人工验收，演示：

- `python-action`；
- `project-required`；
- `project.files.read` Capability；
- 对 `pmc.blendio ^1.0` 的必需依赖；
- 通过 `nexora_sdk.call_component` 实际调用 BlenderIO `inspect`；
- 读写组件自己的项目级状态，并向自身沙箱页面发送受控事件；
- `file.changed`、`render.batch-created` 与 `render.batch-completed` 事件声明；
- 只允许调用自身 `inspect` 命令的隔离页面。

在“功能中心 -> 脚本自动化”中选择此目录，依次执行校验、信任、安装和加入当前装配。卸载 BlenderIO 后重新校验或应用装配，应因必需依赖缺失而拒绝。

调试输入示例：

```json
{
  "blendPath": "scene.blend"
}
```

正式 `.pmc-pack` 使用仓库现有签名链路生成：

```powershell
npm run component:keygen -- C:\secure\nexora-example-key.json
npm run component:pack -- examples\script-automation-blend-audit dist\nexora-example-blend-audit.pmc-pack --key C:\secure\nexora-example-key.json --publisher-id nexora.examples --publisher-name "Nexora Examples"
```

签名私钥只能保存在组件目录和仓库之外。
