import { useEffect, useState, type ChangeEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { ApiError, api, unwrap } from '../../api/client';
import { sha256File } from '../../api/fileHash';
import { errorText, useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Select, toast } from '../../ui';
import { Alert, Group, Row, TextField } from '../fields';
import s from '../settings.module.css';

const UPDATE_KEY = ['update'] as const;
export const JOB_POLL_MS = 1000;

const ACTIVE_JOB_KEY = ['update', 'activeJob'] as const;
const NOTIFIED_JOB_KEY = ['update', 'notifiedJob'] as const;
const MAX_PACKAGE_BYTES = 256 * 1024 * 1024;

/** Update channel and server, online check/install, and offline package upload. */
export function UpdatePage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const info = useQuery({ queryKey: UPDATE_KEY, queryFn: () => unwrap(api.GET('/api/update')) });
  const { error, run } = useRun();
  const [jobId, setJobId] = useState(() => qc.getQueryData<string>(ACTIVE_JOB_KEY));
  const [submitting, setSubmitting] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [offlineFile, setOfflineFile] = useState<File | null>(null);
  const job = useQuery({
    queryKey: ['update', 'job', jobId],
    queryFn: async () => (await unwrap(api.GET('/api/update/jobs/{id}', { params: { path: { id: jobId! } } }))).job,
    enabled: !!jobId,
    refetchInterval: (q) => (q.state.data?.state === 'running' || !q.state.data ? JOB_POLL_MS : false),
  });
  const completedId = job.data?.state === 'done' ? job.data.id : undefined;
  useEffect(() => {
    if (completedId && qc.getQueryData(NOTIFIED_JOB_KEY) !== completedId) {
      qc.setQueryData(NOTIFIED_JOB_KEY, completedId);
      toast(t('settings.update.installed'));
    }
  }, [completedId, qc, t]);
  const busy = submitting || (!!jobId && (job.isPending || job.data?.state === 'running'));

  const d = info.data;
  const saveSettings = (part: Partial<{ channel: 'stable' | 'preview'; server: string }>) =>
    d &&
    run(async () => {
      const next = await unwrap(api.PUT('/api/update/settings', { body: { channel: d.channel, server: d.server, ...part } }));
      qc.setQueryData(UPDATE_KEY, { ...d, ...next });
    });
  const check = () =>
    run(async () => {
      const r = await unwrap(api.POST('/api/update/check'));
      if (d) qc.setQueryData(UPDATE_KEY, { ...d, ...r });
    });
  const pick = (e: ChangeEvent<HTMLInputElement>) => {
    setOfflineFile(e.target.files?.[0] ?? null);
    e.target.value = '';
  };
  const install = async (file?: File) => {
    setSubmitting(true);
    try {
      await run(async () => {
        if (file && file.size > MAX_PACKAGE_BYTES) throw new ApiError(413, 'too_large');
        const result = file
          ? await unwrap(api.PUT('/api/update/offline', {
              params: { header: { 'X-Sha256': await sha256File(file) } },
              body: file as never, bodySerializer: () => file,
              headers: { 'content-type': 'application/octet-stream' },
            }))
          : await unwrap(api.POST('/api/update/apply'));
        qc.setQueryData(ACTIVE_JOB_KEY, result.job.id);
        setJobId(result.job.id);
      });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <>
      <Alert message={info.error ? errorText(t, info.error) : null} />
      {d && (
        <>
          <Group>
            <Row label={t('settings.update.current')}>{d.current}</Row>
            <Row label={t('settings.update.latest')}>{d.latest ?? t('settings.update.unknown')}</Row>
            <Row label={t('settings.update.channel')}>
              <Select
                label={t('settings.update.channel')}
                value={d.channel}
                options={[
                  { value: 'stable', label: t('settings.update.stable') },
                  { value: 'preview', label: t('settings.update.preview') },
                ]}
                onChange={(channel) => void saveSettings({ channel })}
              />
            </Row>
            <Row label={t('settings.update.server')}>
              <TextField label={t('settings.update.server')} width={280} value={d.server} onCommit={(server) => void saveSettings({ server: server.trim() })} />
            </Row>
          </Group>
          <div className={s.actions}>
            <Button onClick={() => void check()}>{t('settings.update.check')}</Button>
            <Button variant="primary" disabled={busy} onClick={() => setConfirming(true)}>
              {t('settings.update.install')}
            </Button>
          </div>
          <Group title={t('settings.update.offline')}>
            <Row label={t('settings.update.offlineFile')} hint={t('settings.update.offlineHint')}>
              <input type="file" aria-label={t('settings.update.offlineFile')} accept=".tar,application/x-tar" disabled={busy} onChange={pick} />
            </Row>
          </Group>
        </>
      )}
      {job.data && (
        <Group title={t('settings.update.progress')}>
          <div className={s.progress} role="progressbar" aria-valuenow={job.data.progress} aria-valuemin={0} aria-valuemax={100}>
            <i style={{ width: `${job.data.progress}%` }} />
          </div>
          <span>{t(`settings.update.state.${job.data.state}`)}</span>
        </Group>
      )}
      <Alert message={error ?? (job.error ? errorText(t, job.error) : null)} />
      {offlineFile && (
        <ConfirmDialog
          title={t('settings.update.installTitle')}
          message={t('settings.update.installMessage')}
          confirmLabel={t('settings.update.install')}
          onCancel={() => setOfflineFile(null)}
          onConfirm={() => {
            const file = offlineFile;
            setOfflineFile(null);
            void install(file);
          }}
        />
      )}
      {confirming && (
        <ConfirmDialog
          title={t('settings.update.installTitle')}
          message={t('settings.update.installMessage')}
          confirmLabel={t('settings.update.install')}
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            setConfirming(false);
            void install();
          }}
        />
      )}
    </>
  );
}
