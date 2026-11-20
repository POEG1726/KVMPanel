# 接口契约与 Mock 服务 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 写出网页 HTTP/WebSocket API 和 `panel` ↔ `kvmd` socket 协议的契约，并提供一个能在 PC 上运行、行为符合契约的 mock 服务，供网页前端、副屏原型和 Rust 后端并行开发。

**Architecture:** 契约以 `software/app/api/openapi.yaml`（HTTP 部分）加上 `software/app/api/README.md`（WebSocket 和 panel socket 消息，配 JSON Schema）为准。Mock 用 Node 24 + TypeScript + Fastify 实现，测试用 Vitest；测试会对照 OpenAPI 里的 schema 校验 mock 的响应，保证 mock 和契约不走样。

**Tech Stack:** Node 24、TypeScript 5（ESM）、Fastify 5、`@fastify/websocket`、`@fastify/cookie`、`@fastify/multipart`、Ajv 8、`yaml`、`jpeg-js`、Vitest。

**Spec:** `docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md`（第 4.5、5、6、7.3、7.4、10 节）

## Global Constraints

- 所有路径都在 `/api/` 下，API 版本写在 `openapi.yaml` 的 `info.version: 0.1.0`。
- 错误统一返回 `{ "error": "<snake_case_code>", "detail"?: object }`。只返回错误码，界面文案由前端按中英文翻译（规格第 7.2 节）。
- 鉴权方式：HttpOnly cookie `kvm_session`。MCP 用 `Authorization: Bearer <key>`。
- 角色分 `admin` 和 `user`。权限键：`console`、`power`、`media`、`terminal`、`files`、`device`、`network`、`updates`、`mcp`、`tailscale`、`panel`、`usb`。
- 配置只由 kvmd 写入（规格第 7.3 节）；网页和 panel 都通过契约修改配置。
- 本机开发端口：mock HTTP 是 `127.0.0.1:8800`；panel socket 在 PC 上用 TCP `127.0.0.1:7701`，在板子上用 Unix socket `/run/kvmd/panel.sock`，两者消息格式相同。
- USB 端点预算按规格第 4.5 节的表：IN 5、OUT 5。默认组合是 键盘 + 相对鼠标 + 绝对鼠标 + 大容量存储 + 上位机数据，占用 IN 4、OUT 1。
- 遥测数据走数据驱动（规格第 10 节）：每个指标包含 `id, label, unit, value, min, max, warn?, group`。
- 不做 H.264 和 WebRTC，视频只有 MJPEG。

## Review Focus

1. 连续输错密码：第 3 次起每次延迟 1 s，第 6 次起锁定账号 5 分钟，返回 `429 {error:"login_locked", detail:{retry_after_s}}`。由 Task 2 测试。
2. `PATCH /api/config` 带未知字段或越界值：返回 `400 {error:"invalid_config", detail:{path}}`，原配置不变。由 Task 3 测试。
3. 在 SD 卡和共享盘根目录下使用 `..` 路径：返回 `400 {error:"bad_path"}`；系统 `/` 根目录允许任意绝对路径（用户已确认保留）。由 Task 7 测试。
4. 上传超过剩余空间，或上传中途断开：返回 `507 {error:"no_space"}`，或者清掉残留的半截文件，列表里不出现。由 Task 7 测试。
5. WebSocket 客户端发来格式错误的 HID 消息：这条消息丢弃，回一条 `{"t":"error","error":"bad_message"}`，连接不断。由 Task 4 测试。

---

## 文件结构

```
software/app/api/
  README.md                 契约总说明 + WebSocket 与 panel socket 消息定义
  openapi.yaml              HTTP 契约（唯一权威）
  schema/panel.schema.json  panel socket 消息的 JSON Schema
  schema/ws.schema.json     /api/ws 消息的 JSON Schema
  mock/package.json, tsconfig.json, vitest.config.ts
  mock/src/server.ts        buildServer(opts): FastifyInstance，注册各路由模块
  mock/src/main.ts          启动 HTTP 8800 + panel TCP 7701
  mock/src/state.ts         内存状态：用户、会话、配置、介质、文件、电源
  mock/src/contract.ts      加载 openapi.yaml，提供 validateResponse(op, status, body)
  mock/src/usb.ts           usbBudget()，端点预算的纯函数
  mock/src/routes/*.ts      auth, config, ws, stream, power, files, media, terminal, system
  mock/src/panel/server.ts  panel socket 服务和遥测生成器
  mock/src/panel/metrics.ts 第 4.1 节的指标定义
  mock/test/*.test.ts       每个路由模块一个测试文件
```

