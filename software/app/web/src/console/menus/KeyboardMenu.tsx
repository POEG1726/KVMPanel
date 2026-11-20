import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useEventsContext } from '../../api/EventsProvider';
import type { ClientMessage } from '../../api/ws-types';
import { Button, Select, toast } from '../../ui';
import { ErrorLine, useConfig, usePatchConfig } from './hooks';
import { ScreenKeyboard } from './ScreenKeyboard';
import { loadShortcuts, MAX_SHORTCUT_KEYS, saveShortcuts, type Shortcut } from './shortcuts';
import s from './menus.module.css';

/** The contract's `paste.text` limit: 64 KiB of UTF-8. */
export const MAX_PASTE = 65536;
/** How long to wait for the server to reject a paste (`unmappable`) before calling it sent. */
export const PASTE_ACK_MS = 1000;

const LAYOUTS = ['us', 'uk', 'de', 'fr', 'es', 'it', 'jp', 'ru'];
const CAD = ['ControlLeft', 'AltLeft', 'Delete'];

type Section = 'paste' | 'osk' | 'shortcuts';

/** `shortcut.keys` is a tuple of 1-8 strings; the lists here are validated to that length. */
const shortcutMessage = (keys: string[]): ClientMessage => ({ t: 'shortcut', keys: keys as never });

export function KeyboardMenu() {
  const { t } = useTranslation();
  const { send, onServerError } = useEventsContext();
  const hid = useConfig()?.hid;
  const { patch, error } = usePatchConfig();
  const [section, setSection] = useState<Section | null>(null);
  const toggle = (next: Section) => setSection((cur) => (cur === next ? null : next));

  const [text, setText] = useState('');
  const bytes = useMemo(() => new TextEncoder().encode(text).length, [text]);
  const [pasteError, setPasteError] = useState<string | null>(null);
  const ack = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => void (ack.current && clearTimeout(ack.current)), []);
  useEffect(
    () =>
      onServerError((code) => {
        // The server answers a bad paste with an error message; that cancels the pending "sent".
        if (ack.current) clearTimeout(ack.current);
        ack.current = null;
        const message = t(`error.${code}`, t('error.unknown'));
        setPasteError(message);
        toast(message, 'error');
      }),
    [onServerError, t],
  );
  const [shortcuts, setShortcuts] = useState<Shortcut[]>(loadShortcuts);
  const [name, setName] = useState('');
  const [keys, setKeys] = useState<string[]>([]);

  const updateShortcuts = (next: Shortcut[]) => {
    setShortcuts(next);
    saveShortcuts(next);
  };

  const record = (e: KeyboardEvent<HTMLInputElement>) => {
    // Tab keeps moving focus; every other key is recorded, and Escape must not also close the panel.
    if (e.code === 'Tab') return;
    e.preventDefault();
    if (e.nativeEvent.repeat || !e.code) return;
    setKeys((cur) => (cur.includes(e.code) || cur.length >= MAX_SHORTCUT_KEYS ? cur : [...cur, e.code]));
  };

  const layoutOptions = (LAYOUTS.includes(hid?.keyboard_layout ?? 'us') ? LAYOUTS : [...LAYOUTS, hid!.keyboard_layout]).map((l) => ({
    value: l,
    label: t(`keyboard.layouts.${l}`, l),
  }));

  return (
    <div className={`${s.panel} ${section === 'osk' ? s.wide : ''}`}>
      <div className={s.head}>{t('keyboard.title')}</div>

      <div className={s.row}>
        <Button aria-expanded={section === 'paste'} onClick={() => toggle('paste')}>
          {t('keyboard.paste')}
        </Button>
      </div>
      {section === 'paste' && (
        <div className={s.form}>
          <textarea aria-label={t('keyboard.pasteText')} value={text} onChange={(e) => setText(e.target.value)} />
          <span className={`${s.count} ${bytes > MAX_PASTE ? s.countOver : ''}`}>{t('keyboard.pasteCount', { n: bytes, max: MAX_PASTE })}</span>
          <Button
            variant="primary"
            disabled={bytes === 0 || bytes > MAX_PASTE}
            onClick={() => {
              setPasteError(null);
              send({ t: 'paste', text });
              if (ack.current) clearTimeout(ack.current);
              ack.current = setTimeout(() => {
                ack.current = null;
                setText('');
                toast(t('keyboard.pasteSent'));
              }, PASTE_ACK_MS);
            }}
          >
            {t('keyboard.pasteSend')}
          </Button>
          <ErrorLine message={pasteError} />
        </div>
      )}

      <div className={s.row}>
        <Button aria-expanded={section === 'osk'} onClick={() => toggle('osk')}>
          {t('keyboard.osk')}
        </Button>
      </div>
      {section === 'osk' && <ScreenKeyboard onKey={(code, down) => send({ t: 'key', code, down })} />}

      <div className={s.row}>
        <Button
          onClick={() => {
            send(shortcutMessage(CAD));
            toast(t('keyboard.cadSent'));
          }}
        >
          {t('keyboard.cad')}
        </Button>
      </div>

      <div className={s.row}>
        <Button aria-expanded={section === 'shortcuts'} onClick={() => toggle('shortcuts')}>
          {t('keyboard.shortcuts')}
        </Button>
      </div>
      {section === 'shortcuts' && (
        <>
          {shortcuts.length === 0 && <p className={s.hint}>{t('keyboard.noShortcuts')}</p>}
          <ul className={s.list}>
            {shortcuts.map((sc, i) => (
              <li key={`${sc.name}-${i}`} className={s.item}>
                <Button onClick={() => send(shortcutMessage(sc.keys))}>{sc.name}</Button>
                <span className={s.itemActions}>
                  <span className={s.value}>{sc.keys.join(' + ')}</span>
                  <Button aria-label={t('keyboard.shortcutDelete', { name: sc.name })} onClick={() => updateShortcuts(shortcuts.filter((_, j) => j !== i))}>
                    {t('common.delete')}
                  </Button>
                </span>
              </li>
            ))}
          </ul>
          <div className={s.form}>
            <input aria-label={t('keyboard.shortcutName')} placeholder={t('keyboard.shortcutName')} value={name} maxLength={32} onChange={(e) => setName(e.target.value)} />
            <input readOnly aria-label={t('keyboard.shortcutKeys')} placeholder={t('keyboard.shortcutKeysHint')} value="" onKeyDown={record} />
            <span className={s.chips}>
              {keys.map((c) => (
                <span key={c} className={s.chip}>
                  {c}
                </span>
              ))}
            </span>
            <span className={s.itemActions}>
              <Button disabled={keys.length === 0} onClick={() => setKeys([])}>
                {t('keyboard.shortcutClear')}
              </Button>
              <Button
                variant="primary"
                disabled={!name.trim() || keys.length === 0}
                onClick={() => {
                  updateShortcuts([...shortcuts, { name: name.trim(), keys }]);
                  setName('');
                  setKeys([]);
                }}
              >
                {t('keyboard.shortcutSave')}
              </Button>
            </span>
          </div>
        </>
      )}

      {hid && (
        <div className={s.row}>
          <span>{t('keyboard.layout')}</span>
          <Select label={t('keyboard.layout')} value={hid.keyboard_layout} options={layoutOptions} onChange={(keyboard_layout) => void patch({ hid: { keyboard_layout } })} />
        </div>
      )}
      <ErrorLine message={error} />
    </div>
  );
}
