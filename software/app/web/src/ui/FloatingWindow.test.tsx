import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import i18n from '../i18n';
import { FloatingWindow } from './FloatingWindow';

const initial = { x: 40, y: 70, w: 600, h: 320 };
const storageKey = 'kvm.float.workspace';

function mount(remember = true) {
  const onClose = vi.fn();
  const onPopOut = vi.fn();
  const view = render(
    <FloatingWindow id="workspace" title="Files / Terminal" initial={initial} remember={remember}
      onClose={onClose} onPopOut={onPopOut}>
      <input aria-label="Terminal input" />
    </FloatingWindow>,
  );
  return { ...view, onClose, onPopOut, win: screen.getByRole('dialog', { name: 'Files / Terminal' }) };
}

function drag(element: HTMLElement, dx: number, dy: number) {
  fireEvent.pointerDown(element, { button: 0, pointerId: 1, clientX: 10, clientY: 12 });
  fireEvent.pointerMove(element, { pointerId: 1, clientX: 10 + dx, clientY: 12 + dy });
  fireEvent.pointerUp(element, { pointerId: 1, clientX: 10 + dx, clientY: 12 + dy });
}

beforeEach(async () => {
  localStorage.clear();
  await i18n.changeLanguage('en');
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it('moves with the title bar, leaving a recoverable title bar in every direction', () => {
  const { win } = mount();
  const title = within(win).getByText('Files / Terminal');
  drag(title, 100, 50);
  expect(win).toHaveStyle({ left: '140px', top: '120px' });

  drag(title, 9999, 9999);
  expect(parseFloat(win.style.left)).toBeLessThanOrEqual(window.innerWidth - 40);
  expect(parseFloat(win.style.top)).toBeLessThanOrEqual(window.innerHeight - 40);
  drag(title, -9999, -9999);
  expect(parseFloat(win.style.left) + parseFloat(win.style.width)).toBeGreaterThanOrEqual(40);
  expect(parseFloat(win.style.top)).toBeGreaterThanOrEqual(0);
});

it('resizes at the grip and restores the remembered normal rectangle when reopened', () => {
  const { win, onClose, unmount } = mount();
  drag(within(win).getByText('Files / Terminal'), 20, 15);
  drag(screen.getByRole('button', { name: /resize/i }), 60, 40);
  expect(win).toHaveStyle({ left: '60px', top: '85px', width: '660px', height: '360px' });
  fireEvent.click(screen.getByRole('button', { name: 'Close' }));
  expect(onClose).toHaveBeenCalledOnce();
  unmount();
  expect(mount().win).toHaveStyle({ left: '60px', top: '85px', width: '660px', height: '360px' });
});

it('ignores stored geometry and does not write gestures when remembering is disabled', () => {
  const previous = JSON.stringify({ x: 1, y: 2, w: 601, h: 322 });
  localStorage.setItem(storageKey, previous);
  const { win } = mount(false);
  expect(win).toHaveStyle({ left: '40px', top: '70px', width: '600px', height: '320px' });
  drag(within(win).getByText('Files / Terminal'), 90, 30);
  expect(localStorage.getItem(storageKey)).toBe(previous);
});

it('falls back for an invalid stored rectangle and refits on viewport resize', () => {
  localStorage.setItem(storageKey, '{"x":"broken","y":0,"w":800,"h":500}');
  const { win } = mount();
  expect(win).toHaveStyle({ left: '40px', top: '70px', width: '600px', height: '320px' });
  vi.stubGlobal('innerWidth', 280);
  vi.stubGlobal('innerHeight', 160);
  fireEvent(window, new Event('resize'));
  expect(parseFloat(win.style.width)).toBeLessThanOrEqual(280);
  expect(parseFloat(win.style.height)).toBeLessThanOrEqual(160);
  expect(parseFloat(win.style.left)).toBeLessThanOrEqual(240);
  expect(parseFloat(win.style.top)).toBeLessThanOrEqual(120);
});

it('keeps the terminal mounted through minimize and maximize and delegates pop-out', () => {
  const { win, onPopOut } = mount();
  const input = screen.getByRole('textbox');
  fireEvent.change(input, { target: { value: 'ls' } });
  fireEvent.click(screen.getByRole('button', { name: 'Minimize' }));
  expect(screen.queryByRole('textbox')).toBeNull();
  expect(input).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Restore' }));
  expect(screen.getByRole('textbox')).toHaveValue('ls');
  fireEvent.click(screen.getByRole('button', { name: 'Maximize' }));
  expect(win).toHaveStyle({ left: '0px', top: '0px', width: window.innerWidth + 'px', height: window.innerHeight + 'px' });
  fireEvent.click(screen.getByRole('button', { name: 'Restore' }));
  expect(win).toHaveStyle({ left: '40px', top: '70px', width: '600px', height: '320px' });
  fireEvent.click(screen.getByRole('button', { name: /new tab/i }));
  expect(onPopOut).toHaveBeenCalledOnce();
});

it('moves and resizes using arrow keys on the corresponding handle only', () => {
  const { win } = mount();
  const bar = within(win).getByText('Files / Terminal').closest('header')!;
  fireEvent.keyDown(bar, { key: 'ArrowRight' });
  fireEvent.keyDown(screen.getByRole('button', { name: /resize/i }), { key: 'ArrowDown' });
  expect(win).toHaveStyle({ left: '50px', top: '70px', width: '600px', height: '330px' });

  fireEvent.keyDown(screen.getByRole('textbox'), { key: 'ArrowLeft' });
  drag(screen.getByRole('button', { name: 'Close' }), 120, 90);
  expect(win).toHaveStyle({ left: '50px', top: '70px', width: '600px', height: '330px' });
});

it('ends the drag when the browser cancels the pointer', () => {
  const { win } = mount();
  const bar = within(win).getByText('Files / Terminal');
  fireEvent.pointerDown(bar, { button: 0, pointerId: 3, clientX: 70, clientY: 80 });
  fireEvent.pointerMove(bar, { pointerId: 4, clientX: 190, clientY: 160 });
  expect(win).toHaveStyle({ left: '40px', top: '70px' });
  fireEvent.pointerMove(bar, { pointerId: 3, clientX: 90, clientY: 100 });
  fireEvent.pointerCancel(bar, { pointerId: 3 });
  fireEvent.pointerMove(bar, { pointerId: 3, clientX: 30, clientY: 80 });
  expect(win).toHaveStyle({ left: '60px', top: '90px' });
});
