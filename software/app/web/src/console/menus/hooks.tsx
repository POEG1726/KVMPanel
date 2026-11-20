import { useCallback, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { api, ApiError, errorKey, patchConfig, unwrap, type Config, type DeepPartial } from '../../api/client';
import { CONFIG_KEY } from '../../session/useSession';
import s from './menus.module.css';

export function useConfig(): Config | undefined {
  return useQuery({ queryKey: CONFIG_KEY, queryFn: () => unwrap(api.GET('/api/config')), staleTime: Infinity }).data;
}

/** Translates a failure: the error code's message (with the contract's `detail` as parameters), else a generic one. */
export function errorText(t: TFunction, e: unknown): string {
  return e instanceof ApiError ? t(errorKey(e), { defaultValue: t('error.unknown'), ...e.detail }) : t('error.unknown');
}

/** Runs async actions and keeps the translated message of the last failure for an inline alert. */
export function useRun() {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(
    async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
      setError(null);
      try {
        return await fn();
      } catch (e) {
        setError(errorText(t, e));
        return undefined;
      }
    },
    [t],
  );
  return { error, run, clear: () => setError(null) };
}

/** PATCHes the config and stores the answer, so every panel (and the video canvas) sees the new values. */
export function usePatchConfig() {
  const qc = useQueryClient();
  const { error, run } = useRun();
  const patch = useCallback(
    (body: DeepPartial<Config>) => run(async () => qc.setQueryData(CONFIG_KEY, await patchConfig(body))),
    [qc, run],
  );
  return { patch, error };
}

export function ErrorLine({ message }: { message: string | null }) {
  return message ? (
    <p role="alert" className={s.error}>
      {message}
    </p>
  ) : null;
}
