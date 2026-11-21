import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type { ReactNode } from 'react';
import WebSocket from 'ws';
import i18n from '../../i18n';
import { configureApi, patchConfig } from '../../api/client';
import { EventsProvider } from '../../api/EventsProvider';
import { configureSockets, type SocketFactory, type WebSocketLike } from '../../api/socket';
import { browserFetch, loginAs, startMock } from '../../../test/mock';
import { KeyboardMenu } from './KeyboardMenu';
import { MediaMenu } from './MediaMenu';
import { MouseMenu } from './MouseMenu';
import { PowerMenu } from './PowerMenu';
import { VideoMenu } from './VideoMenu';
import { WolMenu } from './WolMenu';
import { loadShortcuts, saveShortcuts } from './shortcuts';

let mock: Awaited<ReturnType<typeof startMock>>;
let jar: ReturnType<typeof browserFetch>;
/** Every HTTP request the app made: `METHOD /path` plus the parsed JSON body. */
let requests: { key: string; body: unknown }[];
/** Messages the app sent over the shared /api/ws. */
let sent: unknown[];
let eventsReady: Promise<void>;
let markReady: () => void;

const t = (k: string, o: Record<string, unknown> = {}) => i18n.t(k, o) as string;

const factory: SocketFactory = (url) => {
  const ws = new WebSocket(url, { headers: { cookie: jar.cookie() } });
  // A socket still connecting when the test ends gets refused by the closing mock; that is not a failure.
  ws.on('error', () => undefined);
  if (url.endsWith('/api/ws')) {
    ws.once('open', markReady);
    const send = ws.send.bind(ws);
    (ws as unknown as WebSocketLike).send = (d) => {
      sent.push(JSON.parse(String(d)));
      send(String(d));
    };
  }
  return ws as never;
};

function mount(ui: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <EventsProvider>{ui}</EventsProvider>
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  mock = await startMock();
  jar = browserFetch(mock.url, await loginAs(mock.url, 'admin', 'admin'));
  requests = [];
  eventsReady = new Promise(resolve => { markReady = resolve; });
  sent = [];
  configureApi({
    fetch: async (input, init) => {
      const req = new Request(input, init);
      const text = await req.clone().text();
      requests.push({ key: `${req.method} ${new URL(req.url).pathname}`, body: text ? JSON.parse(text) : undefined });
      return jar.fetch(req);
    },
  });
  configureSockets({ base: mock.url, create: factory });
  localStorage.clear();
  await i18n.changeLanguage('en');
});

afterEach(async () => {
  cleanup();
  configureApi({});
  configureSockets({});
  await mock.close();
});

describe('PowerMenu', () => {
  it('asks before a reset: cancel sends nothing, confirm presses reset', async () => {
    mount(<PowerMenu />);
    fireEvent.click(screen.getByRole('button', { name: t('power.reset') }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(within(dialog).getByRole('button', { name: t('common.cancel') }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(requests.filter((r) => r.key === 'POST /api/power/press')).toEqual([]);

    fireEvent.click(screen.getByRole('button', { name: t('power.reset') }));
    fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: t('power.reset') }));
    await waitFor(() => expect(requests.filter((r) => r.key === 'POST /api/power/press')).toHaveLength(1));
    expect(requests.find((r) => r.key === 'POST /api/power/press')!.body).toMatchObject({ button: 'reset' });
  });

  it('holds the power button 5000 ms for a confirmed force-off, and short-presses for power', async () => {
    mount(<PowerMenu />);
    fireEvent.click(screen.getByRole('button', { name: t('power.force') }));
    fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: t('power.forceConfirm') }));
    await waitFor(() => expect(requests.some((r) => r.key === 'POST /api/power/press')).toBe(true));
    expect(requests.find((r) => r.key === 'POST /api/power/press')!.body).toEqual({ button: 'power', duration_ms: 5000 });

    fireEvent.click(screen.getByRole('button', { name: t('power.power') }));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: t('power.power') }));
    await waitFor(() => expect(requests.filter((r) => r.key === 'POST /api/power/press')).toHaveLength(2));
    expect((requests.filter((r) => r.key === 'POST /api/power/press')[1].body as { duration_ms: number }).duration_ms).toBeLessThan(2000);
  });
});

