import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import WebSocket from 'ws';
import i18n from '../i18n';
import { App } from '../App';
import { useToasts } from '../ui/Toast';
import { configureApi } from '../api/client';
import { configureSockets, type SocketFactory } from '../api/socket';
import { useUi } from '../state/ui';
import { browserFetch, changePasswordAs, loginAs, startMock } from '../../test/mock';
import { SettingsWindow } from './SettingsWindow';
import type { SettingsPageId } from './registry';

let mock: Awaited<ReturnType<typeof startMock>>;
let jar: ReturnType<typeof browserFetch>;
/** Every HTTP request the app made: `METHOD /path` plus the parsed JSON body. */
let requests: { key: string; body: unknown; sha?: string | null }[];
let connections = 0;

const t = (k: string, o: Record<string, unknown> = {}) => i18n.t(k, o) as string;

const factory: SocketFactory = (url) => {
  connections += 1;
  const ws = new WebSocket(url, { headers: { cookie: jar.cookie() } });
  ws.on('error', () => undefined);
  return ws as never;
};

function client() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

function mount(page: SettingsPageId, queryClient = client()) {
  return render(
    <QueryClientProvider client={queryClient}>
      <SettingsWindow page={page} />
    </QueryClientProvider>,
  );
}

const patches = () => requests.filter((r) => r.key === 'PATCH /api/config').map((r) => r.body);

async function setup(cookieLogin: () => Promise<string> = () => loginAs(mock.url, 'admin', 'admin')) {
  jar = browserFetch(mock.url, await cookieLogin());
}

beforeEach(async () => {
  mock = await startMock();
  requests = [];
  connections = 0;
  useToasts.setState({ items: [] });
  await setup();
  configureApi({
    fetch: async (input, init) => {
      const req = new Request(input, init);
      const text = await req.clone().text();
      requests.push({ key: `${req.method} ${new URL(req.url).pathname}`, body: text && text.startsWith('{') ? JSON.parse(text) : undefined, sha: req.headers.get('x-sha256') });
      return jar.fetch(req);
    },
  });
  configureSockets({ base: mock.url, create: factory });
  await i18n.changeLanguage('en');
});

afterEach(async () => {
  vi.useRealTimers();
  cleanup();
  configureApi({});
  configureSockets({});
  useUi.setState({ floatingOpen: false, settingsOpen: false });
  await mock.close();
});

