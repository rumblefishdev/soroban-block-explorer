import type { ContractDetailResponse } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { routes } from '../../router/routes.js';
import { renderWithProviders } from '../../test-utils.js';

import { ContractSummary } from './ContractSummary.js';

const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

function makeContract(
  overrides: Partial<ContractDetailResponse> = {}
): ContractDetailResponse {
  return {
    contract_id: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
    contract_type: 0,
    contract_type_name: 'token',
    is_sac: false,
    sac_asset: null,
    deployer: null,
    deployed_at_ledger: 50_463_389,
    wasm_hash: null,
    stats: {
      recent_events: 0,
      recent_invocations: 0,
      recent_unique_callers: 0,
      stats_window: '7 days',
    },
    ...overrides,
  };
}

describe('ContractSummary SAC asset row (task 0472)', () => {
  it('splits a classic SAC into labelled Asset and Issuer cells', () => {
    renderWithProviders(
      <ContractSummary
        contract={makeContract({
          is_sac: true,
          sac_asset: { asset_code: 'USDC', issuer: ISSUER },
        })}
      />
    );

    // Both values are LABELLED fields, not two bare links in one cell —
    // that ambiguity is exactly what this task fixed.
    expect(screen.getByText('Asset')).toBeInTheDocument();
    expect(screen.getByText('Issuer')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'USDC' })).toHaveAttribute(
      'href',
      routes.asset(`USDC-${ISSUER}`)
    );
    expect(screen.getByRole('link', { name: /^GA5Z/ })).toHaveAttribute(
      'href',
      routes.account(ISSUER)
    );
    // The invented "Mirrors asset" label is gone (scope 5).
    expect(screen.queryByText('Mirrors asset')).toBeNull();
  });

  it('renders the Asset cell only for the native XLM SAC (no issuer exists)', () => {
    renderWithProviders(
      <ContractSummary
        contract={makeContract({
          is_sac: true,
          sac_asset: { asset_code: null, issuer: null },
        })}
      />
    );

    expect(screen.getByRole('link', { name: 'XLM' })).toHaveAttribute(
      'href',
      routes.asset('native')
    );
    expect(screen.queryByText('Issuer')).toBeNull();
  });

  it('renders no asset row at all on a non-SAC contract', () => {
    renderWithProviders(<ContractSummary contract={makeContract()} />);

    expect(screen.queryByText('Asset')).toBeNull();
    expect(screen.queryByText('Issuer')).toBeNull();
  });
});

describe('ContractSummary externally managed executable (task 0548)', () => {
  const OWNER = 'CBGVOLVXQ4XJZC6F4CQKWNKA4VL36VYVVSMV2RJD3QFYVI6RCJFJDDOF';

  it('names the owner and tag, and labels the hash as resolved', () => {
    renderWithProviders(
      <ContractSummary
        contract={makeContract({
          wasm_hash: 'bb'.repeat(32),
          executable_owner: OWNER,
          executable_tag: 'fleet-v2',
        })}
      />
    );

    expect(screen.getByText('Executable')).toBeInTheDocument();
    expect(screen.getByText('Managed by')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /^CBGV/ })).toHaveAttribute(
      'href',
      routes.contract(OWNER)
    );
    expect(screen.getByText('fleet-v2')).toBeInTheDocument();
    expect(screen.getByText('WASM hash (resolved)')).toBeInTheDocument();
  });

  it('keeps the plain WASM hash label and no Executable row for own code', () => {
    renderWithProviders(
      <ContractSummary
        contract={makeContract({ wasm_hash: 'aa'.repeat(32) })}
      />
    );

    expect(screen.getByText('WASM hash')).toBeInTheDocument();
    expect(screen.queryByText('Executable')).toBeNull();
    expect(screen.queryByText('WASM hash (resolved)')).toBeNull();
  });
});
