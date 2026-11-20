type Listener = () => void;
const listeners = new Set<Listener>();

/** Called by the API client (HTTP 401) and sockets (close policy 'login'); the session hook reacts. */
export function emitLoginRequired(): void {
  [...listeners].forEach((l) => l());
}

export function onLoginRequired(l: Listener): () => void {
  listeners.add(l);
  return () => void listeners.delete(l);
}