describe('SettingsWindow', () => {
  it('lists the KVM group then the system group, in the documented order', async () => {
    mount('general');
    await screen.findByRole('navigation');
    const nav = screen.getByRole('navigation');
    const names = within(nav).getAllByRole('button').map((b) => b.textContent);
    expect(names).toEqual(
      ['general', 'input', 'video', 'media', 'panel', 'usb', 'function_key', 'storage', 'network', 'tailscale', 'accounts', 'mcp', 'update', 'device', 'about'].map((id) => t(`settings.page.${id}`)),
    );
    expect(within(nav).getByText(t('settings.group.kvm'))).toBeInTheDocument();
    expect(within(nav).getByText(t('settings.group.system'))).toBeInTheDocument();
  });

  it('opens from the menu bar state and closes on Escape', async () => {
    const onClose = vi.fn();
    render(
      <QueryClientProvider client={client()}>
        <SettingsWindow onClose={onClose} />
      </QueryClientProvider>,
    );
    fireEvent.keyDown(await screen.findByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('takes focus on open, so Escape works at once', async () => {
    mount('about');
    expect(await screen.findByRole('dialog', { name: t('menu.settings') })).toHaveFocus();
  });

  it('closeSettings also clears floatingOpen', () => {
    useUi.getState().openSettings('usb');
    expect(useUi.getState()).toMatchObject({ settingsOpen: true, floatingOpen: true });
    useUi.getState().closeSettings();
    expect(useUi.getState()).toMatchObject({ settingsOpen: false, floatingOpen: false });
  });

  it('applies the language at once after saving it', async () => {
    mount('general');
    fireEvent.change(await screen.findByLabelText(t('settings.general.language')), { target: { value: 'zh' } });
    await waitFor(() => expect(i18n.language).toBe('zh'));
    expect(patches()).toContainEqual({ web: { language: 'zh' } });
  });

  it('only offers serial settings to users with terminal permission', async () => {
    const created = await jar.fetch(new Request(mock.url + '/api/users', {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: 'viewer', password: 'secret1', role: 'user', permissions: ['console'] }),
    }));
    expect(created.status).toBe(201);
    await setup(() => loginAs(mock.url, 'viewer', 'secret1'));
    mount('input');
    await screen.findByLabelText(t('keyboard.layout'));
    expect(screen.queryByLabelText(t('settings.input.serialDevice'))).not.toBeInTheDocument();
  });

  it('shows why clearing the title failed and restores the saved value', async () => {
    mount('general');
    const title = await screen.findByLabelText(t('settings.general.title'));
    fireEvent.change(title, { target: { value: ' ' } });
    fireEvent.blur(title);
    await waitFor(() => expect(useToasts.getState().items).toContainEqual(expect.objectContaining({ text: t('error.invalid_config'), kind: 'error' })));
    expect(title).toHaveValue('KVMPanel');
  });

  it('stores the selected logo with its original MIME type and bytes', async () => {
    mount('general');
    const input = await screen.findByLabelText(t('settings.general.logo'));
    const svg = '<svg xmlns="http://www.w3.org/2000/svg"/>';
    fireEvent.change(input, { target: { files: [new File([svg], 'brand.svg', { type: 'image/svg+xml' })] } });
    await waitFor(() => expect(useToasts.getState().items).toContainEqual(expect.objectContaining({ text: t('settings.general.logoSaved') })));
    const saved = await jar.fetch(mock.url + '/api/branding/logo');
    expect(saved.headers.get('content-type')).toContain('image/svg+xml');
    expect(await saved.text()).toBe(svg);
  });

  it('rolls an optimistic change back and toasts when the server refuses it', async () => {
    mount('general');
    const title = await screen.findByLabelText(t('settings.general.title'));
    // 65 characters is over the contract's maxLength of 64.
    fireEvent.change(title, { target: { value: 'x'.repeat(65) } });
    fireEvent.blur(title);
    await waitFor(() => expect(patches()).toHaveLength(1));
    await waitFor(() => expect(useToasts.getState().items).toContainEqual(expect.objectContaining({ text: t('error.invalid_config'), kind: 'error' })));
    expect(title).toHaveValue('KVMPanel');
  });
});

describe('USB functions page', () => {
  const sw = (name: string) => screen.findByRole('switch', { name: t(`settings.usb.feature.${name}`) });

  it('displays the dedicated telemetry IN cost when absolute mouse is disabled', async () => {
    mount('usb');
    fireEvent.click(await sw('mouse_abs'));
    const telemetry = await sw('telemetry');
    expect(telemetry.parentElement!.parentElement).toHaveTextContent('IN 1 · OUT 0');
  });

  it('shows an endpoints_full tip under the budget bar for a switch that does not fit, and hides it after 2.5 s', async () => {
    mount('usb');
    const net = await sw('net');
    await waitFor(() => expect(screen.getByRole('img', { name: 'IN 4/5' })).toBeInTheDocument());
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });

    fireEvent.click(net);
    const tip = screen.getByTestId('usb-tip');
    expect(tip).toHaveTextContent(t('error.endpoints_full', { in_missing: 1, out_missing: 0 }));
    expect(net).toHaveAttribute('aria-checked', 'false');
    // The tip sits inside the budget bars' container, directly below them, not in a dialog.
    expect(tip.parentElement).toContainElement(screen.getByRole('img', { name: 'OUT 1/5' }));
    expect(screen.getAllByRole('dialog')).toHaveLength(1); // only the settings window itself, no modal for the tip

    act(() => void vi.advanceTimersByTime(2400));
    expect(screen.getByTestId('usb-tip')).toBeInTheDocument();
    act(() => void vi.advanceTimersByTime(100));
    expect(screen.queryByTestId('usb-tip')).not.toBeInTheDocument();
    expect(patches()).toHaveLength(0);
  });

  it('refuses turning the merge off when that would overflow: tip, no PATCH, greyed switch', async () => {
    await jar.fetch(new Request(`${mock.url}/api/config`, { method: 'PATCH', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ usb: { mouse_merge: true, net: true } }) }));
    mount('usb');
    const merge = await sw('mouse_merge');
    await waitFor(() => expect(screen.getByRole('img', { name: 'IN 5/5' })).toBeInTheDocument());
    expect(merge).toHaveAttribute('aria-disabled', 'true');
    fireEvent.click(merge);
    expect(screen.getByTestId('usb-tip')).toHaveTextContent(t('error.endpoints_full', { in_missing: 1, out_missing: 0 }));
    expect(merge).toHaveAttribute('aria-checked', 'true');
    expect(patches()).toHaveLength(0);
  });

  it('enables Apply only while a change is pending, and Revert patches the applied set back', async () => {
    mount('usb');
    const apply = await screen.findByRole('button', { name: t('settings.usb.apply') });
    const revert = screen.getByRole('button', { name: t('settings.usb.revert') });
    await waitFor(() => expect(requests.some((r) => r.key === 'GET /api/usb/applied')).toBe(true));
    expect(apply).toBeDisabled();

    const abs = await sw('mouse_abs');
    fireEvent.click(abs);
    await waitFor(() => expect(apply).toBeEnabled());
    expect(abs).toHaveAttribute('aria-checked', 'false');

    fireEvent.click(revert);
    await waitFor(() => expect(apply).toBeDisabled());
    expect(abs).toHaveAttribute('aria-checked', 'true');
    expect(patches()).toContainEqual({ usb: expect.objectContaining({ mouse_abs: true, keyboard: true, storage: true }) });
  });

  it('asks before applying, then applies and disables Apply again', async () => {
    mount('usb');
    fireEvent.click(await sw('mouse_abs'));
    const apply = screen.getByRole('button', { name: t('settings.usb.apply') });
    await waitFor(() => expect(apply).toBeEnabled());

    fireEvent.click(apply);
    const dialog = await screen.findByRole('dialog', { name: t('settings.usb.applyTitle') });
    fireEvent.click(within(dialog).getByRole('button', { name: t('common.cancel') }));
    expect(requests.some((r) => r.key === 'POST /api/usb/apply')).toBe(false);

    fireEvent.click(apply);
    fireEvent.click(within(await screen.findByRole('dialog', { name: t('settings.usb.applyTitle') })).getByRole('button', { name: t('settings.usb.apply') }));
    await waitFor(() => expect(requests.some((r) => r.key === 'POST /api/usb/apply')).toBe(true));
    await waitFor(() => expect(apply).toBeDisabled());
  });
});

