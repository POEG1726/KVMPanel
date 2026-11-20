import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import i18n from '../i18n';
import { EventsProvider } from '../api/EventsProvider';
import { configureSockets, type SocketFactory } from '../api/socket';
import type { Config } from '../api/client';
import { CONFIG_KEY } from '../session/useSession';
import { useUi } from '../state/ui';
import { MenuBar } from './MenuBar';

const t = (k: string) => i18n.t(k) as string;

/** Sockets that never open: the bar renders from the seeded config alone. */
const inert: SocketFactory = () => ({ readyState: 0, binaryType: '', send() {}, close() {}, onopen: null, onmessage: null, onclose: null });

function mount(menubar: 'auto_hide' | 'pinned', workspace_open: 'tab' | 'float' = 'float') {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  qc.setQueryData(CONFIG_KEY, { web: { menubar, workspace_open } } as Config);
  return render(
    <QueryClientProvider client={qc}>
      <EventsProvider>
        <MenuBar />
      </EventsProvider>
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  vi.useFakeTimers();
  configureSockets({ base: 'http://x', create: inert });
  await i18n.changeLanguage('en');
});

afterEach(() => {
  cleanup();
  configureSockets({});
  vi.useRealTimers();
});

describe('MenuBar', () => {
  it('has the buttons in the documented order', () => {
    mount('pinned');
    const names = screen.getAllByRole('button').map((b) => b.getAttribute('aria-label') ?? b.textContent);
    expect(names).toEqual([t('menu.video'), t('menu.keyboard'), t('menu.mouse'), t('menu.media'), t('menu.workspace'), t('menu.power'), t('menu.wol'), t('menu.settings'), t('menu.fullscreen')]);
  });

  it('keeps the bar and the open panel in view after a drag and a viewport shrink', () => {
    vi.stubGlobal('innerWidth', 900);
    vi.stubGlobal('innerHeight', 600);
    const width = vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockImplementation(function (this: HTMLElement) {
      return this.getAttribute('role') === 'toolbar' ? 320 : 280;
    });
    const height = vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(48);
    const anchor = vi.spyOn(HTMLElement.prototype, 'offsetLeft', 'get').mockReturnValue(290);
    try {
      mount('pinned');
      const bar = screen.getByRole('toolbar');
      const handle = screen.getByTitle(t('menu.drag'));
      fireEvent.pointerDown(handle, { pointerId: 1, button: 0, clientX: 20, clientY: 10 });
      fireEvent.pointerMove(handle, { pointerId: 1, clientX: 5000, clientY: 5000 });
      fireEvent.pointerUp(handle, { pointerId: 1 });
      expect(bar).toHaveStyle({ left: '576px', top: '548px' });

      fireEvent.click(screen.getByRole('button', { name: t('menu.power') }));
      const panel = screen.getByRole('button', { name: t('menu.power') }).nextElementSibling as HTMLElement;
      expect(panel.style.bottom).toContain('100%');
      expect(parseFloat(bar.style.left) + parseFloat(panel.style.left) + 280).toBeLessThanOrEqual(900);

      vi.stubGlobal('innerWidth', 400);
      vi.stubGlobal('innerHeight', 260);
      fireEvent(window, new Event('resize'));
      expect(bar).toHaveStyle({ left: '76px', top: '208px' });
      expect(parseFloat(bar.style.left) + parseFloat(panel.style.left) + 280).toBeLessThanOrEqual(400);
      expect(parseFloat(panel.style.maxHeight)).toBeLessThanOrEqual(208);
    } finally {
      width.mockRestore();
      height.mockRestore();
      anchor.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it('hides itself after 3 seconds in auto_hide mode and returns on hover', () => {
    mount('auto_hide');
    const bar = screen.getByRole('toolbar');
    expect(bar).toHaveAttribute('data-hidden', 'false');
    act(() => void vi.advanceTimersByTime(2900));
    expect(bar).toHaveAttribute('data-hidden', 'false');
    act(() => void vi.advanceTimersByTime(200));
    expect(bar).toHaveAttribute('data-hidden', 'true');
    fireEvent.pointerEnter(screen.getByTestId('menubar-hotzone'));
    expect(bar).toHaveAttribute('data-hidden', 'false');
  });

  it('stays visible while a panel is open and when pinned', () => {
    mount('auto_hide');
    fireEvent.click(screen.getByRole('button', { name: t('menu.power') }));
    act(() => void vi.advanceTimersByTime(10000));
    expect(screen.getByRole('toolbar')).toHaveAttribute('data-hidden', 'false');
    cleanup();
    mount('pinned');
    act(() => void vi.advanceTimersByTime(10000));
    expect(screen.getByRole('toolbar')).toHaveAttribute('data-hidden', 'false');
  });

  it('renders the confirm dialog in <body> and keeps the panel open while its buttons are clicked', () => {
    mount('pinned');
    fireEvent.click(screen.getByRole('button', { name: t('menu.power') }));
    fireEvent.click(screen.getByRole('button', { name: t('power.reset') }));
    const dialog = screen.getByRole('dialog');
    expect(dialog.parentElement?.parentElement).toBe(document.body);
    expect(screen.getByRole('toolbar').contains(dialog)).toBe(false);
    fireEvent.pointerDown(within(dialog).getByRole('button', { name: t('power.reset') }));
    fireEvent.pointerDown(dialog);
    expect(screen.getByRole('button', { name: t('power.force') })).toBeInTheDocument();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole('button', { name: t('power.force') })).toBeNull();
  });

  it('records Ctrl+Shift+Esc in the shortcut field without closing the panel; Escape elsewhere still closes it', () => {
    mount('pinned');
    fireEvent.click(screen.getByRole('button', { name: t('menu.keyboard') }));
    fireEvent.click(screen.getByRole('button', { name: t('keyboard.shortcuts') }));
    const rec = screen.getByLabelText(t('keyboard.shortcutKeys'));
    for (const code of ['ControlLeft', 'ShiftLeft', 'Escape']) fireEvent.keyDown(rec, { code, key: code });
    expect(screen.getByLabelText(t('keyboard.shortcutName'))).toBeInTheDocument();
    for (const c of ['ControlLeft', 'ShiftLeft', 'Escape']) expect(screen.getByText(c)).toBeInTheDocument();
    // Tab is not trapped by the recorder.
    expect(fireEvent.keyDown(rec, { code: 'Tab', key: 'Tab' })).toBe(true);
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(screen.queryByLabelText(t('keyboard.shortcutName'))).toBeNull();
  });

  it('opens /workspace in a browser tab and dismisses the previous menu', () => {
    const open = vi.spyOn(window, 'open').mockReturnValue(null);
    mount('pinned', 'tab');
    fireEvent.click(screen.getByRole('button', { name: t('menu.power') }));
    fireEvent.click(screen.getByRole('button', { name: t('menu.workspace') }));
    expect(open).toHaveBeenCalledWith('/workspace');
    expect(screen.queryByRole('button', { name: t('power.reset') })).toBeNull();
    open.mockRestore();
  });

  it('opens the workspace from the files/terminal button', () => {
    const open = vi.spyOn(useUi.getState(), 'openWorkspace');
    useUi.setState({ openWorkspace: open });
    mount('pinned');
    fireEvent.click(screen.getByRole('button', { name: t('menu.workspace') }));
    expect(open).toHaveBeenCalled();
  });
});
