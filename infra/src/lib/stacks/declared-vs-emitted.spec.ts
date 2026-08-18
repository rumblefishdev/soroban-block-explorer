/**
 * Declared-vs-emitted guard (task 0455 / 0454).
 *
 * The CDK app declares log-filter match strings and custom metric
 * namespaces/names; the Rust crates emit them. The two live in different
 * languages and deployment units, so nothing ties them together at compile
 * time. This test asserts every declared literal appears somewhere under
 * `crates/`, so a rename on either side fails CI instead of leaving a filter
 * or alarm silently matching nothing.
 *
 * ## Why a grep-shaped guard, and when to replace it
 *
 * Three ways to keep the two sides honest were compared (2026-08-18). The
 * ranking is a function of how many contracts exist, so it is recorded here
 * with its trigger rather than re-argued each time:
 *
 * - **A. This test (current).** ~100 lines, no build step, no generated
 *   files. Reads source text, so it is approximate by construction — that is
 *   the price. Fits today's THREE live contracts.
 * - **B. Assert on the synthesized template.** Same idea, exact mechanism:
 *   `Template.fromStack()` exposes `AWS::Logs::MetricFilter` transformations
 *   and dashboard/alarm bodies as real resources, so provenance stops being
 *   inferred from proximity. Costs a stack fixture (the constructor takes
 *   functions, queues, a cluster and a REST API). **Trigger: a second or
 *   third MetricFilter, or the first time this file needs another heuristic.**
 * - **C. One source of truth + codegen.** A contracts file generating the
 *   constants for both sides. Built, measured and REJECTED in `827898d0` as
 *   overkill for three pairs: it does not remove this guard (something must
 *   still assert the generated file is current) and adds five moving parts.
 *   **Trigger: ~10 contracts, or emitters spanning multiple crates.**
 *
 * Recommendation: stay on A, jump to B when a heuristic is added or a filter
 * is, and only reach for C at the contract count that made it lose.
 */
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const infraSrc = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const repoRoot = join(infraSrc, '..', '..');

function collectFiles(dir: string, ext: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'target' || entry.name === 'node_modules') continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) collectFiles(full, ext, out);
    else if (entry.name.endsWith(ext)) out.push(full);
  }
  return out;
}

const stackSources = collectFiles(infraSrc, '.ts')
  .filter((f) => !f.endsWith('.spec.ts') && !f.endsWith('.test.ts'))
  .map((f) => ({ file: f, text: readFileSync(f, 'utf8') }));

// Comments are stripped so a literal surviving only in a `//` comment
// (e.g. the contract note next to a removed emit site) cannot satisfy
// the guard — only actual code counts. Ceiling: a declared literal
// containing `//` would be truncated by the strip; none do today.
const rustCorpus = collectFiles(join(repoRoot, 'crates'), '.rs')
  .map((f) => readFileSync(f, 'utf8').replace(/\/\/[^\n]*/g, ''))
  .join('\n');

// AWS-managed namespaces are emitted by AWS itself, not by our code.
const isCustomNamespace = (ns: string) => !/^(AWS|ECS|CWAgent)\//.test(ns);

describe('declared log-filter strings exist in crates/', () => {
  const literals = stackSources.flatMap(({ file, text }) =>
    [
      ...text.matchAll(
        /stringValue\(\s*'\$\.fields\.(\w+)',\s*'=',\s*'([^']+)'/gs
      ),
    ].map((m) => ({ file, field: m[1], literal: m[2] }))
  );

  it('finds at least one declared filter literal', () => {
    expect(literals.length).toBeGreaterThan(0);
  });

  for (const { file, field, literal } of literals) {
    it(`$.fields.${field} = "${literal}" (${file.slice(
      repoRoot.length + 1
    )})`, () => {
      if (field === 'message') {
        // `message` is the event's format string — match the prose.
        expect(
          rustCorpus.includes(literal),
          `filter matches message "${literal}" but no Rust source emits it`
        ).toBe(true);
      } else {
        // Any other field is a machine contract: require the exact
        // `field = "literal"` pair at a tracing emit site, so renaming
        // the field while keeping the value elsewhere still fails.
        const emit = new RegExp(`${field}\\s*=\\s*"${literal}"`);
        expect(
          emit.test(rustCorpus),
          `filter matches $.fields.${field} = "${literal}" but no Rust source emits that field/value pair`
        ).toBe(true);
      }
    });
  }
});

describe('declared custom metric namespaces exist in crates/', () => {
  const namespaces = new Set(
    stackSources.flatMap(({ text }) =>
      [...text.matchAll(/(?:metricNamespace|namespace):\s*'([^']+)'/g)]
        .map((m) => m[1])
        .filter(isCustomNamespace)
    )
  );

  for (const ns of namespaces) {
    it(`"${ns}"`, () => {
      expect(
        rustCorpus.includes(ns),
        `namespace "${ns}" is read by the stack but no Rust source publishes to it`
      ).toBe(true);
    });
  }
});

describe('custom metric names read from code-published namespaces exist in crates/', () => {
  // A name read via `namespace:` must have a publisher, and there are two
  // legitimate kinds:
  //   1. Rust code (PutMetricData) — the name appears in `crates/`.
  //   2. A `logs.MetricFilter` in this stack mints it from matching log
  //      lines. Such names never appear in Rust by construction; their
  //      emitted-side contract is the filter PATTERN, which the
  //      filter-literal suite above already guards end to end.
  //
  // Kind 2 is an EXPLICIT list, not inferred. A derived version was written
  // first — collect every `metricName` within 400 characters of a
  // `metricNamespace:` — and it failed its own smell test: a fabricated
  // metric read placed near the filter block was silently accepted, because
  // proximity is not provenance. That version failed OPEN, which is the one
  // direction a guard must never fail. Adding an entry here is a deliberate
  // act that has to name the filter construct that mints it; forgetting to
  // add one fails CI loudly, which is the safe direction.
  //
  // The exact version of this check would assert against the SYNTHESIZED
  // template (`Template.fromStack`), where MetricFilter transformations and
  // dashboard bodies are concrete resources rather than source text. Worth
  // doing if this list ever grows past a couple of entries.
  const FILTER_MINTED_METRICS = new Set([
    // Minted by `IndexerChWriteFailureFilter` in cloudwatch-stack.ts from
    // log lines carrying `$.fields.alarm = "ch_write_failure"`.
    'ChWriteFailures',
  ]);

  const names = new Set(
    stackSources.flatMap(({ text }) =>
      [...text.matchAll(/(?<!\w)namespace:\s*'([^']+)'/g)]
        .filter((m) => isCustomNamespace(m[1]))
        .flatMap((m) => {
          const window = text.slice(Math.max(0, m.index - 400), m.index + 400);
          return [...window.matchAll(/metricName:\s*'([^']+)'/g)].map(
            (n) => n[1]
          );
        })
    )
  );

  for (const name of names) {
    it(`"${name}"`, () => {
      expect(
        rustCorpus.includes(name) || FILTER_MINTED_METRICS.has(name),
        `metric "${name}" is read by the stack but nothing publishes it — ` +
          `no Rust source emits it and it is not in FILTER_MINTED_METRICS. ` +
          `If a MetricFilter mints it, add it to that list naming the filter.`
      ).toBe(true);
    });
  }
});