describe('Panel page', () => {
  it('deletes a threshold by PATCHing null', async () => {
    mount('panel');
    fireEvent.change(await screen.findByLabelText(t('settings.panel.thresholdKey')), { target: { value: 'gpu.temp' } });
    fireEvent.change(screen.getByLabelText(t('settings.panel.thresholdNew')), { target: { value: '80' } });
    fireEvent.click(screen.getByRole('button', { name: t('settings.panel.thresholdAdd') }));
    const remove = await screen.findByRole('button', { name: t('settings.panel.thresholdDelete', { key: 'gpu.temp' }) });
    await waitFor(() => expect(patches()).toContainEqual({ panel: { thresholds: { 'gpu.temp': 80 } } }));

    fireEvent.click(remove);
    await waitFor(() => expect(screen.queryByRole('button', { name: t('settings.panel.thresholdDelete', { key: 'gpu.temp' }) })).not.toBeInTheDocument());
    await waitFor(() => expect(patches()).toContainEqual({ panel: { thresholds: { 'gpu.temp': null } } }));
    // The prefix pattern is a normal row too.
    expect(screen.getByRole('button', { name: t('settings.panel.thresholdDelete', { key: 'mem.dimm.*' }) })).toBeInTheDocument();
  });

  it('hides a metric by sending the whole hidden list', async () => {
    mount('panel');
    fireEvent.click(await screen.findByRole('checkbox', { name: 'gpu.temp' }));
    await waitFor(() => expect(patches()).toContainEqual({ panel: { hidden_metrics: ['gpu.temp'] } }));
  });
});

