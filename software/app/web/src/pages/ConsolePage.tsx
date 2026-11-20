import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { api, unwrap } from '../api/client';
import { VideoCanvas } from '../console/VideoCanvas';
import { CONFIG_KEY } from '../session/useSession';

/** The remote screen; the menu bar and floating windows arrive in later tasks. */
export function ConsolePage() {
  const { t } = useTranslation();
  const config = useQuery({ queryKey: CONFIG_KEY, queryFn: () => unwrap(api.GET('/api/config')), staleTime: Infinity });
  return (
    <main style={{ height: '100vh', overflow: 'auto', background: '#000' }}>
      <h1 style={{ position: 'absolute', width: 1, height: 1, overflow: 'hidden', clip: 'rect(0 0 0 0)' }}>{t('console.title')}</h1>
      <VideoCanvas scale={config.data?.video.scale ?? 'fit'} />
    </main>
  );
}
