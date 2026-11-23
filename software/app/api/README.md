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
- **KVM 总开关**：`config.kvm.enabled` 设为 false 时，kvmd 先返回 PATCH 响应，再以 4409 断开所有远程连接，并停止网页监听，只剩副屏。只能从副屏重新打开。网页在保存前必须先确认。
- **设备故障**：硬件缺失、gadget 重建失败、系统命令失败时，HTTP 返回 `503 {error:"device_unavailable"}`（无 HDMI 信号时仍然是 `no_signal`）。其他未知错误返回 `500 {error:"internal"}`。`status` 里的布尔值读不到时按 false 处理。
- **USB 与共享盘**：
  - 允许空的 USB 组合，此时 gadget 会被解绑。
  - 已挂载介质或共享盘已导出给电脑时，关闭 `storage` 会返回 409 `in_use`。
  - 共享盘导出期间，**电脑一侧可读写**，板子一侧只读（同一时间只有一个写入方）。收回导出前，电脑应该先安全弹出。
  - 有正在写入的上传或下载时，切换共享盘会返回 409 `share_busy`。
- **升级包**：按频道提供 JSON 清单，包本身是不超过 256 MiB 的 tar，安装完成后切换 `apps/current`。安装进行中再提交返回 409 `busy`，超过大小返回 413 `too_large`。
- **WebSocket 关闭码**（`/api/ws`、`/api/stream`、`/api/terminal` 通用）：

  | 关闭码 | 含义 | 客户端应该怎么做 |
  |---|---|---|
  | 4401 | 未登录，或会话已失效（登出、改密码、账号被停用或删除） | 回到登录页 |
  | 4403 | 缺少权限，或权限被收回 | 提示没有权限，不要重连 |
  | 4404 | 没有配置串口（仅终端） | 隐藏串口标签页 |
  | 4409 | 管理员断开了所有远程会话，或者 KVM 服务被关闭（`config.kvm.enabled=false`） | 提示已被断开，不要自动重连 |
  | 4400 | 终端 `kind` 参数不合法 | 当作程序错误处理 |
  | 1011 | 终端的 PTY 或串口打开失败 | 提示设备故障，可以手动重试 |
  | 1012 | kvmd 正在重启 | 显示“KVM 服务重启中”，然后退避重连 |

  会话或权限一有变化，服务端就主动关闭这个用户已经打开的连接，不会等到下一条消息才处理。

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

调副屏离线状态页时，用环境变量 `KVMPANEL_MOCK_PC` 指定电脑的初始状态：`on`（默认）、`off`（电源灯灭）或 `waiting`（开着机，但上位机没有数据）。电脑关着时，副屏的开机动作或网页电源键会把它打开，8 秒后上位机开始发数据。网页长按关机后，上位机连接随之断开。

```powershell
$env:KVMPANEL_MOCK_PC = 'off'; npm start --prefix software/app/api/mock
```

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
| `paste` | `text`（UTF-8 编码后不超过 64 KiB） | 按 `config.hid.keyboard_layout` 打出来；有布局打不出的字符时，整条拒绝并回 `unmappable` |
| `shortcut` | `keys`（1–8 个 code） | 按顺序按下，再倒序松开，比如 `["ControlLeft","AltLeft","Delete"]` |
| `ping` | — | 服务端回 `pong` |

`buttons` 是位掩码：1 左键、2 右键、4 中键、8 后退、16 前进。滚轮的方向和速度由客户端按 `config.hid` 换算好再发，服务端原样转发。

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
- 刚连上时，以及每次收到 `resume` 后，服务端都会**立即发送当前一帧**，不受帧差检测影响，保证切回标签页时不会停在旧画面或黑屏。
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

## panel socket：`kvmd` ↔ 副屏

- 板子上是 Unix socket `/run/kvmd/panel.sock`，PC 上的 mock 用 TCP `127.0.0.1:7701`。
- 每行一个 JSON 对象（NDJSON），用 `t` 区分类型。完整定义见 `schema/panel.schema.json` 的 `$defs/ToPanel` 和 `$defs/FromPanel`。
- `kvmd` 就是这个 socket 的服务端。重启时它先发 `kvm_restarting`，随后连接断开。副屏要保持当前画面，退避重连；重连后收到新的 `hello`，表示 `kvmd` 已经恢复。
- 解析得出 `id` 的请求，即使格式不对，回复的 `result` 也会带上同一个 `id`。
- 副屏是本机进程，被视为可信，改配置时不检查权限。但校验规则和 `PATCH /api/config` 相同，包括 `invalid_config` 和 `endpoints_full`。

**kvmd → 副屏**

| `t` | 说明 |
|---|---|
| `hello` | 连上时第一条，带 `version` |
| `config` | 完整配置。连上时推一次，此后每次变更都推，不管改动来自网页还是副屏 |
| `status` | 右上角状态栏用：`eth`、`pc`、`usb_a`、`kvm.sessions`，另有 `lux`。连上时推一次，变化时推，另外每 5 秒推一次 |
| `telemetry` | 每秒一次，包含 `ts` 和 `metrics[]`。每项为 `{id,label,unit,value,min,max,warn?,group}`。仅报告已知、未隐藏的指标。`value: null` 表示采集器暂时无法提供读数，副屏保留该行并显示 `—`。`warn` 已合并 `config.panel.thresholds` |
| `pc` | 上位机连接状态。连上 socket 时推一次，此后每次变化都推。断开期间不发 `telemetry`，副屏切到离线状态页 |
| `board` | 每秒一次，不管电脑是否连着：电脑电源灯 `pc_power`（`on`/`off`/`unknown`）、主机名、版本、运行时间、板子 CPU、SoC 温度、内存和 SD 用量、`eth`（链路速率、双工、MAC、板子自己的收发 bit/s）、`access`（访问地址、Tailscale IP）、`kvm`（远程会话、HDMI 输入、视频流、虚拟介质、正在运行的 USB 功能）。离线状态页用它，字段见 `/Board` |
| `kvm_restarting` | 网页端的 KVM 服务正在重启，副屏自身不受影响 |
| `result` | 对请求的回复，`id` 和请求相同。如果请求那一行本身无法解析，回复里没有 `id`，错误为 `bad_message` |

