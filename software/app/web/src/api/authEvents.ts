type Listener = () => void;
const listeners = new Set<Listener>();

/** Called by the API client (HTTP 401) and sockets (close policy 'login'); the session hook reacts. */
export function emitLoginRequired(): void {
  // Changing your own password ends every session of yours; the cookie is re-issued, so that is no logout.
  if (ownChange) return;
  [...listeners].forEach((l) => l());
}

const OWN_CHANGE_MS = 5000;
let ownChange = false;
let ownTimer: ReturnType<typeof setTimeout> | undefined;

/** Call before POST /api/users/me/password: 4401 closes and 401s are not treated as session loss (sockets reconnect). No timer yet, so a slow request cannot outlive the flag. */
export function beginOwnPasswordChange(): void {
  ownChange = true;
  clearTimeout(ownTimer);
}

/** Call once the POST succeeded: the flag lasts 5 s more, then clears. */
export function settleOwnPasswordChange(): void {
  clearTimeout(ownTimer);
  ownTimer = setTimeout(endOwnPasswordChange, OWN_CHANGE_MS);
}

export function endOwnPasswordChange(): void {
  ownChange = false;
  clearTimeout(ownTimer);
}

export const ownPasswordChangeActive = (): boolean => ownChange;

export function onLoginRequired(l: Listener): () => void {
  listeners.add(l);
  return () => void listeners.delete(l);
}
