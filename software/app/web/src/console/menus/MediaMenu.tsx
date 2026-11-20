import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, unwrap } from '../../api/client';
import { useEventsContext } from '../../api/EventsProvider';
import { useUi } from '../../state/ui';
import { Button, ConfirmDialog, Switch, toast } from '../../ui';
import { ErrorLine, useRun } from './hooks';
import s from './menus.module.css';

const IMAGES_KEY = ['media', 'images'] as const;
const MOUNT_KEY = ['media', 'mounted'] as const;
const JOBS_KEY = ['media', 'jobs'] as const;
const STORAGE_KEY = ['storage'] as const;
const SHARE_KEY = ['share'] as const;

const IMAGE_EXT = /\.(iso|img)$/i;
const MAX_SCAN_DEPTH = 3;
const MAX_SCAN_DIRS = 40;

interface Image {
  path: string;
  name: string;
  size: number;
}

/** SD images are found by walking the card a few levels deep: the contract has no search endpoint. */
async function scanImages(): Promise<Image[]> {
  const found: Image[] = [];
  const queue: { path: string; depth: number }[] = [{ path: '/', depth: 0 }];
  let visited = 0;
  while (queue.length && visited < MAX_SCAN_DIRS) {
    const { path, depth } = queue.shift()!;
    visited += 1;
    const { entries } = await unwrap(api.GET('/api/files', { params: { query: { root: 'sd', path } } }));
    for (const e of entries) {
      const full = `${path === '/' ? '' : path}/${e.name}`;
      if (e.type === 'dir') {
        if (depth + 1 < MAX_SCAN_DEPTH) queue.push({ path: full, depth: depth + 1 });
      } else if (IMAGE_EXT.test(e.name)) {
        found.push({ path: full, name: e.name, size: e.size });
      }
    }
  }
  return found.sort((a, b) => a.path.localeCompare(b.path));
}

/** Decimal units, as SD cards are sold. */
export function formatBytes(n: number): string {
  const units = ['B', 'kB', 'MB', 'GB', 'TB'];
  let v = n;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i += 1;
  }
  return `${i === 0 ? v : v.toFixed(1)} ${units[i]}`;
}

function defaultPath(url: string): string {
  try {
    const last = decodeURIComponent(new URL(url).pathname.split('/').filter(Boolean).at(-1) ?? '');
    return `/images/${last || 'download.iso'}`;
  } catch {
    return '/images/download.iso';
  }
}

