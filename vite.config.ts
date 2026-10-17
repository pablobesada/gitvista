import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { pierreLongLines } from './scripts/pierre-long-lines.ts';
export default defineConfig({
  plugins: [svelte(), pierreLongLines()],
  // Keep the renderer visible to the adapter in development as well as builds.
  optimizeDeps: { exclude: ['@pierre/diffs'], include: ['@pierre/diffs > lru_map'] },
  worker: { plugins: () => [pierreLongLines()] },
  server: { port: 1420, strictPort: true, host: '127.0.0.1', watch: { ignored: ['**/output/**', '**/.playwright-cli/**', '**/target/**', '**/src-tauri/**', '**/crates/**', '**/output-build.log'] } },
  clearScreen: false,
  build: { target: 'safari15', sourcemap: false, reportCompressedSize: false },
});
