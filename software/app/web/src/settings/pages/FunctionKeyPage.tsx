import { useTranslation } from 'react-i18next';
import type { components } from '../../api/schema';
import { Select } from '../../ui';
import { Group, Row } from '../fields';
import { useConfigSection } from '../useConfigSection';

type Action = components['schemas']['FunctionKeyAction'];
const ACTIONS: readonly Action[] = ['none', 'toggle_settings', 'toggle_screen', 'cycle_brightness', 'pause_kvm', 'stop_mcp'];

/** `config.function_key`: what a short and a long press of the panel's key do. */
export function FunctionKeyPage() {
  const { t } = useTranslation();
  const { value, patch } = useConfigSection('function_key');
  if (!value) return null;
  const options = ACTIONS.map((a) => ({ value: a, label: t(`settings.functionKey.action.${a}`) }));
  return (
    <Group>
      <Row label={t('settings.functionKey.short')}>
        <Select label={t('settings.functionKey.short')} value={value.short} options={options} onChange={(short) => void patch({ short })} />
      </Row>
      <Row label={t('settings.functionKey.long')}>
        <Select label={t('settings.functionKey.long')} value={value.long} options={options} onChange={(long) => void patch({ long })} />
      </Row>
    </Group>
  );
}
