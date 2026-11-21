import { useEffect, useState, type FormEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import type { components } from '../../api/schema';
import { useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Select, Switch, toast } from '../../ui';
import { Alert, Group, Row } from '../fields';
import s from '../settings.module.css';

type Network = components['schemas']['NetworkConfig'];
const NETWORK_KEY = ['network'] as const;

/** Form state: everything as strings, so half-typed values are fine until Save. */
interface Form {
  mode: Network['mode'];
  ip: string;
  mask: string;
  gateway: string;
  dnsMode: Network['dns']['mode'];
  dns: string;
  https: boolean;
  httpsPort: string;
  cert: string;
  key: string;
  httpPort: string;
  bind: string;
}

const toForm = (n: Network): Form => ({
  mode: n.mode,
  ip: n.ip ?? '',
  mask: n.mask ?? '',
  gateway: n.gateway ?? '',
  dnsMode: n.dns.mode,
  dns: n.dns.servers.join(', '),
  https: n.https.enabled,
  httpsPort: String(n.https.port),
  cert: n.https.cert_path ?? '',
  key: n.https.key_path ?? '',
  httpPort: String(n.http_port),
  bind: n.bind,
});

/** Empty strings are left out (the contract has no empty values for these). */
const opt = <K extends string>(k: K, v: string): Partial<Record<K, string>> => (v.trim() ? ({ [k]: v.trim() } as Record<K, string>) : {});

function toBody(f: Form): Network {
  return {
    mode: f.mode,
    ...opt('ip', f.ip),
    ...opt('mask', f.mask),
    ...opt('gateway', f.gateway),
    dns: { mode: f.dnsMode, servers: f.dns.split(/[,\s]+/).filter(Boolean) },
    https: { enabled: f.https, port: Number(f.httpsPort), ...opt('cert_path', f.cert), ...opt('key_path', f.key) },
    http_port: Number(f.httpPort),
    bind: f.bind.trim(),
  };
}

/** Network settings. The server decides what is valid; its error is shown here and the page stays put. */
export function NetworkPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const network = useQuery({ queryKey: NETWORK_KEY, queryFn: () => unwrap(api.GET('/api/network')) });
  const { error, run, clear } = useRun();
  const [form, setForm] = useState<Form | null>(null);
  const [confirming, setConfirming] = useState(false);
  useEffect(() => network.data && setForm(toForm(network.data)), [network.data]);
  if (!form) return null;
  const set = <K extends keyof Form>(k: K, v: Form[K]) => setForm((f) => f && { ...f, [k]: v });

  const text = (k: keyof Form, label: string, props: { disabled?: boolean; type?: string } = {}) => (
    <Row label={label}>
      <input className={s.input} aria-label={label} value={form[k] as string} onChange={(e) => set(k, e.target.value as never)} {...props} />
    </Row>
  );

  const save = () => {
    clear();
    void run(async () => {
      qc.setQueryData(NETWORK_KEY, await unwrap(api.PUT('/api/network', { body: toBody(form) })));
      toast(t('settings.network.saved'));
    });
  };

  const isStatic = form.mode === 'static';
  return (
    <form
      onSubmit={(e: FormEvent) => {
        e.preventDefault();
        setConfirming(true);
      }}
    >
      <Group title={t('settings.network.address')}>
        <Row label={t('settings.network.mode')}>
          <Select
            label={t('settings.network.mode')}
            value={form.mode}
            options={[
              { value: 'dhcp', label: 'DHCP' },
              { value: 'static', label: t('settings.network.static') },
            ]}
            onChange={(v) => set('mode', v)}
          />
        </Row>
        {isStatic && (
          <>
            {text('ip', t('settings.network.ip'))}
            {text('mask', t('settings.network.mask'))}
            {text('gateway', t('settings.network.gateway'))}
          </>
        )}
        <Row label={t('settings.network.dnsMode')}>
          <Select
            label={t('settings.network.dnsMode')}
            value={form.dnsMode}
            options={[
              { value: 'dhcp', label: 'DHCP' },
              { value: 'manual', label: t('settings.network.manual') },
            ]}
            onChange={(v) => set('dnsMode', v)}
          />
        </Row>
        {form.dnsMode === 'manual' && text('dns', t('settings.network.dnsServers'))}
      </Group>
      <Group title={t('settings.network.web')}>
        {text('bind', t('settings.network.bind'))}
        {text('httpPort', t('settings.network.httpPort'), { type: 'number' })}
        <Row label={t('settings.network.https')}>
          <Switch label={t('settings.network.https')} checked={form.https} onChange={(v) => set('https', v)} />
        </Row>
        {form.https && (
          <>
            {text('httpsPort', t('settings.network.httpsPort'), { type: 'number' })}
            {text('cert', t('settings.network.cert'))}
            {text('key', t('settings.network.key'))}
          </>
        )}
      </Group>
      <Alert message={error} />
      <div className={s.actions}>
        <Button type="submit" variant="primary">
          {t('settings.network.save')}
        </Button>
      </div>
      {confirming && (
        <ConfirmDialog
          title={t('settings.network.confirmTitle')}
          message={t('settings.network.confirmMessage')}
          confirmLabel={t('settings.network.save')}
          danger
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            setConfirming(false);
            save();
          }}
        />
      )}
    </form>
  );
}
