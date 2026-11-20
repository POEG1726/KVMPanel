import { createContext, useContext, type ReactNode } from 'react';
import { useEvents, type EventsState } from './useEvents';

import { PasteProvider } from '../console/menus/PasteProvider';

const EventsContext = createContext<EventsState | null>(null);

/** Owns the single /api/ws connection; descendants read it with useEventsContext(). */
export function EventsProvider({ children, enabled = true }: { children: ReactNode; enabled?: boolean }) {
  const events = useEvents(enabled);
  return <EventsContext.Provider value={events}><PasteProvider events={events}>{children}</PasteProvider></EventsContext.Provider>;
}

export function useEventsContext(): EventsState {
  const ctx = useContext(EventsContext);
  if (!ctx) throw new Error('useEventsContext must be used inside <EventsProvider>');
  return ctx;
}
