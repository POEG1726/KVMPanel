# KVM 网页前端 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `software/app/web/` 实现 KVM 网页：登录、控制台（远程画面、悬浮菜单栏和下拉面板）、设置窗口（KVM 组和系统组）、文件/终端工作台，按契约对接 mock，以后无改动对接 `kvmd`。

**Architecture:** 采用 Vite + React 18 + TypeScript 单页应用。
- HTTP 用 `openapi-typescript` 从 `software/app/api/openapi.yaml` 生成类型，再用 `openapi-fetch` 调用。
- 服务端状态用 TanStack Query 管理，UI 状态用 zustand。
- 三个 WebSocket 各封装成一个 hook，统一处理关闭码。
- 远程画面画在 `<canvas>` 上，用 `createImageBitmap` 解码 JPEG。
- 终端用 xterm.js，国际化用 i18next（zh/en）。
- 样式用 CSS Modules 加 CSS 变量，深色主题沿用草图 `kvm-web.html` 和 `kvm-files.html`。

**Tech Stack:** Node 24、Vite 6、React 18、TypeScript 5、`openapi-typescript`、`openapi-fetch`、`@tanstack/react-query`、`zustand`、`i18next` + `react-i18next`、`@xterm/xterm` + `@xterm/addon-fit`、Vitest、`@testing-library/react`、jsdom。

**Spec:** `docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md` 第 5、6、7.2 节；契约 `software/app/api/`（openapi.yaml、README.md、schema/）。

## Global Constraints

- **契约是唯一依据。**类型从 openapi.yaml 生成，不手写接口类型；如果发现契约有问题，记到 `software/app/api/ISSUES.md`，不要私自绕过。
- **语言**：中英文两种。`config.web.language` 为 `auto` 时跟随 `navigator.language`（以 `zh` 开头就用中文，否则用英文）。界面文字全部走 i18n 的 key，错误码也映射成 key（比如 `error.endpoints_full`）。
- **开发服务器**：Vite 把 `/api`（包括 WebSocket）代理到 `http://127.0.0.1:8800`，即 mock 或 `kvmd --sim`。
- **关闭码处理**（所有 WebSocket 统一）：
  - 4401：跳到登录页。
  - 4403：提示没有权限，不重连。
  - 4404：仅终端，隐藏串口标签页。
  - 4409：提示“已被管理员断开”，不重连。
  - 1012：显示“KVM 服务重启中”，然后按 1、2、4、8 秒退避重连，最长 10 秒间隔。
  - 其他异常断开：同样退避重连。
- **会改变电脑或板子状态的操作都要先确认**：电源键、复位、强制关机、重启和关机 KVMPanel、重启 KVM 服务、断开所有会话、删除文件、弹出介质、应用 USB 改动。
- **USB 功能页**：端点预算条下方浮出一条提示，不用弹窗；“应用”按钮只在 `/api/usb/applied` 返回 `pending` 时可用；“撤销”就是把 `config.usb` PATCH 回已应用的那组。
- 构建产物输出到 `software/app/web/dist/`，供 kvmd 直接提供。

## Review Focus

1. 标签页切到后台再切回来：视频流要发 `pause`/`resume`，切回后马上恢复画面，不能黑屏。由 Task 4 测试。
2. 输入焦点在浮窗里的终端或文件管理器时，按键不能同时发给远程电脑。由 Task 4 和 Task 8 测试。
3. 上传途中网络断开或用户取消：显示失败，可以重试，列表里不留半截文件。由 Task 8 测试。
4. 会话失效（4401）时，所有打开的 WebSocket 和浮窗都要关掉，然后跳到登录页，不能死循环重连。由 Task 3 测试。
5. 浏览器语言是 zh-TW、en-GB 等变体时，按前缀正确匹配语言，缺少的 key 回退到英文。由 Task 1 测试。

---

## 文件结构

