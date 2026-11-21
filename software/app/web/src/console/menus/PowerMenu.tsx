import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { useEventsContext } from '../../api/EventsProvider';
import { Button, ConfirmDialog } from '../../ui';
import { ErrorLine, useRun } from './hooks';
import s from './menus.module.css';

/** A tap on the front-panel header. */
export const SHORT_PRESS_MS = 500;
/** Long enough for the PC to cut power (the contract treats >= 4000 ms as a force-off). */
export const FORCE_PRESS_MS = 5000;

type Action = 'power' | 'reset' | 'force';

const PRESS: Record<Action, { button: 'power' | 'reset'; duration_ms: number }> = {
  power: { button: 'power', duration_ms: SHORT_PRESS_MS },
  reset: { button: 'reset', duration_ms: SHORT_PRESS_MS },
  force: { button: 'power', duration_ms: FORCE_PRESS_MS },
};

export function PowerMenu() {
  const { t } = useTranslation();
  const { status } = useEventsContext();
  const [pending, setPending] = useState<Action | null>(null);
  const { error, run } = useRun();
  const power = status?.power;

  const confirm = () => {
    const action = pending!;
    setPending(null);
    void run(() => unwrap(api.POST('/api/power/press', { body: PRESS[action] })));
  };

  return (
    <div className={s.panel}>
      <div className={s.head}>
        {t('power.title')} · <span className={power?.on ? s.on : undefined}>{power?.on ? t('power.on') : t('power.off')}</span> · {t('power.hdd')}{' '}
        {power?.hdd ? t('power.lit') : t('power.dark')}
      </div>
      <div className={s.grid2}>
        <Button onClick={() => setPending('power')}>{t('power.power')}</Button>
        <Button onClick={() => setPending('reset')}>{t('power.reset')}</Button>
        <Button variant="danger" className={s.span2} onClick={() => setPending('force')}>
          {t('power.force')}
        </Button>
      </div>
      <ErrorLine message={error} />
      {pending && (
        <ConfirmDialog
          title={t(`power.${pending}Title`)}
          message={t(`power.${pending}Message`)}
          confirmLabel={pending === 'force' ? t('power.forceConfirm') : t(`power.${pending}`)}
          danger={pending !== 'power'}
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        />
      )}
    </div>
  );
}
