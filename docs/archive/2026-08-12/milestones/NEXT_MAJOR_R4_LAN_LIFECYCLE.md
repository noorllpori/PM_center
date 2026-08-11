> **历史归档（2026-08-12）**：本文件仅保留为实现与验收追溯资料，不代表当前公开能力或后续开发依据。请先阅读 docs/README.md。
# R4 局域网服务生命周期接入

> 状态：`accepted`
>
> 日期：2026-08-03
>
> 基线版本：2.8.4
>
> 模块 ID：`builtin.lan-collaboration`

2026-08-03 产品人工验收通过：模块关闭和重新开启行为正常，未发现资源恢复问题。

## 1. 本次范围

本次是 R4 第二个独立验收领域，管理已有局域网协同功能的运行资源：

- UDP `31523` 设备发现与资料广播；
- TCP `31524` 消息、资料、历史同步和文件传输控制；
- 可选 Nexora Server 长连接与重连任务；
- 局域网和服务器文件传输取消；
- 通知点击回到对应会话；
- 全局联系人、消息、头像、接收目录和传输历史数据库。

不新增聊天、联系人或传输产品功能，也不修改协议版本和端口。

## 2. 模块声明

模块为软件级全局内置模块，首次升级默认启用。用户手动停用后，`module-runtime.json` 保存停用选择，后续启动不再自动运行网络服务。

声明的主要 Capability：

- `network.lan.discover`
- `network.lan.message`
- `network.lan.transfer`
- `network.server.connect`
- `network.http.request`
- `notification.send`
- `filesystem.external.read`
- `filesystem.external.write`
- 软件级资料和设置读写

组件 `builtin.lan-collaboration.service` 已登记到 CapabilityGateway。本次保持现有业务调用兼容，后续统一组件运行时再把每次网络和文件操作切换为短期 token。

## 3. 生命周期所有权

ResourceRegistry 使用一个 `network-listener` 聚合资源表示同一服务共同拥有：

- UDP 发现 socket 和任务；
- TCP listener 和连接处理任务；
- Nexora Server 连接、重连和发送通道；
- 活动传输取消表。

这些资源不能独立隐藏或伪造释放，因此由同一个 cleanup 原子停止。

### 启用

1. 初始化或迁移 `lan_collaboration.db`、头像和传输目录；
2. 从旧 `p2p-settings.json` 迁移用户 ID 和名称；
3. 恢复中断传输并清理过期记录；
4. 绑定 UDP `31523` 和 TCP `31524`；
5. 按设置恢复可选 Server 连接；
6. 登记网络聚合资源并执行健康检查。

端口被占用时保留联系人和历史读取能力，模块进入 `degraded` 并展示端口错误，不删除数据。

### 停用

1. 立即设置模块禁用守卫，拒绝新消息、传输和连接命令；
2. 向全部活动局域网和 Server 传输发送取消信号；
3. 停止 Server 重连和长连接；
4. 停止 UDP/TCP listener 及其子连接任务；
5. 等待任务退出，超时任务执行 abort；
6. 将正在传输的记录更新为取消或失败，可由用户重新发起；
7. 清空在线内存状态，保留数据库、头像、消息和接收文件。

## 4. 并发与重复初始化

- 服务初始化、启动监听和停止监听分别使用异步互斥锁串行化；
- 前端 Store 与 Module Manager 同时初始化时只会形成一组监听任务；
- 任务句柄异常结束后，健康检查会识别为监听缺失，手动重新发现或重启模块可重新绑定；
- 停用先关闭命令守卫，避免停止过程中产生新任务；
- 旧通知在模块停用后不会再打开局域网会话。

## 5. 命令守卫

局域网、旧 P2P 兼容命令、文件传输命令和 Server 命令统一检查模块开关。停用时返回稳定前缀：

```text
LAN_COLLABORATION_MODULE_DISABLED
```

R5 完成前，功能中心和固定栏入口仍保留；点击后页面可以显示停用错误，但后端不会重新启动服务或绕过生命周期。

## 6. 自动验证

新增覆盖：

- 停用守卫拒绝新旧局域网命令；
- UDP/TCP 任务停止后原地址可立即重新绑定；
- Server 连接任务和传输通道收到取消信号；
- 多个活动局域网传输统一收到模块停止信号；
- 中断传输只更新运行状态，不删除联系人、消息和文件；
- 原有发现、消息、传输、历史同步和服务器认证测试继续通过。

完整命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm run build
npm run check:platform-contracts
git diff --check
```

自动验证结果：Rust 单元测试 `109 passed`，其中局域网专项测试 `27 passed`；`cargo check`、`cargo fmt --check`、前端构建、平台合同检查和 `git diff --check` 全部通过。

## 7. 人工验收

### 单机资源

- [ ] 启动后设置页显示 `builtin.lan-collaboration` 为运行中；
- [ ] `Get-NetTCPConnection -LocalPort 31523,31524` 可看到监听；
- [ ] 停用模块后两个端口不再被 Nexora 占用；
- [ ] 停用后发送消息、文件或启动发现会收到模块停用提示；
- [ ] 再次启用后两个端口重新绑定，资源计数不增长；
- [ ] 联系人、聊天、头像、接收目录和历史传输仍保留。

### 双机行为

- [ ] 两台设备互相发现并发送大厅和私聊消息；
- [ ] 停用 A 后，B 在超时后显示 A 离线；
- [ ] 重新启用 A 后双方重新发现，无需重建资料；
- [ ] 文件传输过程中停用发送端和接收端各测试一次，双方不残留“传输中”；
- [ ] 再次发起同一文件可以正常完成；
- [ ] 如启用了 Nexora Server，停用时连接断开，重新启用后按保存设置恢复。

## 8. 数据安全与回滚

- 停用不删除 `lan_collaboration.db`、头像缓存、已接收文件或聊天记录；
- 仅把无法继续的活动传输改为可解释的取消/失败状态；
- 可在设置的后台模块中重新启用，无数据库迁移回滚要求；
- 原 `p2p-settings.json` 只读取迁移，不删除；
- 当前协议仍为明文局域网协议，只应在可信网络使用。
