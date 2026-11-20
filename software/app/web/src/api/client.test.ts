import { describe, expect, it } from 'vitest';
import { ApiError, createApi, errorKey, unwrap } from './client';
import { loginCookie, mockUrl } from '../../test/mock';

describe('api client', () => {
  // patchConfig's request body is untyped in the contract (Record<string, never>), hence the cast.
  it('turns a 409 endpoints_full into an ApiError with an i18n key', async () => {
    const api = createApi({ baseUrl: mockUrl(), headers: { cookie: await loginCookie() } });
    const err = await unwrap(api.PATCH('/api/config', { body: { usb: { net: true } } as never })).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    const e = err as ApiError;
    expect(e.status).toBe(409);
    expect(e.code).toBe('endpoints_full');
    expect(errorKey(e)).toBe('error.endpoints_full');
  });

  it('returns the data of a successful call', async () => {
    const api = createApi({ baseUrl: mockUrl(), headers: { cookie: await loginCookie() } });
    const cfg = await unwrap(api.GET('/api/config'));
    expect(cfg).toHaveProperty('usb');
  });
});
