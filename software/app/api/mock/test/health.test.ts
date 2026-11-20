import { describe, expect, it } from 'vitest';
import { buildServer } from '../src/server.js';
import { validateResponse } from '../src/contract.js';

describe('GET /api/health', () => {
  it('returns ok and the contract version without login', async () => {
    const app = await buildServer();
    const res = await app.inject({ method: 'GET', url: '/api/health' });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(body).toEqual({ ok: true, version: '0.1.0' });
    expect(() => validateResponse('getHealth', 200, body)).not.toThrow();
    await app.close();
  });

  it('rejects a body that does not match the contract', () => {
    expect(() => validateResponse('getHealth', 200, { ok: 'yes' })).toThrow();
  });
});
