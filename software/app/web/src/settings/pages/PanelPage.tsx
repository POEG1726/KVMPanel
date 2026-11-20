import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { THRESHOLD_DELETE } from '../../api/client';
import { Button, Select, Slider, Switch } from '../../ui';
import { Group, Row, TextField } from '../fields';
import { METRIC_GROUPS } from '../registry';
import { useConfigSection } from '../useConfigSection';
import s from '../settings.module.css';

const SLEEP_MINUTES = [0, 1, 2, 5, 10, 15, 30, 60, 120];
/** Suggestions for the threshold key: every metric id and each `prefix.*` that groups some of them. */
const KEY_SUGGESTIONS = [...new Set(METRIC_GROUPS.flatMap((g) => g.metrics.flatMap((m) => [m, `${m.slice(0, m.lastIndexOf('.'))}.*`])))].filter((k) => k !== '.*').sort();

/** `config.panel`: brightness, offline sleep, warn thresholds and hidden metrics. */
export function PanelPage() {
  const { t } = useTranslation();
  const { value: panel, patch } = useConfigSection('panel');
  const [key, setKey] = useState('');
  const [limit, setLimit] = useState('');
  if (!panel) return null;

  const sleepOptions = SLEEP_MINUTES.includes(panel.offline_sleep_min) ? SLEEP_MINUTES : [...SLEEP_MINUTES, panel.offline_sleep_min].sort((a, b) => a - b);
  const thresholds = Object.entries(panel.thresholds).sort(([a], [b]) => a.localeCompare(b));
  const hidden = new Set(panel.hidden_metrics);

  const addThreshold = (e: FormEvent) => {
    e.preventDefault();
    const value = Number(limit);
    if (!key.trim() || limit.trim() === '' || Number.isNaN(value)) return;
    void patch({ thresholds: { [key.trim()]: value } }).then((ok) => {
      if (ok) {
        setKey('');
        setLimit('');
      }
    });
  };

  const toggleHidden = (id: string, hide: boolean) => {
    const next = new Set(hidden);
    if (hide) next.add(id);
    else next.delete(id);
    void patch({ hidden_metrics: [...next] });
  };

  return (
    <>
      <Group title={t('settings.panel.display')}>
        <Row label={t('settings.panel.autoBrightness')}>
          <Switch label={t('settings.panel.autoBrightness')} checked={panel.auto_brightness} onChange={(auto_brightness) => void patch({ auto_brightness })} />
        </Row>
        {!panel.auto_brightness && (
          <Row label={t('settings.panel.brightness')}>
            <Slider label={t('settings.panel.brightness')} min={0} max={100} value={panel.brightness} onCommit={(brightness) => void patch({ brightness })} />
          </Row>
        )}
        <Row label={t('settings.panel.offlineSleep')} hint={t('settings.panel.offlineSleepHint')}>
          <Select
            label={t('settings.panel.offlineSleep')}
            value={String(panel.offline_sleep_min)}
            options={sleepOptions.map((m) => ({ value: String(m), label: m === 0 ? t('settings.panel.never') : t('settings.panel.minutes', { n: m }) }))}
            onChange={(v) => void patch({ offline_sleep_min: Number(v) })}
          />
        </Row>
      </Group>

      <Group title={t('settings.panel.thresholds')}>
        <p className={s.muted}>{t('settings.panel.thresholdsHint')}</p>
        <table className={s.table}>
          <tbody>
            {thresholds.map(([k, v]) => (
              <tr key={k}>
                <td className={s.mono}>{k}</td>
                <td>
                  <TextField
                    type="number"
                    width={90}
                    label={t('settings.panel.thresholdValue', { key: k })}
                    value={String(v)}
                    onCommit={(next) => next.trim() !== '' && !Number.isNaN(Number(next)) && void patch({ thresholds: { [k]: Number(next) } })}
                  />
                </td>
                <td>
                  <Button aria-label={t('settings.panel.thresholdDelete', { key: k })} onClick={() => void patch({ thresholds: { [k]: THRESHOLD_DELETE } })}>
                    {t('common.delete')}
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <form className={s.actions} onSubmit={addThreshold}>
          <input className={s.input} list="metric-keys" aria-label={t('settings.panel.thresholdKey')} placeholder="gpu.temp / mem.dimm.*" value={key} onChange={(e) => setKey(e.target.value)} />
          <datalist id="metric-keys">
            {KEY_SUGGESTIONS.map((k) => (
              <option key={k} value={k} />
            ))}
          </datalist>
          <input className={s.input} style={{ width: 90 }} type="number" aria-label={t('settings.panel.thresholdNew')} value={limit} onChange={(e) => setLimit(e.target.value)} />
          <Button type="submit" variant="primary">
            {t('settings.panel.thresholdAdd')}
          </Button>
        </form>
      </Group>

      <Group title={t('settings.panel.hidden')}>
        <p className={s.muted}>{t('settings.panel.hiddenHint')}</p>
        {METRIC_GROUPS.map((g) => (
          <fieldset key={g.id} style={{ border: 0, padding: 0, margin: '0 0 8px' }}>
            <legend className={s.muted}>{t(`settings.panel.group.${g.id}`)}</legend>
            <div className={s.checks}>
              {g.metrics.map((m) => (
                <label key={m}>
                  <input type="checkbox" checked={hidden.has(m)} onChange={(e) => toggleHidden(m, e.target.checked)} />
                  {m}
                </label>
              ))}
            </div>
          </fieldset>
        ))}
      </Group>
    </>
  );
}
