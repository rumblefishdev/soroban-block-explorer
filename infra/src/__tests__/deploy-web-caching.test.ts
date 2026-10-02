import { readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

// Guards the SPA deploy recipe (task 0595). One sync with `--delete` and no
// Cache-Control let a browser keep an old index.html that pointed at deleted
// assets, which loaded as a blank page.
const makefile = readFileSync(
  resolve(dirname(fileURLToPath(import.meta.url)), '../../Makefile'),
  'utf8'
);
// Every environment's SPA deploy follows the same recipe (testnet: task 0553).
describe.each(['deploy-production-web', 'deploy-testnet-web'])(
  '%s caching',
  (target) => {
    const recipe = makefile
      .split(new RegExp(`^${target}:.*$`, 'm'))[1]
      .split(/\n\n/)[0];
    // One logical shell line per `aws s3 sync`, continuations joined.
    const syncs = recipe
      .replace(/\\\n\s*/g, ' ')
      .split('&&')
      .filter((c) => c.includes('aws s3 sync'));

    it('uploads assets first, immutable and never deleted', () => {
      expect(syncs).toHaveLength(2);
      const [assets] = syncs;
      expect(assets).toContain("--include 'assets/*'");
      expect(assets).toContain('max-age=31536000, immutable');
      expect(assets).not.toContain('--delete');
    });

    it('makes browsers revalidate everything else, index.html included', () => {
      const rest = syncs[1];
      expect(rest).toContain("--exclude 'assets/*'");
      expect(rest).toContain('max-age=0');
      expect(rest).toContain('must-revalidate');
    });

    it('invalidates CloudFront only after both syncs', () => {
      expect(recipe.indexOf('create-invalidation')).toBeGreaterThan(
        recipe.lastIndexOf('aws s3 sync')
      );
    });
  }
);