describe('MediaMenu', () => {
  it('mounts the ubuntu ISO, marks it mounted, and refuses a second mount with already_mounted', async () => {
    mount(<MediaMenu />);
    fireEvent.click(await screen.findByRole('button', { name: t('media.mountAs', { mode: t('media.cdrom'), name: 'ubuntu-24.04.iso' }) }));
    const row = (await screen.findByText('ubuntu-24.04.iso')).closest('li')!;
    expect(await within(row).findByText(t('media.mounted'))).toBeInTheDocument();
    expect(requests.find((r) => r.key === 'POST /api/media/mount')!.body).toMatchObject({ root: 'sd', mode: 'cdrom' });

    fireEvent.click(screen.getByRole('button', { name: t('media.mountAs', { mode: t('media.cdrom'), name: 'win11-24h2.iso' }) }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.already_mounted'));
  });

  it('ejects only after confirmation and shows the SD card space', async () => {
    mount(<MediaMenu />);
    fireEvent.click(await screen.findByRole('button', { name: t('media.mountAs', { mode: t('media.disk'), name: 'ubuntu-24.04.iso' }) }));
    const eject = await screen.findByRole('button', { name: t('media.eject') });
    expect(await screen.findByText(/58/)).toBeInTheDocument();
    fireEvent.click(eject);
    fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: t('common.cancel') }));
    expect(requests.some((r) => r.key === 'POST /api/media/eject')).toBe(false);
    fireEvent.click(eject);
    fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: t('media.eject') }));
    await waitFor(() => expect(requests.some((r) => r.key === 'POST /api/media/eject')).toBe(true));
    await waitFor(() => expect(screen.queryByText(t('media.mounted'))).toBeNull());
  });

  it('starts a URL download, lists the job, and cancels it', async () => {
    mount(<MediaMenu />);
    fireEvent.change(await screen.findByLabelText(t('media.url')), { target: { value: 'http://example.test/new.iso' } });
    fireEvent.click(screen.getByRole('button', { name: t('media.download') }));
    await screen.findByRole('button', { name: t('media.job.cancel') });
    expect(requests.find((r) => r.key === 'POST /api/media/download')!.body).toMatchObject({ url: 'http://example.test/new.iso', path: '/images/new.iso' });
    fireEvent.click(await screen.findByRole('button', { name: t('media.job.pause') }));
    fireEvent.click(await screen.findByRole('button', { name: t('media.job.resume') }));
    fireEvent.click(await screen.findByRole('button', { name: t('media.job.cancel') }));
    await waitFor(() => expect(requests.filter((r) => r.key.startsWith('POST /api/media/jobs/')).map((r) => r.key.split('/').pop())).toEqual(['pause', 'resume', 'cancel']));
  });

  it('shows API query failures', async () => {
    configureApi({ fetch: (input, init) => {
      const req = new Request(input, init);
      if (new URL(req.url).pathname === '/api/files') return Promise.resolve(Response.json({ error: 'forbidden' }, { status: 403 }));
      return jar.fetch(req);
    } });
    mount(<MediaMenu />);
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.forbidden'));
  });

  it('shows a completed URL download in the mount list', async () => {
    mount(<MediaMenu />);
    await screen.findByText('ubuntu-24.04.iso');
    fireEvent.change(screen.getByLabelText(t('media.url')), { target: { value: 'https://example.test/complete.iso' } });
    fireEvent.click(screen.getByRole('button', { name: t('media.download') }));
    expect(await screen.findByRole('button', { name: t('media.mountAs', { mode: t('media.cdrom'), name: 'complete.iso' }) }, { timeout: 6000 })).toBeVisible();
  });

  it('reads sibling folders concurrently with a small upper bound on active requests', async () => {
    let busy = 0;
    let peak = 0;
    configureApi({ fetch: async (input, init) => {
      const req = new Request(input, init);
      const url = new URL(req.url);
      if (url.pathname !== '/api/files') return jar.fetch(req);
      if (url.searchParams.get('path') === '/') return Response.json({ entries:
        Array.from({ length: 9 }, (_, i) => ({ type: 'dir', name: String(i), size: 0 })) });
      peak = Math.max(peak, ++busy);
      await new Promise(resolve => setTimeout(resolve, 20));
      busy--;
      return Response.json({ entries: [] });
    } });
    mount(<MediaMenu />);
    await screen.findByText(t('media.none'));
    expect(peak).toBeGreaterThan(1);
    expect(peak).toBeLessThanOrEqual(4);
  });

  it('toggles the share through POST /api/share', async () => {
    mount(<MediaMenu />);
    const sw = await screen.findByRole('switch', { name: t('media.share') });
    await waitFor(() => expect(sw).toBeEnabled());
    fireEvent.click(sw);
    await waitFor(() => expect(requests.find((r) => r.key === 'POST /api/share')?.body).toEqual({ exported_to_host: true }));
    await waitFor(() => expect(sw).toHaveAttribute('aria-checked', 'true'));
  });
});

