# KVMPanel MCP 设计

2026-11-23。实现交给 Codex，本文是唯一的需求来源。标"实现者决定"的地方可以自行取舍，其余按本文执行。

## 1. 目的

让另一台电脑或手机上的 AI 客户端（Claude Code、OpenCode、VS Code、Cursor 等支持远程 HTTP 加自定义请求头的客户端）通过 MCP 看到并操作被控 PC，场景包括超频调参、进 BIOS、装系统和排查开机失败。被控 PC 是开发机本身，所以 **MCP 客户端一定不在被控 PC 上**，重启、强关、进 BIOS 都是正常用法。

参照对象是社区固件 [NanoKVM-OS](https://github.com/dormancygrace/NanoKVM-OS) v4.x 的 MCP（`server/service/mcp/`）。它有 12 个工具：

- 键鼠：`type_text`、`press_keys`、`move_mouse`、`click_mouse`、`drag_mouse`、`scroll_mouse`
- 观察：`screenshot`、`host_status`、`screen_text`（板上 TPU OCR）
- 主机控制（管理员开启后才有）：`power`、`virtual_media`、`wake_on_lan`

本设计要覆盖它除 `screen_text` 以外的全部能力（OCR 见非目标），并加上本板独有的能力：PC 遥测、主机端程序通道、开机守护、进 BIOS、机箱上的操作提示和物理急停。

## 2. 目标、非目标、反目标

**目标**

- 标准 MCP（Streamable HTTP）。主流客户端只需填 URL 和 Bearer key，不装任何桥接程序。
- 具备 NanoKVM-OS 除 OCR 外的全部工具能力，坐标、参数和返回字段同样直观。
- 新增：遥测读取与统计、截图裁剪和缩放、等待画面变化或稳定、最近画面回看、开机守护、进 BIOS 和启动菜单、向主机投放文件、分权限的 key、操作日志、机箱提示与急停、主机端程序通道（窗口、光标、显示器、锁屏状态、剪贴板、弹出共享盘）。
- `kvmd --sim` 下完整可用，真实客户端可以直接对着模拟器联调。

**非目标**

- **这一版完全不做 OCR**：板子和主机端程序里都不实现，也不留占位。工具表里没有 `screen_text`，`schema/mcp.schema.json` 里没有，主机程序也没有 OCR 的开关、方法或能力字段。
  - 后续计划在 T113 板上跑 PP-OCR。PP-OCRv 用哪一版、tiny/small/medium 用哪个大小都没定，由用户先在 T113 上实测速度和内存，再另起设计。
- 不做 resources 订阅和进度通知。它们需要有状态会话和 SSE，第一版只做无状态 JSON。
- 不做 stdio 桥接程序，也不做 MCP 之外的新远程协议。
- 不自动识别主板型号来决定 BIOS 按键，由用户在设置里指定。
- 主机端通道不执行任意命令，不读写主机文件。

**反目标**（技术上能用，但出现即视为失败）

- AI 的输入和人在网页上的操作互相打架，或者人无法立刻收回控制。
- 一个只想让 AI 看屏幕的 key 也能强关电脑。
- 画面回看或等待工具让采集和编码长期常驻，拖慢网页 KVM 或副屏。
- 剪贴板这类涉及隐私的读取在用户不知情时可用。

## 3. 约束

- 后端是 `software/app/ipkvm`（Rust、axum 0.8、tokio）。不另起进程，MCP 是 `kvmd` 里的一组路由。
- 板子是 T113-i，双核 Cortex-A7，512 MiB DDR3。采集最高 1080p30。
- 改契约按 [接口契约](../../../software/app/api/README.md) 的顺序：先改 `openapi.yaml`、`README.md` 和 `schema/`，再改 mock 和它的测试，最后改网页和后端。
- 现有输入路径 `crate::console::input()` 已支持 `key`、`mouse_abs`（0–65535）、`mouse_rel`、`wheel`、`paste`（按 US、UK、DE 布局校验，最多 64 KiB）和 `shortcut`。截图走 `crate::console::snapshot()`，电源灯是 PE12，电源键 PE13，复位 PE3。
- 主机端通道复用遥测用的厂商 HID 集合。现在只有 Output report 3，从主机发往板子，走 EP0 SET_REPORT，Linux f_hid 只保留最新一份。描述符在 `software/os/buildroot/board/kvmpanel/rootfs-overlay/usr/sbin/kvmpanel-gadget`。
- 主机程序 `software/app/pc-telemetry` 是原生 C/Win32，带设置窗口和托盘。

## 4. 方案取舍

**MCP 服务怎么实现**

| 方案 | 优点 | 缺点 |
|---|---|---|
| A. 在 kvmd 里手写最小 JSON-RPC（推荐） | 无新依赖，ARM 静态构建不受影响。无状态 JSON 只需实现 `initialize`、`ping`、`tools/*`、`resources/list`、`resources/read`、`prompts/*` 和通知。现有 `jsonschema` 可以直接校验工具参数 | 协议版本升级要自己跟 |
| B. 用官方 Rust SDK `rmcp` 的 Streamable HTTP 服务 | 协议细节由 SDK 负责 | 依赖较重，和 axum 0.8 的版本搭配、无状态 JSON 响应模式需要先核实，ARM 体积增加 |
| C. 板外桥接（stdio 程序转 REST） | 板子不用改 | 每台客户端都要装程序，和目标冲突 |

选 **A**。如果实现者核实 B 能以无状态 JSON 模式挂进现有 axum 路由，并且 ARM 静态体积增加不超过约 1 MiB，可以改用 B，在开发说明里写明原因。

**主机端通道走哪条路**

| 方案 | 优点 | 缺点 |
|---|---|---|
| A. 扩展现有厂商 HID 集合（推荐） | 不需要驱动，不增加 USB 端点，遥测开着就有 | 每包 63 字节，EP0 只保留最新一份，需要应答流控，吞吐约几十 KB/s |
| B. 另开 CDC ACM 串口 | 快 | 占 2 个 IN 和 1 个 OUT 端点，USB 预算本来就紧张 |
| C. 主机程序走局域网连板子 | 最快 | 要做发现和鉴权，安全面变大 |

选 **A**。传的都是窗口标题、坐标和剪贴板文本这类小数据，够用。

## 5. 传输与鉴权

- **入口**：`/api/mcp/v1`（精确路径）。`POST` 处理 JSON-RPC，返回 `application/json`。`GET` 返回 405，表示不提供 SSE。无状态，不发 `Mcp-Session-Id`。
  - `/api/mcp` 已被管理接口占用，所以入口不能放在那里。
  - 现有的 `/api/mcp/v1/screenshot` 和 `/api/mcp/v1/hid` 两个 REST 接口保留，方便用 curl 写脚本，按第 6 节的权限范围校验。
- **协议版本**：支持 `2025-06-18` 和 `2025-11-25`。按客户端请求的版本协商；不认识的版本回最新一版。也接受请求头 `MCP-Protocol-Version`。
- **鉴权**：`Authorization: Bearer <key>`，key 形如 `kvp_` 加 32 字节随机数的 base64url。服务端只存 SHA-256，用常量时间比较。
- **出错时**：
  - MCP 总开关关着：403 `mcp_disabled`
  - key 不对：401 `unauthenticated`
  - `Host` 或 `Origin` 不在允许列表里：403 `bad_origin`。规则见下面的"防 DNS rebinding"。
  - 请求体上限 1 MiB。axum `Json` 默认是 2 MiB，这里要显式设置。
- **防 DNS rebinding**：
  - 允许列表由配置和当前状态生成：配置的主机名、`<主机名>.local`、板子各网卡的 IP、Tailscale 名称和 IP、`localhost`、`127.0.0.1`。
  - `Host` 头（去掉端口）必须在列表里。
  - 没有 `Origin` 头的请求（命令行和桌面客户端）只检查 `Host`。带了 `Origin` 时，它的主机也必须在列表里。
  - 不提供 CORS。浏览器里直连的客户端不支持；MCP Inspector 请用它的代理模式。
- **监听**：和网页同一个 HTTP/HTTPS 监听。KVM 总开关 `config.kvm.enabled=false` 时 MCP 也随之不可用。
- **服务信息**：`serverInfo.name = "kvmpanel"`，`version` 用 kvmd 版本。`instructions` 写一段简短说明：坐标约定、建议先调 `host_status`、危险操作需要确认令牌。

## 6. Key 与权限范围

一台设备可以有多把 key，最多 8 把。每把 key 有：名称、权限范围列表、创建时间、最近使用时间和末 4 位。

| 范围 | 可用工具 |
|---|---|
| `observe` | 截图类、`host_status`、遥测类、`host_info`、REST 截图 |
| `input` | 键鼠类、REST HID |
| `power` | `power`、`boot_watch`、`enter_firmware`、`wake_on_lan` |
| `media` | `virtual_media`、`host_drop_file`、`share` |
| `host_private` | `host_clipboard`。主机程序那边也必须打开对应开关 |

- `tools/list` 只列出这把 key 的权限范围内的工具。越权调用返回工具错误 `forbidden_scope`。
- **管理接口**（需要 `mcp` 权限，契约要先改）：
  - `GET /api/mcp`：返回 `{enabled, keys:[{id,name,scopes,created,last_used,hint}]}`
  - `PUT /api/mcp`：设置 `{enabled}`
  - `POST /api/mcp/keys`：参数 `{name, scopes}`，只在这一次返回完整 key
  - `PATCH /api/mcp/keys/{id}`：修改名称和权限范围
  - `DELETE /api/mcp/keys/{id}`
- **迁移**：旧接口 `POST /api/mcp/regenerate` 和 `DELETE /api/mcp/key` 删除。已存在的单把 key 迁移成名为 `default`、权限范围为 `observe`+`input` 的 key，等于它现在的能力。这是我的决定。
- **危险操作两步确认**：强关、复位、断电重启；在 USB 连着主机时卸载可写磁盘镜像；从主机收回共享盘。板子看不出主机是否正在读写介质，所以只要 USB 连着、介质可写，就一律要确认。光盘和只读镜像不用确认。
  - 第一次调用不执行，返回 `{needs_confirm:true, confirm_token, expires_in:30, summary}`。
  - 第二次带同一参数和 `confirm_token` 才执行。令牌只能用一次，绑定这把 key 和这组参数。
  - 工具还要标注 `destructiveHint: true`，让客户端弹出确认。

## 7. 坐标与通用约定

- 所有坐标都归一化到 0–1，原点在左上角，对应整个 HDMI 画面。
- 截图裁剪后坐标仍按整屏计算。返回里带 `region`，告诉模型这张图在整屏中的位置。
- 键名用 `KeyboardEvent.code`，与 `/api/ws` 相同。
- 工具的业务失败用 `isError: true` 返回，`structuredContent.error` 填 snake_case 错误码，比如 `no_signal`、`abs_mouse_disabled`、`human_active`、`host_agent_offline`、`not_configured`。参数校验失败返回 JSON-RPC 错误 `-32602`。
- 每个工具都要给 `inputSchema`、`outputSchema`，以及 `readOnlyHint`、`destructiveHint` 等标注。
- 会长时间阻塞的工具，单次最多等 120 秒。文档里提醒用户调大客户端的工具超时，比如 Claude Code 的 `MCP_TOOL_TIMEOUT`。

## 8. 工具

### 8.1 观察（`observe`）

- **`screenshot`**：截一张 JPEG，用 MCP image content 返回，同时给结构化字段。
  - 参数：
    - `region` `{x,y,w,h}`，可选，归一化坐标，先裁剪再编码
    - `max_width`，可选，等比缩小到不超过这个宽度，默认 1280，最小 320
    - `quality`，40–90，默认 70
  - 返回：`{width, height, source_width, source_height, region, size}`
  - **不碰视频流的编码器**。`Hardware::frame()` 和帧通道只提供 JPEG，硬件编码器改画质就会重开，所以 MCP 一律从视频流已有的 JPEG 出发：
    - 不裁剪、不缩放、也没给 `quality` 时，直接返回流里的 JPEG。
    - 否则用软件 libjpeg-turbo 解码（可以用 DCT 缩放，1/2、1/4、1/8 很便宜），裁剪和缩放后再用软件编码。`quality` 只作用在这次重编码上。
  - 视频流当时没开时，按现有 `snapshot()` 的方式临时开一次。
- **`screen_wait`**：等画面满足条件，再返回一张截图。
  - 参数：
    - `until`：`changed`（与调用时那一帧不同）或 `stable`（连续 `stable_ms` 不变，默认 1500）
    - `region`，可选，只看这块区域
    - `timeout_ms`，最多 120000
    - `threshold`，变化像素比例，默认 0.5%
  - 返回：截图，加 `{met, waited_ms}`
  - 帧差：把流里的 JPEG 用 DCT 1/8 缩放解码，比较亮度，不额外编码。
- **`screen_history`**：取最近的画面，用来回看一闪而过的蓝屏或报错。
  - 参数：`seconds`，最多 30；`max_frames`，最多 8，按时间均匀抽取
  - 返回：多张缩小到 640 宽的图，每张带时间戳
  - 环形缓冲订阅帧通道，每秒存一帧流里的 JPEG，最多 30 帧，大约 6 MB。**只在某把 key 最近 10 分钟调用过 MCP 时运行**，否则停止，不让采集常驻。没有缓冲时返回 `history_inactive`，并顺带开始记录。
- **`host_status`**：不截图就能看的状态，不改变任何东西。返回：
  - `power`：电源灯 `on`、`off` 或 `unknown`，电源键和复位键是否可用
  - `video`：HDMI 是否有信号、分辨率、帧率
  - `keyboard`：Caps、Num、Scroll 三个锁定灯
  - `usb`：哪些 USB 功能在用；绝对鼠标没开时，提示只能用相对移动
  - `telemetry`：上位机是否在线
  - `host_agent`：主机端程序是否在线，打开了哪些能力
  - `media`：当前挂载了什么
  - `control`：网页上有几个可输入会话，最近一次人工输入距今几秒，MCP 是否被急停
- **`telemetry_read`**：读一次当前遥测。
  - 参数：`ids` 或 `groups`，可选，支持 `mem.dimm.*` 这种前缀通配
  - 返回：`[{id,label,unit,value,group,warn}]`，`value` 可以为 null
  - 数据来自 `panel::metrics` 的现有状态。上位机离线时返回 `telemetry_offline`。
- **`telemetry_watch`**：观察一段时间，返回统计。
  - 参数：`seconds`，1–120；`ids` 或 `groups`
  - 返回：每项的 `min`、`max`、`avg`、`last`、`samples`、`nulls`，以及超过 `warn` 阈值的次数
  - 用途：跑稳定性测试时看温度、频率和供电。
- **`telemetry_catalogue`**：所有已知指标，包括隐藏项，含单位和量程。
- **`host_info`**：来自主机端程序（见第 9 节），返回：
  - 前台窗口的标题和进程名
  - 光标位置，按采集画面归一化
  - 显示器布局、分辨率和缩放
  - 会话状态：`unlocked`、`locked`、`secure_desktop` 或 `screensaver`
  - 主机程序离线时返回 `host_agent_offline`。

### 8.2 输入（`input`）

- `type_text`：参数 `text`，UTF-8 编码后最多 16 KiB。走现有 `paste` 路径，有布局打不出的字符时整条拒绝，返回 `unmappable` 并列出字符。
- `press_keys`：参数 `keys`，1–8 个；`hold_ms`，0–2000，默认 50。按顺序按下，再倒序松开。`hold_ms` 可以长按单个键。
- `move_mouse`：参数 `mode`，`absolute`（默认）或 `relative`。绝对模式给 `x`、`y`，相对模式给 `dx`、`dy`，后者自动拆成每份 ±127 的报告。
- `click_mouse`：参数 `button` 为 `left`、`right` 或 `middle`；`clicks`，1–5；可选 `x`、`y`，先移动再点。
- `drag_mouse`：参数 `from`、`to`、`button`、`steps`（1–200）、`duration_ms`（0–5000）。
- `scroll_mouse`：参数 `dy`，正数向下；可选 `x`、`y`，先移动再滚。
- 绝对模式下 USB 没开绝对鼠标时，返回 `abs_mouse_disabled`。
- **和人工输入的仲裁**（我的决定）：
  - 网页上有可输入会话，并且最近 5 秒内有人工输入时，MCP 输入返回 `human_active`，不排队。
  - MCP 的输入串行执行。一个输入工具结束时，释放它按下的所有键和按钮。
  - 现有防休眠抖动在 MCP 输入期间跳过，与拖动时一样。

### 8.3 电源与启动（`power`）

- **`power`**：参数 `action`：
  - `on`：电源灯灭时短按
  - `shutdown`：短按，交给系统正常关机
  - `force_off`：长按 5 秒
  - `reset`：按复位键
  - `cycle`：强关，等 5 秒再开
  - `press`：短按，不看电源灯
  - `force_off`、`reset`、`cycle` 需要确认令牌。
  - 返回：执行后的电源灯状态。
  - 复用 `/api/power/press` 的实现，`on` 的判断与 panel 的 `pc_power_on` 相同。
- **`boot_watch`**：开机守护。
  - 参数：
    - `start`：`none`（只观察）、`on`、`reset` 或 `cycle`。`reset` 和 `cycle` 需要确认令牌。
    - `timeout_s`，最多 120
    - `until`：`signal`（HDMI 出画面）、`stable`（画面稳定）或 `os`（遥测或主机端程序上线），默认 `os`
    - `retries`，0–3，默认 0。失败时强关，再从 `on` 重来；大于 0 时要带确认令牌。
  - 返回：每一轮的时间线：电源灯亮、HDMI 出信号、画面稳定、遥测上线、主机端程序上线，各自距开始的毫秒数；最终结果 `ok`、`no_power`、`no_signal`、`stuck` 或 `os_timeout`；最后一张截图。
- **`enter_firmware`**：进 BIOS 或启动菜单。
  - 参数：
    - `target`：`setup` 或 `boot_menu`
    - `start`：`on`、`reset` 或 `cycle`，后两种需要确认令牌
  - 流程：执行开机动作后，按设置里的按键和间隔连按，直到连按时长用完，或者 HDMI 出信号后再多按 3 秒。结束后返回截图，由模型判断是否进去了。
  - 按键在设置里指定，**不按主板型号推断**：

    ```
    config.host = {
      setup_key:       "Delete",   // 进 BIOS 的键，KeyboardEvent.code
      boot_menu_key:   null,       // 启动菜单的键，null 表示未设置
      tap_interval_ms: 200,        // 50–1000
      tap_duration_s:  15          // 3–60
    }
    ```

    - 默认值是我的决定：台式机主板几乎都用 Del 进 BIOS，启动菜单的键各家不同，所以默认不设。
    - 对应的键没设置时，返回 `not_configured`。
    - 这一节的权限是 `device`。网页设置页加一组"主机启动按键"字段。
- **`wake_on_lan`**：参数 `mac`，或者 `name`（已保存的 WoL 设备）。复用 `/api/wol/wake`。

### 8.4 介质与文件（`media`）

- **`virtual_media`**：参数 `action`：
  - `list`：列出 SD 上可挂载的镜像
  - `status`
  - `mount`：参数 `path` 和 `mode`（`cdrom` 或 `disk`），可选 `readonly`
  - `eject`：按第 6 节的规则决定是否需要确认令牌
  - 复用 `/api/media`。
- **`host_drop_file`**：把小文件放进共享盘，交给主机。
  - 参数：`name`，只能是文件名或一层子目录；`content`，UTF-8 文本或 base64，解码后最多 512 KiB，保证整个请求不超过 1 MiB；`export`，默认 true，写完导出给主机。更大的文件请走网页上传。
  - 共享盘已经导出给主机时，返回 `share_exported`，提示调用 `share` 收回。
  - 复用 `storage/share.rs` 和文件上传逻辑。
- **`share`**：管理共享盘归属。
  - 参数 `action`：`status`、`export`、`reclaim`。
  - 板子看不出主机有没有安全弹出，所以 `reclaim` 的处理分两种：
    - 主机端程序在线时，先请它刷新缓存并弹出共享盘（第 9 节的 `eject` 方法），弹出成功再收回，不需要确认令牌。
    - 主机端程序不在线，或者弹出失败时，需要确认令牌。说明里写明可能丢失主机尚未写入的数据。

### 8.5 主机隐私（`host_private`，默认关闭）

- **`host_clipboard`**：读主机剪贴板里的文本，最多 16 KiB。
  - 要求 key 有 `host_private` 范围，而且主机程序里"允许 KVM 读取剪贴板"已打开。这个开关**默认关闭**。

### 8.6 Resources 和 prompts

都是只读，不支持订阅。

- **resources**：
  - `kvmpanel://host/status`：与 `host_status` 相同
  - `kvmpanel://telemetry/catalogue`
  - `kvmpanel://screen/latest`：当前画面，image blob，需要 `observe`
- **prompts**：
  - `boot_failure_triage`：开机失败排查流程
  - `stability_check`：压力测试时看遥测
  - `enter_bios`：进 BIOS 并找到某项设置
  - 内容各几行，说明建议的工具调用顺序和注意事项。

## 9. 主机端程序通道

**报告格式**

- 厂商 HID 集合新增 **Input report 4**，从板子发往主机，63 字节。主机回复仍走 **Output report 3**。
- 为兼容现有遥测解码器，回复行以 `<` 开头，现有解码器遇到会当未知行丢弃。
- `kvmpanel-gadget` 的描述符要同步加上 Input report 4。

**请求与回复**

- 请求：`?<seq>:<method>[:<args>]`，`seq` 是 0–65535 的循环序号。方法有 `info`、`clipboard`、`eject`。`eject` 请主机刷新并安全弹出共享盘，回复成功或失败原因。
- 回复：`<<seq>:<part>/<total>:<data>`。`data` 是 JSON 文本切出的一段。
- 流控：主机每发一包，等板子用 Input report 回 `!<seq>:<part>` 应答后，再发下一包。这样避开 EP0 只保留最新一份的问题。
- **吞吐的实际情况**：板子现在每 8 ms 读一次 EP0（`panel.rs` 的 `collect`）。每包约 46 字节有效数据要等一次应答，算下来大约 5–8 KB/s。
  - 板端读取可以改成 `poll()` 驱动。改不改由实现者决定，但改完要实测一次吞吐。
  - 一次回复最多 16 KiB，对应剪贴板上限，超过时截断并标 `truncated`。
  - 超时按空闲计算：2 秒没收到下一包就判失败；整次传输最多 10 秒。
- **主机端只有一个写入者**：主机程序里遥测发送线程和回复共用 Output report 3，而板子只保留最新一份，所以所有写入都走同一个发送队列。发回复期间暂停遥测，发完恢复。遥测本来就容忍丢样。

**能力声明**

- 能力**单独发一份报告**，内容为 `^caps=info,clipboard,eject`。
- 不能塞进现有心跳：板子按 `!<16hex>:<interval>` 解析心跳，多出字段会让整个心跳作废。也不能用 `:` 开头，那是设备名的格式。
- 旧版 kvmd 不认识 `^`，会丢掉这份报告，不影响遥测。
- 主机程序在会话开始时发一次，开关变化时再发。
- 板子据此填写 `host_status.host_agent`，没声明的能力直接返回 `host_feature_disabled`。

**主机程序改动**（`software/app/pc-telemetry`）

- 读 Input report 4，处理 `info`、`clipboard`、`eject`。`eject` 只弹出 KVMPanel 共享盘那个 LUN，用 VID/PID 和 LUN 序号识别。
- 设置窗口加两项：
  - "允许 KVM 读取剪贴板"，默认关
  - "KVM 采集的显示器"，默认主显示器。用来把光标坐标换算到采集画面。
- 不加任何 OCR 相关的代码、开关或占位。
- `info` 里的光标坐标要换算成采集显示器上的归一化坐标。这同时给绝对鼠标校准提供了依据：Windows 下绝对 HID 坐标映射到主显示器还是整个虚拟桌面，待实板确认。

## 10. 机箱提示与急停

- **副屏提示**：panel socket 新增 `kvm → panel` 消息 `mcp`，内容为 `{active, key_name, tool}`。
  - 最近 10 秒内有 MCP 输入、电源或介质操作时 `active=true`，副屏顶部显示"AI 正在操作"和 key 名称。
  - 只有观察类调用时不显示。
- **灯效**：MCP 处于 `active` 时，WS2812 通过 `/sys/class/leds/` 显示统一的提示色，默认琥珀色呼吸，结束后恢复。
  - 现在 kvmd 还不控制 LED，这里新增一个很小的 LED 模块，找不到 LED 节点就跳过。
- **网页**：`/api/ws` 的 `status` 增加 `mcp` 字段，内容同上。远程控制台顶部显示提示条，带一个"收回 AI 控制"按钮，等同急停。
- **急停**：
  - 作用：把 `mcp.enabled` 设为 false，立即释放 MCP 按下的键和按钮，让进行中的长工具（`boot_watch`、`enter_firmware`、`screen_wait`）尽快结束，并返回 `stopped`。之后只能在网页上由有 `mcp` 权限的人重新打开。
  - 触发入口：
    - 网页：`POST /api/mcp/stop`，有 `console` 权限的非只读会话就能用。按停止不需要 `mcp` 权限，重新打开需要。
    - 副屏：提示横幅上有"停止"触摸按钮，通过 panel socket 新请求 `mcp_stop` 触发。
    - 功能键：`FunctionKeyAction` 枚举新增 `stop_mcp`，kvmd 实现它的处理函数。`kvmd` 已通过 GPADC0 的 IIO 读取机箱 FUNC，配置为 `stop_mcp` 时调用同一个急停入口。可写 ADC 文件模拟已验证物理键入口能取消正在执行的 MCP 按键。实板 ADC 读数、窗口余量和实际急停待测。
  - 默认的功能键动作不变，是否设成急停由用户决定。

## 11. 操作日志

- 存在数据目录的 `mcp-audit.jsonl` 里，超过 1 MiB 时轮转，保留 2 份。
- 每条记录：时间、key 名称、工具、参数摘要、结果码、耗时。
  - 文本参数只记长度，不记内容。
  - 截图不记图片。
  - 观察类工具合并记录：同一把 key 每分钟只记一条，附调用次数。
- `GET /api/mcp/audit?limit=100`，需要 `mcp` 权限，网页 MCP 页显示最近记录。
- 电源、介质、急停和 key 增删一律逐条记录。

## 12. 网页改动

MCP 设置页改成四块：

1. 总开关。
2. key 列表：新建时选名称和权限范围，只显示一次完整 key 并带复制按钮；可以改权限范围和删除。
3. 客户端配置示例，填入当前地址和 key：
   - Claude Code：`claude mcp add --transport http kvmpanel <url>/api/mcp/v1 --header "Authorization: Bearer <key>"`
   - OpenCode 的 JSON 格式（`type: remote`、`url`、`headers`）
   - 自签证书的提示
   - Claude Desktop 的配置文件只支持 stdio，它的远程连接器从云端发起，访问不到局域网，也不能带自定义请求头，所以不给示例。用户自己用 `mcp-remote` 一类桥接是可以的，但不是本项目的交付物。
4. 最近操作日志。

另外：

- 设置里加"主机启动按键"（`config.host`）和功能键的 `stop_mcp` 选项。
- 中英文文案都要补。

## 13. 验证

依次记录代码、构建、模拟和实板的结果：

1. 契约：OpenAPI、ws 和 panel schema 更新后，mock 测试通过。
   - mock 只实现网页要用的部分：key 管理、`/api/mcp/stop`、操作日志、`config.host`、ws `status.mcp` 和 panel 的 `mcp` 消息。MCP 的 JSON-RPC 只在 kvmd 里实现，用 `kvmd --sim` 测。
   - 工具的 `inputSchema` 和 `outputSchema` 写进契约，新建 `schema/mcp.schema.json`，由 `build.rs` 编进 kvmd。`tools/list` 和参数校验都从这一份来。
2. 后端单元测试：
   - JSON-RPC 的初始化、版本协商、`tools/list` 按权限范围过滤
   - 参数校验、确认令牌（过期、复用、参数不符）
   - 人工输入仲裁、急停打断长工具
   - 日志合并与轮转
   - 主机通道的分包、应答和超时
3. `kvmd --sim` 集成测试：用 HTTP 跑完每个工具一次。模拟器要能模拟电源灯、HDMI 信号的出现和消失、遥测上线，以及一个模拟主机端程序。
4. 真实客户端：在另一台机器上用 Claude Code 接 `kvmd --sim`，跑通截图、点击、`boot_watch` 和 `enter_firmware`。也可以用 MCP Inspector。
5. ARM 静态构建通过，记录二进制体积的变化。
6. Windows 主机程序：x64 构建通过，用模拟的 Input report 验证 `info`、`clipboard`、`eject` 和剪贴板开关默认关闭。
7. 实板（有板后）：
   - 绝对坐标映射
   - 进 BIOS 连按是否生效
   - `boot_watch` 的时间线
   - 主机通道吞吐
   - WS2812 提示色
   - 功能键急停

## 14. 交给实现者决定

- MCP 协议层手写还是用 `rmcp`，按第 4 节的条件。
- 帧差算法、环形缓冲的具体实现、JPEG 缩放方式。
- 各工具内部复用现有函数的方式，以及模块怎么拆分。
- 错误码可以按需增加，但要写进契约。

## 15. 我替用户做的决定（请核对）

- 入口放在 `/api/mcp/v1`，旧的单把 key 迁移成 `observe`+`input` 范围的 `default` key。
- 人工输入 5 秒内 MCP 输入返回 `human_active`，人优先。
- `setup_key` 默认 `Delete`，`boot_menu_key` 默认不设。
- 剪贴板读取默认关闭，并且单独用 `host_private` 范围管理。（OCR 由用户决定：这一版不做，之后在 T113 上试 PP-OCR。）
- 画面回看只在最近 10 分钟有 MCP 调用时运行。
- 长工具单次最多 120 秒。
- MCP 截图一律从视频流已有的 JPEG 出发，裁剪和缩放用软件重编码，不改视频流的编码器。
- `host_drop_file` 上限 512 KiB，请求体保持 1 MiB。
- 新增 `share` 工具：主机端程序在线时先请它安全弹出再收回；不在线时需要确认令牌。
- USB 连着时卸载可写磁盘镜像一律要确认。
- 主机通道回复上限 16 KiB，空闲 2 秒超时，整次最多 10 秒；能力单独用 `^caps=` 报告。
- 急停的网页入口只需 `console` 权限，副屏横幅上也有停止按钮；物理 FUNC 的软件入口已接通，实板验证见第 13 节。
- 示例客户端去掉 Claude Desktop。
- `Host` 和 `Origin` 用允许列表校验，不支持浏览器直连。
- mock 不实现 MCP 协议本身；工具 schema 放进契约。

## 16. 实现前问答（2026-11-23）

实现者开工前提的问题，以下是答复，与正文冲突时以本节为准。

1. **人工输入随时接管**。MCP 的键鼠操作进行中（长按、拖动、`enter_firmware` 连按、REST HID）出现第一个有效人工输入时，立即取消 MCP 的输入操作，释放 MCP 按住的键和按钮，再处理人工输入。被打断的工具返回 `human_took_over`。第 8.2 节"5 秒内拒绝"的规则照旧，用于调用开始前。
2. **急停对进行中动作的处理**。立即释放 MCP 按住的键和按钮，以及仍拉住的电源线和复位线；取消后续的开机动作和重试。强关的 5 秒脉冲被中途放开时，电脑保持原状态，这可以接受。介质和共享盘的切换不打断，跑到一致状态后再结束。所以电源按键路径要改成可以取消，不能同步阻塞等 `gpioset`。
3. **`boot_watch` 的计时和判定**。
   - `timeout_s` 约束整次调用，包括电源动作、等待和所有重试，上限 120 秒。
   - `start: none` 时 `retries` 必须为 0，否则参数错误。
   - 只认动作之后出现的新证据：HDMI 要先看到掉信号再恢复，遥测和主机端程序要出现新会话。复位后 10 秒内一直没掉信号时，HDMI 这一阶段记为 `unknown`，继续按 `stable` 或 `os` 判断。
4. **自动弹出共享盘**。只在能唯一确认目标就是发出请求的这台设备的 LUN 2（与这个 HID 属于同一个 USB 复合设备），而且安全弹出成功时，才自动收回。普通用户权限不够或无法唯一确认时，回复失败，板端按第 8.4 节走确认令牌。不要求普通用户权限下一定能自动弹出。
5. **剪贴板只读当前会话**。只在当前活动、已解锁的本地会话里读取。锁屏、安全桌面、切换用户、后台会话都返回 `host_feature_disabled`。开关跟随各自的 Windows 用户设置。
6. **回看可以主动启动采集**。MCP 活跃窗口（最近 10 分钟有调用）内允许为回看启动采集。已有网页视频流时直接复用；没有时只按回看需要的约 1 fps 采集，不按 `video.fps_max` 满帧率跑。窗口结束后停止。
7. **`press_keys` 的上限**。总共最多 8 个键，其中普通键（非修饰键）最多 6 个，超过时整条拒绝。不改 USB 键盘报告。
8. **能力声明与在线判断**。主机程序每 5 秒重发一次 `^caps=`，开关变化时立即发。板端把有效心跳、能力声明和回复包都算作主机端程序在线的证据。遥测读数的新鲜度仍按原来的 3 秒规则判断。回复期间遥测暂停，不能导致主机端程序被判离线。
9. **`telemetry_watch` 的计数**。按观察期间板端实际收到的每个指标的新读数计数。显式 `null` 计入 `nulls`，没有新读数时不重复计入旧值；`last` 可以取缓存值。
10. **`host_drop_file` 的编码**。加参数 `encoding`，取值 `utf8` 或 `base64`，省略时按 `utf8`。
11. **人工输入优先可以设置**（用户 2026-11-23 决定，覆盖第 8.2 节的 5 秒和本节第 1 条）。
    - MCP 设置页增加两项："人工输入优先"开关，默认打开；"交还等待时间"，1–60 秒，默认 **2 秒**。
    - 开关打开时：第 1 条的接管规则照旧；最后一次人工输入之后，等过交还等待时间，MCP 才能再输入，期间返回 `human_active`。
    - 开关关闭时：人工输入不打断 MCP，MCP 也不因人工输入被拦截，双方的输入可能交错。急停不受这个开关影响。
    - `host_status.control` 带上这两项的当前值。