---

### Task 1: 脚手架、契约加载和健康检查

**Files:**
- Create: `software/app/api/README.md`, `software/app/api/openapi.yaml`, `software/app/api/mock/{package.json,tsconfig.json,vitest.config.ts}`, `software/app/api/mock/src/{server.ts,main.ts,state.ts,contract.ts}`
- Test: `software/app/api/mock/test/health.test.ts`
- Modify: `software/app/README.md`（补充 `api/`、`web/` 两个目录的说明，写明 `ipkvm/` 是 Rust crate `kvmd`）

**Interfaces:**
- Produces: `buildServer(opts?: { now?: () => number }): Promise<FastifyInstance>`；`validateResponse(operationId: string, status: number, body: unknown): void`，校验不通过时 throw，错误信息里带 Ajv 的错误；`createState(): State`。
- Produces: `GET /api/health` 返回 `200 {ok: true, version: "0.1.0"}`，operationId 为 `getHealth`。这个接口不需要登录。
- `README.md` 写明：传输方式总览（HTTP、`/api/ws`、`/api/stream`、`/api/terminal`、panel socket）、错误格式、鉴权方式、端口，以及“契约有变动时，先改契约，再改 mock 和测试”。

- [ ] **Step 1: 写失败测试** `health.test.ts`：用 `app.inject({method:'GET',url:'/api/health'})` 请求，断言 status 是 200、body 是 `{ok:true,version:'0.1.0'}`，并且 `validateResponse('getHealth',200,body)` 不 throw；再断言 `validateResponse('getHealth',200,{ok:'yes'})` 会 throw。
- [ ] **Step 2: 运行** `npm test --prefix software/app/api/mock`，预期 FAIL，因为模块还不存在。
- [ ] **Step 3: 实现**脚手架、`contract.ts`（用 `yaml` 解析，再把 `components/schemas` 注册进 Ajv）、`server.ts` 和 `getHealth`。
- [ ] **Step 4: 运行测试**，预期 PASS；再运行 `npx tsc --noEmit -p software/app/api/mock`，预期无错误。
- [ ] **Step 5: 提交** `feat(api): contract skeleton and mock server`

### Task 2: 登录、账号和权限

**Files:**
- Modify: `openapi.yaml`, `mock/src/state.ts`, `mock/src/server.ts`
- Create: `mock/src/routes/auth.ts`
- Test: `mock/test/auth.test.ts`

**Interfaces:**
- Produces（HTTP）：
  - `POST /api/auth/login {username,password}`：成功返回 `200 {user}` 并设置 cookie；密码错返回 `401 invalid_credentials`；被锁返回 `429 login_locked`。
  - `POST /api/auth/logout`：返回 `204`。
  - `GET /api/auth/me`：返回 `200 {user}`；未登录返回 `401 unauthenticated`。
  - `GET/POST /api/users`、`PATCH/DELETE /api/users/{name}`：仅 admin。
  - `POST /api/users/me/password {old,new}`：成功后让该用户的其他会话全部失效。
- `User = {name, role:'admin'|'user', permissions: string[], view_only: boolean, disabled: boolean}`。
- Produces（mock 内部）：`requirePerm(perm: string)`，Fastify preHandler，没有权限时返回 `403 forbidden`。
- Mock 的初始账号是 `admin/admin`，并带 `must_change_password: true`。

