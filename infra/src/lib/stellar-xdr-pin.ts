import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * The `stellar-xdr` major the workspace pins, read from `Cargo.toml`'s
 * `[workspace.dependencies]` at synth time (task 0646). The indexer carries
 * it as `STELLAR_XDR_MAJOR`, set in the same deploy that builds its binary,
 * so the protocol watch reads what runs.
 *
 * A pin is a caret requirement (`"28"`, `"28.0.1"`), so Cargo.lock cannot
 * resolve another major; the first number is the one the binary decodes with.
 */
export function stellarXdrMajor(cargoWorkspacePath: string): number {
  const toml = readFileSync(join(cargoWorkspacePath, 'Cargo.toml'), 'utf8');
  const section = toml.split(/^\[workspace\.dependencies\]$/m)[1];
  if (!section) {
    throw new Error('no [workspace.dependencies] section in Cargo.toml');
  }

  const line = section.split(/^\[/m)[0].match(/^stellar-xdr\s*=\s*(.+)$/m);
  if (!line) {
    throw new Error('no stellar-xdr entry in [workspace.dependencies]');
  }

  const version = line[1].match(/"[=^~]?(\d+)(\.\d+){0,2}"/);
  if (!version) {
    throw new Error(`cannot read a version out of: stellar-xdr = ${line[1]}`);
  }
  return Number(version[1]);
}
