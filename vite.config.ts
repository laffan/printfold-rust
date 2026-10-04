import { defineConfig } from 'vite';

// Tauri expects a fixed dev port and serves the built files from ../dist.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  root: 'src',
  publicDir: false,
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    // Safari/WKWebView targets for macOS 11+ and iPadOS 15+.
    target: ['safari15'],
    minify: process.env.TAURI_ENV_DEBUG ? false : 'esbuild',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    chunkSizeWarningLimit: 2000,
  },
});