export function MediaMenu() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { status } = useEventsContext();
  const { error, run } = useRun();
  const openWorkspace = useUi((u) => u.openWorkspace);
  const [ejecting, setEjecting] = useState(false);
  const [url, setUrl] = useState('');
  const [path, setPath] = useState<string | null>(null);

  const images = useQuery({ queryKey: IMAGES_KEY, queryFn: scanImages });
  const mounted = useQuery({ queryKey: MOUNT_KEY, queryFn: () => unwrap(api.GET('/api/media')) });
  const storage = useQuery({ queryKey: STORAGE_KEY, queryFn: () => unwrap(api.GET('/api/storage')) });
  const share = useQuery({ queryKey: SHARE_KEY, queryFn: () => unwrap(api.GET('/api/share')) });
  const jobs = useQuery({
    queryKey: JOBS_KEY,
    queryFn: async () => (await unwrap(api.GET('/api/media/jobs'))).jobs,
    refetchInterval: (q) => (q.state.data?.some((j) => j.state === 'running') ? 1000 : false),
  });

  // Another session (or the PC) can change the mount; the status stream tells us.
  const mountedName = status?.media.mounted ?? null;
  useEffect(() => {
    void qc.invalidateQueries({ queryKey: MOUNT_KEY });
    void qc.invalidateQueries({ queryKey: STORAGE_KEY });
  }, [mountedName, qc]);

  const sd = storage.data?.find((v) => v.root === 'sd');
  const current = mounted.data?.mounted ?? null;
  const modeLabel = (m: 'cdrom' | 'disk') => t(m === 'cdrom' ? 'media.cdrom' : 'media.disk');
  const refreshMount = () => qc.invalidateQueries({ queryKey: MOUNT_KEY });
  const refreshJobs = () => qc.invalidateQueries({ queryKey: JOBS_KEY });

  const mount = (image: Image, mode: 'cdrom' | 'disk') =>
    void run(async () => {
      await unwrap(api.POST('/api/media/mount', { body: { root: 'sd', path: image.path, mode } }));
      await refreshMount();
    });

  const eject = () => {
    setEjecting(false);
    void run(async () => {
      await unwrap(api.POST('/api/media/eject'));
      await refreshMount();
    });
  };

  const download = () =>
    void run(async () => {
      await unwrap(api.POST('/api/media/download', { body: { url: url.trim(), path: path ?? defaultPath(url) } }));
      setUrl('');
      setPath(null);
      await refreshJobs();
    });

  const control = (id: string, action: 'pause' | 'resume' | 'cancel') =>
    void run(async () => {
      await unwrap(api.POST('/api/media/jobs/{id}/{action}', { params: { path: { id, action } } }));
      await refreshJobs();
      if (action === 'cancel') toast(t('media.job.canceled'));
    });

  const toggleShare = (exported_to_host: boolean) =>
    void run(async () => qc.setQueryData(SHARE_KEY, await unwrap(api.POST('/api/share', { body: { exported_to_host } }))));

  return (
    <div className={s.panel}>
      <div className={s.head}>
        {t('media.title')}
        {sd && ` · ${t('media.sdSpace', { free: formatBytes(sd.free), total: formatBytes(sd.total) })}`}
      </div>

      {images.data?.length === 0 && <p className={s.hint}>{t('media.none')}</p>}
      <ul className={s.list}>
        {images.data?.map((img) => {
          const isMounted = current?.path === img.path;
          return (
            <li key={img.path} className={s.item}>
              <span className={s.itemMain}>
                <span>{img.name}</span>
                <span className={s.value}>
                  {formatBytes(img.size)}
                  {isMounted && current && (
                    <>
                      {' · '}
                      {modeLabel(current.mode)}
                      {' · '}
                      <span className={s.itemSub}>{t('media.mounted')}</span>
                    </>
                  )}
                </span>
              </span>
              <span className={s.itemActions}>
                {isMounted ? (
                  <Button onClick={() => setEjecting(true)}>{t('media.eject')}</Button>
                ) : (
                  (['cdrom', 'disk'] as const).map((mode) => (
                    <Button key={mode} aria-label={t('media.mountAs', { name: img.name, mode: modeLabel(mode) })} onClick={() => mount(img, mode)}>
                      {modeLabel(mode)}
                    </Button>
                  ))
                )}
              </span>
            </li>
          );
        })}
      </ul>

      <div className={s.row}>
        <Button onClick={openWorkspace}>{t('media.upload')}</Button>
      </div>

      <div className={s.form}>
        <input aria-label={t('media.url')} placeholder={t('media.urlPlaceholder')} value={url} onChange={(e) => setUrl(e.target.value)} />
        <input aria-label={t('media.path')} value={path ?? defaultPath(url)} onChange={(e) => setPath(e.target.value)} />
        <Button variant="primary" disabled={!url.trim()} onClick={download}>
          {t('media.download')}
        </Button>
      </div>

      {jobs.data && jobs.data.length > 0 && (
        <>
          <div className={s.head}>{t('media.jobs')}</div>
          <ul className={s.list}>
            {jobs.data.map((j) => (
              <li key={j.id} className={s.item}>
                <span className={s.itemMain}>
                  <span>{j.path.split('/').at(-1)}</span>
                  <span className={s.value}>
                    {t(`media.job.${j.state}`)}
                    {j.total > 0 && ` · ${Math.floor((j.received / j.total) * 100)}%`}
                  </span>
                  {j.total > 0 && (
                    <span className={s.progress}>
                      <i style={{ width: `${Math.min(100, (j.received / j.total) * 100)}%` }} />
                    </span>
                  )}
                </span>
                {(j.state === 'running' || j.state === 'paused') && (
                  <span className={s.itemActions}>
                    <Button key="toggle" onClick={() => control(j.id, j.state === 'running' ? 'pause' : 'resume')}>
                      {t(j.state === 'running' ? 'media.job.pause' : 'media.job.resume')}
                    </Button>
                    <Button key="cancel" onClick={() => control(j.id, 'cancel')}>
                      {t('media.job.cancel')}
                    </Button>
                  </span>
                )}
              </li>
            ))}
          </ul>
        </>
      )}

      <div className={s.row}>
        <span>
          {t('media.share')}
          <span className={s.hint} style={{ display: 'block', margin: 0 }}>
            {t('media.shareHint')}
          </span>
        </span>
        <Switch label={t('media.share')} checked={share.data?.exported_to_host ?? false} disabled={!share.data} onChange={toggleShare} />
      </div>

      <ErrorLine message={error} />
      {ejecting && (
        <ConfirmDialog title={t('media.ejectTitle')} message={t('media.ejectMessage')} confirmLabel={t('media.eject')} danger onConfirm={eject} onCancel={() => setEjecting(false)} />
      )}
    </div>
  );
}
