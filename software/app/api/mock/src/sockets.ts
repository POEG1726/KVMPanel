import type WebSocket from 'ws';
import type { Permission, State } from './state.js';
import { accountForToken, hasPerm } from './routes/auth.js';

/** Close codes shared by /api/ws, /api/stream and /api/terminal (see README). */
export const CLOSE = { unauthenticated: 4401, forbidden: 4403, disconnected: 4409, restarting: 1012 } as const;

interface Tracked { socket: WebSocket; token: string; perm: Permission }

const registries = new WeakMap<State, Set<Tracked>>();

function registry(state: State): Set<Tracked> {
  let set = registries.get(state);
  if (!set) {
    set = new Set();
    registries.set(state, set);
  }
  return set;
}

/** Registers an authenticated socket so session/permission changes can close it. */
export function trackSocket(state: State, socket: WebSocket, token: string, perm: Permission): void {
  const entry = { socket, token, perm };
  registry(state).add(entry);
  socket.on('close', () => registry(state).delete(entry));
}

/**
 * Installs the bus listeners. Call after all routes are registered so route-specific
 * listeners (e.g. /api/ws sending kvm_restarting) run before the sockets are closed.
 */
export function installSocketPolicy(state: State): void {
  const closeAll = (code: number, reason: string) => registry(state).forEach((t) => t.socket.close(code, reason));
  state.events.on('sessions_changed', () => {
    registry(state).forEach((t) => {
      const acc = accountForToken(state, t.token);
      if (!acc) t.socket.close(CLOSE.unauthenticated, 'session ended');
      else if (!hasPerm(acc, t.perm)) t.socket.close(CLOSE.forbidden, 'permission revoked');
    });
  });
  state.events.on('kvm_restarting', () => closeAll(CLOSE.restarting, 'kvm restarting'));
  state.events.on('disconnect_sessions', () => closeAll(CLOSE.disconnected, 'disconnected by admin'));
}
