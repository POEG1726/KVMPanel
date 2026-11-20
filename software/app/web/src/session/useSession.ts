import { useCallback, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, unwrap } from '../api/client';
import type { components } from '../api/schema';
import { onLoginRequired } from '../api/authEvents';
import { closeAllSockets } from '../api/socket';
import { closeAllFloating } from '../state/ui';

export type User = components['schemas']['User'];
export const ME_KEY = ['me'] as const;
export const CONFIG_KEY = ['config'] as const;

export const fetchMe = (): Promise<User> => unwrap(api.GET('/api/auth/me')).then((r) => r.user);

/**
 * The signed-in user plus logout. When a socket's close policy becomes 'login' or any request
 * returns 401, every socket and floating window is closed and the app goes to /login.
 */
export function useSession() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const me = useQuery({ queryKey: ME_KEY, queryFn: fetchMe, retry: false, staleTime: Infinity });

  const endSession = useCallback(() => {
    closeAllSockets();
    closeAllFloating();
    void qc.cancelQueries();
    navigate('/login', { replace: true });
  }, [navigate, qc]);

  useEffect(() => onLoginRequired(endSession), [endSession]);

  const logout = useCallback(async () => {
    try {
      await api.POST('/api/auth/logout');
    } finally {
      endSession();
    }
  }, [endSession]);

  return { user: me.data, isLoading: me.isPending, error: me.error, logout };
}
