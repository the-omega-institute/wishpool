/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

/*
 * The API binary serves `/api` and `/auth` on the same origin in production;
 * the dev server forwards both so the session cookie stays first-party.
 */
const API_ORIGIN = 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      output: {
        // Long-lived vendor chunks cache across app releases.
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined;
          if (id.includes('katex')) return 'katex';
          if (/[\\/]node_modules[\\/](react|react-dom|scheduler)[\\/]/.test(id)) return 'react';
          return 'markdown';
        },
      },
    },
  },
  server: {
    proxy: {
      '/api': { target: API_ORIGIN, changeOrigin: false },
      '/auth': { target: API_ORIGIN, changeOrigin: false },
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: false,
  },
});
