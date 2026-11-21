import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText, useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Switch } from '../../ui';
import { Alert, Group, Row } from '../fields';

const SHARE_KEY = ['share'] as const;
const MEDIA_KEY = ['media'] as const;

/** Virtual media and the file-share disk: what the PC currently sees. */
export function MediaPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const share = useQuery({ queryKey: SHARE_KEY, queryFn: () => unwrap(api.GET('/api/share')) });
  const media = useQuery({ queryKey: MEDIA_KEY, queryFn: () => unwrap(api.GET('/api/media')) });
  const { error, run } = useRun();
  const [ejecting, setEjecting] = useState(false);

  const setShare = (exported_to_host: boolean) =>
    run(async () => qc.setQueryData(SHARE_KEY, await unwrap(api.POST('/api/share', { body: { exported_to_host } }))));
  const eject = () =>
    run(async () => {
      await unwrap(api.POST('/api/media/eject'));
      await qc.invalidateQueries({ queryKey: MEDIA_KEY });
    });

  const mounted = media.data?.mounted;
  return (
    <>
      <Group title={t('settings.media.share')}>
        <Row label={t('media.share')} hint={t('media.shareHint')}>
          <Switch label={t('media.share')} checked={!!share.data?.exported_to_host} disabled={!share.data} onChange={(on) => void setShare(on)} />
        </Row>
      </Group>
      <Group title={t('settings.media.mounted')}>
        <Row label={mounted ? mounted.path : t('settings.media.nothing')} hint={mounted ? t('media.mountedMode', { mode: t(`media.${mounted.mode}`) }) : undefined}>
          {mounted && <Button onClick={() => setEjecting(true)}>{t('media.eject')}</Button>}
        </Row>
      </Group>
      <Alert message={error ?? (share.error ? errorText(t, share.error) : null)} />
      {ejecting && (
        <ConfirmDialog
          title={t('media.ejectTitle')}
          message={t('media.ejectMessage')}
          confirmLabel={t('media.eject')}
          danger
          onCancel={() => setEjecting(false)}
          onConfirm={() => {
            setEjecting(false);
            void eject();
          }}
        />
      )}
    </>
  );
}
