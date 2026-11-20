import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { ME_KEY, fetchMe } from '../../session/useSession';
import { Select, Slider, Switch } from '../../ui';
import { Group, Row, TextField } from '../fields';
import { useConfigSection } from '../useConfigSection';

const LAYOUTS = ['us', 'uk', 'de', 'fr', 'es', 'it', 'jp', 'ru'];
/** Keys that make sense as a browser-shortcut prefix: they are rarely pressed alone. */
const LEADER_KEYS = ['ControlRight', 'AltRight', 'ScrollLock', 'Pause', 'Insert', 'F12'];
const BAUDS = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600, 1500000] as const;

/** `config.hid` and `config.serial`. */
export function InputPage() {
  const { t } = useTranslation();
  const { value: hid, patch } = useConfigSection('hid');
  const { value: serial, patch: patchSerial } = useConfigSection('serial');
  const me = useQuery({ queryKey: ME_KEY, queryFn: fetchMe, retry: false, staleTime: Infinity }).data;
  if (!hid || !serial) return null;
  const canSerial = me?.role === 'admin' || me?.permissions.includes('terminal');

  const withCurrent = (list: string[], cur: string) => (list.includes(cur) ? list : [...list, cur]);
  const leader = hid.leader_key ?? '';

  return (
    <>
      <Group title={t('settings.input.keyboard')}>
        <Row label={t('keyboard.layout')}>
          <Select
            label={t('keyboard.layout')}
            value={hid.keyboard_layout}
            options={withCurrent(LAYOUTS, hid.keyboard_layout).map((l) => ({ value: l, label: t(`keyboard.layouts.${l}`, l) }))}
            onChange={(keyboard_layout) => void patch({ keyboard_layout })}
          />
        </Row>
        <Row label={t('settings.input.leader')} hint={t('settings.input.leaderHint')}>
          <Select
            label={t('settings.input.leader')}
            value={leader}
            options={[{ value: '', label: t('settings.input.leaderNone') }, ...withCurrent(LEADER_KEYS, leader).filter(Boolean).map((k) => ({ value: k, label: k }))]}
            onChange={(v) => void patch({ leader_key: v || null })}
          />
        </Row>
      </Group>
      <Group title={t('settings.input.mouse')}>
        <Row label={t('mouse.mode')}>
          <Select
            label={t('mouse.mode')}
            value={hid.mouse_mode}
            options={[
              { value: 'abs', label: t('mouse.abs') },
              { value: 'rel', label: t('mouse.rel') },
            ]}
            onChange={(mouse_mode) => void patch({ mouse_mode })}
          />
        </Row>
        <Row label={t('mouse.wheelDir')}>
          <Select
            label={t('mouse.wheelDir')}
            value={hid.wheel_dir}
            options={[
              { value: 'normal', label: t('mouse.wheelNormal') },
              { value: 'reverse', label: t('mouse.wheelReverse') },
            ]}
            onChange={(wheel_dir) => void patch({ wheel_dir })}
          />
        </Row>
        <Row label={t('mouse.wheelSpeed')}>
          <Slider label={t('mouse.wheelSpeed')} min={1} max={10} value={hid.wheel_speed} onCommit={(wheel_speed) => void patch({ wheel_speed })} />
        </Row>
        <Row label={t('mouse.jiggler')}>
          <Switch label={t('mouse.jiggler')} checked={hid.jiggler} onChange={(jiggler) => void patch({ jiggler })} />
        </Row>
      </Group>
      {canSerial && (
      <Group title={t('settings.input.serial')}>
        <Row label={t('settings.input.serialDevice')} hint={t('settings.input.serialDeviceHint')}>
          <TextField
            label={t('settings.input.serialDevice')}
            value={serial.device ?? ''}
            placeholder="/dev/ttyS1"
            onCommit={(v) => void patchSerial({ device: v.trim() || null })}
          />
        </Row>
        <Row label={t('settings.input.baud')}>
          <Select
            label={t('settings.input.baud')}
            value={String(serial.baud)}
            options={BAUDS.map((b) => ({ value: String(b), label: String(b) }))}
            onChange={(v) => void patchSerial({ baud: Number(v) as (typeof BAUDS)[number] })}
          />
        </Row>
      </Group>
      )}
    </>
  );
}
