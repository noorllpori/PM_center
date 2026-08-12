# 项目资料与组件存储示例

此目录可直接在 Nexora 的“脚本开发者工作台”中选择、校验并设为受信任开发目录。

三个命令分别演示三种 Capability，避免把多个权限合并到一个过宽命令中：

- `inspect-project`：读取受控项目快照和分页文件列表。
- `write-project-blob`：写入 `.pm_center/components/nexora.example.project-storage/state` 对应的受控 Blob；组件不自行拼接真实路径。
- `resolve-storage-directory`：在获得 `project.storage.direct` 后取得本组件自己的缓存目录。

先打开任意项目，再从工作台手动运行命令。停用或卸载组件不会删除 `state`；缓存中心可以清理 `cache`。