```
software/app/web/
  package.json, vite.config.ts, tsconfig.json, index.html, vitest.config.ts
  src/main.tsx, src/App.tsx                 路由：/login、/（控制台）、/workspace（新标签页模式）
  src/api/schema.d.ts                       openapi-typescript 生成，不手改
  src/api/client.ts                         openapi-fetch 实例，ApiError，errorKey()
  src/api/sockets.ts                        connectSocket()：关闭码策略和退避重连
  src/api/useEvents.ts, useStream.ts, useTerminal.ts
  src/i18n/index.ts, en.json, zh.json
  src/state/ui.ts                           zustand：菜单栏、浮窗、toast
  src/features/auth/                        LoginPage, RequireAuth, ChangePasswordDialog
  src/features/console/                     VideoCanvas, input.ts, MenuBar, StatusArea, menus/*
  src/features/settings/                    SettingsWindow, pages/*
  src/features/workspace/                   WorkspaceWindow, TerminalPane, FileManager
  src/ui/                                   Button, Switch, Slider, Select, ConfirmDialog, Toast, Window
  test/setup.ts, test/mockServer.ts         globalSetup：在随机端口启动 ../api/mock 的 buildServer
```

---

### Task 1: 脚手架、类型生成、国际化和测试环境

**Files:** 上表里的工程文件，加上 `src/api/schema.d.ts`、`src/i18n/*`、`test/*`。
**Interfaces（Produces）：**
- `npm run gen:api`：执行 `openapi-typescript ../api/openapi.yaml -o src/api/schema.d.ts`。
- `pickLanguage(setting: 'auto'|'en'|'zh', navigatorLanguages: readonly string[]): 'en'|'zh'`。
- 测试的 globalSetup 用 `buildServer()` 加上 `startPanelServer` 以外的部分，把 mock 起在随机端口，通过 `process.env.MOCK_URL` 传给测试。

- [ ] **Step 1: 写失败测试** `src/i18n/pick.test.ts`：
  - `pickLanguage('auto',['zh-TW'])` 为 `'zh'`。
  - `pickLanguage('auto',['en-GB','zh-CN'])` 为 `'en'`。
  - `pickLanguage('auto',['fr'])` 为 `'en'`。
  - `pickLanguage('zh',['en'])` 为 `'zh'`。
  - 再测一个只在 en.json 里有的 key，用 zh 渲染时回退成英文文本。
- [ ] **Step 2:** 运行 `npm test --prefix software/app/web`，预期 FAIL。
- [ ] **Step 3: 实现**脚手架、`pickLanguage`、i18n 初始化（`fallbackLng: 'en'`）和 globalSetup。
- [ ] **Step 4:** 运行测试，预期 PASS；`npm run build --prefix software/app/web` 成功。
- [ ] **Step 5: 提交** `feat(web): scaffold, generated API types, i18n`

### Task 2: API 客户端和 WebSocket 基础

**Interfaces（Produces）：**
- `api`：带 `credentials:'include'` 的 openapi-fetch 客户端。
- `class ApiError { status: number; code: string; detail?: Record<string, unknown> }`。
- `unwrap<T>(r): T`：出错时 throw `ApiError`。
- `errorKey(e: ApiError): string`，返回 `'error.'+code`。
- `connectSocket(path: string, handlers: { onOpen?, onMessage(data: string|ArrayBuffer), onClosed?(policy: ClosePolicy) }, opts?: { reconnect?: boolean }): { send(d): void; close(): void }`。
- `type ClosePolicy = 'login'|'forbidden'|'no_serial'|'disconnected'|'restarting'|'retry'`。
- `closePolicy(code: number): ClosePolicy`。
- `useEvents(): { status: StatusMessage|null; connected: boolean; banner: ClosePolicy|null; send(m: ClientMessage): void }`。

- [ ] **Step 1: 写失败测试**：
  - `closePolicy` 的映射：4401→login、4403→forbidden、4404→no_serial、4409→disconnected、1012→restarting、1006→retry。
  - 对 mock 发 `PATCH /api/config {usb:{net:true}}`，得到 `ApiError{status:409, code:'endpoints_full'}`，`errorKey` 为 `error.endpoints_full`。
  - `useEvents` 连上 mock 后 1 秒内拿到 status。
  - 收到 4409 时不重连（用假定时器断言 10 秒内没有新连接）。
  - 收到 1012 时按 1、2、4 秒重连。
