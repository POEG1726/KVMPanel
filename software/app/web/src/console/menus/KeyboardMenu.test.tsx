import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import i18n from '../../i18n';
import { EventsProvider } from '../../api/EventsProvider';
import { configureSockets, type SocketFactory, type WebSocketLike } from '../../api/socket';
import { ToastHost, useToasts } from '../../ui/Toast';
import { KeyboardMenu } from './KeyboardMenu';
import { PASTE_ACK_MS } from './PasteProvider';

const t = (k: string, o: Record<string, unknown> = {}) => i18n.t(k, o) as string;

let sock: WebSocketLike & { sent: string[] };

const factory: SocketFactory = () => {
  const s = {
    sent: [] as string[],
    readyState: 1,
    binaryType: '',
    send: (d: unknown) => void s.sent.push(String(d)),
    close() {},
    onopen: null,
    onmessage: null,
    onclose: null,
  } as unknown as typeof sock;
  sock = s;
  setTimeout(() => s.onopen?.(undefined as never));
  return s;
};

const serverSays = (m: object) => act(async () => void sock.onmessage?.({ data: JSON.stringify(m) } as never));
const sleep = (ms: number) => act(async () => void (await new Promise((r) => setTimeout(r, ms))));

function PanelToggle() {
  const [open, setOpen] = useState(true);
  return <><button onClick={() => setOpen(v => !v)}>Toggle panel</button>{open && <KeyboardMenu />}</>;
}

function mount() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <EventsProvider>
        <PanelToggle />
        <ToastHost />
      </EventsProvider>
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  configureSockets({ base: 'http://x', create: factory });
  useToasts.setState({ items: [] });
  await i18n.changeLanguage('en');
});
afterEach(() => {
  cleanup();
  configureSockets({});
});

const openPaste = (value: string) => {
  fireEvent.click(screen.getByRole('button', { name: t('keyboard.paste') }));
  fireEvent.change(screen.getByLabelText(t('keyboard.pasteText')), { target: { value } });
};

describe('paste limit', () => {
  it('counts UTF-8 bytes: 21845 x CJK fits, 21846 x CJK (65538 bytes) is disabled', async () => {
    mount();
    await sleep(10);
    openPaste('中'.repeat(21845));
    expect(screen.getByText(t('keyboard.pasteCount', { n: 65535, max: 65536 }))).toBeInTheDocument();
    expect(screen.getByRole('button', { name: t('keyboard.pasteSend') })).toBeEnabled();
    fireEvent.change(screen.getByLabelText(t('keyboard.pasteText')), { target: { value: '中'.repeat(21846) } });
    expect(screen.getByText(t('keyboard.pasteCount', { n: 65538, max: 65536 }))).toBeInTheDocument();
    expect(screen.getByRole('button', { name: t('keyboard.pasteSend') })).toBeDisabled();
  });
});

describe('paste result', () => {
  it('shows unmappable and never says "sent" when the server rejects the paste', async () => {
    mount();
    await sleep(10);
    openPaste('héllo ☃');
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.pasteSend') }));
    expect(JSON.parse(sock.sent[0])).toEqual({ t: 'paste', text: 'héllo ☃' });
    await serverSays({ t: 'error', error: 'unmappable' });
    expect(screen.getAllByRole('alert')[0]).toHaveTextContent(t('error.unmappable'));
    await sleep(PASTE_ACK_MS + 200);
    expect(screen.queryByText(t('keyboard.pasteSent'))).toBeNull();
    expect(screen.getByLabelText(t('keyboard.pasteText'))).toHaveValue('héllo ☃'); // kept for editing
  });

  it('reports "sent" only after the server stayed quiet for a second', async () => {
    mount();
    await sleep(10);
    openPaste('hello');
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.pasteSend') }));
    expect(screen.queryByText(t('keyboard.pasteSent'))).toBeNull();
    await sleep(PASTE_ACK_MS + 200);
    expect(screen.getByText(t('keyboard.pasteSent'))).toBeInTheDocument();
    expect(screen.getByLabelText(t('keyboard.pasteText'))).toHaveValue('');
  });
});

it('keeps an edited draft and reports the paste result after closing the keyboard panel', async () => {
  mount();
  await sleep(10);
  openPaste('original');
  fireEvent.click(screen.getByRole('button', { name: t('keyboard.pasteSend') }));
  fireEvent.change(screen.getByLabelText(t('keyboard.pasteText')), { target: { value: 'next draft' } });
  fireEvent.click(screen.getByText('Toggle panel'));
  await sleep(PASTE_ACK_MS + 50);
  expect(screen.getByText(t('keyboard.pasteSent'))).toBeInTheDocument();
  fireEvent.click(screen.getByText('Toggle panel'));
  fireEvent.click(screen.getByRole('button', { name: t('keyboard.paste') }));
  expect(screen.getByLabelText(t('keyboard.pasteText'))).toHaveValue('next draft');
});

it('translates bad_message after leaving the keyboard panel', async () => {
  mount();
  await sleep(10);
  fireEvent.click(screen.getByText('Toggle panel'));
  await serverSays({ t: 'error', error: 'bad_message' });
  expect(i18n.exists('error.bad_message', { lng: 'en' })).toBe(true);
  expect(i18n.exists('error.bad_message', { lng: 'zh', fallbackLng: false })).toBe(true);
  expect(screen.getByText(t('error.bad_message'))).toBeInTheDocument();
});

it('records modified Tab while a plain Tab can leave', () => {
  mount();
  fireEvent.click(screen.getByRole('button', { name: t('keyboard.shortcuts') }));
  const rec = screen.getByLabelText(t('keyboard.shortcutKeys'));
  fireEvent.keyDown(rec, { code: 'ControlLeft', ctrlKey: true });
  expect(fireEvent.keyDown(rec, { code: 'Tab', key: 'Tab', ctrlKey: true })).toBe(false);
  expect(screen.getByText('Tab')).toBeInTheDocument();
  expect(fireEvent.keyDown(rec, { code: 'Tab', key: 'Tab' })).toBe(true);
});