- [ ] **Step 1: 写失败测试**：
  - 正确登录后，`/api/auth/me` 返回 admin。
  - 错误密码返回 401。
  - **连续 5 次错误后第 6 次返回 429，且 `detail.retry_after_s` 为 300；第 3 次起每次响应耗时至少 1000 ms**（注入假时钟，断言调用了延迟）。
  - 改密码后，旧 cookie 访问 `/me` 返回 401。
  - user 角色访问 `/api/users` 返回 403。
  - 以上所有响应都要通过 `validateResponse`。
- [ ] **Step 2: 运行**，预期 FAIL。
- [ ] **Step 3: 实现** `auth.ts`，以及会话表和每个用户的 `token_version`。
- [ ] **Step 4: 运行**，预期 PASS。
- [ ] **Step 5: 提交** `feat(api): auth, users and permissions`

### Task 3: 统一配置和 USB 端点预算

**Files:**
- Modify: `openapi.yaml`
- Create: `mock/src/usb.ts`, `mock/src/routes/config.ts`
- Test: `mock/test/usb.test.ts`, `mock/test/config.test.ts`

**Interfaces:**
- Produces: `GET /api/config` 返回完整的 `Config`；`PATCH /api/config`（JSON Merge Patch）返回 `200 Config`。超出预算返回 `409 {error:"endpoints_full", detail:{in_missing, out_missing}}`，字段不合法返回 `400 invalid_config`。每个顶层分区需要对应的权限键。
- `Config` 的顶层分区和字段如下。默认值来自规格；规格没给的值由实现者定，并写进 schema 的 `default`。
  - `panel`：`{language:'en'|'zh' = 'en', auto_brightness:true, brightness:0..100 = 70, offline_sleep_min:0..120 = 5, thresholds:{'cpu.tctl':85, 'gpu.mem_junction':95, 'mem.dimm_temp':60}, hidden_metrics:string[] = ['sys.fan']}`
  - `function_key`：`{short: Action = 'toggle_settings', long: Action = 'toggle_screen'}`；`Action = 'none'|'toggle_settings'|'toggle_screen'|'cycle_brightness'|'pause_kvm'`
  - `usb`：`{keyboard:true, mouse_rel:true, mouse_abs:true, mouse_merge:false, storage:true, net:false, net_mode:'ncm'|'rndis' = 'ncm', acm:false, telemetry:true}`
  - `web`：`{language:'auto'|'en'|'zh' = 'auto', workspace_open:'float'|'tab' = 'float', remember_float:true, menubar:'auto_hide'|'pinned' = 'auto_hide', title:'KVMPanel'}`
  - `video`：`{quality:50..100 = 80, fps_max:1..60 = 30, scale:'fit'|'1:1'|'fill' = 'fit', frame_diff:true}`
  - `hid`：`{keyboard_layout = 'us', mouse_mode:'abs'|'rel' = 'abs', wheel_dir:'normal'|'reverse' = 'normal', wheel_speed:1..10 = 1, jiggler:false, leader_key:string|null}`
  - `serial`：`{device:string|null = null, baud:115200}`
  - `device`：`{hostname:'kvmpanel', ssh:true, mdns:true, watchdog:false}`
  - `kvm`：`{enabled:true}`
- Produces: `usbBudget(u: Config['usb']): {in: number, out: number, fits: boolean, in_missing: number, out_missing: number}`。规则：
  - 每项的占用按规格第 4.5 节的表计算。
  - `mouse_merge && mouse_rel && mouse_abs` 时，绝对鼠标的 IN 记为 0。
  - `telemetry` 只在 `!mouse_rel && !mouse_abs` 时占 1 个 IN。
- `GET /api/usb/budget` 返回当前的 `usbBudget`。`POST /api/usb/apply` 返回 `202 {reenumerating:true}`，并通过 `/api/ws` 推送 `usb_reenumerated` 事件。

- [ ] **Step 1: 写失败测试** `usb.test.ts`：
  - 默认组合得到 `{in:4,out:1,fits:true}`。
  - 默认组合加 `net:true`，得到 `{in:6,fits:false,in_missing:1}`。
  - 默认组合加 `mouse_merge:true`，得到 `in:3`，再开 `net` 得到 `in:5,fits:true`。
  - 两个鼠标都关、只开 `keyboard` 和 `telemetry`，得到 `in:2`。
  - 开 `acm` 和 `net`，关两个鼠标和存储，得到 `{in:6,fits:false}`（键盘 1 + 网卡 2 + 串口 2 + 上位机数据 1）。
