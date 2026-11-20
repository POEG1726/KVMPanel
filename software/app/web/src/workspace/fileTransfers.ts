import { useEffect, useRef, useState } from 'react';
import type { QueryClient } from '@tanstack/react-query';
import { useQueryClient } from '@tanstack/react-query';
import { ApiError } from '../api/client';
import type { components } from '../api/schema';
import i18n from '../i18n';
import { toast } from '../ui';
import { uploadFile } from './upload';

type Target = components['schemas']['PathRef'];
export const childPath = (directory: string, name: string) => (directory === '/' ? '' : directory) + '/' + name;

export function refreshFiles(client: QueryClient, root: Target['root']) {
  void client.invalidateQueries({ queryKey: ['files', root] });
  void client.invalidateQueries({ queryKey: ['storage'] });
  if (root === 'sd') void client.invalidateQueries({ queryKey: ['media', 'images'] });
}

interface Transfer {
  id: number;
  file: File;
  target: Target;
  state: 'queued' | 'hashing' | 'sending' | 'cancelled' | 'failed';
  progress: number;
}

/** Start one worker/PUT at a time. The destination is fixed when enqueued, including on retry. */
export function useFileTransfers() {
  const client = useQueryClient();
  const [jobs, setJobs] = useState<Transfer[]>([]);
  const nextId = useRef(0);
  const active = jobs.find(j => j.state === 'queued' || j.state === 'hashing' || j.state === 'sending');
  const id = active?.id;
  const file = active?.file;
  const target = active?.target;
  useEffect(() => {
    if (id === undefined || !file || !target) return;
    const controller = new AbortController();
    const update = (patch: Partial<Transfer>) => {
      if (!controller.signal.aborted) setJobs(list => list.map(j => j.id === id ? { ...j, ...patch } : j));
    };
    update({ state: 'hashing' });
    void uploadFile(file, target, controller.signal, progress => update({ state: 'sending', progress }))
      .then(() => {
        if (controller.signal.aborted) return;
        refreshFiles(client, target.root);
        setJobs(list => list.filter(j => j.id !== id));
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        update({ state: 'failed' });
        toast(error instanceof ApiError
          ? i18n.t('error.' + error.code, { defaultValue: i18n.t('error.unknown'), ...error.detail })
          : i18n.t('files.transferError', { name: file.name }), 'error');
      });
    return () => controller.abort();
  }, [id, file, target, client]);

  function enqueue(files: ArrayLike<File>, directory: Target) {
    const added = Array.from(files, file => ({
      id: ++nextId.current, file,
      target: { root: directory.root, path: childPath(directory.path, file.name) },
      state: 'queued' as const, progress: 0,
    }));
    setJobs(list => [...list, ...added]);
  }

  function cancel(id: number) {
    setJobs(list => list.map(j => j.id === id ? { ...j, state: 'cancelled' } : j));
  }

  function retry(id: number) {
    setJobs(list => {
      const old = list.find(j => j.id === id);
      return old ? [...list.filter(j => j !== old), { ...old, state: 'queued', progress: 0 }] : list;
    });
  }
  const dismiss = (id: number) => setJobs(list => list.filter(j => j.id !== id));
  return { jobs, enqueue, cancel, retry, dismiss };
}
