import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri expects a fixed dev-server port and must not clear the terminal output.
// Two pages: the editor window and the overlay windows over games.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    rolldownOptions: { input: { main: 'index.html', overlay: 'overlay.html' } },
  },
  // No module cache: the source scans in the tests use import.meta.glob, whose file list the
  // cache would keep from an earlier run.
  test: { environment: 'node', include: ['src/**/*.test.ts'], fsModuleCache: false },
});