- [ ] **Step 2: 写失败测试** `config.test.ts`：
  - GET 返回的默认值逐项等于上表。
  - `PATCH {panel:{brightness:30}}` 只改这一项。
  - `PATCH {usb:{net:true}}` 返回 409，`detail.in_missing` 为 1，且配置没变。
  - **`PATCH {panel:{brightness:200}}` 和 `PATCH {panel:{nope:1}}` 都返回 400，`detail.path` 分别为 `/panel/brightness` 和 `/panel/nope`，配置不变。**
  - 缺少 `usb` 权限的 user 改 `usb` 分区时返回 403。
- [ ] **Step 3: 运行**，预期 FAIL。
- [ ] **Step 4: 实现** `usb.ts` 和 `config.ts`，用 Ajv 按 `Config` schema 做 `additionalProperties:false` 校验。
- [ ] **Step 5: 运行**，预期 PASS。
- [ ] **Step 6: 提交** `feat(api): unified config and USB endpoint budget`

### Task 4: 事件和输入 WebSocket `/api/ws`

**Files:**
- Modify: `README.md`（加 `/api/ws` 一节）, `schema/ws.schema.json`
- Create: `mock/src/routes/ws.ts`
- Test: `mock/test/ws.test.ts`

**Interfaces:**
- 每条消息是一个 JSON 对象，用字段 `t` 区分类型。
- 客户端 → 服务端：
  - `{t:'key', code:string(KeyboardEvent.code), down:boolean}`
  - `{t:'mouse_abs', x:0..65535, y:0..65535, buttons:number}`
  - `{t:'mouse_rel', dx:-127..127, dy:-127..127, buttons:number}`
  - `{t:'wheel', dy:-127..127}`
  - `{t:'paste', text:string(≤ 64 KiB)}`
  - `{t:'shortcut', keys:string[]}`
  - `{t:'ping'}`
- 服务端 → 客户端：
  - `{t:'status', hdmi:{signal:boolean,width,height,fps}, stream:{fps,kbps}, power:{on:boolean,hdd:boolean}, locks:{num,caps,scroll}, sessions:[{user,ip,since,view_only}], media:{mounted:string|null,mode:'cdrom'|'disk'|null}}`：连上时立即推一次，之后每秒推一次。
  - `{t:'usb_reenumerated'}`
  - `{t:'error', error:'bad_message'|'forbidden'}`
  - `{t:'pong'}`
- 只读（view_only）会话发送的输入一律回 `forbidden`。
- 连接要求已登录并持有 `console` 权限，否则以 4401 关闭。

- [ ] **Step 1: 写失败测试**（Fastify `injectWS`）：
  - 连上后 1 秒内收到 `status`，且通过 `ws.schema.json` 校验。
  - 发送 `{t:'key',code:'CapsLock',down:true}` 后，下一条 `status` 里 `locks.caps` 变为 true。
  - **发送 `{t:'key'}` 或非 JSON 内容，收到 `{t:'error',error:'bad_message'}`，随后 `ping` 仍然得到 `pong`。**
  - 只读会话发送 `key` 时收到 `forbidden`。
  - 未登录连接被以 4401 关闭。
- [ ] **Step 2: 运行**，预期 FAIL。
- [ ] **Step 3: 实现** `ws.ts`：在 mock 里模拟锁定键状态，sessions 从会话表取。
- [ ] **Step 4: 运行**，预期 PASS。
- [ ] **Step 5: 提交** `feat(api): event and HID websocket`

### Task 5: 视频流 `/api/stream`

**Files:**
- Modify: `README.md`（加视频流一节）, `openapi.yaml`（`GET /api/stream/snapshot.jpg`）
- Create: `mock/src/routes/stream.ts`
- Test: `mock/test/stream.test.ts`

