import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { ConfirmDialog } from './ConfirmDialog';
import { Slider } from './Slider';
import '../i18n';

afterEach(cleanup);

it('commits a slider change once when release is followed by blur before the response', () => {
  const commit = vi.fn();
  render(<Slider label="Quality" value={50} min={0} max={100} onCommit={commit} />);
  const slider = screen.getByRole('slider');
  fireEvent.change(slider, { target: { value: '60' } });
  fireEvent.pointerUp(slider);
  fireEvent.blur(slider);
  expect(commit.mock.calls).toEqual([[60]]);
});

it('cycles Tab inside a confirmation and restores focus to its opener after Escape', () => {
  function View() {
    const [visible, setVisible] = useState(false);
    return <><button onClick={() => setVisible(true)}>Open</button>
      {visible && <ConfirmDialog title="Reset?" message="Confirm reset" confirmLabel="Reset" onCancel={() => setVisible(false)} onConfirm={() => setVisible(false)} />}
    </>;
  }
  render(<View />);
  const open = screen.getByRole('button', { name: 'Open' });
  open.focus();
  fireEvent.click(open);
  const cancel = screen.getByRole('button', { name: 'Cancel' });
  const confirm = screen.getByRole('button', { name: 'Reset' });
  expect(cancel).toHaveFocus();
  fireEvent.keyDown(cancel, { key: 'Tab', shiftKey: true });
  expect(confirm).toHaveFocus();
  fireEvent.keyDown(confirm, { key: 'Tab' });
  expect(cancel).toHaveFocus();
  fireEvent.keyDown(cancel, { key: 'Escape' });
  expect(open).toHaveFocus();
});
