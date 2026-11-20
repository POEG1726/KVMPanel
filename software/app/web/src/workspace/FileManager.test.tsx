import type {} from 'vitest/jsdom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { createServer, request, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { onLoginRequired } from '../api/authEvents';
import { api, configureApi, unwrap } from '../api/client';
import i18n from '../i18n';
import { ToastHost, useToasts } from '../ui/Toast';
import { browserFetch, loginAs, startMock } from '../../test/mock';
import { ME_KEY } from '../session/useSession';
import { FileManager } from './FileManager';

let mock: Awaited<ReturnType<typeof startMock>>;
let cookie: string;
let proxy: Server | undefined;
let uploadWindow: Window | undefined;
const t = (key: string, args = {}) => i18n.t(key, args) as string;

beforeEach(async () => {
  mock = await startMock();
  cookie = await loginAs(mock.url, 'admin', 'admin');
  configureApi({ fetch: browserFetch(mock.url, cookie).fetch });
  await i18n.changeLanguage('en');
});
afterEach(async () => {
  cleanup();
  configureApi({});
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  uploadWindow?.close();
  uploadWindow = undefined;
  if (proxy) {
    proxy.closeAllConnections();
    await new Promise<void>((done) => proxy!.close(() => done()));
    proxy = undefined;
  }
  useToasts.setState({ items: [] });
  await mock.close();
});

function mount(onOpenTerminal?: (path: string) => void) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><FileManager onOpenTerminal={onOpenTerminal} /><ToastHost /></QueryClientProvider>);
}

async function menu(name: string) {
  fireEvent.contextMenu(await screen.findByRole('button', { name }));
  return within(screen.getByRole('menu'));
}

async function entries() {
  return (await unwrap(api.GET('/api/files', { params: { query: { root: 'sd', path: '/' } } }))).entries;
}

