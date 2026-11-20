import createClient from 'openapi-fetch';
import type { components, paths } from './schema';
import { emitLoginRequired } from './authEvents';

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    public detail?: Record<string, unknown>,
  ) {
    super(code);
    this.name = 'ApiError';
  }
}

export interface ApiOptions {
  /** Empty (same origin) in the browser; tests point it at the mock server. */
  baseUrl?: string;
  /** Extra headers, e.g. a Cookie header for Node tests. */
  headers?: Record<string, string>;
}

let customFetch: typeof fetch | undefined;

/** Test hook: route the shared `api` through another fetch (e.g. one with a cookie jar). Call with `{}` to reset. */
export function configureApi(next: { fetch?: typeof fetch }): void {
  customFetch = next.fetch;
}

export function createApi(opts: ApiOptions = {}) {
  return createClient<paths>({
    // An absolute origin keeps `Request` construction valid outside the browser (jsdom tests).
    baseUrl: opts.baseUrl ?? (typeof location === 'undefined' ? '' : location.origin),
    credentials: 'include',
    headers: opts.headers,
    fetch: (req) => (customFetch ?? globalThis.fetch)(req),
  });
}

export const api = createApi();

interface ApiResult<T> {
  data?: T;
  error?: unknown;
  response: Response;
}

/** Returns the response data or throws an ApiError built from the `{error, detail?}` body. */
export async function unwrap<T>(result: PromiseLike<ApiResult<T>>): Promise<T> {
  const r = await result;
  if (r.error !== undefined || !r.response.ok) {
    const body = (typeof r.error === 'object' && r.error !== null ? r.error : {}) as { error?: string; detail?: Record<string, unknown> };
    const err = new ApiError(r.response.status, body.error ?? 'unknown', body.detail);
    // A 401 from login itself is a wrong password, not an expired session.
    if (err.status === 401 && err.code !== 'invalid_credentials') emitLoginRequired();
    throw err;
  }
  return r.data as T;
}

export const errorKey = (e: ApiError): string => `error.${e.code}`;

type DeepPartial<T> = T extends object ? { [K in keyof T]?: DeepPartial<T[K]> } : T;
type Config = components['schemas']['Config'];

/** PATCH /api/config. The contract leaves the body untyped; this is the one place it is cast. */
export async function patchConfig(patch: DeepPartial<Config>): Promise<Config> {
  return unwrap(api.PATCH('/api/config', { body: patch as never }));
}
