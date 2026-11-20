import { useTranslation } from 'react-i18next';
import { useEventsContext } from '../../api/EventsProvider';
import { Select, Slider, Switch } from '../../ui';
import { ErrorLine, useConfig, usePatchConfig } from './hooks';
import s from './menus.module.css';

const FPS = [5, 10, 15, 20, 25, 30, 45, 60];

export function VideoMenu() {
  const { t } = useTranslation();
  const { status } = useEventsContext();
  const video = useConfig()?.video;
  const { patch, error } = usePatchConfig();
  const hdmi = status?.hdmi;
  return (
    <div className={s.panel}>
      <div className={s.head}>{t('video.title')}</div>
      <div className={s.row}>
        <span>{t('video.resolution')}</span>
        <span className={s.value}>
          {hdmi?.signal ? `${t('video.followHdmi')} · ${hdmi.width}×${hdmi.height}@${hdmi.fps}` : t('menu.noSignal')}
        </span>
      </div>
      {video && (
        <>
          <div className={s.row}>
            <span>{t('video.quality')}</span>
            <Slider label={t('video.quality')} min={50} max={100} value={video.quality} onCommit={(quality) => void patch({ video: { quality } })} />
          </div>
          <div className={s.row}>
            <span>{t('video.fpsMax')}</span>
            <Select
              label={t('video.fpsMax')}
              value={String(video.fps_max)}
              options={(FPS.includes(video.fps_max) ? FPS : [...FPS, video.fps_max].sort((a, b) => a - b)).map((n) => ({ value: String(n), label: String(n) }))}
              onChange={(v) => void patch({ video: { fps_max: Number(v) } })}
            />
          </div>
          <div className={s.row}>
            <span>{t('video.scale')}</span>
            <Select
              label={t('video.scale')}
              value={video.scale}
              options={[
                { value: 'fit', label: t('video.scaleFit') },
                { value: '1:1', label: t('video.scaleOne') },
                { value: 'fill', label: t('video.scaleFill') },
              ]}
              onChange={(scale) => void patch({ video: { scale } })}
            />
          </div>
          <div className={s.row}>
            <span>{t('video.frameDiff')}</span>
            <Switch label={t('video.frameDiff')} checked={video.frame_diff} onChange={(frame_diff) => void patch({ video: { frame_diff } })} />
          </div>
        </>
      )}
      <ErrorLine message={error} />
    </div>
  );
}
