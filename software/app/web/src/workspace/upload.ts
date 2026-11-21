import type { components } from '../api/schema';
import { ApiError } from '../api/client';
import { emitLoginRequired } from '../api/authEvents';

type Target = components['schemas']['PathRef'];

function checksum(file: File, signal: AbortSignal): Promise<string> {
  return new Promise((resolve, reject) => {
    signal.throwIfAborted();
    const worker = new Worker(new URL('./hash.worker.ts', import.meta.url), { type: 'module' });
    const stop = () => {
      worker.terminate();
      signal.removeEventListener('abort', abort);
    };
    const abort = () => { stop(); reject(signal.reason); };
    signal.addEventListener('abort', abort, { once: true });
    worker.onmessage = ({ data }: MessageEvent<string | null>) => {
      stop();
      if (data) resolve(data);
      else reject(new Error('hash_failed'));
    };
    worker.onerror = () => { stop(); reject(new Error('hash_failed')); };
    worker.postMessage(file);
  });
}

/** The server commits only a complete, verified PUT. Never delete the target after failure:
 * a previous file with the same name may still exist. XHR supplies upload progress and abort. */
export async function uploadFile(file: File, target: Target, signal: AbortSignal, progress: (percent: number) => void) {
  const digest = await checksum(file, signal);
  signal.throwIfAborted();
  await new Promise<void>((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const abort = () => xhr.abort();
    xhr.open('PUT', '/api/files/upload?' + new URLSearchParams({ root: target.root, path: target.path }));
    xhr.withCredentials = true;
    xhr.responseType = 'json';
    xhr.setRequestHeader('Content-Type', 'application/octet-stream');
    xhr.setRequestHeader('X-Sha256', digest);
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable && event.total) progress(Math.floor(100 * event.loaded / event.total));
    };
    xhr.onload = () => {
      if (xhr.status === 201) { resolve(); return; }
      if (xhr.status === 401) emitLoginRequired();
      const error: components['schemas']['Error'] | null = xhr.response;
      reject(new ApiError(xhr.status, error?.error ?? 'unknown', error?.detail));
    };
    xhr.onerror = () => reject(new Error('network'));
    xhr.onabort = () => reject(signal.reason);
    xhr.onloadend = () => signal.removeEventListener('abort', abort);
    signal.addEventListener('abort', abort, { once: true });
    progress(0);
    xhr.send(file);
  });
}
