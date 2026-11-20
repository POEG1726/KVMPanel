import { createRef } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { EventsProvider } from '../api/EventsProvider';
import { configureSockets } from '../api/socket';
import { VideoCanvas } from '../console/VideoCanvas';
import { loginAs, startMock, wsFactory } from '../../test/mock';
import { CONFIG_KEY } from '../session/useSession';
import '../i18n';
import { TerminalPane, type TerminalActions } from './TerminalPane';

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    input = document.createElement('textarea');
    output = document.createElement('pre');
    open(el: HTMLElement) { el.append(this.input, this.output); }
    focus() { this.input.focus(); }
    loadAddon() {}
    write(data: Uint8Array) { this.output.textContent += new TextDecoder().decode(data); }
    onBinary() { return { dispose() {} }; }
    onData(cb: (v: string) => void) {
      this.input.oninput = () => cb(this.input.value.replace('\n', '\r'));
      return { dispose() {} };
    }
    dispose() { this.input.remove(); this.output.remove(); }
  },
}));
vi.mock('@xterm/addon-fit', () => ({ FitAddon: class { fit() {} } }));

let mock: Awaited<ReturnType<typeof startMock>>;
let factory: ReturnType<typeof wsFactory>;
beforeEach(async () => {
  mock = await startMock();
  factory = wsFactory(await loginAs(mock.url, 'admin', 'admin'));
});
afterEach(async () => { cleanup(); configureSockets({}); await mock.close(); });

it('sends terminal input to the mock PTY, fits it, and keeps remote HID silent', async () => {
  const frames: { url: string; data: unknown }[] = [];
  configureSockets({ base: mock.url, create: (url) => {
    const socket = factory(url);
    const send = socket.send.bind(socket);
    socket.send = data => { frames.push({ url, data }); send(data); };
    return socket;
  } });
  const qc = new QueryClient();
  qc.setQueryData(CONFIG_KEY, { hid: {} });
  const actions = createRef<TerminalActions>();
  render(<QueryClientProvider client={qc}><EventsProvider>
    <VideoCanvas scale="fit" /><TerminalPane ref={actions} serial={{ device: null, baud: 115200 }} />
  </EventsProvider></QueryClientProvider>);
  await screen.findByText(/mock shell/);
  const input = screen.getByRole('textbox');
  input.focus();
  fireEvent.keyDown(input, { code: 'KeyL' });
  fireEvent.input(input, { target: { value: 'ls\n' } });
  await screen.findByText(/data\s+etc\s+images\s+root/);
  expect(frames.filter(f => f.url.endsWith('/api/ws') && String(f.data).includes('"key"'))).toEqual([]);
  expect(frames.some(f => f.url.includes('/api/terminal?kind=local') && String(f.data).includes('"resize"'))).toBe(true);
  act(() => actions.current!.openDirectory("/mnt/sd/it's here"));
  await waitFor(() => expect(frames.some(f => typeof f.data !== 'string' && new TextDecoder().decode(f.data as Uint8Array) === "cd '/mnt/sd/it'\\''s here'\r")).toBe(true));
  expect(actions.current!.openDirectory('/mnt/sd/bad\rname')).toBe(false);
});

it('hides a stale configured serial tab on 4404 and keeps the local shell', async () => {
  configureSockets({ base: mock.url, create: factory });
  render(<TerminalPane serial={{ device: '/dev/ttyMissing', baud: 115200 }} />);
  await screen.findByText(/mock shell/);
  fireEvent.click(screen.getByRole('tab', { name: /ttyMissing/ }));
  await waitFor(() => expect(screen.queryByRole('tab', { name: /ttyMissing/ })).toBeNull());
  expect(screen.getByRole('tab', { selected: true })).toBeVisible();
});

it('returns to the local terminal after a serial port or baud change', async () => {
  configureSockets({ base: mock.url, create: () => ({
    readyState: 1, binaryType: '', send() {}, close() {}, onopen: null, onclose: null, onmessage: null,
  }) });
  const { rerender } = render(<TerminalPane serial={{ device: '/dev/ttyS1', baud: 115200 }} />);
  fireEvent.click(screen.getByRole('tab', { name: /ttyS1/ }));
  rerender(<TerminalPane serial={{ device: '/dev/ttyS2', baud: 115200 }} />);
  expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('Local shell');
  expect(screen.getByRole('tabpanel')).toBeVisible();
  fireEvent.click(screen.getByRole('tab', { name: /ttyS2/ }));
  rerender(<TerminalPane serial={{ device: '/dev/ttyS2', baud: 9600 }} />);
  expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('Local shell');
  expect(screen.getByRole('tabpanel')).toBeVisible();
});
