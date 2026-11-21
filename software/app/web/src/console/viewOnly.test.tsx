import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { browserFetch, loginAs, startMock, wsFactory } from '../../test/mock';
import { configureApi } from '../api/client';
import { configureSockets } from '../api/socket';
import { useEventsContext } from '../api/EventsProvider';
import type { ClientMessage } from '../api/ws-types';
import i18n from '../i18n';
import { ConsolePage } from '../pages/ConsolePage';
import { RequireAuth } from '../session/RequireAuth';

const inputs: ClientMessage[] = [
  { t: 'key', code: 'KeyA', down: true }, { t: 'mouse_abs', x: 1, y: 2, buttons: 0 },
  { t: 'mouse_rel', dx: 1, dy: 2, buttons: 0 }, { t: 'wheel', dy: 1 },
  { t: 'paste', text: 'hello' }, { t: 'shortcut', keys: ['KeyA'] },
];

function Probe() {
  const { send } = useEventsContext();
  return <button onClick={() => {
    inputs.forEach(message => send(message));
    send({ t: 'ping' });
  }}>Send test frames</button>;
}

let mock: Awaited<ReturnType<typeof startMock>>;
afterEach(async () => {
  cleanup();
  configureApi({});
  configureSockets({});
  vi.restoreAllMocks();
  await mock?.close();
});

it('reads view-only from the authenticated account and follows live changes without losing video or ping', async () => {
  mock = await startMock();
  const admin = await loginAs(mock.url, 'admin', 'admin');
  const headers = { cookie: admin, 'content-type': 'application/json' };
  const created = await fetch(mock.url + '/api/users', { method: 'POST', headers,
    body: JSON.stringify({ name: 'observer', role: 'user', permissions: ['console'], password: 'secret1', view_only: true }) });
  expect(created.status).toBe(201);
  const cookie = await loginAs(mock.url, 'observer', 'secret1');
  configureApi({ fetch: browserFetch(mock.url, cookie).fetch });
  const sent: ClientMessage[] = [];
  let statusReady = false;
  const decode = vi.spyOn(globalThis, 'createImageBitmap');
  let deliverStatus = false;
  const factory = wsFactory(cookie);
  configureSockets({ base: mock.url, create(url) {
    const sock = factory(url);
    if (url.endsWith('/api/ws')) {
      const send = sock.send.bind(sock);
      sock.send = data => { sent.push(JSON.parse(String(data))); send(data); };
      // Hold back the initial status to verify the HTTP user alone protects the very first input.
      const onMessage = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(sock), 'onmessage')!;
      Object.defineProperty(sock, 'onmessage', { set(callback) {
        onMessage.set!.call(sock, (event: MessageEvent) => {
          if (JSON.parse(event.data).t === 'status') {
            statusReady = true;
            if (!deliverStatus) return;
          }
          callback(event);
        });
      } });
    }
    return sock;
  } });
  await i18n.changeLanguage('en');
  render(<QueryClientProvider client={new QueryClient()}><MemoryRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
    <Routes><Route element={<RequireAuth />}><Route path="/" element={<><ConsolePage /><Probe /></>} /></Route></Routes>
  </MemoryRouter></QueryClientProvider>);
  await waitFor(() => expect(statusReady).toBe(true));
  await waitFor(() => expect(decode).toHaveBeenCalled());
  const canvas = screen.getByRole('application');
  fireEvent.pointerMove(canvas, { clientX: 12, clientY: 15 });
  fireEvent.keyDown(canvas, { code: 'KeyA' });
  fireEvent.wheel(canvas, { deltaY: 100 });
  fireEvent.click(screen.getByText('Send test frames'));
  expect(sent).toEqual([{ t: 'ping' }]);
  fireEvent.click(screen.getByRole('button', { name: i18n.t('menu.keyboard') }));
  const cad = screen.getByRole('button', { name: i18n.t('keyboard.cad') });
  expect(cad).toBeDisabled();
  expect(screen.getByRole('button', { name: i18n.t('keyboard.paste') })).toBeDisabled();
  expect(screen.getByRole('button', { name: i18n.t('keyboard.osk') })).toBeDisabled();
  expect(screen.getByText(i18n.t('console.viewOnlyHint'))).toBeVisible();
  fireEvent.click(cad);
  expect(screen.queryByText(i18n.t('keyboard.cadSent'))).toBeNull();

  fireEvent.click(screen.getByRole('button', { name: i18n.t('menu.mouse') }));
  expect(screen.getByText(i18n.t('console.viewOnlyHint'))).toBeVisible();
  expect(screen.getByLabelText(i18n.t('mouse.cursor'))).toBeEnabled();
  expect(screen.queryByLabelText(i18n.t('mouse.jiggler'))).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: i18n.t('menu.keyboard') }));
  const currentCad = () => screen.getByRole('button', { name: i18n.t('keyboard.cad') });
  deliverStatus = true;
  async function changeViewOnly(value: boolean) {
    const res = await fetch(mock.url + '/api/users/observer', { method: 'PATCH', headers, body: JSON.stringify({ view_only: value }) });
    expect(res.ok).toBe(true);
  }
  await act(() => changeViewOnly(false));
  await waitFor(() => expect(currentCad()).toBeEnabled());
  fireEvent.click(currentCad());
  expect(sent.some(m => m.t === 'shortcut')).toBe(true);
  await act(() => changeViewOnly(true));
  await waitFor(() => expect(currentCad()).toBeDisabled());
  sent.length = 0;
  fireEvent.click(screen.getByText('Send test frames'));
  expect(sent).toEqual([{ t: 'ping' }]);
});
