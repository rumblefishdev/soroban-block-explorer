#!/usr/bin/env node
// Lint, typecheck and test the Nx projects a change reaches.
//   staged — the files in the index (pre-commit);
//   push   — the commits since the upstream, or since develop for a branch
//            that has none yet (pre-push).
import { execFileSync, spawnSync } from 'node:child_process';

const mode = process.argv[2];

const git = (...args) =>
  execFileSync('git', args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'ignore'],
  }).trim();

// A target no project has is skipped by `nx affected`, so the list is fixed.
const affected = (args, input) => {
  const result = spawnSync(
    'nx',
    [
      'affected',
      '-t',
      'lint',
      'typecheck',
      'test',
      '--outputStyle=static',
      ...args,
    ],
    {
      input,
      stdio: [input === undefined ? 'ignore' : 'pipe', 'inherit', 'inherit'],
    }
  );
  if (result.error) {
    throw result.error;
  }
  process.exit(result.status ?? 1);
};

if (mode === 'staged') {
  // Deleted files count: Nx maps a path to its project whether or not the
  // file still exists, and an importer of a deleted file must be re-checked.
  // Markdown feeds no lint, typecheck or test, and lint-staged has already
  // formatted it, so a commit of documents alone never starts Nx.
  const files = git('diff', '--cached', '--name-only', '-z')
    .split('\0')
    .filter((file) => file && !file.endsWith('.md'));
  if (files.length === 0) {
    process.exit(0);
  }
  affected(['--stdin'], files.join('\n'));
} else if (mode === 'push') {
  // A branch without an upstream is compared with develop, not origin/HEAD:
  // that is master, and the check would cover everything develop has gained
  // since the last release.
  let base;
  try {
    base = git('rev-parse', '--abbrev-ref', '@{upstream}');
  } catch {
    base = 'origin/develop';
  }
  console.log(`Running affected checks against ${base}.`);
  affected([`--base=${base}`, '--head=HEAD']);
} else {
  console.error('Expected mode to be one of: staged, push.');
  process.exit(1);
}
