# KVMPanel Web

React SPA for the KVMPanel IP-KVM. API contract: `../api/openapi.yaml`.

## Install

    npm install
    npm install --prefix ../api/mock   # needed once: tests start the mock server

## Develop

    npm run dev

Needs the mock (`npm start --prefix ../api/mock`) or `kvmd --sim` listening on
127.0.0.1:8800. Vite proxies `/api` (including WebSocket) there.

## Test

    npm test

The vitest globalSetup starts the mock on a random port and exposes it as
`process.env.MOCK_URL`.

## Build

    npm run build

Output goes to `dist/` (untracked).

## Regenerate API types

    npm run gen:api

Writes `src/api/schema.d.ts` from `../api/openapi.yaml`.

    npm run gen:ws

Regenerates the event socket types from `../api/schema/ws.schema.json`.

## 工作台与联调

MCP 设置页可维护作用域密钥、显示一次性客户端配置并查询操作日志。远程控制台收到 `status.mcp` 后显示当前 AI 操作，可写会话能直接停止。联调使用 API mock 的管理和状态接口。MCP JSON-RPC 由 `kvmd --sim` 提供。 [接口](../api/README.md#mcp)

工作台和控制台共用登录会话。浮窗最小化会保留终端连接。弹出到新标签页会建立新的终端会话。上传先在 Worker 中分块计算 SHA-256，再通过 XHR 发送文件、显示进度。板子使用普通 HTTP 时也可以计算校验值。文件管理器的“在终端中打开”使用板子的默认目录映射，可自定义根目录时的限制已记入[契约问题](../api/ISSUES.md)。

2026-11-20 使用本机 Chrome 和独立 mock 分别以 `zh-TW`、`en-GB` 走查，覆盖首次登录与改密、全部控制台菜单、15 个设置页面、两种工作台入口、终端输入、文件上传，以及设备页触发的重启和断开提示。在另一会话改密后，已打开的浮窗和终端关闭，页面回到登录页，随后 5 秒没有重连。自动化测试还会在传输途中断开连接或取消，检查服务端目录后再重试。

`kvmd` 按静态目录提供网页。构建会为 `/login/`、`/workspace/` 生成与根页面共用资源的入口。使用 `kvmd --sim --web-dir software/app/web/dist` 已验证直接访问和刷新、工作台弹出，以及上传后下载的字节一致性。
