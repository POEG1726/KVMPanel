import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText, useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Select, Slider, Switch } from '../../ui';
import { Alert, Group, Row } from '../fields';
import { useConfigSection } from '../useConfigSection';
import s from '../settings.module.css';

const FPS = [5, 10, 15, 20, 25, 30, 45, 60];
const EDID_KEY = ['edid'] as const;

/** `config.video`, plus the EDID the PC sees on the HDMI input. */
export function VideoPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { value: video, patch } = useConfigSection('video');
  const edid = useQuery({ queryKey: EDID_KEY, queryFn: () => unwrap(api.GET('/api/video/edid')) });
  const { error, run } = useRun();
  const [hex, setHex] = useState('');
  const [confirm, setConfirm] = useState<'apply' | 'reset' | null>(null);
  useEffect(() => setHex(edid.data?.hex ?? ''), [edid.data?.hex]);

  const clean = hex.replace(/\s+/g, '').toLowerCase();
  const act = (kind: 'apply' | 'reset') =>
    run(async () => {
      const next =
        kind === 'apply'
          ? await unwrap(api.PUT('/api/video/edid', { body: { hex: clean } }))
          : await unwrap(api.POST('/api/video/edid/reset'));
      qc.setQueryData(EDID_KEY, next);
    });

  if (!video) return null;
  return (
    <>
      <Group>
        <Row label={t('video.quality')}>
          <Slider label={t('video.quality')} min={50} max={100} value={video.quality} onCommit={(quality) => void patch({ quality })} />
        </Row>
        <Row label={t('video.fpsMax')}>
          <Select
            label={t('video.fpsMax')}
            value={String(video.fps_max)}
            options={(FPS.includes(video.fps_max) ? FPS : [...FPS, video.fps_max].sort((a, b) => a - b)).map((n) => ({ value: String(n), label: String(n) }))}
            onChange={(v) => void patch({ fps_max: Number(v) })}
          />
        </Row>
        <Row label={t('video.scale')}>
          <Select
            label={t('video.scale')}
            value={video.scale}
            options={[
              { value: 'fit', label: t('video.scaleFit') },
              { value: '1:1', label: t('video.scaleOne') },
              { value: 'fill', label: t('video.scaleFill') },
            ]}
            onChange={(scale) => void patch({ scale })}
          />
        </Row>
        <Row label={t('video.frameDiff')}>
          <Switch label={t('video.frameDiff')} checked={video.frame_diff} onChange={(frame_diff) => void patch({ frame_diff })} />
        </Row>
      </Group>
      <Group title={t('settings.video.edid')}>
        <Alert message={edid.error ? errorText(t, edid.error) : null} />
        {edid.data && (
          <>
            <Row label={t('settings.video.edidPreset')} hint={t('settings.video.edidHint')}>
              <span>{edid.data.preset === '1080p' ? t('settings.video.preset1080p') : t('settings.video.presetCustom')}</span>
            </Row>
            <textarea className={s.textarea} aria-label={t('settings.video.edidHex')} value={hex} onChange={(e) => setHex(e.target.value)} spellCheck={false} />
            <div className={s.actions}>
              <Button variant="primary" disabled={clean === edid.data.hex} onClick={() => setConfirm('apply')}>
                {t('settings.video.edidApply')}
              </Button>
              <Button onClick={() => setConfirm('reset')}>{t('settings.video.edidReset')}</Button>
            </div>
          </>
        )}
        <Alert message={error} />
      </Group>
      {confirm && (
        <ConfirmDialog
          title={t(`settings.video.${confirm === 'apply' ? 'edidApplyTitle' : 'edidResetTitle'}`)}
          message={t('settings.video.edidConfirmMessage')}
          confirmLabel={t(confirm === 'apply' ? 'settings.video.edidApply' : 'settings.video.edidReset')}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            const kind = confirm;
            setConfirm(null);
            void act(kind);
          }}
        />
      )}
    </>
  );
}
