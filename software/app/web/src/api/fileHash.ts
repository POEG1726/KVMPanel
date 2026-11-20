import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex } from '@noble/hashes/utils.js';

const CHUNK_BYTES = 1024 * 1024;

/** Incremental SHA-256 also works on plain HTTP when the board is on a local network. */
export async function sha256File(file: Blob, signal?: AbortSignal): Promise<string> {
  const hash = sha256.create();
  for (let offset = 0; offset < file.size; offset += CHUNK_BYTES) {
    signal?.throwIfAborted();
    const chunk = await file.slice(offset, offset + CHUNK_BYTES).arrayBuffer();
    hash.update(new Uint8Array(chunk));
  }
  signal?.throwIfAborted();
  return bytesToHex(hash.digest());
}
