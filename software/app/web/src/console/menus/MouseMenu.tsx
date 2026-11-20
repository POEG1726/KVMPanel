import { useTranslation } from 'react-i18next';
import { useEventsContext } from '../../api/EventsProvider';
import { CURSORS, useUi, type CursorStyle } from '../../state/ui';
import { Select, Slider, Switch } from '../../ui';
import { ErrorLine, useConfig, usePatchConfig } from './hooks';
import s from './menus.module.css';

export function MouseMenu() {
  const { t } = useTranslation();
  const { viewOnly } = useEventsContext();
  const hid = useConfig()?.hid;
  const { patch, error } = usePatchConfig();
  const cursor = useUi((u) => u.cursor);
  const setCursor = useUi((u) => u.setCursor);
  const cursorLabel: Record<CursorStyle, string> = {
    hide: t('mouse.cursorHide'),
    default: t('mouse.cursorDefault'),
    cross: t('mouse.cursorCross'),
    dot: t('mouse.cursorDot'),
  };
  return (
    <div className={s.panel}>
      <div className={s.head}>{t('mouse.title')}</div>
      {viewOnly && <p className={s.hint}>{t('console.viewOnlyHint')}</p>}
      {hid && !viewOnly && (
        <>
          <div className={s.row}>
            <span>{t('mouse.mode')}</span>
            <Select
              label={t('mouse.mode')}
              value={hid.mouse_mode}
              options={[
                { value: 'abs', label: t('mouse.abs') },
                { value: 'rel', label: t('mouse.rel') },
              ]}
              onChange={(mouse_mode) => void patch({ hid: { mouse_mode } })}
            />
          </div>
          {hid.mouse_mode === 'rel' && <p className={s.hint}>{t('mouse.relHint')}</p>}
        </>
      )}
      <div className={s.row}>
        <span>{t('mouse.cursor')}</span>
        <Select label={t('mouse.cursor')} value={cursor} options={CURSORS.map((c) => ({ value: c, label: cursorLabel[c] }))} onChange={setCursor} />
      </div>
      {hid && !viewOnly && (
        <>
          <div className={s.row}>
            <span>{t('mouse.wheelDir')}</span>
            <Select
              label={t('mouse.wheelDir')}
              value={hid.wheel_dir}
              options={[
                { value: 'normal', label: t('mouse.wheelNormal') },
                { value: 'reverse', label: t('mouse.wheelReverse') },
              ]}
              onChange={(wheel_dir) => void patch({ hid: { wheel_dir } })}
            />
          </div>
          <div className={s.row}>
            <span>{t('mouse.wheelSpeed')}</span>
            <Slider label={t('mouse.wheelSpeed')} min={1} max={10} value={hid.wheel_speed} onCommit={(wheel_speed) => void patch({ hid: { wheel_speed } })} />
          </div>
          <div className={s.row}>
            <span>{t('mouse.jiggler')}</span>
            <Switch label={t('mouse.jiggler')} checked={hid.jiggler} onChange={(jiggler) => void patch({ hid: { jiggler } })} />
          </div>
        </>
      )}
      <ErrorLine message={error} />
    </div>
  );
}
