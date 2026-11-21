import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import i18n from '../i18n';
import { ME_KEY, CONFIG_KEY, useSession } from './useSession';
import { App } from '../App';
import { configureApi, patchConfig } from '../api/client';
import { configureSockets, type SocketFactory } from '../api/socket';
import { useUi } from '../state/ui';
import WebSocket from 'ws';
import { ToastHost } from '../ui';
import { browserFetch, changePasswordAs, loginAs, startMock } from '../../test/mock';

let mock: Awaited<ReturnType<typeof startMock>>;
let jar: ReturnType<typeof browserFetch>;
/** /api/ws plus /api/stream. */
let connections = 0;
let opened = 0;
const disconnected: Promise<void>[] = [];

const countingFactory: SocketFactory = (url) => {
  connections += 1;
  const socket = new WebSocket(url, { headers: { cookie: jar.cookie() } });
  socket.once('open', () => { opened++; });
  disconnected.push(new Promise(resolve => socket.once('close', () => resolve())));
  return socket as never;
};

function Where() {
  return <div data-testid="where">{useLocation().pathname}</div>;
}

function mount(path = '/') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <App />
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return client;
}

const t = (k: string, o: Record<string, unknown> = {}) => i18n.t(k, o) as string;

function signIn(username: string, password: string) {
  fireEvent.change(screen.getByLabelText(t('login.username')), { target: { value: username } });
  fireEvent.change(screen.getByLabelText(t('login.password')), { target: { value: password } });
  fireEvent.click(screen.getByRole('button', { name: t('login.submit') }));
}

beforeEach(async () => {
  mock = await startMock();
  jar = browserFetch(mock.url);
  configureApi({ fetch: jar.fetch });
  configureSockets({ base: mock.url, create: countingFactory });
  connections = 0;
  opened = 0;
  await i18n.changeLanguage('en');
});

afterEach(async () => {
  cleanup();
  configureApi({});
  configureSockets({});
  useUi.setState({ floatingOpen: false });
  // Close the server after even a just-created stream has finished its handshake and cleanup.
  await Promise.all(disconnected.splice(0));
  await mock.close();
});

describe('login', () => {
  it('redirects to /login when not signed in', async () => {
    mount('/');
    expect(await screen.findByRole('button', { name: t('login.submit') })).toBeInTheDocument();
    expect(screen.getByTestId('where')).toHaveTextContent('/login');
  });

  it('shows the invalid_credentials message for a wrong password', async () => {
    mount('/login');
    signIn('admin', 'nope');
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.invalid_credentials'));
  });

  it('shows the server lockout error when it omits the optional countdown detail', async () => {
    configureApi({ fetch: async () => Response.json({ error: 'login_locked' }, { status: 429 }) });
    mount('/login');
    signIn('admin', 'admin');
    expect(await screen.findByRole('alert')).toHaveTextContent(t('error.login_locked'));
  });

  it('locks out on the 6th attempt with a 300 s countdown and disabled submit', async () => {
    mount('/login');
    for (let i = 0; i < 5; i++) {
      signIn('admin', 'nope');
      await waitFor(() => expect(screen.getByRole('button', { name: t('login.submit') })).toBeEnabled());
      await screen.findByRole('alert');
    }
    signIn('admin', 'nope');
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(t('login.locked', { seconds: 300 })));
    expect(screen.getByRole('button', { name: t('login.submit') })).toBeDisabled();
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(t('login.locked', { seconds: 299 })), { timeout: 2500 });
  });

  it('forces a password change on first admin login, then opens the console', async () => {
    const requests: string[] = [];
    configureApi({ fetch: async (input, init) => {
      const req = new Request(input, init);
      requests.push(new URL(req.url).pathname);
      return jar.fetch(req);
    } });
    mount('/login');
    signIn('admin', 'admin');
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).queryByRole('button', { name: /close/i })).toBeNull();
    // A config request with the pre-change cookie could return 401 after the password rotates.
    expect(requests).not.toContain('/api/config');
    const formField = within(dialog).getByLabelText(t('changePassword.old'));
    formField.focus();
    fireEvent.keyDown(formField, { key: 'Escape' });
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText(t('changePassword.old')), { target: { value: 'admin' } });
    fireEvent.change(within(dialog).getByLabelText(t('changePassword.new')), { target: { value: 'secret1' } });
    fireEvent.change(within(dialog).getByLabelText(t('changePassword.confirm')), { target: { value: 'secret1' } });
    fireEvent.click(within(dialog).getByRole('button', { name: t('changePassword.submit') }));
    expect(await screen.findByRole('heading', { name: t('console.title') })).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByTestId('where')).toHaveTextContent('/');
    await waitFor(() => expect(opened).toBe(2));
  });

  it('applies config.web.language after login', async () => {
    const first = await loginAs(mock.url, 'admin', 'admin');
    const second = await changePasswordAs(mock.url, first, 'admin', 'secret1');
    jar.setCookie(second);
    await patchConfig({ web: { language: 'zh' } });
    mount('/');
    expect(await screen.findByRole('heading', { name: i18n.getFixedT('zh')('console.title') })).toBeInTheDocument();
    expect(i18n.language).toBe('zh');
  });
});

