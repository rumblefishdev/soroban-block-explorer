import { screen, waitFor } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import {
  SourceBrowser,
  buildTree,
  countRows,
  defaultFile,
  languageOf,
} from '../SourceBrowser.js';

import type { VerifiedSourceFile } from '@rumblefish/api-types';

const file = (
  path: string,
  content: string | null = 'x\n'
): VerifiedSourceFile => ({
  path,
  size: content?.length ?? 4,
  content,
});

// The LayerZero monorepo archive verifies several contracts, one package each
// (testnet sample, 2026-10-14) — the built package must open first.
const monorepo = [
  file('contracts/Cargo.toml'),
  file('contracts/dependencies/utils/src/lib.rs', 'pub mod utils;\n'),
  file('contracts/workers/dvn/Cargo.toml'),
  file('contracts/workers/dvn/src/lib.rs', '#![no_std]\npub mod dvn;\n'),
  file('contracts/workers/executor/src/lib.rs'),
];

describe('defaultFile', () => {
  it("opens the built package's lib.rs, not the first lib.rs", () => {
    expect(defaultFile(monorepo, 'workers/dvn/Cargo.toml')?.path).toBe(
      'contracts/workers/dvn/src/lib.rs'
    );
  });

  it('matches the package folder whole, not a longer name ending in it', () => {
    const files = [
      file('source/wrapped-token/src/lib.rs'),
      file('source/token/src/lib.rs'),
    ];
    expect(defaultFile(files, 'token/Cargo.toml')?.path).toBe(
      'source/token/src/lib.rs'
    );
    expect(
      defaultFile([file('xsrc/lib.rs'), file('a/src/lib.rs')], null)?.path
    ).toBe('a/src/lib.rs');
  });

  it('falls back to any lib.rs, any .rs, then the first file', () => {
    expect(defaultFile(monorepo, null)?.path).toBe(
      'contracts/dependencies/utils/src/lib.rs'
    );
    expect(defaultFile([file('a.toml'), file('b/main.rs')], null)?.path).toBe(
      'b/main.rs'
    );
    expect(defaultFile([file('a.toml')], null)?.path).toBe('a.toml');
    expect(defaultFile([], null)).toBeUndefined();
  });
});

describe('buildTree', () => {
  it('lists folders before files, each alphabetical', () => {
    const root = buildTree([
      file('z.toml'),
      file('b/x.rs'),
      file('a.md'),
      file('a/y.rs'),
    ]);
    expect(root.children.map((c) => c.name)).toEqual([
      'a',
      'b',
      'a.md',
      'z.toml',
    ]);
  });
});

describe('countRows', () => {
  it('counts every row on the way to the open file, expanded', () => {
    // contracts › (dependencies, workers, Cargo.toml) › workers › (dvn,
    // executor) › dvn › (src, Cargo.toml) › src › lib.rs
    expect(
      countRows(buildTree(monorepo), 'contracts/workers/dvn/src/lib.rs')
    ).toBe(9);
    expect(countRows(buildTree(monorepo), '')).toBe(1);
  });
});

describe('languageOf', () => {
  it.each([
    ['src/lib.rs', 'rust'],
    ['Cargo.toml', 'toml'],
    ['Cargo.lock', 'toml'],
    ['test_snapshots/t.json', 'json'],
    ['README.md', 'markdown'],
    ['contracts/x/Makefile', 'makefile'],
    ['.gitignore', 'none'],
  ])('%s → %s', (path, lang) => {
    expect(languageOf(path)).toBe(lang);
  });
});

