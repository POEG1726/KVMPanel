import { create } from 'zustand';
import s from './ui.module.css';

interface Item {
  id: number;
  text: string;
  kind: 'info' | 'error';
}

interface ToastState {
  items: Item[];
  dismiss(id: number): void;
}

let next = 1;
export const useToasts = create<ToastState>((set) => ({
  items: [],
  dismiss: (id) => set((st) => ({ items: st.items.filter((i) => i.id !== id) })),
}));

/** Shows a message for 4 seconds. */
export function toast(text: string, kind: Item['kind'] = 'info'): void {
  const id = next++;
  useToasts.setState((st) => ({ items: [...st.items, { id, text, kind }] }));
  setTimeout(() => useToasts.getState().dismiss(id), 4000);
}

export function ToastHost() {
  const items = useToasts((st) => st.items);
  return (
    <div className={s.toasts} role="status" aria-live="polite">
      {items.map((i) => (
        <div key={i.id} className={`${s.toast} ${i.kind === 'error' ? s.toastError : ''}`}>
          {i.text}
        </div>
      ))}
    </div>
  );
}