- [ ] **Step 2:** 运行，预期 FAIL。**Step 3: 实现。** **Step 4:** 运行，预期 PASS。
- [ ] **Step 5: 提交** `feat(web): API client and socket policy`

### Task 3: 登录和会话

**Interfaces：**
- `<LoginPage/>`：显示错误码对应的文案。处于 `login_locked` 时显示 `retry_after_s` 倒计时，倒计时期间禁用提交按钮。
- `<RequireAuth/>`：调用 `GET /api/auth/me`。返回 401 时跳到 `/login`；`must_change_password` 为 true 时，弹出不能关闭的 `<ChangePasswordDialog/>`。
- `useSession()`：返回 `{ user, logout() }`。任何 socket 的策略变成 `login`，或任何请求返回 401，都会清理全部 socket 和浮窗，然后跳到 `/login`。

- [ ] **Step 1: 写失败测试**（RTL，对接 mock）：
  - 错误密码时显示 `error.invalid_credentials` 的文案。
  - 第 6 次尝试显示带 300 秒倒计时的锁定提示。
  - admin 首次登录弹出改密对话框，改密成功后进入控制台。
  - **在另一个会话里改密码后，当前页面的 socket 以 4401 关闭，页面跳到 /login，并且 5 秒内不再发起新连接。**
- [ ] **Step 2–4:** 运行，预期 FAIL；实现；运行，预期 PASS。
- [ ] **Step 5: 提交** `feat(web): login, forced password change, session expiry`

### Task 4: 控制台画面和输入

**Interfaces：**
- `<VideoCanvas scale={'fit'|'1:1'|'fill'} onPointer={...}/>`：订阅 `useStream()`，用 `createImageBitmap` 解码，用 `requestAnimationFrame` 绘制最新的一帧（解码中的旧帧直接丢弃）。
- `useStream()`：`document.visibilityState` 变为 hidden 时发 `{t:'pause'}`，变回 visible 时发 `{t:'resume'}`。
- `input.ts`：
  - `keyMessage(e: KeyboardEvent): ClientMessage|null`：使用 `e.code`，忽略输入法组合中的按键。
  - `absMessage(x, y, rect, buttons)`：把坐标换算到 0–65535。
  - `relMessage(dx, dy, buttons)`：裁剪到 ±127。
  - `wheelMessage(deltaY)`：裁剪到 ±127。
  - `buttonsMask(e: MouseEvent)`：按契约的位掩码返回。
- 只有焦点在 VideoCanvas 上时才捕获键盘。浮窗、对话框或输入框取得焦点时不发送按键。

- [ ] **Step 1: 写失败测试**：
  - `absMessage` 在 1920×1080 的画面上点 (960, 540) 得到 x、y 约为 32767。
  - `relMessage(500,-500,1)` 得到 dx 127、dy −127。
  - `buttonsMask` 中 middle 对应 4。
  - `keyMessage` 在 `isComposing` 时返回 null。
  - **把 visibilityState 切成 hidden 再切回 visible，依次发出 pause 和 resume。**
  - **焦点在 `<input>` 上时按 KeyA，不发送任何 ws 消息。**
  - 连接 mock 后，canvas 在 2 秒内画出至少一帧（在 jsdom 里桩掉 `createImageBitmap`，断言调用了 drawImage）。
- [ ] **Step 2–4:** 运行，预期 FAIL；实现；运行，预期 PASS。
- [ ] **Step 5: 提交** `feat(web): video canvas and HID input`

### Task 5: 菜单栏和下拉面板

**Interfaces：**
- `<MenuBar/>`：可以拖动，`config.web.menubar` 为 `auto_hide` 时 3 秒后自动隐藏。
  - 按钮依次为：画面、键盘、鼠标、镜像、文件/终端、电源（带 `status.power.on` 状态点）、WoL；状态区显示分辨率、帧率和 NUM/CAPS/SCR；最后是设置和全屏。