describe('file browser', () => {
  it('navigates from root, creates and renames a folder, and opens it in the terminal', async () => {
    const cd = vi.fn();
    mount(cd);
    fireEvent.click(await screen.findByRole('button', { name: 'images' }));
    await screen.findByText('ubuntu-24.04.iso');
    fireEvent.click(screen.getByRole('button', { name: t('files.mkdir') }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: "Alice's folder" } });
    fireEvent.submit(screen.getByRole('textbox').closest('form')!);
    fireEvent.click((await menu("Alice's folder")).getByRole('menuitem', { name: t('files.openTerminal') }));
    expect(cd).toHaveBeenCalledWith("/mnt/sd/images/Alice's folder");
    fireEvent.click((await menu("Alice's folder")).getByRole('menuitem', { name: t('files.rename') }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'utilities' } });
    fireEvent.submit(screen.getByRole('textbox').closest('form')!);
    await screen.findByText('utilities');
    expect(screen.queryByText("Alice's folder")).toBeNull();
    fireEvent.click((await menu('notes.txt')).getByRole('menuitem', { name: t('files.openTerminal') }));
    expect(cd).toHaveBeenLastCalledWith('/mnt/sd/images');
    const download = (await menu('notes.txt')).getByRole('menuitem', { name: t('files.download') });
    expect(download).not.toHaveAttribute('download');
    fireEvent.click(download);
    const frame = document.querySelector('iframe')!;
    expect(frame).toHaveAttribute('sandbox', 'allow-same-origin allow-downloads');
    const url = new URL(frame.src);
    expect(url.pathname).toBe('/api/files/download');
    expect(url.searchParams.get('path')).toBe('/images/notes.txt');
    expect(url.searchParams.get('root')).toBe('sd');
    expect(await screen.findByText(/free of/)).toBeVisible();
  });

  it.each(['not_found', 'unauthenticated'] as const)('handles %s from the native streaming download', async code => {
    mount();
    fireEvent.click(await screen.findByRole('button', { name: 'images' }));
    const download = (await menu('notes.txt')).getByRole('menuitem', { name: t('files.download') });
    expect(download).not.toHaveAttribute('download');
    fireEvent.click(download);
    const frame = document.querySelector('iframe')!;
    const url = new URL(frame.src);
    if (code === 'not_found') {
      await unwrap(api.POST('/api/files/delete', { body: { root: 'sd', path: '/images/notes.txt' } }));
    }
    const response = await fetch(mock.url + url.pathname + url.search, { headers: { cookie: code === 'unauthenticated' ? '' : cookie } });
    expect(response.status).toBe(code === 'unauthenticated' ? 401 : 404);
    // jsdom doesn't navigate attachment frames. Deliver the real server error as
    // a browser JSON document; browsers display JSON in a <pre>.
    const errorDoc = new DOMParser().parseFromString('<pre></pre>', 'text/html');
    errorDoc.querySelector('pre')!.textContent = await response.text();
    Object.defineProperty(errorDoc, 'URL', { value: frame.src });
    Object.defineProperty(frame, 'contentDocument', { value: errorDoc });
    const expired = vi.fn();
    const unsubscribe = onLoginRequired(expired);
    try {
      fireEvent.load(frame);
      if (code === 'unauthenticated') expect(expired).toHaveBeenCalledOnce();
      else {
        expect(await screen.findByText(t('error.not_found'))).toBeVisible();
        expect(expired).not.toHaveBeenCalled();
      }
    } finally { unsubscribe(); }
  });

  it('asks before recursive delete; a mounted ISO prevents deleting its parent', async () => {
    mount();
    fireEvent.click(await screen.findByRole('button', { name: 'images' }));
    fireEvent.click((await menu('ubuntu-24.04.iso')).getByRole('menuitem', { name: t('files.mount') }));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: t('files.mount') }));
    await screen.findByText(t('media.mounted'));
    fireEvent.click(within(screen.getByRole('navigation', { name: t('files.path') })).getByRole('button', { name: t('settings.storage.root.sd') }));
    fireEvent.click((await menu('images')).getByRole('menuitem', { name: t('common.delete') }));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: t('common.cancel') }));
    expect((await entries()).some(e => e.name === 'images')).toBe(true);
    fireEvent.click((await menu('images')).getByRole('menuitem', { name: t('common.delete') }));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: t('common.delete') }));
    expect(await screen.findByText(t('error.in_use'))).toBeVisible();
    expect((await entries()).some(e => e.name === 'images')).toBe(true);
  });

  it('gives an admin all file actions even without explicit permission entries', async () => {
    const { user } = await unwrap(api.GET('/api/auth/me'));
    const client = new QueryClient();
    client.setQueryData(ME_KEY, { ...user, permissions: [] });
    render(<QueryClientProvider client={client}><FileManager onOpenTerminal={vi.fn()} /></QueryClientProvider>);
    fireEvent.click(await screen.findByRole('button', { name: 'images' }));
    const actions = await menu('ubuntu-24.04.iso');
    expect(actions.getByText(t('files.mount'))).toBeVisible();
    expect(actions.getByText(t('files.openTerminal'))).toBeVisible();
  });

  it('hides the system root and media/terminal actions for a files-only user', async () => {
    await unwrap(api.POST('/api/users', { body: { name: 'reader', password: 'secret1', role: 'user', permissions: ['files'] } }));
    configureApi({ fetch: browserFetch(mock.url, await loginAs(mock.url, 'reader', 'secret1')).fetch });
    mount(vi.fn());
    fireEvent.click(await screen.findByRole('button', { name: 'images' }));
    const actions = await menu('ubuntu-24.04.iso');
    expect(screen.queryByRole('button', { name: t('files.system') })).toBeNull();
    expect(actions.queryByText(t('files.mount'))).toBeNull();
    expect(actions.queryByText(t('files.openTerminal'))).toBeNull();
  });
});

// jsdom has XMLHttpRequest and FileReader but no Web Workers. Replace only the
// worker boundary; the browser XHR still streams to the actual authenticated mock.
function uploadEnvironment(origin: string) {
  // XHR captures the window's original origin. Give it its own same-origin document.
  const realm = new jsdom.constructor('', { url: origin });
  uploadWindow = realm.window;
  realm.window.document.cookie = cookie + '; Path=/';
  vi.stubGlobal('XMLHttpRequest', realm.window.XMLHttpRequest);
  vi.stubGlobal('Worker', class {
    onmessage?: (event: { data: string }) => void;
    postMessage(file: File) {
      const reader = new FileReader();
      reader.onload = () => this.onmessage?.({ data: createHash('sha256').update(new Uint8Array(reader.result as ArrayBuffer)).digest('hex') });
      reader.readAsArrayBuffer(file);
    }
    terminate() {}
  });
}

