import { useCallback, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { toast } from '../ui/Toast';
import { api, ApiError, errorKey, unwrap } from '../api/client';
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
  const { t } = useTranslation();
  const qc = useQueryClient();
  const me = useQuery({ queryKey: ME_KEY, queryFn: fetchMe, retry: false, staleTime: Infinity });

  const endSession = useCallback(() => {
    closeAllSockets();
    closeAllFloating();
    qc.clear();
    navigate('/login', { replace: true });
  }, [navigate, qc]);

  useEffect(() => onLoginRequired(endSession), [endSession]);

  const logout = useCallback(async () => {
    try {
      await unwrap(api.POST('/api/auth/logout'));
    } catch (err) {
      if (!(err instanceof ApiError && err.status === 401)) {
        toast(err instanceof ApiError ? t(errorKey(err), t('error.unknown')) : t('error.unknown'), 'error');
      }
      return;
    }
    endSession();
  }, [endSession, t]);

  return { user: me.data, isLoading: me.isPending, error: me.error, logout };
}