- 下拉面板放在 `menus/`，内容按设计文档第 5.1 节和草图 `kvm-web.html`：
  - **画面**：分辨率只读，画质、帧率上限、缩放和帧差检测都 PATCH 到 `config.video`。
  - **键盘**：
    - 粘贴文本：发 `paste`，输入框里实时显示字数，超过 65536 时禁止发送。
    - 屏幕键盘：用一个简单的布局组件实现，按键发 `key` 消息。
    - Ctrl+Alt+Del：发 `shortcut`。
    - 自定义快捷键：保存在 localStorage。
    - 键盘布局：写入 `config.hid.keyboard_layout`。
  - **鼠标**：绝对或相对模式（相对模式点击画面后进入 pointer lock）、光标样式、滚轮方向和速度、防休眠抖动。
  - **镜像**：
    - 显示 SD 卡剩余空间和 ISO 列表，可以挂载（光驱或磁盘）、弹出。
    - 上传入口打开文件工作台。
    - 可以从 URL 下载，并显示下载任务和它的暂停、恢复、取消。
    - 共享盘开关调用 `POST /api/share`。
  - **电源**：短按电源键、复位、长按强制关机（5000 ms）都要先确认；显示电源灯和硬盘灯状态。
  - **WoL**：设备列表，可以添加（MAC 格式不对时显示 `error.bad_mac`）、删除、唤醒。

- [ ] **Step 1: 写失败测试**：
  - 电源面板：点“复位”先出现确认框，取消后不发请求，确认后发出 `pressPower {button:'reset'}`。
  - 镜像面板：挂载 ubuntu ISO 后，列表里显示“已挂载”；挂载第二个时显示 `error.already_mounted`。
  - 粘贴超过 65536 个字符时，发送按钮禁用。
  - WoL 添加 `zz` 时显示 MAC 格式错误。
- [ ] **Step 2–4:** 运行，预期 FAIL；实现；运行，预期 PASS。
- [ ] **Step 5: 提交** `feat(web): menu bar and dropdown panels`

### Task 6: 设置窗口（KVM 组和系统组）

**Interfaces：**
- `<SettingsWindow page={...}/>`：左栏分组和页面列表按设计文档第 5.2 节。
- 页面通过 `useConfigSection(section)` 读写 `config`，写入时用 PATCH 并做乐观更新，失败时回滚并 toast 错误码。
- `<UsbFunctionsPage/>`：
  - 预算条显示 IN、OUT 各 5 格，用 `GET /api/usb/budget` 加本地预测：切换某一项前，先用 `usbBudget` 的同一规则在前端预测。
  - 前端那份 `usbBudget` 实现，测试与 `software/app/api/mock/test/usb.test.ts` 的用例逐条一致。
  - 超出预算时，该项变灰；点击时，在预算条下方浮出提示，2.5 秒后消失。
  - `pending` 时可以点“应用”，应用前先确认。“撤销”把 `config.usb` PATCH 回 `/api/usb/applied`。
- `<PanelPage/>`：
  - 自动亮度和亮度。
  - 离线息屏时长。
  - 阈值表：可以增删；键可以是指标 id，也可以是 `prefix.*`；删除时 PATCH null。
  - 隐藏指标多选，选项来自契约 README 列出的指标 id。
- 其余页面（通用、键鼠、画面加 EDID、镜像与共享、Function 键、存储、网络、Tailscale、账号与权限、MCP、更新、设备、关于）按契约字段一一对应。
  - 账号页：用户增删改，权限用勾选，可以设为只读，可以停用。
  - 设备页：每个动作都先确认。
  - MCP 页：只在重新生成后显示一次完整 key，并提供复制按钮。

