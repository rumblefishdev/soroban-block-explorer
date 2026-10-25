import { dirname } from 'path';
import { fileURLToPath } from 'url';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  root: dirname(fileURLToPath(import.meta.url)),
  test: {
    environment: 'node',
    include: ['src/**/*.{test,spec}.ts', 'lambdas/**/*.test.mjs'],
    // A full CDK synth runs in several tests; on the CI runner, beside the web
    // tests, one took 1.3 s, 2.5 s and 5.1 s on three runs of 2026-10-19 — the
    // last over vitest's 5 s default.
    testTimeout: 30_000,
  },
});