**Interfaces:**
- WebSocket `/api/stream`：服务端每条**二进制消息就是一整张 JPEG**。客户端可以发 `{t:'pause'}` 和 `{t:'resume'}`。
- `config.video.frame_diff` 为 true 时，画面没变化就不发送。
- 服务端发送有背压：上一帧还没发完时，直接丢弃新帧。
- `GET /api/stream/snapshot.jpg` 返回 `200 image/jpeg`。没有 HDMI 信号时返回 `503 {error:"no_signal"}`。
- EDID：`GET /api/video/edid` 返回 `{hex:string, preset:'1080p'|'custom'}`；`PUT /api/video/edid {hex}`，长度必须是 128 或 256 字节的十六进制，否则返回 400 `bad_edid`；`POST /api/video/edid/reset` 恢复 1080p 模板。
- Mock 的画面：1920×1080，用 `jpeg-js` 画一条每帧右移的彩色竖条，帧率按 `config.video.fps_max`。为了省 CPU，可以先编码约 30 张缓存起来循环播放。

- [ ] **Step 1: 写失败测试**：
  - 连上后收到的二进制消息以 `FF D8` 开头、以 `FF D9` 结尾。
  - `fps_max=5` 时，2 秒内收到 8 到 12 帧。
  - 发送 `pause` 后 1 秒内收不到帧。
  - snapshot 返回 image/jpeg。
  - 在 mock 状态里把 `hdmi.signal` 设为 false 后，snapshot 返回 503 `no_signal`。
  - `PUT /api/video/edid {hex:'00ff'}` 返回 400 `bad_edid`；reset 后 `preset` 为 `'1080p'`。
- [ ] **Step 2: 运行**，预期 FAIL。
- [ ] **Step 3: 实现**。
- [ ] **Step 4: 运行**，预期 PASS。
- [ ] **Step 5: 提交** `feat(api): MJPEG stream websocket`

### Task 6: 电源和网络唤醒

**Files:**
- Modify: `openapi.yaml`
- Create: `mock/src/routes/power.ts`
- Test: `mock/test/power.test.ts`

**Interfaces:**
- `POST /api/power/press {button:'power'|'reset', duration_ms: 200..12000}`：返回 `204`，需要 `power` 权限。duration 至少 4000 时就是长按强制关机。
- `GET /api/power` 返回 `200 {on, hdd}`。
- WoL：`GET/POST /api/wol/devices`（`{name, mac}`，MAC 格式不对返回 400 `bad_mac`）、`DELETE /api/wol/devices/{mac}`、`POST /api/wol/wake {mac}` 返回 204。

- [ ] **Step 1: 写失败测试**：
  - 短按 power 后，`GET /api/power` 的 `on` 翻转。
  - 长按 5000 ms 后 `on` 变为 false。
  - `duration_ms:50` 返回 400。
  - 没有 `power` 权限返回 403。
  - `POST /api/wol/devices {mac:'zz'}` 返回 400 `bad_mac`。
  - 增、删、查和唤醒的流程走通。
- [ ] **Step 2: 运行**，预期 FAIL。**Step 3: 实现。** **Step 4: 运行**，预期 PASS。
- [ ] **Step 5: 提交** `feat(api): power control and wake-on-lan`

### Task 7: 存储、文件管理器和虚拟介质

**Files:**
- Modify: `openapi.yaml`
- Create: `mock/src/routes/files.ts`, `mock/src/routes/media.ts`
- Test: `mock/test/files.test.ts`, `mock/test/media.test.ts`

