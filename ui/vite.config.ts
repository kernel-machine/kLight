import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The engine serves the built UI; in development Vite proxies the API.
export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    proxy: {
      '/ws': { target: 'ws://127.0.0.1:9400', ws: true },
      '/api': 'http://127.0.0.1:9400',
    },
  },
  build: { outDir: 'dist', emptyOutDir: true },
  test: { include: ['src/**/*.test.ts'], environment: 'node', setupFiles: ['src/test-setup.ts'] },
});