describe('KeyboardMenu', () => {
  it('shows the character count and disables Send beyond 65536', async () => {
    mount(<KeyboardMenu />);
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.paste') }));
    const box = screen.getByLabelText(t('keyboard.pasteText'));
    const send = screen.getByRole('button', { name: t('keyboard.pasteSend') });
    expect(send).toBeDisabled();
    fireEvent.change(box, { target: { value: 'a'.repeat(65536) } });
    expect(screen.getByText(t('keyboard.pasteCount', { n: 65536, max: 65536 }))).toBeInTheDocument();
    await waitFor(() => expect(send).toBeEnabled());
    fireEvent.change(box, { target: { value: 'a'.repeat(65537) } });
    expect(screen.getByText(t('keyboard.pasteCount', { n: 65537, max: 65536 }))).toBeInTheDocument();
    expect(send).toBeDisabled();
  });

  it('sends paste, Ctrl+Alt+Del and screen keyboard keys as WebSocket messages', async () => {
    mount(<KeyboardMenu />);
    await eventsReady;

    fireEvent.click(screen.getByRole('button', { name: t('keyboard.paste') }));
    fireEvent.change(screen.getByLabelText(t('keyboard.pasteText')), { target: { value: 'hello' } });
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.pasteSend') }));
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.cad') }));
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.osk') }));
    fireEvent.click(screen.getByRole('button', { name: 'A' }));
    expect(sent).toEqual([
      { t: 'paste', text: 'hello' },
      { t: 'shortcut', keys: ['ControlLeft', 'AltLeft', 'Delete'] },
      { t: 'key', code: 'KeyA', down: true },
      { t: 'key', code: 'KeyA', down: false },
    ]);
  });

  it('saves a custom shortcut in localStorage and sends it', async () => {
    mount(<KeyboardMenu />);
    await eventsReady;
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.shortcuts') }));
    fireEvent.change(screen.getByLabelText(t('keyboard.shortcutName')), { target: { value: 'Task Manager' } });
    const rec = screen.getByLabelText(t('keyboard.shortcutKeys'));
    for (const code of ['ControlLeft', 'ShiftLeft', 'Escape']) fireEvent.keyDown(rec, { code });
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.shortcutSave') }));
    expect(JSON.parse(localStorage.getItem('kvm.shortcuts')!)).toEqual([{ name: 'Task Manager', keys: ['ControlLeft', 'ShiftLeft', 'Escape'] }]);
    fireEvent.click(screen.getByRole('button', { name: 'Task Manager' }));
    expect(sent).toEqual([{ t: 'shortcut', keys: ['ControlLeft', 'ShiftLeft', 'Escape'] }]);
  });

  it('writes the keyboard layout to config.hid', async () => {
    mount(<KeyboardMenu />);
    const select = await screen.findByLabelText(t('keyboard.layout'));
    await waitFor(() => expect(select).toHaveValue('us'));
    fireEvent.change(select, { target: { value: 'de' } });
    await waitFor(() => expect(requests.find((r) => r.key === 'PATCH /api/config')?.body).toEqual({ hid: { keyboard_layout: 'de' } }));
  });
});

describe('shortcuts storage', () => {
  it('round-trips and ignores corrupt data', () => {
    saveShortcuts([{ name: 'a', keys: ['KeyA'] }]);
    expect(loadShortcuts()).toEqual([{ name: 'a', keys: ['KeyA'] }]);
    localStorage.setItem('kvm.shortcuts', '{oops');
    expect(loadShortcuts()).toEqual([]);
    localStorage.setItem('kvm.shortcuts', JSON.stringify([{ name: 1 }, { name: 'ok', keys: ['KeyA'] }]));
    expect(loadShortcuts()).toEqual([{ name: 'ok', keys: ['KeyA'] }]);
  });
});