describe('Video page EDID', () => {
  it('restores the board EDID after confirming reset, even with an unsaved draft', async () => {
    mount('video');
    const hex = await screen.findByLabelText(t('settings.video.edidHex')) as HTMLTextAreaElement;
    await waitFor(() => expect(hex.value.length).toBeGreaterThanOrEqual(256));
    const original = hex.value;
    fireEvent.change(hex, { target: { value: 'bad draft' } });
    fireEvent.click(screen.getByRole('button', { name: t('settings.video.edidReset') }));
    const dlg = await screen.findByRole('dialog', { name: t('settings.video.edidResetTitle') });
    fireEvent.click(within(dlg).getByRole('button', { name: t('settings.video.edidReset') }));
    await waitFor(() => expect(hex).toHaveValue(original));
    fireEvent.change(hex, { target: { value: original.slice(0, -2) + (original.endsWith('00') ? '01' : '00') } });
    fireEvent.click(screen.getByRole('button', { name: t('settings.video.edidApply') }));
    const apply = await screen.findByRole('dialog', { name: t('settings.video.edidApplyTitle') });
    fireEvent.click(within(apply).getByRole('button', { name: t('settings.video.edidApply') }));
    await screen.findByText(t('settings.video.presetCustom'));
  });
});

describe('Network page', () => {
  it('shows invalid_body for a static address without an IP and stays on the page', async () => {
    mount('network');
    fireEvent.change(await screen.findByLabelText(t('settings.network.mode')), { target: { value: 'static' } });
    fireEvent.click(screen.getByRole('button', { name: t('settings.network.save') }));
    const title = t('settings.network.confirmTitle');
    fireEvent.click(within(await screen.findByRole('dialog', { name: title })).getByRole('button', { name: t('settings.network.save') }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.invalid_body'));
    expect(screen.getByRole('heading', { name: t('settings.page.network') })).toBeInTheDocument();
    expect(screen.getByLabelText(t('settings.network.mode'))).toHaveValue('static');
  });
});

describe('Network confirm', () => {
  it('sends nothing when the save confirmation is cancelled', async () => {
    mount('network');
    fireEvent.click(await screen.findByRole('button', { name: t('settings.network.save') }));
    fireEvent.click(within(await screen.findByRole('dialog', { name: t('settings.network.confirmTitle') })).getByRole('button', { name: t('common.cancel') }));
    expect(requests.some((r) => r.key === 'PUT /api/network')).toBe(false);
  });
});

describe('Update page offline upload', () => {
  const pick = async () => {
    mount('update');
    const input = await screen.findByLabelText(t('settings.update.offlineFile'));
    fireEvent.change(input, { target: { files: [new File(['pkg'], 'u.tar')] } });
    return screen.findByRole('dialog', { name: t('settings.update.installTitle') });
  };

  it('asks first; cancel uploads nothing', async () => {
    fireEvent.click(within(await pick()).getByRole('button', { name: t('common.cancel') }));
    expect(requests.some((r) => r.key === 'PUT /api/update/offline')).toBe(false);
  });

  it('uploads with the SHA-256 header after confirming', async () => {
    fireEvent.click(within(await pick()).getByRole('button', { name: t('settings.update.install') }));
    await screen.findByRole('progressbar');
    expect(requests.find((r) => r.key === 'PUT /api/update/offline')?.sha).toBe('e0ce1a7f3f76d7bfda0ffdae6e0226aa63b3009c4c4a770c321876aecad0a6c1');
  });
});

describe('Update job progress', () => {
  it.each(['online', 'offline'] as const)('recovers the pending %s submission and follows its job after reopening', async (kind) => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    let submits = 0;
    configureApi({ fetch: async (input, init) => {
      const req = new Request(input, init);
      if (req.method === 'POST' || req.method === 'PUT') {
        submits++;
        const response = await jar.fetch(req);
        await gate;
        return response;
      }
      return jar.fetch(req);
    } });
    const qc = client();
    mount('update', qc);
    if (kind === 'offline') {
      fireEvent.change(await screen.findByLabelText(t('settings.update.offlineFile')), { target: { files: [new File(['pkg'], 'update.tar')] } });
    } else {
      fireEvent.click(await screen.findByRole('button', { name: t('settings.update.install') }));
    }
    const confirm = await screen.findByRole('dialog', { name: t('settings.update.installTitle') });
    fireEvent.click(within(confirm).getByRole('button', { name: t('settings.update.install') }));
    await waitFor(() => expect(submits).toBe(1));
    cleanup();
    mount('update', qc);
    try {
      expect(await screen.findByRole('button', { name: t('settings.update.install') })).toBeDisabled();
      expect(screen.getByLabelText(t('settings.update.offlineFile'))).toBeDisabled();
    } finally { release(); }
    await screen.findByRole('progressbar');
    expect(submits).toBe(1);
  });

  it('does not start an offline PUT with a new session after hashing finishes', async () => {
    const file = new File(['pkg'], 'update.tar');
    const chunk = file.slice();
    const data = await chunk.arrayBuffer();
    let finish!: (bytes: ArrayBuffer) => void;
    const hashRead = vi.spyOn(chunk, 'arrayBuffer').mockReturnValue(new Promise<ArrayBuffer>((resolve) => { finish = resolve; }));
    vi.spyOn(file, 'slice').mockReturnValue(chunk);
    const qc = client();
    mount('update', qc);
    fireEvent.change(await screen.findByLabelText(t('settings.update.offlineFile')), { target: { files: [file] } });
    const confirm = await screen.findByRole('dialog', { name: t('settings.update.installTitle') });
    fireEvent.click(within(confirm).getByRole('button', { name: t('settings.update.install') }));
    await waitFor(() => expect(hashRead).toHaveBeenCalled());
    cleanup();
    qc.clear();
    mount('update', qc);
    expect(await screen.findByRole('button', { name: t('settings.update.install') })).toBeEnabled();
    let uploads = 0;
    configureApi({ fetch: async (input, init) => {
      const req = new Request(input, init);
      if (req.method === 'PUT') uploads++;
      return jar.fetch(req);
    } });
    await act(async () => { finish(data); });
    expect(uploads).toBe(0);
  });

  it('discards an update response that arrives after logout and login', async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    let waiting = false;
    let polls = 0;
    configureApi({ fetch: async (input, init) => {
      const req = new Request(input, init);
      const path = new URL(req.url).pathname;
      if (path === '/api/update/apply') {
        waiting = true;
        await gate;
        return Response.json({ job: { id: 'old-job', state: 'running', progress: 0 } });
      }
      if (path.includes('/api/update/jobs/')) polls++;
      return jar.fetch(req);
    } });
    const qc = client();
    mount('update', qc);
    fireEvent.click(await screen.findByRole('button', { name: t('settings.update.install') }));
    const confirm = await screen.findByRole('dialog', { name: t('settings.update.installTitle') });
    fireEvent.click(within(confirm).getByRole('button', { name: t('settings.update.install') }));
    await waitFor(() => expect(waiting).toBe(true));
    cleanup();
    qc.clear();
    mount('update', qc);
    await screen.findByRole('button', { name: t('settings.update.install') });
    await act(async () => { release(); });
    cleanup();
    mount('update', qc);
    expect(await screen.findByRole('button', { name: t('settings.update.install') })).toBeEnabled();
    expect(polls).toBe(0);
  });

  it('resumes tracking the installation on returning to settings and announces completion once per job', async () => {
    const qc = client();
    mount('update', qc);
    fireEvent.click(await screen.findByRole('button', { name: t('settings.update.install') }));
    const confirm = await screen.findByRole('dialog', { name: t('settings.update.installTitle') });
    fireEvent.click(within(confirm).getByRole('button', { name: t('settings.update.install') }));
    await screen.findByRole('progressbar');
    const jobRequest = requests.find((r) => r.key.startsWith('GET /api/update/jobs/'))!;
    const id = jobRequest.key.split('/').pop()!;
    cleanup();

    // The mock leaves jobs running. Simulate kvmd finishing while the settings window was closed.
    configureApi({ fetch: async (input, init) => {
      const req = new Request(input, init);
      return new URL(req.url).pathname === `/api/update/jobs/${id}`
        ? Response.json({ job: { id, state: 'done', progress: 100 } })
        : jar.fetch(req);
    } });
    mount('update', qc);
    await screen.findByText(t('settings.update.state.done'));
    const notifications = () => useToasts.getState().items.filter((m) =>
      [i18n.getFixedT('en')('settings.update.installed'), i18n.getFixedT('zh')('settings.update.installed')].includes(m.text));
    await waitFor(() => expect(notifications()).toHaveLength(1));
    await act(() => i18n.changeLanguage('zh').then(() => {}));
    expect(notifications()).toHaveLength(1);
  });
});

