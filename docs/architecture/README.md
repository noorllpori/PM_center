# Nexora 本体架构资料

本文档组记录 Nexora 2.8.5 当前开发线的实际架构。它服务于维护本体、审查大版本改动和决定后续扩展边界的人员；它不是第三方组件 API 手册。

## 阅读顺序

1. [CURRENT_ARCHITECTURE.md](CURRENT_ARCHITECTURE.md)：当前进程、运行时、数据与责任边界。
2. [COHERENCE_AND_EXTENSION_AUDIT.md](COHERENCE_AND_EXTENSION_AUDIT.md)：逻辑自洽检查、真实扩展能力、风险与优先级。
3. 面向使用安装版 Nexora 开发组件或装配空间的人员，请转到 [../extension-development/README.md](../extension-development/README.md)。

## 文档状态

- **已实现**：可以从当前源码、Schema、示例或自动测试中确认的行为。
- **待实机验收**：代码和本机测试存在，但仍需要 MSI、第二台设备、真实 Blender 或大规模素材验证。
- **规划**：仅说明未来方向，不能作为组件开发依赖。

本文不替代下列“机器可验证”的合同来源：

- `shared/pmc-platform/schemas/v1/`：组件、Profile、装配空间、表现模板和包格式的 v1 Schema；
- `shared/pmc-platform/src/`：Schema 背后的 Rust 语义校验；
- `src-tauri/resources/script-sdk/nexora_sdk/`：Python 脚本组件 SDK v1；
- `examples/`：当前可安装、可验证的组件和模板示例；
- `docs/NEXT_MAJOR_*`：每个里程碑的设计、实现记录与验收证据。

## 维护规则

任何会改变下列任一项的提交，必须同步更新本目录或明确链接到新的合同版本：组件清单字段、Profile/装配空间格式、组件进程协议、Capability、组件开发流程、实际可公开贡献的 UI 类型。不要只更新历史里程碑文档。
