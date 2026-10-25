import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { stellarXdrMajor } from '../stellar-xdr-pin.js';

function workspace(toml: string): string {
  const root = mkdtempSync(join(tmpdir(), 'xdr-pin-'));
  writeFileSync(join(root, 'Cargo.toml'), toml);
  return root;
}
const deps = (...lines: string[]) =>
  `[workspace.dependencies]\n${lines.join('\n')}\n\n[profile.release]\n`;

describe('stellarXdrMajor', () => {
  it.each([
    ['a bare major', deps('stellar-xdr = { version = "28" }'), 28],
    ['an exact version', deps('stellar-xdr = { version = "=29.0.0" }'), 29],
    ['a full version', deps('stellar-xdr = "28.10.1"'), 28],
    ['no spaces around =', deps('stellar-xdr="28"'), 28],
    [
      'not a longer key ending in stellar-xdr',
      deps('soroban-stellar-xdr = "27"', 'stellar-xdr = "28"'),
      28,
    ],
  ])('reads %s', (_name, toml, major) => {
    expect(stellarXdrMajor(workspace(toml))).toBe(major);
  });

  it.each([
    [
      'no section',
      '[dependencies]\nstellar-xdr = "28"\n',
      'no [workspace.dependencies] section in Cargo.toml',
    ],
    [
      'no entry in the section',
      deps('serde = "1"') + '[patch.crates-io]\nstellar-xdr = "28"\n',
      'no stellar-xdr entry in [workspace.dependencies]',
    ],
    [
      'an entry with no version',
      deps('stellar-xdr = { workspace = true }'),
      'cannot read a version out of: stellar-xdr = { workspace = true }',
    ],
  ])('throws on %s', (_name, toml, error) => {
    expect(() => stellarXdrMajor(workspace(toml))).toThrow(error);
  });
});
