import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { api, unwrap } from '../api/client';
import { VideoCanvas } from '../console/VideoCanvas';
import { MenuBar } from '../console/MenuBar';
import { CONFIG_KEY } from '../session/useSession';
import { CURSOR_CSS, useUi } from '../state/ui';
import { ToastHost } from '../ui';

/** The remote screen with its floating menu bar; floating windows arrive in a later task. */
export function ConsolePage() {
  const { t } = useTranslation();
  const config = useQuery({ queryKey: CONFIG_KEY, queryFn: () => unwrap(api.GET('/api/config')), staleTime: Infinity });
  const cursor = useUi((u) => u.cursor);
  return (
    <main style={{ height: '100vh', overflow: 'auto', background: '#000' }}>
      <h1 style={{ position: 'absolute', width: 1, height: 1, overflow: 'hidden', clip: 'rect(0 0 0 0)' }}>{t('console.title')}</h1>
      <VideoCanvas scale={config.data?.video.scale ?? 'fit'} cursor={CURSOR_CSS[cursor]} />
      <MenuBar />
      <ToastHost />
    </main>
  );
}
