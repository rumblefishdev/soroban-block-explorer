import type { ContractListItem } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { routes } from '../../router/routes.js';
import { renderWithProviders } from '../../test-utils.js';

import { ContractsTable } from './ContractsTable.js';

function makeContract(
  overrides: Partial<ContractListItem> = {}
): ContractListItem {
  return {
    contract_id: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
    contract_type: 0,
    contract_type_name: 'token',
    is_sac: false,
    sac_asset: null,
    deployer: null,
    deployed_at_ledger: 50_463_389,
    recent_invocations: 0,
    ...overrides,
  };
}

const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

describe('ContractsTable SAC chip (task 0441)', () => {
  it('links a classic SAC to its mirrored asset', () => {
    renderWithProviders(
      <ContractsTable
        rows={[
          makeContract({
            is_sac: true,
            sac_asset: { asset_code: 'USDC', issuer: ISSUER },
          }),
        ]}
      />
    );
    const chip = screen.getByText('SAC · USDC');
    expect(chip.closest('a')).toHaveAttribute(
      'href',
      routes.asset(`USDC-${ISSUER}`)
    );
  });

  it('links the native XLM SAC to the native asset page', () => {
    renderWithProviders(
      <ContractsTable
        rows={[
          makeContract({
            is_sac: true,
            sac_asset: { asset_code: null, issuer: null },
          }),
        ]}
      />
    );
    const chip = screen.getByText('SAC · XLM');
    expect(chip.closest('a')).toHaveAttribute('href', routes.asset('native'));
  });

  it('degrades to the bare unlinked badge when the facet is unresolvable', () => {
    renderWithProviders(
      <ContractsTable
        rows={[makeContract({ is_sac: true, sac_asset: null })]}
      />
    );
    const chip = screen.getByText('SAC');
    expect(chip.closest('a')).toBeNull();
  });

  it('renders no SAC chip on a non-SAC contract', () => {
    renderWithProviders(<ContractsTable rows={[makeContract()]} />);
    expect(screen.queryByText(/^SAC/)).toBeNull();
  });
});
