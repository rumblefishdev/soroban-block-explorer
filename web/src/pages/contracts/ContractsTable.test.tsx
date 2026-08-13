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

describe('ContractsTable SAC chip (tasks 0441 + 0472)', () => {
  it('links a classic SAC to its mirrored asset, no redundant Token chip', () => {
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
    // 0472: the Token type chip is dropped on SAC rows — on prod the pair
    // is 100% redundant (Token ⟺ is_sac, 3,946/3,946).
    expect(screen.queryByText('Token')).toBeNull();
  });

  it('keeps the visible text as the accessible name, issuer as description', () => {
    // WCAG 2.5.3 (review 2026-08-13): a Tooltip title used to REPLACE the
    // link's accessible name with the issuer string, so voice control could
    // not target the chip by its visible text. `describeChild` moves the
    // issuer to aria-describedby instead.
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
    const link = screen.getByRole('link', { name: 'SAC · USDC' });
    expect(link).toHaveAccessibleDescription(`USDC issued by ${ISSUER}`);
    // No keyboard-dead role="button" nested inside the anchor.
    expect(link.querySelector('[role="button"]')).toBeNull();
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
    // Native has no issuer — the description says what it is instead.
    expect(
      screen.getByRole('link', { name: 'SAC · XLM' })
    ).toHaveAccessibleDescription('Native XLM');
  });

  it('degrades to the bare unlinked badge when the facet is unresolvable', () => {
    renderWithProviders(
      <ContractsTable
        rows={[makeContract({ is_sac: true, sac_asset: null })]}
      />
    );
    const chip = screen.getByText('SAC');
    expect(chip.closest('a')).toBeNull();
    expect(screen.queryByText('Token')).toBeNull();
  });

  it('keeps the plain type chip on a non-SAC contract', () => {
    renderWithProviders(<ContractsTable rows={[makeContract()]} />);
    expect(screen.queryByText(/^SAC/)).toBeNull();
    // A hypothetical non-SAC type-0 row (zero exist on prod) still gets its
    // type named rather than an empty cell.
    expect(screen.getByText('Token')).toBeInTheDocument();
  });
});
