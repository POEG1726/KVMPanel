import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { patchConfig, type Config, type DeepPartial } from '../api/client';
import { errorText, useConfig } from '../console/menus/hooks';
import { CONFIG_KEY } from '../session/useSession';
import { toast } from '../ui';

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** Maps where a null in a patch deletes the key (same rule as the server). */
const DELETABLE = new Set(['panel.thresholds']);

/** Mirrors the server's merge, so the optimistic value equals what the PATCH answer will contain. */
export function applyPatch(target: unknown, patch: unknown, path: string): unknown {
  if (!isObj(patch) || !isObj(target)) return patch;
  const out: Record<string, unknown> = { ...target };
  for (const [k, v] of Object.entries(patch)) {
    if (v === null && DELETABLE.has(path)) delete out[k];
    else out[k] = applyPatch(target[k], v, `${path}.${k}`);
  }
  return out;
}

/**
 * One config section: its current value and `patch(part)`, which updates the cached config at once,
 * PATCHes it, and on failure restores the old value and toasts the error. Resolves true on success.
 */
export function useConfigSection<K extends keyof Config>(section: K) {
  const qc = useQueryClient();
  const { t } = useTranslation();
  const config = useConfig();

  const patch = useCallback(
    async (part: DeepPartial<Config[K]>): Promise<boolean> => {
      const before = qc.getQueryData<Config>(CONFIG_KEY);
      if (before) qc.setQueryData(CONFIG_KEY, { ...before, [section]: applyPatch(before[section], part, section) });
      try {
        qc.setQueryData(CONFIG_KEY, await patchConfig({ [section]: part } as DeepPartial<Config>));
        return true;
      } catch (e) {
        if (before) qc.setQueryData(CONFIG_KEY, before);
        // Another patch may have landed meanwhile; the server's copy settles it.
        void qc.invalidateQueries({ queryKey: CONFIG_KEY });
        toast(errorText(t, e), 'error');
        return false;
      }
    },
    [qc, section, t],
  );

  return { value: config?.[section], patch };
}
