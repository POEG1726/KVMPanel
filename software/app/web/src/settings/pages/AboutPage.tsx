import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { Group, Row } from '../fields';

export function AboutPage() {
  const { t } = useTranslation();
  const about = useQuery({ queryKey: ['about'], queryFn: () => unwrap(api.GET('/api/about')) });
  if (!about.data) return null;
  return (
    <Group>
      <Row label={t('settings.about.name')}>{about.data.name}</Row>
      <Row label={t('settings.about.version')}>{about.data.version}</Row>
      <Row label={t('settings.about.licenses')}>
        <a href={about.data.licenses_url} target="_blank" rel="noreferrer">
          {t('settings.about.open')}
        </a>
      </Row>
    </Group>
  );
}
