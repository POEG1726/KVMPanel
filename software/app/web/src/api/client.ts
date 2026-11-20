import createClient from 'openapi-fetch';
import type { paths } from './schema';

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

export function createApi(opts: ApiOptions = {}) {
  return createClient<paths>({ baseUrl: opts.baseUrl ?? '', credentials: 'include', headers: opts.headers });
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
    throw new ApiError(r.response.status, body.error ?? 'unknown', body.detail);
  }
  return r.data as T;
}

export const errorKey = (e: ApiError): string => `error.${e.code}`;