describe('verified uploads', () => {
  it('PUTs a dropped file with the SHA-256 header and refreshes the directory', async () => {
    uploadEnvironment(mock.url);
    const headers = vi.spyOn(XMLHttpRequest.prototype, 'setRequestHeader');
    mount();
    await screen.findByText('images');
    const file = new File(['bytes'], 'upload ?.txt');
    fireEvent.drop(screen.getByRole('region', { name: t('files.title') }), { dataTransfer: { files: [file] } });
    await screen.findByRole('button', { name: file.name });
    expect(headers).toHaveBeenCalledWith('X-Sha256', createHash('sha256').update('bytes').digest('hex'));
    expect((await entries()).find(e => e.name === file.name)?.size).toBe(5);
  });

  it.each(['cancel', 'network'] as const)('keeps no partial file on %s; retry stores the complete file', async (failure) => {
    let pause!: () => void;
    let disconnect!: () => void;
    let first = true;
    const partial = new Promise<void>((done) => { pause = done; });
    proxy = createServer((req, res) => {
      const upstream = request(new URL(req.url!, mock.url), { method: 'PUT', headers: { ...req.headers, cookie } }, (reply) => {
        res.writeHead(reply.statusCode!, reply.headers);
        reply.pipe(res);
      });
      upstream.on('error', () => res.destroy());
      res.on('close', () => upstream.destroy());
      if (!first) { req.pipe(upstream); return; }
      first = false;
      // Leave one PUT in flight at the mock, then cancel from the UI or drop the network.
      req.once('data', (chunk: Buffer) => upstream.write(chunk.subarray(0, 2), () => {
        disconnect = () => res.destroy();
        pause();
      }));
    });
    await new Promise<void>((done) => proxy!.listen(0, '127.0.0.1', done));
    uploadEnvironment('http://127.0.0.1:' + (proxy.address() as AddressInfo).port);
    mount();
    await screen.findByText('images');
    fireEvent.change(screen.getByLabelText(t('files.choose')), { target: { files: [new File(['abcdefg'], 'partial.bin')] } });
    await act(async () => { await partial; });
    if (failure === 'cancel') fireEvent.click(screen.getByRole('button', { name: t('common.cancel') }));
    else disconnect();
    await screen.findByText(t(failure === 'cancel' ? 'files.cancelled' : 'files.failed'));
    expect((await entries()).some(e => e.name === 'partial.bin')).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: t('files.retry') }));
    await screen.findByRole('button', { name: 'partial.bin' });
    expect((await entries()).find(e => e.name === 'partial.bin')?.size).toBe(7);
  });

  it('can cancel hashing without sending a PUT', async () => {
    uploadEnvironment(mock.url);
    const stop = vi.fn();
    vi.stubGlobal('Worker', class { postMessage() {} terminate = stop; });
    const send = vi.spyOn(XMLHttpRequest.prototype, 'send');
    mount();
    await screen.findByText('images');
    fireEvent.change(screen.getByLabelText(t('files.choose')), { target: { files: [new File(['data'], 'pending.bin')] } });
    await screen.findByText(t('files.hashing'));
    fireEvent.click(screen.getByRole('button', { name: t('common.cancel') }));
    await screen.findByText(t('files.cancelled'));
    expect(stop).toHaveBeenCalledOnce();
    expect(send).not.toHaveBeenCalled();
  });

  it('handles a 401 from the upload channel through the common session listener', async () => {
    uploadEnvironment(mock.url);
    uploadWindow!.document.cookie = 'kvm_session=expired; Path=/';
    const expired = vi.fn();
    const unsubscribe = onLoginRequired(expired);
    try {
      mount();
      await screen.findByText('images');
      fireEvent.change(screen.getByLabelText(t('files.choose')), { target: { files: [new File(['a'], 'small.bin')] } });
      await screen.findByText(t('files.failed'));
      expect(expired).toHaveBeenCalledOnce();
    } finally { unsubscribe(); }
  });

  it('displays share_busy from the exported shared volume', async () => {
    uploadEnvironment(mock.url);
    await unwrap(api.POST('/api/share', { body: { exported_to_host: true } }));
    mount();
    fireEvent.click(await screen.findByRole('button', { name: t('settings.storage.root.share') }));
    await screen.findByText('readme.txt');
    fireEvent.change(screen.getByLabelText(t('files.choose')), { target: { files: [new File(['a'], 'small.bin')] } });
    expect(await screen.findByText(t('error.share_busy'))).toBeVisible();
  });

  it('shows the contract error when an upload would replace a directory', async () => {
    uploadEnvironment(mock.url);
    mount();
    await screen.findByText('images');
    fireEvent.change(screen.getByLabelText(t('files.choose')), { target: { files: [new File(['oops'], 'images')] } });
    await waitFor(() => expect(useToasts.getState().items.some(item => item.text === t('error.is_dir'))).toBe(true));
  });
});
