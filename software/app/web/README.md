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