**Interfaces:**
- 根目录 `Root = 'sd'|'share'|'system'`。`path` 一律是根目录内的 POSIX 路径。
- `GET /api/storage` 返回 `[{root, total, free, mounted}]`。
- `GET /api/files?root&path` 返回 `{entries:[{name, type:'dir'|'file', size, mtime, mounted?:boolean}]}`。
- `GET /api/files/download?root&path`：流式下载。
- `PUT /api/files/upload?root&path`：请求体就是原始文件流，可以带 `X-Sha256` 校验头。成功返回 `201`。空间不够返回 `507 no_space`。校验失败返回 `422 checksum_mismatch`，并删除已写入的文件。
- `POST /api/files/mkdir|rename|delete`，参数为 `{root, path, to?}`。
- 路径规则：在 `sd` 和 `share` 下，规范化后越出根目录，或包含 `..` 段，都返回 `400 bad_path`；`system` 允许任意绝对路径（用户确认）。
- 访问 `system` 需要 `files` 权限，并且必须是 admin 角色。
- 介质：
  - `GET /api/media` 返回 `{mounted:{root,path,mode}|null, readonly:boolean}`。
  - `POST /api/media/mount {root:'sd', path, mode:'cdrom'|'disk', readonly?}`：只接受 `.iso`、`.img`。已挂载时返回 `409 already_mounted`；`cdrom` 模式下文件不是 ISO 或 UDF 时返回 `422 bad_image`。
  - `POST /api/media/eject` 返回 204。
  - `POST /api/media/download {url, path, sha256?}` 返回 `202 {job}`；`GET /api/media/jobs` 和 `POST /api/media/jobs/{id}/pause|resume|cancel` 管理下载任务。
- 共享盘只允许一个写入方：`GET /api/share` 返回 `{exported_to_host:boolean}`；`exported_to_host` 为 true 时，往 `share` 根目录写入返回 `409 share_busy`。
- Mock 在内存里放一棵文件树，SD 卡容量 58 GB、已用 23.4 GB，内容就是草图里的那几个文件。

- [ ] **Step 1: 写失败测试** `files.test.ts`：
  - 列出 `sd:/images`，得到 `ubuntu-24.04.iso` 等文件。
  - 上传后再列出能看到新文件，下载回来的内容一致。
  - **`root=sd&path=/images/../../etc` 和 `path=../x` 都返回 400 `bad_path`；`root=system&path=/etc` 由 admin 访问时返回 200。**
  - **声明 `Content-Length` 大于 free 时返回 507 `no_space`；上传中途断开连接（发送一半就 `destroy`）后，列表里没有这个文件。**
  - `X-Sha256` 不匹配时返回 422，文件不存在。
  - user 角色访问 `system` 返回 403。
  - `exported_to_host` 时往 `share` 写入返回 409。
- [ ] **Step 2: 写失败测试** `media.test.ts`：
  - 挂载 ISO 后，`GET /api/media` 能看到挂载信息，文件列表里该项 `mounted:true`。
  - 重复挂载返回 409。
  - 挂载 `.txt` 返回 422 `bad_image`。
  - 弹出后状态变为 null。
  - 下载任务能走完暂停、恢复、取消的状态流转。
- [ ] **Step 3: 运行**，预期 FAIL。**Step 4: 实现。** **Step 5: 运行**，预期 PASS。
- [ ] **Step 6: 提交** `feat(api): storage, file manager and virtual media`

### Task 8: 终端 `/api/terminal`

**Files:**
- Modify: `README.md`（加终端一节）
- Create: `mock/src/routes/terminal.ts`
- Test: `mock/test/terminal.test.ts`

**Interfaces:**
- WebSocket `/api/terminal?kind=local|serial`：二进制消息是原始的终端字节流，双向；文本消息只有 `{t:'resize', cols, rows}`。
- 需要 `terminal` 权限。`kind=serial` 且 `config.serial.device` 为 null 时，以 4404 关闭。
- Mock 是一个回显 shell：收到 `\r` 后输出 `\r\n` 和提示符 `root@kvmpanel:~# `；输入 `ls` 时打印固定的几行。
- 自定义脚本（需要 `terminal` 权限）：`GET /api/scripts` 返回 `[{name,size}]`；`PUT /api/scripts/{name}`（请求体是脚本）返回 201；`DELETE /api/scripts/{name}` 返回 204；`POST /api/scripts/{name}/run` 返回 `200 {exit_code, output}`。名称只允许 `[A-Za-z0-9._-]{1,64}`，否则返回 400 `bad_name`。

- [ ] **Step 1: 写失败测试**：
  - 连上后收到包含提示符的输出。
  - 发送 `ls\r` 后收到 `images`。
  - 发送 resize 后连接不断。
  - 上传脚本 `hello.sh` 后运行得到 `exit_code:0`；名称为 `../x` 时返回 400 `bad_name`。
  - 未配置串口时 serial 以 4404 关闭。
  - 没有 `terminal` 权限时以 4403 关闭。
