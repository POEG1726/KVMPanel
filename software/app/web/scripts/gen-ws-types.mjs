// Generates src/api/ws-types.d.ts from the contract's ws.schema.json (run via `npm run gen:ws`).
import { readFileSync, writeFileSync } from 'node:fs';
import { compile } from 'json-schema-to-typescript';

const src = JSON.parse(readFileSync(new URL('../../api/schema/ws.schema.json', import.meta.url), 'utf8'));
const root = {
  title: 'WsMessages',
  type: 'object',
  additionalProperties: false,
  properties: {
    client: { $ref: '#/$defs/ClientMessage' },
    server: { $ref: '#/$defs/ServerMessage' },
  },
  $defs: src.$defs,
};
let ts = await compile(root, 'WsMessages', { bannerComment: '/* Generated from api/schema/ws.schema.json by `npm run gen:ws`. Do not edit. */', additionalProperties: false });
ts += '\nexport type StatusMessage = Status;\n';
writeFileSync(new URL('../src/api/ws-types.d.ts', import.meta.url), ts);