- [ ] **Step 1: 写失败测试**：
  - 前端的 `usbBudget` 和 mock 的用例逐条一致。
  - 在 USB 页点“虚拟网卡”：开关不变，预算条下方出现 `endpoints_full` 的提示，2.5 秒后提示消失（用假定时器）。
  - 关掉绝对鼠标后，“应用”按钮可用；点“撤销”后，“应用”重新禁用。
  - 阈值页：删除 `gpu.temp` 时，发出的 PATCH 值为 null。
  - 网络页：选静态 IP 但没填 IP 就保存，提示 `invalid_body`，页面不跳。
  - MCP 页：重新生成后能看到完整 key，刷新页面后只剩 key_hint。
- [ ] **Step 2–4:** 运行，预期 FAIL；实现；运行，预期 PASS。
- [ ] **Step 5: 提交** `feat(web): settings window`

### Task 7: 浮窗组件

**Interfaces：**
- `<FloatingWindow id title initial={x,y,w,h} onClose onPopOut>`：可以拖动标题栏、拖动右下角缩放、最小化、最大化。
- `config.web.remember_float` 为 true 时，位置和大小存到 localStorage 的 `kvm.float.<id>` 键。

- [ ] **Step 1: 写失败测试**：
  - 拖动标题栏 (100,50) 后，窗口位置跟着变。
  - 窗口不能被拖出视口，至少保留 40 px 标题栏在可见范围内。
  - 关闭后重新打开，恢复上次的位置。
- [ ] **Step 2–4:** 运行，预期 FAIL；实现；运行，预期 PASS。
- [ ] **Step 5: 提交** `feat(web): floating window`

### Task 8: 文件/终端工作台

**Interfaces：**
- `<Workspace mode={'float'|'tab'}/>`：
  - 左边是 `<TerminalPane/>`：多个标签页，第一个是 `kind=local`；收到 4404 时隐藏串口标签。
  - 右边是 `<FileManager/>`，中间的分隔条可以拖动调宽度。
- 菜单栏按钮按 `config.web.workspace_open` 决定行为：浮窗模式打开 FloatingWindow，新标签页模式执行 `window.open('/workspace')`。浮窗标题栏的“⧉ 新标签页”按钮会关闭浮窗并打开新标签页。
- `<FileManager/>`：
  - 根目录可以切换：SD 卡、共享盘、系统 `/`。系统根目录只对 admin 显示。
  - 有路径导航和列表（文件夹在前）。
  - 上传：支持拖放，带进度，先在 Web Worker 里算 SHA-256 再放进 `X-Sha256`，可以取消。
  - 右键菜单：下载、重命名、删除（先确认）、挂载为光驱（仅 .iso）、在终端中打开（向当前终端发送 `cd '<path>'\r`）。
  - 错误码 `share_busy`、`in_use`、`is_dir`、`no_space` 都用 toast 显示。
- 终端标签页用 `@xterm/addon-fit`，窗口尺寸变化时发 resize。

- [ ] **Step 1: 写失败测试**：
  - 在终端输入 `ls` 回车后，屏幕上出现 `images`。
  - 焦点在终端里时按键，不会向 `/api/ws` 发送 key。
  - 上传一个小文件，列表刷新后能看到它。
  - **上传中途取消后，列表里没有这个文件，并显示“已取消”，可以重试。**
  - 在挂载着 ISO 时删除 `/images`，提示 `error.in_use`。
  - 普通 user 看不到“系统 /”这个根目录。
  - `workspace_open` 设为 `tab` 时，点按钮调用 `window.open('/workspace')`。
- [ ] **Step 2–4:** 运行，预期 FAIL；实现；运行，预期 PASS。
- [ ] **Step 5: 提交** `feat(web): files and terminal workspace`

### Task 9: 在浏览器里走查

- [ ] 启动 mock 和 `npm run dev`，在浏览器里分别用 zh 和 en 走一遍：登录、改密、控制台各菜单、设置的所有页面、浮窗和新标签页两种工作台、断开与重启横幅（用设备页的动作触发）。把发现的问题按对应任务修好，每处修复都要带测试。
- [ ] 运行 `npm run build`，产物输出到 `dist/`，然后提交 `chore(web): walkthrough fixes`