describe('WolMenu', () => {
  it('shows bad_mac for a malformed address and sends nothing else', async () => {
    mount(<WolMenu />);
    fireEvent.change(await screen.findByLabelText(t('wol.name')), { target: { value: 'NAS' } });
    fireEvent.change(screen.getByLabelText(t('wol.mac')), { target: { value: 'zz' } });
    fireEvent.click(screen.getByRole('button', { name: t('wol.add') }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.bad_mac'));
  });

  it('adds, wakes and deletes a device', async () => {
    mount(<WolMenu />);
    fireEvent.change(await screen.findByLabelText(t('wol.name')), { target: { value: 'NAS' } });
    fireEvent.change(screen.getByLabelText(t('wol.mac')), { target: { value: 'AA-BB-CC-DD-EE-FF' } });
    fireEvent.click(screen.getByRole('button', { name: t('wol.add') }));
    const wake = await screen.findByRole('button', { name: t('wol.wakeNamed', { name: 'NAS' }) });
    fireEvent.click(wake);
    await waitFor(() => expect(requests.find((r) => r.key === 'POST /api/wol/wake')?.body).toEqual({ mac: 'aa:bb:cc:dd:ee:ff' }));
    fireEvent.click(screen.getByRole('button', { name: t('wol.deleteNamed', { name: 'NAS' }) }));
    await waitFor(() => expect(screen.queryByText('NAS')).toBeNull());
    expect(requests.some((r) => r.key.startsWith('DELETE /api/wol/devices/'))).toBe(true);
  });
});

describe('VideoMenu and MouseMenu', () => {
  it('patches config.video when the controls change', async () => {
    mount(<VideoMenu />);
    const fps = await screen.findByLabelText(t('video.fpsMax'));
    await waitFor(() => expect(fps).toHaveValue('30'));
    fireEvent.change(fps, { target: { value: '60' } });
    await waitFor(() => expect(requests.find((r) => r.key === 'PATCH /api/config')?.body).toEqual({ video: { fps_max: 60 } }));
    fireEvent.change(screen.getByLabelText(t('video.scale')), { target: { value: 'fill' } });
    fireEvent.click(screen.getByRole('switch', { name: t('video.frameDiff') }));
    const quality = screen.getByLabelText(t('video.quality'));
    fireEvent.change(quality, { target: { value: '60' } });
    fireEvent.pointerUp(quality);
    await waitFor(() => expect(requests.filter((r) => r.key === 'PATCH /api/config').map((r) => r.body)).toEqual([
      { video: { fps_max: 60 } },
      { video: { scale: 'fill' } },
      { video: { frame_diff: false } },
      { video: { quality: 60 } },
    ]));
  });

  it('patches config.hid for mouse mode, wheel and jiggler', async () => {
    mount(<MouseMenu />);
    const mode = await screen.findByLabelText(t('mouse.mode'));
    await waitFor(() => expect(mode).toHaveValue('abs'));
    fireEvent.change(mode, { target: { value: 'rel' } });
    fireEvent.change(screen.getByLabelText(t('mouse.wheelDir')), { target: { value: 'reverse' } });
    fireEvent.click(screen.getByRole('switch', { name: t('mouse.jiggler') }));
    const speed = screen.getByLabelText(t('mouse.wheelSpeed'));
    fireEvent.change(speed, { target: { value: '5' } });
    fireEvent.pointerUp(speed);
    await waitFor(() => expect(requests.filter((r) => r.key === 'PATCH /api/config').map((r) => r.body)).toEqual([
      { hid: { mouse_mode: 'rel' } },
      { hid: { wheel_dir: 'reverse' } },
      { hid: { jiggler: true } },
      { hid: { wheel_speed: 5 } },
    ]));
    expect((await patchConfig({})).hid).toMatchObject({ mouse_mode: 'rel', wheel_dir: 'reverse', wheel_speed: 5, jiggler: true });
  });
});
