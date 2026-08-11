# Nexora 文档

> 当前基线：Nexora 2.8.5，整理日期：2026-08-12。本文档区只保留当前开发与安装版扩展所需的资料。

## 阅读顺序

1. [项目现状与开发约定](PROJECT_CONTEXT.md)：当前能力、已知边界和下一步。
2. [实施路线](NEXT_MAJOR_IMPLEMENTATION_ROADMAP.md)：R0-R18 状态、依赖与验收门槛。
3. [R15-R18 总路线](NEXT_MAJOR_R15_R18_CLOUD_ECOSYSTEM_AND_HOST_ABI.md)：云端生态、公开宿主 ABI 与可视化装配的固定范围。
4. [R17-R18 界面模板专项](NEXT_MAJOR_R17_R18_INTERFACE_TEMPLATE_SLOTS.md)：正在验证的界面模板和插槽装配能力。
5. [本体架构资料](architecture/README.md)：仅供维护 Nexora 本体时阅读。
6. [扩展与装配空间开发指南](extension-development/README.md)：仅面向安装版 Nexora EXE 的组件、模板与方案开发者。

## 文档状态

- 已实现：当前源码、Schema、示例或测试可以确认。
- verifying：代码与自动检查已完成，仍待人工、真实素材、双机或安装环境验收。
- in progress：正在开发，不能作为稳定外部合同。
- pending：范围已冻结但尚未开始实现。

Schema、Rust 合同、Python SDK 和组件示例是机器可验证的事实来源。说明文档不得与它们冲突。

## 历史资料

R0-R14 的阶段设计、验收记录、旧总规划和暂缓方向均已移入 [2026-08-12 归档](archive/2026-08-12/README.md)。归档用于追溯，不作为当前开发或第三方接口依据。
