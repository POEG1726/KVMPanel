import { useEffect, useRef, useState } from 'react';
import { Button } from '../../ui';
import s from './menus.module.css';

interface Key {
  code: string;
  label: string;
}

const k = (code: string, label = code): Key => ({ code, label });
const letters = (chars: string): Key[] => [...chars].map((c) => k(`Key${c}`, c));
const digits = [...'1234567890'].map((c) => k(`Digit${c}`, c));

const ROWS: Key[][] = [
  [k('Escape', 'Esc'), ...Array.from({ length: 12 }, (_, i) => k(`F${i + 1}`))],
  [k('Backquote', '`'), ...digits, k('Minus', '-'), k('Equal', '='), k('Backspace', '⌫')],
  [k('Tab'), ...letters('QWERTYUIOP'), k('BracketLeft', '['), k('BracketRight', ']'), k('Backslash', '\\')],
  [k('CapsLock', 'Caps'), ...letters('ASDFGHJKL'), k('Semicolon', ';'), k('Quote', "'"), k('Enter')],
  [k('ShiftLeft', 'Shift'), ...letters('ZXCVBNM'), k('Comma', ','), k('Period', '.'), k('Slash', '/'), k('ShiftRight', 'Shift')],
  [k('ControlLeft', 'Ctrl'), k('MetaLeft', 'Win'), k('AltLeft', 'Alt'), k('Space'), k('AltRight', 'Alt'), k('MetaRight', 'Win'), k('ControlRight', 'Ctrl')],
  [k('Insert', 'Ins'), k('Delete', 'Del'), k('Home'), k('End'), k('PageUp', 'PgUp'), k('PageDown', 'PgDn'), k('ArrowLeft', '←'), k('ArrowUp', '↑'), k('ArrowDown', '↓'), k('ArrowRight', '→')],
];

/** Modifiers latch (tap to hold, tap again to release) so they can combine with the next key. */
const LATCHING = new Set(['ControlLeft', 'ControlRight', 'ShiftLeft', 'ShiftRight', 'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight']);

interface Props {
  onKey(code: string, down: boolean): void;
}

export function ScreenKeyboard({ onKey }: Props) {
  const [held, setHeld] = useState<ReadonlySet<string>>(new Set());
  // Releases still-latched modifiers if the keyboard closes, so the PC never keeps Ctrl pressed.
  const latest = useRef({ held, onKey });
  latest.current = { held, onKey };
  useEffect(
    () => () => {
      latest.current.held.forEach((code) => latest.current.onKey(code, false));
    },
    [],
  );

  const press = (code: string) => {
    if (LATCHING.has(code)) {
      const next = new Set(held);
      const down = !next.delete(code);
      if (down) next.add(code);
      setHeld(next);
      onKey(code, down);
      return;
    }
    onKey(code, true);
    onKey(code, false);
    // The latched modifiers apply to this one key only.
    held.forEach((m) => onKey(m, false));
    setHeld(new Set());
  };

  return (
    <div className={s.osk}>
      {ROWS.map((row, i) => (
        <div key={i} className={s.oskRow}>
          {row.map((key, j) => (
            <Button
              key={`${key.code}-${j}`}
              className={`${s.oskKey} ${held.has(key.code) ? s.oskKeyOn : ''}`}
              style={key.code === 'Space' ? { flexGrow: 6 } : undefined}
              aria-pressed={LATCHING.has(key.code) ? held.has(key.code) : undefined}
              onClick={() => press(key.code)}
            >
              {key.label}
            </Button>
          ))}
        </div>
      ))}
    </div>
  );
}
