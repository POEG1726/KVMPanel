import { sha256File } from '../api/fileHash';

self.onmessage = async ({ data }: MessageEvent<File>) => {
  try {
    self.postMessage(await sha256File(data));
  } catch {
    self.postMessage(null);
  }
};
