import { useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText, useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Select, Switch, toast } from '../../ui';
import { Alert, Group, Row } from '../fields';
import { EP_IN, EP_OUT, usbBudget, type UsbConfig } from '../usbBudget';
import { useConfigSection } from '../useConfigSection';
import s from '../settings.module.css';

type Feature = 'keyboard' | 'mouse_rel' | 'mouse_abs' | 'mouse_merge' | 'storage' | 'net' | 'acm' | 'telemetry';

/** Endpoint cost per function, shown beside each switch (spec 4.5). Telemetry's IN depends on the mice. */
const FEATURES: readonly { key: Feature; in: string; out: string }[] = [
  { key: 'keyboard', in: '1', out: '0' },
  { key: 'mouse_rel', in: '1', out: '0' },
  { key: 'mouse_abs', in: '1', out: '0' },
  { key: 'mouse_merge', in: '0', out: '0' },
  { key: 'storage', in: '1', out: '1' },
  { key: 'net', in: '2', out: '1' },
  { key: 'acm', in: '2', out: '1' },
  { key: 'telemetry', in: '0*', out: '0' },
];

/** How long the "endpoints full" hint stays below the budget bar. */
export const TIP_MS = 2500;
const APPLIED_KEY = ['usb', 'applied'] as const;
const BUDGET_KEY = ['usb', 'budget'] as const;

function Bar({ label, used, max }: { label: string; used: number; max: number }) {
  return (
    <div className={s.barRow}>
      <span className={s.barLabel}>{`${label} ${used}/${max}`}</span>
      <span className={s.cells} data-full={used >= max} role="img" aria-label={`${label} ${used}/${max}`}>
        {Array.from({ length: max }, (_, i) => (
          <i key={i} data-on={i < used} />
        ))}
      </span>
    </div>
  );
}

/** `config.usb` with the endpoint budget; changes only take effect after "Apply". */
export function UsbFunctionsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { value: usb, patch } = useConfigSection('usb');
  const applied = useQuery({ queryKey: APPLIED_KEY, queryFn: () => unwrap(api.GET('/api/usb/applied')) });
  const server = useQuery({ queryKey: BUDGET_KEY, queryFn: () => unwrap(api.GET('/api/usb/budget')) });
  const { error, run } = useRun();
  const [tip, setTip] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const tipTimer = useRef<ReturnType<typeof setTimeout>>();
  useEffect(() => () => clearTimeout(tipTimer.current), []);

  if (!usb) return null;

  // While the server's answer is on its way, the local prediction keeps the bar in step with the switches.
  const local = usbBudget(usb);
  const budget = server.isFetching || !server.data ? local : server.data;

  const refresh = () => Promise.all([qc.invalidateQueries({ queryKey: APPLIED_KEY }), qc.invalidateQueries({ queryKey: BUDGET_KEY })]);
  const change = async (part: Partial<UsbConfig>) => {
    if (await patch(part)) await refresh();
  };

  const showTip = (missing: { in_missing: number; out_missing: number }) => {
    setTip(t('error.endpoints_full', missing));
    clearTimeout(tipTimer.current);
    tipTimer.current = setTimeout(() => setTip(null), TIP_MS);
  };

  const toggle = (key: Feature, on: boolean) => {
    // Turning something on may not fit; turning off always does. Predict with the board's rule.
    const next = usbBudget({ ...usb, [key]: on });
    if (on && !next.fits) return showTip(next);
    void change({ [key]: on } as Partial<UsbConfig>);
  };

  const apply = () =>
    run(async () => {
      await unwrap(api.POST('/api/usb/apply'));
      toast(t('settings.usb.applying'));
      await refresh();
    });

  const pending = !!applied.data?.pending;
  return (
    <>
      <div className={s.bars}>
        <Bar label="IN" used={budget.in} max={EP_IN} />
        <Bar label="OUT" used={budget.out} max={EP_OUT} />
        {tip && (
          <div role="status" className={s.tip} data-testid="usb-tip">
            {tip}
          </div>
        )}
      </div>
      <Group>
        {FEATURES.map((f) => (
          <Row key={f.key} label={t(`settings.usb.feature.${f.key}`)} hint={t(`settings.usb.featureHint.${f.key}`)}>
            <span className={s.cost}>{`IN ${f.in} · OUT ${f.out}`}</span>
            {f.key === 'net' && usb.net && (
              <Select
                label={t('settings.usb.netMode')}
                value={usb.net_mode}
                options={[
                  { value: 'ncm', label: 'NCM' },
                  { value: 'rndis', label: 'RNDIS' },
                ]}
                onChange={(net_mode) => void change({ net_mode })}
              />
            )}
            <Switch
              label={t(`settings.usb.feature.${f.key}`)}
              checked={usb[f.key]}
              unavailable={!usb[f.key] && !usbBudget({ ...usb, [f.key]: true }).fits}
              onChange={(on) => toggle(f.key, on)}
            />
          </Row>
        ))}
      </Group>
      <div className={s.actions}>
        <Button variant="primary" disabled={!pending} onClick={() => setConfirming(true)}>
          {t('settings.usb.apply')}
        </Button>
        <Button disabled={!pending || !applied.data} onClick={() => applied.data && void change(applied.data.usb)}>
          {t('settings.usb.revert')}
        </Button>
        {pending && <span className={s.muted}>{t('settings.usb.pending')}</span>}
      </div>
      <Alert message={error ?? (applied.error ? errorText(t, applied.error) : null)} />
      {confirming && (
        <ConfirmDialog
          title={t('settings.usb.applyTitle')}
          message={t('settings.usb.applyMessage')}
          confirmLabel={t('settings.usb.apply')}
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            setConfirming(false);
            void apply();
          }}
        />
      )}
    </>
  );
}
