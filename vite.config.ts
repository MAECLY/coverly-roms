import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import { configDefaults } from 'vitest/config';
import { createReadStream, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

const host = process.env.TAURI_DEV_HOST;

/** The pdf.js data the PDF viewer fetches (extras D18). */
const PDFJS_DIRS = ['cmaps', 'standard_fonts'] as const;
const PDFJS_ROOT = path.resolve(import.meta.dirname, 'node_modules/pdfjs-dist');
const PDFJS_FILE = /^[\w.-]+$/;

/**
 * pdf.js's CMaps and standard fonts under `/pdfjs/` (extras D18): copied
 * into `dist/pdfjs/` at build, served from `node_modules` in dev. Only
 * plain file names inside those two folders are ever served.
 */
function pdfjsAssets(): Plugin {
  return {
    name: 'coverly:pdfjs-assets',
    configureServer(server) {
      server.middlewares.use('/pdfjs', (req, res, next) => {
        const [dir, name, ...rest] = (req.url ?? '')
          .split('?')[0]!
          .split('/')
          .filter(Boolean);
        const known = PDFJS_DIRS.find((d) => d === dir);
        if (!known || !name || rest.length > 0 || !PDFJS_FILE.test(name))
          return next();
        const file = path.join(PDFJS_ROOT, known, name);
        try {
          if (!statSync(file).isFile()) return next();
        } catch {
          return next();
        }
        res.setHeader('Content-Type', 'application/octet-stream');
        createReadStream(file).pipe(res);
      });
    },
    generateBundle() {
      for (const dir of PDFJS_DIRS) {
        for (const name of readdirSync(path.join(PDFJS_ROOT, dir))) {
          if (!PDFJS_FILE.test(name)) continue;
          this.emitFile({
            type: 'asset',
            fileName: `pdfjs/${dir}/${name}`,
            source: readFileSync(path.join(PDFJS_ROOT, dir, name)),
          });
        }
      }
    },
  };
}

export default defineConfig(async () => ({
  plugins: [react(), pdfjsAssets()],
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, 'src'),
    },
  },
  clearScreen: false,
  server: {
    port: 1431,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1432,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    target:
      process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_ENV_DEBUG ? 'oxc' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rolldownOptions: {
      output: {
        // pdf.js stays in its own chunk, loaded when a PDF opens (D18).
        // Only pdf.js's own code: not the `?url` module naming the worker
        // (the app imports it at startup) nor the helpers pdf.js shares
        // with the app (lowered class fields), or the entry would import
        // the whole of pdf.js at startup.
        codeSplitting: {
          groups: [
            {
              name: 'pdfjs',
              test: /node_modules[\\/]pdfjs-dist[\\/][^?]*$/,
              includeDependenciesRecursively: false,
            },
          ],
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    // An apply driven through the UI may wait up to APPLY_WAIT (5 s) for its
    // result; the per-test budget must outlast it on a slow CI runner.
    testTimeout: 20_000,
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    exclude: [...configDefaults.exclude, 'target/**', 'src-tauri/**'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'lcov'],
      reportsDirectory: './coverage',
      exclude: [
        'dist/**',
        'node_modules/**',
        'src/test/**',
        'src/**/*.test.{ts,tsx}',
        'src/main.tsx',
        'src/vite-env.d.ts',
        '**/*.config.{ts,js}',
      ],
    },
  },
}));