describe('MCP page', () => {
  function createDraft(name: string) {
    fireEvent.click(screen.getByRole('button', { name: t('settings.mcp.create') }));
    fireEvent.change(screen.getByRole('textbox', { name: t('settings.mcp.name') }), { target: { value: name } });
  }

  it.each([
    ['en', 'Human input priority', 'Handback delay'],
    ['zh', '人工输入优先', '交还等待时间'],
  ])('shows and saves the MCP input policy in %s', async (language, priorityLabel, waitLabel) => {
    await i18n.changeLanguage(language);
    mount('mcp');
    const priority = screen.getByRole('switch', { name: priorityLabel });
    await waitFor(() => expect(priority).toBeEnabled());
    expect(priority).toBeChecked();
    fireEvent.click(priority);
    await waitFor(() => expect(priority).not.toBeChecked());
    await waitFor(() => expect(priority).toBeEnabled());

    const wait = screen.getByRole('spinbutton', { name: waitLabel });
    expect(wait).toHaveValue(2);
    fireEvent.change(wait, { target: { value: '61' } });
    fireEvent.blur(wait);
    expect(wait).toHaveValue(2);
    wait.focus();
    fireEvent.change(wait, { target: { value: '12' } });
    fireEvent.keyDown(wait, { key: 'Enter' });
    await waitFor(() => expect(screen.getByRole('spinbutton', { name: waitLabel })).toHaveValue(12));
    expect(requests.filter(({ key }) => key === 'PATCH /api/mcp').map(({ body }) => body)).toEqual([
      { human_input_priority: false }, { human_input_wait_s: 12 },
    ]);

    cleanup();
    mount('mcp');
    await waitFor(() => expect(screen.getByRole('spinbutton', { name: waitLabel })).toHaveValue(12));
    expect(screen.getByRole('switch', { name: priorityLabel })).not.toBeChecked();
  });

  it('creates a scoped key, edits it, and confirms deletion', async () => {
    mount('mcp');
    const button = await screen.findByRole('button', { name: t('settings.mcp.create') });
    await waitFor(() => expect(button).toBeEnabled());
    createDraft('observer');
    fireEvent.click(screen.getByRole('checkbox', { name: t('settings.mcp.scope.input') }));
    fireEvent.click(screen.getByRole('button', { name: t('settings.mcp.save') }));
    await screen.findByTestId('mcp-key');
    expect(requests.find((r) => r.key === 'POST /api/mcp/keys')?.body).toEqual({ name: 'observer', scopes: ['observe', 'input'] });
    const edit = await screen.findByRole('button', { name: t('settings.mcp.edit') });
    await waitFor(() => expect(edit).toBeEnabled());
    fireEvent.click(edit);
    fireEvent.change(screen.getByRole('textbox', { name: t('settings.mcp.name') }), { target: { value: 'renamed' } });
    fireEvent.click(screen.getByRole('checkbox', { name: t('settings.mcp.scope.input') }));
    fireEvent.click(screen.getByRole('button', { name: t('settings.mcp.save') }));
    const keys = screen.getByRole('heading', { name: t('settings.mcp.keys') }).closest('section')!;
    await within(keys).findByText('renamed');
    expect(requests.find((r) => r.key.startsWith('PATCH /api/mcp/keys/'))?.body).toEqual({ name: 'renamed', scopes: ['observe'] });
    const remove = screen.getByRole('button', { name: t('settings.mcp.revoke') });
    await waitFor(() => expect(remove).toBeEnabled());
    fireEvent.click(remove);
    const dialog = screen.getByRole('dialog', { name: t('settings.mcp.revokeTitle') });
    fireEvent.click(within(dialog).getByRole('button', { name: t('settings.mcp.revoke') }));
    await screen.findByText(t('settings.mcp.noKey'));
    expect(screen.queryByTestId('mcp-key')).not.toBeInTheDocument();
  });

  it('disables key creation and shows the server error when the key state could not be read', async () => {
    configureApi({ fetch: async (input, init) => {
      const req = new Request(input, init);
      return new URL(req.url).pathname === '/api/mcp'
        ? Response.json({ error: 'forbidden' }, { status: 403 }) : jar.fetch(req);
    } });
    mount('mcp');
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.forbidden'));
    expect(screen.getByRole('button', { name: t('settings.mcp.create') })).toBeDisabled();
  });

  it('copies the one-time key on HTTP, then only shows its hint on a fresh mount', async () => {
    mount('mcp');
    const button = await screen.findByRole('button', { name: t('settings.mcp.create') });
    await waitFor(() => expect(button).toBeEnabled());
    createDraft('observer');
    fireEvent.click(screen.getByRole('button', { name: t('settings.mcp.save') }));
    const key = (await screen.findByTestId('mcp-key')).textContent!;
    expect(key).toMatch(/^kvp_/);
    let copied = '';
    const copy = vi.fn(() => { copied = window.getSelection()?.toString() ?? ''; return true; });
    Object.defineProperty(document, 'execCommand', { configurable: true, value: copy });
    try {
      fireEvent.click(screen.getByRole('button', { name: t('settings.mcp.copy') }));
      await waitFor(() => expect(copy).toHaveBeenCalledWith('copy'));
      expect(copied).toBe(key);
    } finally {
      Reflect.deleteProperty(document, 'execCommand');
    }
    cleanup();
    mount('mcp');
    await screen.findByText((text) => text.startsWith(t('settings.mcp.keyHint', { hint: key.slice(-4) })));
    expect(screen.queryByTestId('mcp-key')).not.toBeInTheDocument();
    expect(document.body.textContent).not.toContain(key);
  });
});