**副屏 → kvmd**（都带整数 `id`）

| `t` | 回复 |
|---|---|
| `get_config` | `result.data` 是完整配置 |
| `patch_config`（`patch`） | 成功时 `result.data` 是新配置，失败时带 `error` 和 `detail` |
| `action`（`reboot`、`shutdown`、`restart_kvm`、`reset_hid`、`disconnect_sessions`、`eject_media`、`usb_apply`、`pc_power_on`） | `result.ok`。`pc_power_on` 等于短按一次电脑电源键；电源灯亮着时拒绝，错误为 `pc_on`，保证副屏不会把电脑关掉 |
| `metric_catalogue` | `result.data` 含当前已知指标的 `id`、`label`、`unit`、`group`，包括已隐藏的字段。副屏可通过 `patch_config` 更新 `panel.hidden_metrics` 和 `panel.orientation` |
| `action`（`wake_display`） | 重置副屏离线息屏倒计时。`board.display_asleep` 为 `true` 时，副屏消费触摸作为唤醒动作 |
| `device_info` | `result.data` 和 `GET /api/device` 相同 |

**告警阈值**：`config.panel.thresholds` 的键可以是指标 id，比如 `gpu.temp`；也可以是以 `.*` 结尾的前缀，比如 `mem.dimm.*`。同一个指标同时匹配时，精确的 id 优先于前缀。在 PATCH 里把某个键设为 null 就删除这个阈值。默认值为 `cpu.tctl` 85、`gpu.mem_junction` 95、`mem.dimm.*` 60。

**指标 id**：布局模板按 id 取数据，见 `mock/src/panel/metrics.ts`。

| 分组 | id |
|---|---|
| CPU | `cpu.clock_eff`、`cpu.bclk`、`cpu.fclk`、`cpu.vcore`、`cpu.tctl`、`cpu.load`、`cpu.ppt`、`cpu.core.0`–`cpu.core.15`（0–7 属于 CCD0） |
| GPU | `gpu.clock`、`gpu.temp`、`gpu.mem_junction`、`gpu.load`、`gpu.vram_used`、`gpu.power` |
| 内存 | `mem.used`、`mem.dimm.<槽位>`（只发有内存的槽位，比如 `a2`、`b2`）、`mem.mclk`、`mem.uclk` |
| 整机 | `sys.power`、`ssd.0.temp`、`ssd.1.temp`、`net.rx`、`net.tx` |

## MCP

MCP 客户端向 `/api/mcp/v1` 发 Streamable HTTP JSON-RPC POST，带 `Authorization: Bearer <key>`。支持版本 `2025-06-18` 和 `2025-11-25`。服务无状态，请求最多 1 MiB，不提供 SSE。 GET 返回 405。有自签证书时，先让客户端信任本机证书。需给客户端配置最多 120 秒的工具等待时间，例如 Claude Code 的 `MCP_TOOL_TIMEOUT`。

工具列表、输入输出结构和标注见 [MCP Schema](schema/mcp.schema.json)。工具结果的 `structuredContent` 是对象，图像置于 `content`。业务失败用 `isError:true` 和 `structuredContent.error`，无效参数用 JSON-RPC `-32602`。请求危险操作先拿绑定同一 key 和同组参数的单次令牌，30 秒有效。

网页通过 `GET/PUT /api/mcp` 管理总开关，通过 `/api/mcp/keys` 新建 key，使用 `/api/mcp/keys/{id}` 修改和删除。创建返回 `{key,entry}`，之后从列表只能读到末四位。查询操作日志用 `GET /api/mcp/audit`。网页登录账号有可写控制台权限即可 `POST /api/mcp/stop`，重新开启要有 MCP 管理权限。原有 Bearer 截图和 HID 接口保留，分别检查 `observe` 和 `input`。

`PATCH /api/mcp` 用 `mcp` 权限单独更新人工输入优先策略，可提交一个或两个字段。`human_input_priority` 默认 `true`，`human_input_wait_s` 是 1–60 秒的整数，默认 `2`。`GET /api/mcp` 与 `host_status.control` 返回这两个当前值。优先打开时，有效人工输入可接管正在执行的 MCP 输入。有可写控制台会话时，距离最近人工输入不足等待时间的 MCP 输入返回 `human_active`。关闭优先后，双方可交错输入，急停仍可停止 MCP。

网页状态中的 `status.mcp` 含 `{active,key_name,tool}`。panel socket 发送同一结构并加 `t:mcp`，副屏发 `{t:mcp_stop,id?}` 可急停。主机启动按键用 `config.host` 保存，由 `device` 权限修改。Windows 主机程序使用 [主机端 HID 通道](host-agent.md)。
