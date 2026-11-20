import { create } from 'zustand';
import type { SettingsPageId } from '../settings/registry';

export type CursorStyle = 'hide' | 'default' | 'cross' | 'dot';
const CURSOR_KEY = 'kvm.cursor';
export const CURSORS: readonly CursorStyle[] = ['hide', 'default', 'cross', 'dot'];

function loadCursor(): CursorStyle {
  try {
    const v = localStorage.getItem(CURSOR_KEY) as CursorStyle | null;
    if (v && CURSORS.includes(v)) return v;
  } catch {
    // storage unavailable: use the default
  }
  return 'default';
}

interface UiState {
  /** True while either the workspace or settings is floating above the console. */
  floatingOpen: boolean;
  setFloatingOpen(open: boolean): void;
  workspaceOpen: boolean;
  openWorkspace(mode?: 'float' | 'tab'): void;
  closeWorkspace(): void;
  /** The settings window and the page it shows. */
  settingsOpen: boolean;
  settingsPage: SettingsPageId;
  /** Opens the settings window, optionally on a given page. */
  openSettings(page?: SettingsPageId): void;
  closeSettings(): void;
  /** Local cursor over the remote screen; a per-browser preference. */
  cursor: CursorStyle;
  setCursor(c: CursorStyle): void;
}

export const useUi = create<UiState>((set) => ({
  floatingOpen: false,
  setFloatingOpen: (open) => set({ floatingOpen: open }),
  workspaceOpen: false,
  openWorkspace: (mode) => {
    if (mode === 'tab') window.open('/workspace');
    else set({ workspaceOpen: true, floatingOpen: true });
  },
  closeWorkspace: () => set((st) => ({ workspaceOpen: false, floatingOpen: st.settingsOpen })),
  settingsOpen: false,
  settingsPage: 'general',
  openSettings: (page) => set((st) => ({ floatingOpen: true, settingsOpen: true, settingsPage: page ?? st.settingsPage })),
  closeSettings: () => set((st) => ({ settingsOpen: false, floatingOpen: st.workspaceOpen })),
  cursor: loadCursor(),
  setCursor: (cursor) => {
    try {
      localStorage.setItem(CURSOR_KEY, cursor);
    } catch {
      // storage unavailable: the choice lasts for this page only
    }
    set({ cursor });
  },
}));

export const closeAllFloating = (): void => {
  useUi.setState({ workspaceOpen: false, settingsOpen: false, floatingOpen: false });
};

/** CSS `cursor` value for each local cursor style. */
export const CURSOR_CSS: Record<CursorStyle, string> = {
  hide: 'none',
  default: 'default',
  cross: 'crosshair',
  // A small dot: reads as a pointer without covering the remote one.
  dot: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='8' height='8'%3E%3Ccircle cx='4' cy='4' r='3' fill='%235ec4ff'/%3E%3C/svg%3E") 4 4, default`,
};
