import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri serves the built files; `npm run dev` serves them with a mock daemon
// (see src/lib/rpc.ts) for working on the UI in a browser.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: 'dist', target: 'safari16', emptyOutDir: true },
});
