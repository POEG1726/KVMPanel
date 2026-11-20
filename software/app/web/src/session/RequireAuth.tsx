import { useEffect } from 'react';
import { Navigate, Outlet, useLocation } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, ApiError, errorKey, unwrap } from '../api/client';
import { EventsProvider, useEventsContext } from '../api/EventsProvider';
import i18n, { pickLanguage } from '../i18n';
import { ChangePasswordDialog } from './ChangePasswordDialog';
import { CONFIG_KEY, useSession } from './useSession';

function ConnectionBanner() {
  const { banner } = useEventsContext();
  const { t } = useTranslation();
  return banner && banner !== 'login' ? <div role="alert" className="connection-banner">{t('connection.' + banner)}</div> : null;
}

/** Layout route: requires a session, forces a password change when asked, then opens the events socket once. */
export function RequireAuth() {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const { user, isLoading, error } = useSession();
  const config = useQuery({
    queryKey: CONFIG_KEY,
    queryFn: () => unwrap(api.GET('/api/config')),
    enabled: !!user && !user.must_change_password,
    retry: false,
    staleTime: Infinity,
  });
  const language = config.data?.web.language;

  useEffect(() => {
    if (language) void i18n.changeLanguage(pickLanguage(language, navigator.languages));
  }, [language]);

  const title = config.data?.web.title;
  useEffect(() => {
    document.title = title || 'KVMPanel';
    return () => { document.title = 'KVMPanel'; };
  }, [title]);

  if (error instanceof ApiError && error.status === 401) return <Navigate to="/login" replace />;
  if (error) return <p role="alert">{error instanceof ApiError ? t(errorKey(error), t('error.unknown')) : t('error.unknown')}</p>;
  if (isLoading || !user) return null;
  // The password change ends this user's other sessions, so the socket is only opened afterwards.
  if (user.must_change_password) return <ChangePasswordDialog />;
  return (
    <EventsProvider enabled={pathname !== '/workspace' || user.role === 'admin' || user.permissions.includes('console')}>
      <ConnectionBanner />
      <Outlet />
    </EventsProvider>
  );
}
