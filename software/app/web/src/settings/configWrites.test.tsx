import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { PropsWithChildren } from 'react';
import { afterEach, expect, it } from 'vitest';
import { configureApi, type Config } from '../api/client';
import { usePatchConfig } from '../console/menus/hooks';
import { CONFIG_KEY } from '../session/useSession';
import { useConfigSection } from './useConfigSection';
import '../i18n';

afterEach(() => configureApi({}));

it('orders menu and settings writes and preserves later optimistic edits when a write fails', async () => {
  const qc = new QueryClient();
  const initial = { web: { title: 'old' }, video: { quality: 50 } } as Config;
  qc.setQueryData(CONFIG_KEY, initial);
  const pending: ((response: Response) => void)[] = [];
  const received: unknown[] = [];
  let server = initial;
  configureApi({ fetch: async (input, init) => {
    const req = new Request(input, init);
    if (req.method === 'GET') return Response.json(server);
    received.push(await req.json());
    return new Promise<Response>((resolve) => pending.push(resolve));
  } });
  const { result } = renderHook(() => ({
    web: useConfigSection('web'), menu: usePatchConfig(),
  }), { wrapper: ({ children }: PropsWithChildren) => <QueryClientProvider client={qc}>{children}</QueryClientProvider> });

  let first!: Promise<boolean>;
  let second!: Promise<unknown>;
  let third!: Promise<boolean>;
  act(() => {
    first = result.current.web.patch({ title: 'first' });
    second = result.current.menu.patch({ video: { quality: 70 } });
    third = result.current.web.patch({ title: 'latest' });
  });
  await waitFor(() => expect(received.length).toBeGreaterThan(0));
  expect(received).toEqual([{ web: { title: 'first' } }]);
  expect(qc.getQueryData(CONFIG_KEY)).toMatchObject({ web: { title: 'latest' }, video: { quality: 70 } });

  server = { ...server, web: { ...server.web, title: 'first' } };
  await act(async () => { pending[0](Response.json(server)); expect(await first).toBe(true); });
  await waitFor(() => expect(received).toHaveLength(2));
  expect(qc.getQueryData(CONFIG_KEY)).toMatchObject({ web: { title: 'latest' }, video: { quality: 70 } });

  await act(async () => {
    pending[1](Response.json({ error: 'invalid_config' }, { status: 400 }));
    await second;
  });
  await waitFor(() => expect(received).toHaveLength(3));
  expect(qc.getQueryData(CONFIG_KEY)).toMatchObject({ web: { title: 'latest' }, video: { quality: 50 } });
  server = { ...server, web: { ...server.web, title: 'latest' } };
  await act(async () => { pending[2](Response.json(server)); expect(await third).toBe(true); });
  expect(qc.getQueryData(CONFIG_KEY)).toEqual(server);
});
