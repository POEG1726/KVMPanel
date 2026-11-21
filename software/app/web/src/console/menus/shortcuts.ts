export interface Shortcut {
  name: string;
  /** KeyboardEvent.code values, pressed in order and released in reverse (max 8, like the contract). */
  keys: string[];
}

export const SHORTCUTS_KEY = 'kvm.shortcuts';
export const MAX_SHORTCUT_KEYS = 8;

/** Saved shortcuts; anything unreadable or malformed is dropped rather than breaking the menu. */
export function loadShortcuts(): Shortcut[] {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(SHORTCUTS_KEY) ?? '[]');
    if (!Array.isArray(raw)) return [];
    return raw.filter(
      (s): s is Shortcut =>
        typeof s?.name === 'string' &&
        Array.isArray(s.keys) &&
        s.keys.length >= 1 &&
        s.keys.length <= MAX_SHORTCUT_KEYS &&
        s.keys.every((k: unknown) => typeof k === 'string'),
    );
  } catch {
    return [];
  }
}

export function saveShortcuts(list: Shortcut[]): void {
  try {
    localStorage.setItem(SHORTCUTS_KEY, JSON.stringify(list));
  } catch {
    // storage unavailable: the list lasts until the page closes
  }
}
