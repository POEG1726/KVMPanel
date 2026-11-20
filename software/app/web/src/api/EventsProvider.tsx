import { createContext, useContext, type ReactNode } from 'react';
import { useEvents, type EventsState } from './useEvents';

import type { User } from '../session/useSession';
import { PasteProvider } from '../console/menus/PasteProvider';

const EventsContext = createContext<EventsState | null>(null);

/** Owns the single /api/ws connection; descendants read it with useEventsContext(). */
export function EventsProvider({ children, enabled = true, user }: { children: ReactNode; enabled?: boolean; user?: User }) {
  const events = useEvents(enabled, user);
  return <EventsContext.Provider value={events}><PasteProvider events={events}>{children}</PasteProvider></EventsContext.Provider>;
}

export function useEventsContext(): EventsState {
  const ctx = useContext(EventsContext);
  if (!ctx) throw new Error('useEventsContext must be used inside <EventsProvider>');
  return ctx;
}
