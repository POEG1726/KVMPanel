import type { QueryClient } from '@tanstack/react-query';
import { api, ApiError, patchConfig, unwrap, type Config, type DeepPartial } from './client';
import { CONFIG_KEY } from '../session/useSession';

type Patch = DeepPartial<Config>;
const object = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** Mirror PATCH, including the null deletion marker for panel.thresholds. */
function merge(target: unknown, patch: unknown, path = ''): unknown {
  if (!object(patch) || !object(target)) return patch;
  const result = { ...target };
  for (const [key, value] of Object.entries(patch)) {
    if (value === null && path === 'panel.thresholds') delete result[key];
    else result[key] = merge(target[key], value, path ? `${path}.${key}` : key);
  }
  return result;
}

interface Writes {
  confirmed?: Config;
  pending: Patch[];
  tail: Promise<void>;
}

// Key by the query instance so clearing the session cache also isolates its outstanding writes.
const writes = new WeakMap<object, Writes>();

export function writeConfig(client: QueryClient, patch: Patch): Promise<Config> {
  const cache = client.getQueryCache();
  const query = cache.build<Config>(client, { queryKey: CONFIG_KEY });
  const queue: Writes = writes.get(query) ?? {
    confirmed: client.getQueryData<Config>(CONFIG_KEY), pending: [], tail: Promise.resolve(),
  };
  writes.set(query, queue);
  const active = () => cache.get<Config>(query.queryHash) === query;
  const publish = () => {
    if (!active() || !queue.confirmed) return;
    const visible = queue.pending.reduce<Config>((cfg, edit) => merge(cfg, edit) as Config, queue.confirmed);
    client.setQueryData(CONFIG_KEY, visible);
  };

  void client.cancelQueries({ queryKey: CONFIG_KEY, exact: true });
  queue.pending.push(patch);
  publish();
  const send = queue.tail.then(async () => {
    if (!active()) throw new DOMException('Session closed', 'AbortError');
    try {
      queue.confirmed = await patchConfig(patch);
      return queue.confirmed;
    } catch (error) {
      // The server may have applied a write whose response was lost. Reconcile before sending the next edit.
      if (active() && !(error instanceof ApiError && error.status === 401)) {
        try { queue.confirmed = await unwrap(api.GET('/api/config')); }
        catch { /* Offline: retain the last confirmed response. */ }
      }
      throw error;
    } finally {
      queue.pending.shift();
      publish();
      if (!queue.pending.length) writes.delete(query);
    }
  });
  queue.tail = send.then(() => {}, () => {});
  return send;
}
