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

工作台和控制台共用登录会话。浮窗最小化会保留终端连接。弹出到新标签页会建立新的终端会话。上传先在 Worker 中分块计算 SHA-256，再通过 XHR 发送文件、显示进度。板子使用普通 HTTP 时也可以计算校验值。文件管理器的“在终端中打开”使用板子的默认目录映射，可自定义根目录时的限制已记入[契约问题](../api/ISSUES.md)。

2026-11-20 使用本机 Chrome 和独立 mock 分别以 `zh-TW`、`en-GB` 走查，覆盖首次登录与改密、全部控制台菜单、15 个设置页面、两种工作台入口、终端输入、文件上传，以及设备页触发的重启和断开提示。在另一会话改密后，已打开的浮窗和终端关闭，页面回到登录页，随后 5 秒没有重连。自动化测试还会在传输途中断开连接或取消，检查服务端目录后再重试。
