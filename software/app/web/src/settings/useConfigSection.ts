import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { Config, DeepPartial } from '../api/client';
import { errorText, useConfig } from '../console/menus/hooks';
import { writeConfig } from '../api/configWrites';
import { toast } from '../ui';

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
      try {
        await writeConfig(qc, { [section]: part } as DeepPartial<Config>);
        return true;
      } catch (e) {
        toast(errorText(t, e), 'error');
        return false;
      }
    },
    [qc, section, t],
  );

  return { value: config?.[section], patch };
}