describe('Device page', () => {
  it('asks before every action: cancel sends nothing, confirm sends it', async () => {
    mount('device');
    const restart = await screen.findByRole('button', { name: t('settings.device.action.restart_kvm') });
    fireEvent.click(restart);
    const title = t('settings.device.confirm.restart_kvm.title');
    fireEvent.click(within(await screen.findByRole('dialog', { name: title })).getByRole('button', { name: t('common.cancel') }));
    expect(requests.some((r) => r.key === 'POST /api/device/action')).toBe(false);

    fireEvent.click(restart);
    fireEvent.click(within(await screen.findByRole('dialog', { name: title })).getByRole('button', { name: t('settings.device.action.restart_kvm') }));
    await waitFor(() => expect(requests).toContainEqual(expect.objectContaining({ key: 'POST /api/device/action', body: { action: 'restart_kvm' } })));
  });

  it('warns that only the panel can turn the KVM service on again before switching it off', async () => {
    mount('device');
    fireEvent.click(await screen.findByRole('button', { name: t('settings.device.kvmDisable') }));
    const dialog = await screen.findByRole('dialog', { name: t('settings.device.confirm.kvm_off.title') });
    expect(dialog).toHaveTextContent(t('settings.device.confirm.kvm_off.message'));
    expect(patches()).toHaveLength(0);
    fireEvent.click(within(dialog).getByRole('button', { name: t('common.cancel') }));
    expect(patches()).toHaveLength(0);
    fireEvent.click(screen.getByRole('button', { name: t('settings.device.kvmDisable') }));
    const accepted = await screen.findByRole('dialog', { name: t('settings.device.confirm.kvm_off.title') });
    fireEvent.click(within(accepted).getByRole('button', { name: t('settings.device.kvmDisable') }));
    await waitFor(() => expect(patches()).toEqual([{ kvm: { enabled: false } }]));
  });
});

