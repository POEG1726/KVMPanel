import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { App } from '../App';
import { configureSockets, type WebSocketLike } from '../api/socket';
import { configureApi } from '../api/client';
import { closeAllFloating } from '../state/ui';
import { CONFIG_KEY, ME_KEY } from '../session/useSession';
import i18n from '../i18n';

afterEach(() => { cleanup(); closeAllFloating(); configureSockets({}); configureApi({}); vi.useRealTimers(); });

function consoleView(viewOnly = false) {
  const sockets = new Map<string, WebSocketLike>();
  configureSockets({ create(url) {
    const socket = { readyState: 1, binaryType: '', send: vi.fn(), close() {}, onopen: null, onmessage: null, onclose: null };
    sockets.set(new URL(url).pathname, socket);
    return socket;
  } });
  const client = new QueryClient();
  client.setQueryData(ME_KEY, { role: 'admin', permissions: [], must_change_password: false, view_only: viewOnly });
  client.setQueryData(CONFIG_KEY, { hid: {}, video: { scale: 'fit' }, web: { title: 'Lab KVM', language: 'en', menubar: 'auto_hide' } });
  render(<QueryClientProvider client={client}><MemoryRouter><App /></MemoryRouter></QueryClientProvider>);
  return sockets.get('/api/ws')!;
}

it.each([false, true])('shows MCP activity and only offers stop to writable sessions (view only: %s)', async (viewOnly) => {
  const request = vi.fn(async (_request: Request) => new Response(null, { status: 204 }));
  configureApi({ fetch: request });
  const ws = consoleView(viewOnly);
  const status = { t: 'status', power: { on: true }, sessions: [], stream: { fps: 0, kbps: 0 } };
  act(() => ws.onmessage?.({ data: JSON.stringify({
    ...status, mcp: { active: true, key_name: 'diagnostics', tool: 'boot_watch' },
  }) }));
  expect(screen.getByText('diagnostics')).toBeVisible();
  const stop = screen.queryByRole('button', { name: i18n.t('console.mcpStop') });
  if (viewOnly) {
    expect(stop).not.toBeInTheDocument();
  } else {
    fireEvent.click(stop!);
    await waitFor(() => expect(request).toHaveBeenCalled());
    expect(request.mock.calls).toHaveLength(1);
    const [sent] = request.mock.lastCall!;
    expect(sent.method).toBe('POST');
    expect(new URL(sent.url).pathname).toBe('/api/mcp/stop');
  }
  act(() => ws.onmessage?.({ data: JSON.stringify({
    ...status, mcp: { active: false, key_name: null, tool: null },
  }) }));
  expect(screen.queryByText('diagnostics')).not.toBeInTheDocument();
});

it.each([[4409, 'disconnected'], [1012, 'restarting'], [4403, 'forbidden']] as const)('renders the %i banner even when settings cover the console', (code, policy) => {
  const ws = consoleView();
  fireEvent.click(screen.getByRole('button', { name: i18n.t('menu.settings') }));
  act(() => ws.onclose?.({ code }));
  expect(screen.getByText(i18n.t('connection.' + policy))).toBeVisible();
});

it('uses the configured document title, offers the uploaded logo, and measures latency from pong', () => {
  vi.useFakeTimers();
  const ws = consoleView();
  expect(document.title).toBe('Lab KVM');
  expect(document.querySelector('img[src^="/api/branding/logo"]')).not.toBeNull();
  act(() => { ws.onopen?.(undefined as never); vi.advanceTimersByTime(3000); });
  expect(ws.send).toHaveBeenCalledWith('{"t":"ping"}');
  act(() => { vi.advanceTimersByTime(29); ws.onmessage?.({ data: '{"t":"pong"}' }); });
  expect(screen.getByText(/29 ms/)).toBeVisible();
});

it('lets a keyboard user reveal an auto-hidden toolbar and holds it open while it has focus', () => {
  vi.useFakeTimers();
  consoleView();
  const toolbar = screen.getByRole('toolbar');
  act(() => vi.advanceTimersByTime(3100));
  expect(toolbar).toHaveAttribute('inert');
  const reveal = screen.getByRole('button', { name: i18n.t('menu.bar') });
  fireEvent.keyDown(reveal, { key: 'Enter' });
  expect(toolbar).not.toHaveAttribute('inert');
  expect(toolbar.contains(document.activeElement)).toBe(true);
  act(() => vi.advanceTimersByTime(3100));
  expect(toolbar).not.toHaveAttribute('inert');
});
