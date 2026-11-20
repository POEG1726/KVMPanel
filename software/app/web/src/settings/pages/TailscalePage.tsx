import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText, useRun } from '../../console/menus/hooks';
import { Button } from '../../ui';
import { Alert, Group, Row } from '../fields';
import s from '../settings.module.css';

type Action = 'install' | 'uninstall' | 'up' | 'down' | 'login' | 'logout';
const KEY = ['tailscale'] as const;
export const POLL_MS = 3000;

/** Tailscale state; actions are asynchronous on the board, so the state is polled while the page is open. */
export function TailscalePage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const state = useQuery({ queryKey: KEY, queryFn: () => unwrap(api.GET('/api/tailscale')), refetchInterval: POLL_MS });
  const { error, run } = useRun();
  const d = state.data;

  const act = (action: Action) =>
    run(async () => {
      await unwrap(api.POST('/api/tailscale/{action}', { params: { path: { action } } }));
      await qc.invalidateQueries({ queryKey: KEY });
    });
  const button = (action: Action, primary = false) => (
    <Button variant={primary ? 'primary' : 'default'} onClick={() => void act(action)}>
      {t(`settings.tailscale.${action}`)}
    </Button>
  );

  return (
    <>
      <Alert message={state.error ? errorText(t, state.error) : null} />
      {d && (
        <>
          <Group>
            <Row label={t('settings.tailscale.installed')}>{t(d.installed ? 'common.yes' : 'common.no')}</Row>
            <Row label={t('settings.tailscale.running')}>{t(d.running ? 'common.yes' : 'common.no')}</Row>
            {d.ip && (
              <Row label={t('settings.tailscale.ip')}>
                <span className={s.mono}>{d.ip}</span>
              </Row>
            )}
            {d.login_url && (
              <Row label={t('settings.tailscale.loginUrl')} hint={t('settings.tailscale.loginHint')}>
                <a href={d.login_url} target="_blank" rel="noreferrer">
                  {d.login_url}
                </a>
              </Row>
            )}
          </Group>
          {d.error && <p role="alert" className={s.alert}>{`${t('settings.tailscale.failed')}: ${d.error}`}</p>}
          <div className={s.actions}>
            {!d.installed ? (
              button('install', true)
            ) : (
              <>
                {d.running ? button('down') : button('up', true)}
                {button('login')}
                {button('logout')}
                {button('uninstall')}
              </>
            )}
          </div>
        </>
      )}
      <Alert message={error} />
    </>
  );
}