- [ ] **Step 2: 运行**，预期 FAIL。**Step 3: 实现。** **Step 4: 运行**，预期 PASS。
- [ ] **Step 5: 提交** `feat(api): terminal websocket`

### Task 9: 系统类接口

**Files:**
- Modify: `openapi.yaml`
- Create: `mock/src/routes/system.ts`
- Test: `mock/test/system.test.ts`

**Interfaces:**
- 需要 `device` 权限：
  - `GET /api/device` 返回 `{hostname, version, kernel, uptime_s, ip, mac, link_mbps, soc_temp_c, mem:{used,total}, data:{used,total}, lux}`。
  - `POST /api/device/action {action:'reboot'|'shutdown'|'restart_kvm'|'reset_hid'|'disconnect_sessions'}` 返回 `202`。其中 `restart_kvm` 会通过 `/api/ws` 推送断开事件，mock 实际执行断开。
- 需要 `network` 权限：
  - `GET/PUT /api/network`，内容为 `{mode:'dhcp'|'static', ip?, mask?, gateway?, dns:{mode:'dhcp'|'manual', servers:string[]}, https:{enabled, port, cert_path?, key_path?}, http_port, bind}`。
  - static 模式缺少 ip 时返回 400。
- 需要 `tailscale` 权限：`GET /api/tailscale` 返回 `{installed, running, ip?, login_url?}`；`POST /api/tailscale/{action}` 返回 202。
- 需要 `updates` 权限：
  - `GET /api/update` 返回 `{current, latest?, channel:'stable'|'preview', server}`。
  - `POST /api/update/check` 返回 200。
  - `POST /api/update/apply` 返回 `202 {job}`。
  - `PUT /api/update/offline` 用于上传升级包，SHA-256 不对时返回 422。
  - 按规格第 10 节，升级只替换应用包。
- 需要 `mcp` 权限：`GET /api/mcp` 返回 `{enabled, key_hint}`；`POST /api/mcp/regenerate` 返回 `{key}`，只在这一次返回完整 key。
- `GET /api/about` 返回 `{name:'KVMPanel', version, licenses_url}`，无需登录。
- 品牌（需要 `device` 权限）：`PUT /api/branding/logo`（`image/png` 或 `image/svg+xml`，不超过 256 KiB，否则返回 413 `too_large`）返回 204；`GET /api/branding/logo` 无需登录（登录页要用），未设置时返回 404。

- [ ] **Step 1: 写失败测试**：
  - 每个 GET 接口都能通过 `validateResponse`。
  - static 网络配置缺 ip 时返回 400。
  - `restart_kvm` 之后，已连接的 `/api/ws` 被关闭。
  - 重新生成 MCP key 后，`key_hint` 等于新 key 的末 4 位。
  - 离线升级包 SHA-256 不匹配时返回 422。
  - 上传 300 KiB 的 logo 返回 413；上传一张小 PNG 后，不登录 GET 得到相同的字节。
- [ ] **Step 2: 运行**，预期 FAIL。**Step 3: 实现。** **Step 4: 运行**，预期 PASS。
- [ ] **Step 5: 提交** `feat(api): device, network, tailscale, update, mcp endpoints`

### Task 10: panel socket 协议和遥测生成器

**Files:**
- Modify: `README.md`（加 panel socket 一节）
- Create: `schema/panel.schema.json`, `mock/src/panel/metrics.ts`, `mock/src/panel/server.ts`
- Modify: `mock/src/main.ts`（启动 7701）
- Test: `mock/test/panel.test.ts`

