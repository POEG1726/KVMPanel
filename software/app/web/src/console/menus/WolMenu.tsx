import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, unwrap } from '../../api/client';
import { Button, toast } from '../../ui';
import { ErrorLine, useRun } from './hooks';
import s from './menus.module.css';

const WOL_KEY = ['wol'] as const;

export function WolMenu() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const devices = useQuery({ queryKey: WOL_KEY, queryFn: async () => (await unwrap(api.GET('/api/wol/devices'))).devices });
  const { error, run } = useRun();
  const [name, setName] = useState('');
  const [mac, setMac] = useState('');
  const refresh = () => qc.invalidateQueries({ queryKey: WOL_KEY });

  const add = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      await unwrap(api.POST('/api/wol/devices', { body: { name: name.trim(), mac: mac.trim() } }));
      setName('');
      setMac('');
      await refresh();
    });
  };

  return (
    <div className={s.panel}>
      <div className={s.head}>{t('wol.title')}</div>
      {devices.data?.length === 0 && <p className={s.hint}>{t('wol.empty')}</p>}
      <ul className={s.list}>
        {devices.data?.map((d) => (
          <li key={d.mac} className={s.item}>
            <span className={s.itemMain}>
              {d.name}
              <span className={s.value}>{d.mac}</span>
            </span>
            <span className={s.itemActions}>
              <Button
                aria-label={t('wol.wakeNamed', { name: d.name })}
                onClick={() =>
                  void run(async () => {
                    await unwrap(api.POST('/api/wol/wake', { body: { mac: d.mac } }));
                    toast(t('wol.sent', { name: d.name }));
                  })
                }
              >
                {t('wol.wake')}
              </Button>
              <Button
                aria-label={t('wol.deleteNamed', { name: d.name })}
                onClick={() =>
                  void run(async () => {
                    await unwrap(api.DELETE('/api/wol/devices/{mac}', { params: { path: { mac: d.mac } } }));
                    await refresh();
                  })
                }
              >
                {t('common.delete')}
              </Button>
            </span>
          </li>
        ))}
      </ul>
      <form className={s.form} onSubmit={add}>
        <input aria-label={t('wol.name')} placeholder={t('wol.name')} value={name} maxLength={64} onChange={(e) => setName(e.target.value)} />
        <input aria-label={t('wol.mac')} placeholder="aa:bb:cc:dd:ee:ff" value={mac} onChange={(e) => setMac(e.target.value)} />
        <Button type="submit" variant="primary" disabled={!name.trim() || !mac.trim()}>
          {t('wol.add')}
        </Button>
      </form>
      <ErrorLine message={error} />
    </div>
  );
}