describe('SourceBrowser', () => {
  it('opens the built package and keeps one tab stop in the tree', () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );

    const selected = screen.getByRole('treeitem', { selected: true });
    expect(selected).toHaveTextContent('lib.rs');
    expect(selected).toHaveAttribute('tabindex', '0');
    const stops = screen
      .getAllByRole('treeitem')
      .filter((r) => r.tabIndex === 0);
    expect(stops).toHaveLength(1);
    expect(
      screen.getByText('contracts/workers/dvn/src/lib.rs')
    ).toBeInTheDocument();
  });

  it('moves with the arrow keys and opens a folder with ArrowRight', async () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );
    const user = userEvent.setup();

    screen.getByRole('treeitem', { selected: true }).focus();
    await user.keyboard('{ArrowUp}');
    expect(document.activeElement).toHaveTextContent('src');
    const executor = screen.getByRole('treeitem', { name: /executor/ });
    expect(executor).toHaveAttribute('aria-expanded', 'false');
    executor.focus();
    await user.keyboard('{ArrowRight}');
    expect(executor).toHaveAttribute('aria-expanded', 'true');
    await user.keyboard('{ArrowLeft}');
    expect(executor).toHaveAttribute('aria-expanded', 'false');
  });

  it('ArrowLeft on a file goes up to its folder', async () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );
    const user = userEvent.setup();

    screen.getByRole('treeitem', { selected: true }).focus();
    await user.keyboard('{ArrowLeft}');
    expect(document.activeElement).toHaveTextContent(/^src$/);
  });

  it('keeps one tab stop when the filter hides the open file', async () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );

    await userEvent.type(screen.getByRole('textbox'), 'executor');
    // The filter is debounced: wait until the open file is filtered out.
    await waitFor(() =>
      expect(screen.queryByRole('treeitem', { selected: true })).toBeNull()
    );
    const stops = screen
      .getAllByRole('treeitem')
      .filter((r) => r.tabIndex === 0);
    expect(stops).toHaveLength(1);
  });

  it('offers to copy the open file', () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );

    expect(
      screen.getByRole('button', { name: 'Copy file' })
    ).toBeInTheDocument();
  });

  it('opens a clicked file', async () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );

    const cargo = screen
      .getAllByRole('treeitem', { name: /Cargo\.toml/ })
      .at(0);
    if (cargo == null) throw new Error('no Cargo.toml row');
    await userEvent.click(cargo);
    expect(screen.getByRole('treeitem', { selected: true })).toHaveTextContent(
      'Cargo.toml'
    );
  });

  it('opens the file named in the URL', () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />,
      {
        initialEntries: ['/c?file=contracts/Cargo.toml'],
        routePath: '/c',
      }
    );

    expect(
      screen.getByText('contracts/Cargo.toml', { selector: 'span, p' })
    ).toBeInTheDocument();
  });

  it('says so when the filter matches nothing', async () => {
    renderWithProviders(<SourceBrowser files={monorepo} manifestPath={null} />);

    await userEvent.type(screen.getByRole('textbox'), 'nothing-like-this');
    expect(await screen.findByText('No files match')).toBeInTheDocument();
  });

  it('lists a non-text file and says it is not text', () => {
    renderWithProviders(
      <SourceBrowser files={[file('logo.png', null)]} manifestPath={null} />
    );

    expect(screen.getByText('Not UTF-8 text')).toBeInTheDocument();
  });

  it('warns about hidden characters and shows them in place', () => {
    const rlo = String.fromCharCode(0x202e);
    renderWithProviders(
      <SourceBrowser
        files={[file('src/lib.rs', `let a = 1; // x${rlo}y\n`)]}
        manifestPath={null}
      />
    );

    expect(
      screen.getByText(/holds 1 invisible or direction-changing character/)
    ).toBeInTheDocument();
  });

  it('gives each tree row its full path on hover', () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );

    expect(
      screen.getByTitle('contracts/workers/dvn/src/lib.rs')
    ).toBeInTheDocument();
  });

  it('says an empty archive holds no files', () => {
    renderWithProviders(<SourceBrowser files={[]} manifestPath={null} />);

    expect(
      screen.getByText('The verified source archive holds no files')
    ).toBeInTheDocument();
  });
});

describe('SourceBrowser on a phone', () => {
  /** MUI's `useMediaQuery` reads `window.matchMedia`, which jsdom lacks;
   *  matching every `(max-width…)` query simulates a narrow screen. */
  function stubNarrowViewport() {
    vi.stubGlobal(
      'matchMedia',
      (query: string) =>
        ({
          matches: query.includes('max-width'),
          media: query,
          addEventListener: () => undefined,
          removeEventListener: () => undefined,
          addListener: () => undefined,
          removeListener: () => undefined,
          onchange: null,
          dispatchEvent: () => false,
        } as MediaQueryList)
    );
  }
  afterEach(() => vi.unstubAllGlobals());

  it('folds the tree under Files (N) so the code comes first', async () => {
    stubNarrowViewport();
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );

    expect(screen.queryByRole('tree')).toBeNull();
    expect(
      screen.getByText('contracts/workers/dvn/src/lib.rs')
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Files (5)' }));
    expect(screen.getByRole('tree')).toBeInTheDocument();
  });

  it('shows the tree open beside the code on a wide screen', () => {
    renderWithProviders(
      <SourceBrowser files={monorepo} manifestPath="workers/dvn/Cargo.toml" />
    );

    expect(screen.getByRole('tree')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Files \(/ })).toBeNull();
  });
});
