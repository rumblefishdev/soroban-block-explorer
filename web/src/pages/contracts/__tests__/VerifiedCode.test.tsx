import { screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import {
  VerifiedCode,
  isShownVerification,
  verifiedByNames,
  type ShownVerification,
} from '../VerifiedCode.js';

import type {
  VerificationStatus,
  VerifiedSourceResponse,
} from '@rumblefish/api-types';

const sourceHook = vi.hoisted(() => ({ useContractVerifiedSource: vi.fn() }));
vi.mock('../../../api/index.js', () => sourceHook);

// A real testnet contract and its verified hash (BlockSmith staging, 2026-10-14).
const CONTRACT = 'CD37GB5NIHBI5EGUNIJXEHDNVDDG23HJAVSJTAKSMDQQVKJMBSK6E5EW';
const HASH = '2419cbfd2f1af27b3668b1fac2c468dedfb9927c12cb3c8a65b32213f8665b70';

function verification(
  overrides: Partial<ShownVerification> = {}
): ShownVerification {
  return {
    contract_id: CONTRACT,
    wasm_hash: HASH,
    status: 'verified',
    checked_at: '2026-10-14T13:30:48.961Z',
    verifiers: [
      {
        name: 'SCVS Dev Verifier',
        url: 'https://results.dev.example',
        status: 'verified',
      },
      {
        name: 'SCVS Staging Verifier',
        url: 'https://results.staging.example',
        status: 'verified',
      },
    ],
    source: {
      url: 'https://results.staging.example/sources/ecf4bc7d.tar.gz',
      sha256:
        'ecf4bc7db300358330af9bc1a272267f1c870a849b68db0f768bcea4b6078533',
      format: 'tar.gz',
    },
    build: {
      package: 'verified-demo',
      manifest_path: 'contracts/verified-demo/Cargo.toml',
      image: 'registry.example/env@sha256:2e21e7a2',
      options: ['--locked', '--package=verified-demo'],
      arguments: [],
      verified_at: '2026-09-30T10:27:04.170Z',
      report_url: 'https://results.staging.example/reports/r.json',
    },
    ...overrides,
  };
}

const files: VerifiedSourceResponse = {
  contract_id: CONTRACT,
  wasm_hash: HASH,
  archive_sha256:
    'ecf4bc7db300358330af9bc1a272267f1c870a849b68db0f768bcea4b6078533',
  files: [
    { path: 'source/Cargo.toml', size: 10, content: '[workspace]\n' },
    {
      path: 'source/contracts/verified-demo/src/lib.rs',
      size: 11,
      content: '#![no_std]\n',
    },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  sourceHook.useContractVerifiedSource.mockReturnValue({
    data: files,
    isPending: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  });
});

describe('isShownVerification', () => {
  it.each<[VerificationStatus, boolean]>([
    ['verified', true],
    ['verified_private', true],
    ['mismatched', false],
    ['disputed', false],
    ['unverified', false],
    ['none', false],
    ['unavailable', false],
  ])('%s → %s', (status, shown) => {
    expect(isShownVerification(verification({ status }))).toBe(shown);
  });

  it('a verified status without a hash is not shown', () => {
    expect(isShownVerification({ ...verification(), wasm_hash: null })).toBe(
      false
    );
  });

  it('nothing is not shown', () => {
    expect(isShownVerification(undefined)).toBe(false);
  });
});

describe('VerifiedCode tab', () => {
  it('names the verifiers, the package and the attribution', () => {
    renderWithProviders(
      <VerifiedCode contractId={CONTRACT} verification={verification()} />
    );

    expect(screen.getByText('Verified')).toBeInTheDocument();
    expect(screen.getByText('package verified-demo')).toBeInTheDocument();
    expect(
      screen.getByText(
        /Source verification by SCVS Dev Verifier, SCVS Staging Verifier/
      )
    ).toBeInTheDocument();
    expect(
      screen.getByRole('link', { name: 'BlockSmith' })
    ).toBeInTheDocument();
    expect(screen.getByText(/2419cbfd…665b70/)).toBeInTheDocument();
  });

  it('downloads the published archive', () => {
    renderWithProviders(
      <VerifiedCode contractId={CONTRACT} verification={verification()} />
    );

    expect(
      screen.getByRole('link', { name: /Download source \.tar\.gz/ })
    ).toHaveAttribute(
      'href',
      'https://results.staging.example/sources/ecf4bc7d.tar.gz'
    );
  });

  it('never links a non-http URL from the third-party answer', () => {
    renderWithProviders(
      <VerifiedCode
        contractId={CONTRACT}
        verification={verification({
          source: { url: 'javascript:alert(1)', sha256: 'aa', format: 'zip' },
        })}
      />
    );

    expect(screen.queryByRole('link', { name: /Download source/ })).toBeNull();
  });

  it('opens the built package and counts the files', () => {
    renderWithProviders(
      <VerifiedCode contractId={CONTRACT} verification={verification()} />
    );

    expect(
      screen.getByText('source/contracts/verified-demo/src/lib.rs')
    ).toBeInTheDocument();
    expect(
      screen.getByText(/2 files · 0 KB unpacked · matches the published sha256/)
    ).toBeInTheDocument();
  });

  it('keeps the build recipe collapsed until asked', async () => {
    renderWithProviders(
      <VerifiedCode contractId={CONTRACT} verification={verification()} />
    );

    expect(screen.queryByText('Cargo manifest')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: /Build recipe/ }));
    expect(screen.getByText('Cargo manifest')).toBeInTheDocument();
    expect(
      screen.getByText('empty list — the default, contract build')
    ).toBeInTheDocument();
    // One element per build option, so each reads on its own line.
    expect(screen.getByText('--locked')).toBeInTheDocument();
    expect(screen.getByText('--package=verified-demo')).toBeInTheDocument();
    expect(
      screen.getByRole('link', { name: /Full verification report/ })
    ).toBeInTheDocument();
  });

  it('shows every recipe value exactly as published', async () => {
    // Values come from a third party: nothing collapsed, joined or replaced.
    const exact = { normalizer: (t: string) => t };
    renderWithProviders(
      <VerifiedCode
        contractId={CONTRACT}
        verification={verification({
          build: {
            package: 'verified-demo',
            manifest_path: '',
            image: '—',
            options: ['--features=a  b', '', '--x\n--y'],
            arguments: ['a b', 'c'],
            verified_at: '2026-10-01T12:00:00.123456+02:00',
            report_url: null,
          },
        })}
      />
    );
    await userEvent.click(screen.getByRole('button', { name: /Build recipe/ }));

    expect(screen.getByText('--features=a  b', exact)).toBeInTheDocument();
    expect(screen.getByText('--x\n--y', exact)).toBeInTheDocument();
    // Two arguments stay two, and `a b` stays one.
    expect(screen.getByText('a b', exact)).toBeInTheDocument();
    expect(screen.getByText('c', exact)).toBeInTheDocument();
    // A published dash is a value, not our "missing" wording.
    expect(screen.getByText('—', exact)).toBeInTheDocument();
    expect(screen.queryByText('not published')).toBeNull();
    // The timestamp as published, not re-formatted.
    expect(
      screen.getByText('2026-10-01T12:00:00.123456+02:00', exact)
    ).toBeInTheDocument();
    // The empty manifest and the empty option each still take a frame.
    expect(screen.queryByText(/not given/)).toBeNull();
  });

  it('words a missing value apart from any published one', async () => {
    renderWithProviders(
      <VerifiedCode
        contractId={CONTRACT}
        verification={verification({
          build: {
            package: null,
            manifest_path: null,
            image: null,
            options: [],
            arguments: [],
            verified_at: null,
            report_url: null,
          },
        })}
      />
    );
    await userEvent.click(screen.getByRole('button', { name: /Build recipe/ }));

    expect(screen.getAllByText('not published')).toHaveLength(2);
    expect(screen.getByText('not given')).toBeInTheDocument();
    expect(
      screen.getByText('not given — the workspace root')
    ).toBeInTheDocument();
    expect(screen.getByText('none')).toBeInTheDocument();
    expect(
      screen.getByText('empty list — the default, contract build')
    ).toBeInTheDocument();
  });

  it('says the source is not published for verified_private, and fetches nothing', () => {
    renderWithProviders(
      <VerifiedCode
        contractId={CONTRACT}
        verification={verification({
          status: 'verified_private',
          source: null,
        })}
      />
    );

    expect(
      screen.getByText(
        'Verified by SCVS Dev Verifier, SCVS Staging Verifier — the source code is not published.'
      )
    ).toBeInTheDocument();
    expect(sourceHook.useContractVerifiedSource).not.toHaveBeenCalled();
  });

  it('shows the query error state with retry when the files fail', () => {
    sourceHook.useContractVerifiedSource.mockReturnValue({
      data: undefined,
      isPending: false,
      isError: true,
      error: new Error('boom'),
      refetch: vi.fn(),
    });
    renderWithProviders(
      <VerifiedCode contractId={CONTRACT} verification={verification()} />
    );

    expect(
      screen.getByRole('button', { name: 'Try again' })
    ).toBeInTheDocument();
  });

  it('names as verifiers only those whose own answer is verified', () => {
    const v = verification({
      verifiers: [
        { name: 'A', url: 'https://a.example', status: 'verified' },
        { name: 'B', url: 'https://b.example', status: 'mismatched' },
      ],
    });
    expect(verifiedByNames(v)).toEqual(['A']);
  });

  it('shows the build recipe for verified_private too, without an archive hash', async () => {
    renderWithProviders(
      <VerifiedCode
        contractId={CONTRACT}
        verification={verification({
          status: 'verified_private',
          source: null,
        })}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: /Build recipe/ }));
    expect(screen.getByText('Cargo manifest')).toBeInTheDocument();
    expect(screen.queryByText('Source archive sha256')).toBeNull();
  });

  it('refetches files cached for code the contract no longer runs, once', () => {
    const refetch = vi.fn();
    sourceHook.useContractVerifiedSource.mockReturnValue({
      data: { ...files, wasm_hash: '00'.repeat(32) },
      isPending: false,
      isFetching: false,
      isError: false,
      error: null,
      refetch,
    });
    renderWithProviders(
      <VerifiedCode contractId={CONTRACT} verification={verification()} />
    );

    // Still the old hash after the one retry: say so instead of looping.
    expect(refetch).toHaveBeenCalledTimes(1);
    expect(
      screen.getByText("The contract's code changed while this page was open")
    ).toBeInTheDocument();
    expect(
      screen.queryByText('source/contracts/verified-demo/src/lib.rs')
    ).toBeNull();
  });

  it('hides the attribution when no verifier answered', () => {
    renderWithProviders(
      <VerifiedCode
        contractId={CONTRACT}
        verification={verification({ verifiers: [] })}
      />
    );

    expect(screen.queryByText(/aggregated by/)).toBeNull();
  });
});