**Interfaces:**
- 传输格式：换行分隔的 JSON（NDJSON），每行一个对象，用字段 `t` 区分类型。请求带 `id`，响应原样带回同一个 `id`。
- kvmd → panel：
  - `{t:'hello', version:'0.1.0'}`
  - `{t:'telemetry', ts, metrics: Metric[]}`：每秒一次。上位机断开时停止发送，并改发 `{t:'pc', connected:false, last_seen_ts}`。
  - `{t:'status', eth:{state:'up'|'no_ip'|'down', mbps, ip}, pc:{state:'ok'|'partial'|'off'}, usb_a:{device:string|null}, kvm:{enabled, sessions:number}, lux}`：状态变化时推送，另外每 5 秒推一次。
  - `{t:'config', config: Config}`：连上时推一次，配置每次变更后再推。
  - `{t:'kvm_restarting'}`
- panel → kvmd：
  - `{t:'get_config', id}`
  - `{t:'patch_config', id, patch}`，响应为 `{t:'result', id, ok, error?, detail?}`。错误码和 HTTP 的 `PATCH /api/config` 相同，包括 `endpoints_full`。
  - `{t:'action', id, action}`：action 与 Task 9 的 device action 相同，另加 `eject_media` 和 `usb_apply`。
  - `{t:'device_info', id}`：响应与 `GET /api/device` 相同。
- `Metric = {id, label, unit, value: number|null, min, max, warn?: number, group:'cpu'|'gpu'|'mem'|'sys'}`。
- `metrics.ts` 定义第 4.1 节的指标 id（布局模板按这些 id 取数据）：
  - CPU：`cpu.clock_eff`(GHz, 3–5.8)、`cpu.bclk`(MHz, 99–101)、`cpu.fclk`(MHz, 1900–2100)、`cpu.vcore`(V, 0.9–1.4)、`cpu.tctl`(°C, 30–95, warn 85)、`cpu.load`(%, 0–100)、`cpu.ppt`(W, 0–230)、`cpu.core.0`…`cpu.core.15`(%, 0–100，0–7 是 CCD0)。
  - GPU：`gpu.clock`(MHz, 0–3000)、`gpu.temp`(°C, 30–90)、`gpu.mem_junction`(°C, 30–100, warn 95)、`gpu.load`(%)、`gpu.vram_used`(GB, 0–32)、`gpu.power`(W, 0–600)。
  - 内存：`mem.used`(GB, 0–64)、`mem.dimm.a1`/`a2`/`b1`/`b2`(°C, 30–60, warn 60；a1 和 b1 恒为 null)、`mem.mclk`(MHz)、`mem.uclk`(MHz)。
  - 整机：`sys.power`(W, 0–900)、`ssd.0.temp`(label 为 `SSD 2T`)、`ssd.1.temp`(label 为 `990 EVO 4T`)(°C, 25–80)、`net.rx`/`net.tx`(MB/s)。
- 生成器用正弦波加噪声产生数据，波形同草图 `e2-v5.html`。
- `config.panel.hidden_metrics` 里的 id 不下发。
- `main.ts` 同时监听 TCP 7701。设置环境变量 `KVMPANEL_PANEL_SOCK` 时，改为监听这个 Unix socket 路径。

- [ ] **Step 1: 写失败测试**：
  - 连上 TCP 后依次收到 `hello` 和 `config`，1.5 秒内收到 `telemetry`，每条都通过 `panel.schema.json` 校验。
  - telemetry 里包含 16 个 `cpu.core.*`，`mem.dimm.a1.value === null`。
  - `patch_config {usb:{net:true}}` 返回 `ok:false,error:'endpoints_full'`。
  - 通过 HTTP `PATCH /api/config` 改亮度后，panel 连接收到新的 `config`。
  - 模拟上位机断开后，停止发送 telemetry，并收到 `pc` 消息。
  - 一行坏 JSON 会得到 `{t:'result',ok:false,error:'bad_message'}`，连接不断。
- [ ] **Step 2: 运行**，预期 FAIL。**Step 3: 实现。** **Step 4: 运行**，预期 PASS。
- [ ] **Step 5: 端到端检查**：运行 `npm start --prefix software/app/api/mock` 启动 mock，再执行 `curl http://127.0.0.1:8800/api/health`，预期得到 `{"ok":true,"version":"0.1.0"}`。
- [ ] **Step 6: 提交** `feat(api): panel socket protocol and telemetry generator`