describe('Accounts page', () => {
  it('creates an account with chosen permissions and disables it', async () => {
    mount('accounts');
    fireEvent.change(await screen.findByLabelText(t('settings.accounts.newName')), { target: { value: 'viewer' } });
    fireEvent.change(screen.getByLabelText(t('settings.accounts.newUserPassword')), { target: { value: 'secret1' } });
    fireEvent.click(screen.getByRole('checkbox', { name: `${t('settings.accounts.add')}: ${t('settings.accounts.perm.files')}` }));
    fireEvent.click(screen.getByRole('switch', { name: `${t('settings.accounts.add')}: ${t('settings.accounts.viewOnly')}` }));
    fireEvent.click(screen.getByRole('button', { name: t('settings.accounts.create') }));
    const disable = await screen.findByRole('switch', { name: `viewer: ${t('settings.accounts.disabled')}` });
    expect(requests).toContainEqual(expect.objectContaining({ key: 'POST /api/users', body: { name: 'viewer', password: 'secret1', role: 'user', permissions: ['console', 'files'], view_only: true } }));

    fireEvent.click(disable);
    await waitFor(() => expect(screen.getByRole('switch', { name: `viewer: ${t('settings.accounts.disabled')}` })).toHaveAttribute('aria-checked', 'true'));
  });
});

