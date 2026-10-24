import { defineConfig } from 'vite-plus';

export default defineConfig({
  // The code is not formatter-managed; these options only mirror its style for `vp fmt`.
  fmt: { singleQuote: true, printWidth: 120 },
  lint: {
    ignorePatterns: ['dist/**'],
    options: { typeAware: true, typeCheck: true },
  },
  // Relative base so the build works from any static host path (e.g. GitHub Pages).
  base: './',
  worker: { format: 'es' },
  build: { target: 'es2022', chunkSizeWarningLimit: 1500 },
});
