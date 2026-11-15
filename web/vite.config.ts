import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The app is served by the TVMaster daemon under /admin/, so asset URLs are
// relative. In development, API and WebSocket calls go to a running daemon.
const daemon = process.env.TVMASTER_URL ?? 'http://127.0.0.1:8787';

export default defineConfig({
  base: './',
  plugins: [react()],
  server: {
    proxy: {
      '/api': { target: daemon, ws: true, changeOrigin: true },
      '/stream': daemon,
      '/iptv': daemon,
    },
  },
  build: { outDir: 'dist', emptyOutDir: true, sourcemap: false },
  test: { environment: 'node' },
});
