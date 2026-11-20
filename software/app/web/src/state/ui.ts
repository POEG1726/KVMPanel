import { create } from 'zustand';

interface UiState {
  /** True while any floating window is open (later tasks replace this with the window list). */
  floatingOpen: boolean;
  setFloatingOpen(open: boolean): void;
}

export const useUi = create<UiState>((set) => ({
  floatingOpen: false,
  setFloatingOpen: (open) => set({ floatingOpen: open }),
}));

export const closeAllFloating = (): void => useUi.getState().setFloatingOpen(false);
