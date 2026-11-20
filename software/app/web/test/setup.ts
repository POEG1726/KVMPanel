import '@testing-library/jest-dom/vitest';

// jsdom has neither canvas drawing nor image decoding; pages that mount the video canvas get inert stubs.
if (typeof createImageBitmap === 'undefined') {
  globalThis.createImageBitmap = (async () => ({ width: 1, height: 1, close() {} })) as never;
}
HTMLCanvasElement.prototype.getContext = (() => ({ drawImage() {} })) as never;
