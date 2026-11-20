import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText } from '../../console/menus/hooks';
import { Alert, Group, Row, bytes } from '../fields';

/** Capacity of the file-manager roots (SD card, share, system). */
export function StoragePage() {
  const { t } = useTranslation();
  const volumes = useQuery({ queryKey: ['storage'], queryFn: () => unwrap(api.GET('/api/storage')) });
  return (
    <Group>
      <Alert message={volumes.error ? errorText(t, volumes.error) : null} />
      {volumes.data?.map((v) => (
        <Row key={v.root} label={t(`settings.storage.root.${v.root}`)} hint={v.mounted ? undefined : t('settings.storage.notMounted')}>
          <span>{t('settings.storage.usage', { free: bytes(v.free), total: bytes(v.total) })}</span>
        </Row>
      ))}
    </Group>
  );
}
