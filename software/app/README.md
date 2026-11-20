# 应用

- `api/`：接口契约（OpenAPI、WebSocket 和 panel socket 消息）和 PC 上用的 mock 服务。
- `ipkvm/`：KVM 后端，Rust crate `kvmd`。负责远程视频、键鼠、硬件、配置和 USB gadget，同时提供网页文件。
- `web/`：KVM 网页前端（React + TypeScript）。
- `dashboard/`：副屏界面，是独立进程，通过 panel socket 和 `kvmd` 通信。

设计见 [应用设计](../../docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md)。
