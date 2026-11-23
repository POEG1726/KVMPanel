import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import type { components } from '../../api/schema';
import { errorText, useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Switch, toast } from '../../ui';
import { Alert, Group, Row, TextField, bytes } from '../fields';
import { useConfigSection } from '../useConfigSection';
import s from '../settings.module.css';

type Action = components['schemas']['DeviceAction'];
/** Every action changes the board or the PC's USB devices, so each one asks first. */
const ACTIONS: readonly { action: Action; danger: boolean }[] = [
  { action: 'reset_hid', danger: false },
  { action: 'disconnect_sessions', danger: false },
  { action: 'restart_kvm', danger: false },
  { action: 'reboot', danger: true },
  { action: 'shutdown', danger: true },
];

/** Board information, `config.device`, the maintenance actions and the KVM master switch. */
export function DevicePage() {
  const { t } = useTranslation();
  const info = useQuery({ queryKey: ['device'], queryFn: () => unwrap(api.GET('/api/device')) });
  const { value: device, patch } = useConfigSection('device');
  const { value: kvm, patch: patchKvm } = useConfigSection('kvm');
  const { value: host, patch: patchHost } = useConfigSection('host');
  const { error, run } = useRun();
  const [pending, setPending] = useState<Action | 'kvm_off' | null>(null);

  const d = info.data;
  const confirm = (what: Action | 'kvm_off') => {
    setPending(null);
    void run(async () => {
      if (what === 'kvm_off') {
        // The server answers first, then drops every remote connection (4409) and stops the web listener.
        await patchKvm({ enabled: false });
        return;
      }
      await unwrap(api.POST('/api/device/action', { body: { action: what } }));
      toast(t(`settings.device.sent.${what}`));
    });
  };

  return (
    <>
      <Alert message={info.error ? errorText(t, info.error) : null} />
      {d && (
        <Group title={t('settings.device.info')}>
          <Row label={t('settings.device.version')}>{d.version}</Row>
          <Row label={t('settings.device.kernel')}>{d.kernel}</Row>
          <Row label={t('settings.device.uptime')}>{t('settings.device.uptimeValue', { hours: Math.floor(d.uptime_s / 3600), minutes: Math.floor((d.uptime_s % 3600) / 60) })}</Row>
          <Row label={t('settings.device.ip')}>{d.ip ?? '-'}</Row>
          <Row label={t('settings.device.mac')}>{d.mac}</Row>
          <Row label={t('settings.device.link')}>{d.link_mbps ? `${d.link_mbps} Mbps` : t('settings.device.noLink')}</Row>
          <Row label={t('settings.device.temp')}>{d.soc_temp_c === null ? '-' : `${d.soc_temp_c} °C`}</Row>
          <Row label={t('settings.device.memory')}>{`${bytes(d.mem.used)} / ${bytes(d.mem.total)}`}</Row>
          <Row label={t('settings.device.data')}>{`${bytes(d.data.used)} / ${bytes(d.data.total)}`}</Row>
          <Row label={t('settings.device.lux')}>{d.lux === null ? '-' : `${d.lux} lx`}</Row>
        </Group>
      )}
      {device && (
        <Group title={t('settings.device.settings')}>
          <Row label={t('settings.device.hostname')}>
            <TextField label={t('settings.device.hostname')} value={device.hostname} onCommit={(hostname) => void patch({ hostname: hostname.trim() })} />
          </Row>
          <Row label="SSH">
            <Switch label="SSH" checked={device.ssh} onChange={(ssh) => void patch({ ssh })} />
          </Row>
          <Row label="mDNS">
            <Switch label="mDNS" checked={device.mdns} onChange={(mdns) => void patch({ mdns })} />
          </Row>
          <Row label={t('settings.device.watchdog')}>
            <Switch label={t('settings.device.watchdog')} checked={device.watchdog} onChange={(watchdog) => void patch({ watchdog })} />
          </Row>
        </Group>
      )}
      {host && (
        <Group title={t('settings.host.title')}>
          <Row label={t('settings.host.setup')} hint={t('settings.host.code')}>
            <TextField label={t('settings.host.setup')} value={host.setup_key ?? ''} onCommit={(key) =>
              void run(() => patchHost({ setup_key: key.trim() || null }))} />
          </Row>
          <Row label={t('settings.host.bootMenu')} hint={t('settings.host.code')}>
            <TextField label={t('settings.host.bootMenu')} value={host.boot_menu_key ?? ''} onCommit={(key) =>
              void run(() => patchHost({ boot_menu_key: key.trim() || null }))} />
          </Row>
          <Row label={t('settings.host.interval')}>
            <TextField type="number" min={50} max={1000} label={t('settings.host.interval')} value={String(host.tap_interval_ms)}
              onCommit={(value) => void run(() => patchHost({ tap_interval_ms: Number(value) }))} />
          </Row>
          <Row label={t('settings.host.duration')}>
            <TextField type="number" min={3} max={60} label={t('settings.host.duration')} value={String(host.tap_duration_s)}
              onCommit={(value) => void run(() => patchHost({ tap_duration_s: Number(value) }))} />
          </Row>
        </Group>
      )}
      <Group title={t('settings.device.actions')}>
        <div className={s.actions}>
          {ACTIONS.map(({ action, danger }) => (
            <Button key={action} variant={danger ? 'danger' : 'default'} onClick={() => setPending(action)}>
              {t(`settings.device.action.${action}`)}
            </Button>
          ))}
        </div>
      </Group>
      {kvm?.enabled && (
        <Group title={t('settings.device.kvmSwitch')}>
          <Row label={t('settings.device.kvmEnabled')} hint={t('settings.device.kvmHint')}>
            <Button variant="danger" onClick={() => setPending('kvm_off')}>
              {t('settings.device.kvmDisable')}
            </Button>
          </Row>
        </Group>
      )}
      <Alert message={error} />
      {pending && (
        <ConfirmDialog
          title={t(`settings.device.confirm.${pending}.title`)}
          message={t(`settings.device.confirm.${pending}.message`)}
          confirmLabel={pending === 'kvm_off' ? t('settings.device.kvmDisable') : t(`settings.device.action.${pending}`)}
          danger={pending === 'kvm_off' || pending === 'reboot' || pending === 'shutdown'}
          onCancel={() => setPending(null)}
          onConfirm={() => confirm(pending)}
        />
      )}
    </>
  );
}
