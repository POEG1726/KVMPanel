import { EventEmitter } from 'node:events';
import { contract } from './contract.js';

/** In-memory state of the mock kvmd. Each route module adds the fields it owns. */
export interface State {
  version: string;
  /** In-process event bus; WebSocket and panel socket servers forward its events. */
  events: EventEmitter;
  now: () => number;
}

export function createState(opts: { now?: () => number } = {}): State {
  return {
    version: contract.info.version,
    events: new EventEmitter(),
    now: opts.now ?? Date.now,
  };
}