describe('own password change flag', () => {
  it('has no timer until the request has succeeded', async () => {
    const a = await import('../api/authEvents');
    vi.useFakeTimers();
    a.beginOwnPasswordChange();
    vi.advanceTimersByTime(60_000);
    expect(a.ownPasswordChangeActive()).toBe(true);
    a.settleOwnPasswordChange();
    vi.advanceTimersByTime(5000);
    expect(a.ownPasswordChangeActive()).toBe(false);
  });
});

describe('own password change', () => {
  function Where() {
    return <div data-testid="where">{useLocation().pathname}</div>;
  }

  it('reconnects the sockets after the 4401 instead of leaving for /login', async () => {
    const cookie = await loginAs(mock.url, 'admin', 'admin');
    const next = await changePasswordAs(mock.url, cookie, 'admin', 'secret1');
    jar = browserFetch(mock.url, next);
    render(
      <QueryClientProvider client={client()}>
        <MemoryRouter initialEntries={['/']}>
          <App />
          <Where />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    await screen.findByRole('toolbar');
    await waitFor(() => expect(connections).toBeGreaterThanOrEqual(2));
    const before = connections;

    act(() => useUi.getState().openSettings('accounts'));
    fireEvent.change(await screen.findByLabelText(t('changePassword.old')), { target: { value: 'secret1' } });
    fireEvent.change(screen.getByLabelText(t('changePassword.new')), { target: { value: 'secret2' } });
    fireEvent.change(screen.getByLabelText(t('changePassword.confirm')), { target: { value: 'secret2' } });
    fireEvent.click(screen.getByRole('button', { name: t('changePassword.submit') }));

    await waitFor(() => expect(requests.some((r) => r.key === 'POST /api/users/me/password')).toBe(true));
    // Both sockets (events, stream) come back with the re-issued cookie.
    await waitFor(() => expect(connections).toBeGreaterThanOrEqual(before + 2), { timeout: 6000 });
    expect(screen.getByTestId('where')).toHaveTextContent('/');
    expect(screen.getByRole('toolbar')).toBeInTheDocument();
  });

  it('a wrong current password shows the error and does not suppress a later real logout', async () => {
    mount('accounts');
    fireEvent.change(await screen.findByLabelText(t('changePassword.old')), { target: { value: 'nope' } });
    fireEvent.change(screen.getByLabelText(t('changePassword.new')), { target: { value: 'secret2' } });
    fireEvent.change(screen.getByLabelText(t('changePassword.confirm')), { target: { value: 'secret2' } });
    fireEvent.click(screen.getByRole('button', { name: t('changePassword.submit') }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.wrong_password'));
    const { ownPasswordChangeActive } = await import('../api/authEvents');
    expect(ownPasswordChangeActive()).toBe(false);
  });
});

it('keeps keyboard focus inside the settings dialog at both ends of the tab order', async () => {
  mount('about');
  const dialog = screen.getByRole('dialog');
  const close = within(dialog).getByRole('button', { name: t('common.close') });
  const link = await within(dialog).findByRole('link');
  close.focus();
  fireEvent.keyDown(close, { key: 'Tab', shiftKey: true });
  expect(link).toHaveFocus();
  fireEvent.keyDown(link, { key: 'Tab' });
  expect(close).toHaveFocus();
});
