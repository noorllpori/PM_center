# Nexora Server

独立的 Nexora 设备协作服务端。默认监听 `0.0.0.0:7412`，SQLite 数据写入 `PMC_DATA_DIR`，文件只在内存中流式转发。

```bash
PMC_SERVER_NAME="My Nexora" PMC_SERVER_PASSWORD="change-me" cargo run --release
```

环境变量：`PMC_SERVER_BIND`、`PMC_SERVER_NAME`、`PMC_SERVER_PASSWORD`、`PMC_DATA_DIR`、`PMC_MAX_TRANSFER_BYTES`。

公网部署应使用 `deploy/Caddyfile.example` 提供 HTTPS/WSS。HTTP 仅适合可信网络测试。
