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

## 事件和输入：`/api/ws`

- 要求已登录且有 `console` 权限。未登录以 4401 关闭，没有 `console` 权限以 4403 关闭。
- 每条消息都是 JSON 对象，用 `t` 区分类型。完整定义见 `schema/ws.schema.json` 里的 `$defs/ClientMessage` 和 `$defs/ServerMessage`。

**客户端 → 服务端**

| `t` | 字段 | 说明 |
|---|---|---|
| `key` | `code`（`KeyboardEvent.code`）、`down` | 按下或松开一个键 |
| `mouse_abs` | `x`、`y`（0–65535）、`buttons` | 绝对坐标 |
| `mouse_rel` | `dx`、`dy`（−127–127）、`buttons` | 相对移动 |
| `wheel` | `dy`（−127–127） | 滚轮 |
| `paste` | `text`（不超过 64 KiB） | 把文本当键盘输入打出来 |
| `shortcut` | `keys`（1–8 个 code） | 按顺序按下，再倒序松开，比如 `["ControlLeft","AltLeft","Delete"]` |
| `ping` | — | 服务端回 `pong` |

`buttons` 是位掩码：1 左键、2 右键、4 中键、8 后退、16 前进。

**服务端 → 客户端**

| `t` | 说明 |
|---|---|
| `status` | 连上时推一次，状态变化时推，另外每秒至少推一次。字段：`hdmi`、`stream`、`power`、`locks`（被控电脑的 Num/Caps/Scroll 灯）、`sessions`（当前打开 `/api/ws` 的连接）、`media` |
| `usb_reenumerated` | USB gadget 已经重建 |
| `kvm_restarting` | kvmd 即将重启，随后服务端以 1012 关闭连接 |
| `error` | `bad_message`：格式不对，这条消息丢弃，连接不断；`forbidden`：只读会话发了输入 |
| `pong` | 回应 `ping` |

## 视频流：`/api/stream`

- 要求已登录且有 `console` 权限，关闭码和 `/api/ws` 相同。
- 服务端发给客户端的**每条二进制消息就是一整张 JPEG**，以 `FF D8` 开头、`FF D9` 结尾。分辨率跟随 HDMI 输入，帧率不超过 `config.video.fps_max`。
- 客户端可以发文本消息 `{"t":"pause"}` 和 `{"t":"resume"}`，比如标签页切到后台时暂停。
- `config.video.frame_diff` 为 true 时，画面没变就不发。客户端不能把“一段时间没收到帧”当成断线，判断连接状态请看 `/api/ws`。
- 上一帧还没发完时，新帧直接丢弃（背压），所以慢速网络下帧率会自动降下来。
- 截图：`GET /api/stream/snapshot.jpg`。EDID：`/api/video/edid`。详见 `openapi.yaml`。

## 终端：`/api/terminal`

- 地址是 `/api/terminal?kind=local` 或 `?kind=serial`，要求有 `terminal` 权限。
- 关闭码：
  - 4401：未登录。
  - 4403：没有 `terminal` 权限。
  - 4404：`kind=serial` 但 `config.serial.device` 为 null，这时前端不显示串口标签页。
- 二进制消息双向传原始终端字节，直接交给 xterm.js。
- 文本消息只有一种：`{"t":"resize","cols":120,"rows":40}`。
- 服务端可能在握手完成后立刻发出第一段输出，客户端要在连接建立前就挂好消息监听。
- 自定义脚本用 `/api/scripts` 管理，详见 `openapi.yaml`。
