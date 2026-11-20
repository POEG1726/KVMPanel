import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import type { EventsState } from '../../api/useEvents';
import type { Error as ServerError } from '../../api/ws-types';
import { toast } from '../../ui/Toast';

export const PASTE_ACK_MS = 1000;

interface PasteState {
  text: string;
  setText(text: string): void;
  submit(): void;
  pending: boolean;
  error: ServerError['error'] | null;
}

const Context = createContext<PasteState | null>(null);

/** A paste may still be rejected after the user closes the keyboard menu. */
export function PasteProvider({ events, children }: { events: EventsState; children: ReactNode }) {
  const { t } = useTranslation();
  const { connected, onServerError, send } = events;
  const [text, setText] = useState('');
  const [error, setError] = useState<PasteState['error']>(null);
  const [pending, setPending] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const revision = useRef(0);

  useEffect(() => onServerError(code => {
    if (timer.current !== undefined) {
      clearTimeout(timer.current);
      timer.current = undefined;
      setPending(false);
      setError(code);
    }
    toast(t('error.' + code), 'error');
  }), [onServerError, t]);

  useEffect(() => {
    if (!connected) setPending(false);
    return () => {
      clearTimeout(timer.current);
      timer.current = undefined;
    };
  }, [connected]);

  function submit() {
    if (!connected || timer.current !== undefined) return;
    const submitted = revision.current;
    setError(null);
    setPending(true);
    send({ t: 'paste', text });
    timer.current = setTimeout(() => {
      timer.current = undefined;
      setPending(false);
      if (revision.current === submitted) setText('');
      toast(t('keyboard.pasteSent'));
    }, PASTE_ACK_MS);
  }

  const edit = (draft: string) => {
    revision.current++;
    setText(draft);
  };
  return <Context.Provider value={{ text, setText: edit, pending, error, submit }}>{children}</Context.Provider>;
}

export function usePaste() {
  const state = useContext(Context);
  if (!state) throw new Error('usePaste must be used inside PasteProvider');
  return state;
}