describe('session expiry', () => {
  it('goes to /login on 4401 when another session changes the password, and never reconnects', async () => {
    const first = await loginAs(mock.url, 'admin', 'admin');
    await changePasswordAs(mock.url, first, 'admin', 'secret1');
    const cookieA = await loginAs(mock.url, 'admin', 'secret1');
    const cookieB = await loginAs(mock.url, 'admin', 'secret1');
    jar.setCookie(cookieA);
    mount('/');
    expect(await screen.findByRole('heading', { name: t('console.title') })).toBeInTheDocument();
    await waitFor(() => expect(connections).toBe(2), { timeout: 5000 });
    act(() => useUi.setState({ floatingOpen: true }));

    await act(async () => { await changePasswordAs(mock.url, cookieB, 'secret1', 'secret2'); });

    await waitFor(() => expect(screen.getByTestId('where')).toHaveTextContent('/login'));
    expect(useUi.getState().floatingOpen).toBe(false);
    await new Promise((r) => setTimeout(r, 5000));
    expect(connections).toBe(2);
    expect(screen.getByTestId('where')).toHaveTextContent('/login');
  });

  it('goes to /login on an HTTP 401 from a request', async () => {
    const first = await loginAs(mock.url, 'admin', 'admin');
    jar.setCookie(await changePasswordAs(mock.url, first, 'admin', 'secret1'));
    const client = mount('/');
    expect(await screen.findByRole('heading', { name: t('console.title') })).toBeInTheDocument();
    jar.setCookie('kvm_session=dead');
    await act(async () => { await expect(patchConfig({ web: { language: 'en' } })).rejects.toMatchObject({ status: 401 }); });
    await waitFor(() => expect(screen.getByTestId('where')).toHaveTextContent('/login'));
    expect(client.getQueryData(ME_KEY)).toBeUndefined();
    expect(client.getQueryData(CONFIG_KEY)).toBeUndefined();
  });
});

it('reports an offline sign-out and keeps the current session available for a retry', async () => {
  const first = await loginAs(mock.url, 'admin', 'admin');
  jar.setCookie(first);
  configureApi({ fetch: async (input, init) => {
    const req = new Request(input, init);
    if (new URL(req.url).pathname === '/api/auth/logout') throw new TypeError('offline');
    return jar.fetch(req);
  } });
  let logout: (() => Promise<void>) | undefined;
  function View() {
    const session = useSession();
    logout = session.logout;
    return <p>{session.user?.name}</p>;
  }
  const client = new QueryClient();
  render(<QueryClientProvider client={client}><MemoryRouter><View /><Where /><ToastHost /></MemoryRouter></QueryClientProvider>);
  await screen.findByText('admin');
  await act(async () => { await logout!(); });
  expect(screen.getByTestId('where')).toHaveTextContent(/^\/$/);
  expect(screen.getByText(t('error.unknown'))).toBeVisible();
  expect(client.getQueryData(ME_KEY)).toBeDefined();
});
