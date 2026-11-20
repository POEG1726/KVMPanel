# KVMPanel 接口契约

`kvmd`（Rust 后端）、网页前端（`software/app/web`）和副屏（`software/app/dashboard`）之间的约定。设计依据：[应用设计](../../../docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md)。

**改契约的顺序**：先改这里（`openapi.yaml`、本文件、`schema/`），再改 mock 和它的测试，最后再改前端或后端的实现。

## 传输方式

| 通道 | 用途 | 定义 |
|---|---|---|
| HTTP `/api/...` | 账号、配置、文件、介质、电源、系统 | `openapi.yaml` |
| WebSocket `/api/ws` | 状态推送和键鼠输入，消息为 JSON | 本文「事件和输入」，`schema/ws.schema.json` |
| WebSocket `/api/stream` | MJPEG 画面，每条二进制消息是一整张 JPEG | 本文「视频流」 |
| WebSocket `/api/terminal` | 终端字节流 | 本文「终端」 |
| panel socket | `kvmd` ↔ 副屏进程，NDJSON | 本文「panel socket」，`schema/panel.schema.json` |

## 通用约定

- **错误**：统一返回 `{ "error": "<snake_case_code>", "detail"?: {...} }`（`openapi.yaml` 里的 `Error` 模型）。接口只给错误码，中英文提示由前端或副屏自己翻译。
- **鉴权**：
  - 网页登录后拿到 HttpOnly cookie `kvm_session`。
  - MCP 客户端改用 `Authorization: Bearer <key>`。
  - WebSocket 握手同样校验 cookie。未登录以 4401 关闭；登录了但缺少权限，以 4403 关闭。
- **权限键**：`console`、`power`、`media`、`terminal`、`files`、`device`、`network`、`updates`、`mcp`、`tailscale`、`panel`、`usb`。`admin` 角色拥有全部权限。
- **配置**：只有 `kvmd` 能写配置。网页和副屏都通过契约修改，不直接写文件。

## 端口

| 场景 | HTTP | panel socket |
|---|---|---|
| 板子 | 80 / 443（可配置） | Unix socket `/run/kvmd/panel.sock` |
| PC 上的 mock | `127.0.0.1:8800`（环境变量 `KVMPANEL_MOCK_PORT`） | TCP `127.0.0.1:7701`，或环境变量 `KVMPANEL_PANEL_SOCK` 指定的 Unix socket |

## Mock 服务

```powershell
npm install --prefix software/app/api/mock
```

```powershell
npm start --prefix software/app/api/mock
```

```powershell
npm test --prefix software/app/api/mock
```

Mock 的测试会对照 `openapi.yaml` 校验每一个响应。初始账号为 `admin` / `admin`。
