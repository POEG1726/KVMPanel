import { useEffect, useState, type ChangeEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText, useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Select, toast } from '../../ui';
import { Alert, Group, Row, TextField } from '../fields';
import s from '../settings.module.css';

const UPDATE_KEY = ['update'] as const;
export const JOB_POLL_MS = 1000;

async function sha256Hex(file: Blob): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** Update channel and server, online check/install, and offline package upload. */
export function UpdatePage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const info = useQuery({ queryKey: UPDATE_KEY, queryFn: () => unwrap(api.GET('/api/update')) });
  const { error, run } = useRun();
  const [jobId, setJobId] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [offlineFile, setOfflineFile] = useState<File | null>(null);
  const job = useQuery({
    queryKey: ['update', 'job', jobId],
    queryFn: async () => (await unwrap(api.GET('/api/update/jobs/{id}', { params: { path: { id: jobId! } } }))).job,
    enabled: !!jobId,
    refetchInterval: (q) => (q.state.data?.state === 'running' || !q.state.data ? JOB_POLL_MS : false),
  });
  const done = job.data?.state === 'done';
  useEffect(() => {
    if (done) toast(t('settings.update.installed'));
  }, [done, t]);

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
  const install = () =>
    run(async () => {
      setJobId((await unwrap(api.POST('/api/update/apply'))).job.id);
    });
  const pick = (e: ChangeEvent<HTMLInputElement>) => {
    setOfflineFile(e.target.files?.[0] ?? null);
    e.target.value = '';
  };
  const upload = async (file: File) => {
    await run(async () => {
      const sha = await sha256Hex(file);
      const r = await unwrap(
        api.PUT('/api/update/offline', { params: { header: { 'X-Sha256': sha } }, body: file as never, bodySerializer: (b: unknown) => b as never, headers: { 'content-type': 'application/octet-stream' } }),
      );
      setJobId(r.job.id);
    });
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
            <Button variant="primary" disabled={job.data?.state === 'running'} onClick={() => setConfirming(true)}>
              {t('settings.update.install')}
            </Button>
          </div>
          <Group title={t('settings.update.offline')}>
            <Row label={t('settings.update.offlineFile')} hint={t('settings.update.offlineHint')}>
              <input type="file" aria-label={t('settings.update.offlineFile')} accept=".tar,application/x-tar" onChange={pick} />
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
      <Alert message={error} />
      {offlineFile && (
        <ConfirmDialog
          title={t('settings.update.installTitle')}
          message={t('settings.update.installMessage')}
          confirmLabel={t('settings.update.install')}
          onCancel={() => setOfflineFile(null)}
          onConfirm={() => {
            const file = offlineFile;
            setOfflineFile(null);
            void upload(file);
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
