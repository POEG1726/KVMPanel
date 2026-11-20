import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [
    react(),
    {
      name: 'kvmd-route-entries',
      enforce: 'post',
      generateBundle(_, bundle) {
        // kvmd serves directories directly; each BrowserRouter entry needs a physical index.
        const index = bundle['index.html'];
        if (index?.type !== 'asset') this.error('Missing application index.html');
        for (const route of ['login', 'workspace']) {
          this.emitFile({ type: 'asset', fileName: route + '/index.html', source: index.source });
        }
      },
    },
  ],
  optimizeDeps: { include: ['@xterm/xterm', '@xterm/addon-fit'] },
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:8800', changeOrigin: true, ws: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}', 'test/**/*.test.{ts,tsx}'],
    globalSetup: ['test/global-setup.ts'],
    setupFiles: ['test/setup.ts'],
    testTimeout: 15000,
  },
});
