import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { routes } from '../../../router/routes.js';
import { renderWithProviders } from '../../../test-utils.js';
import { OwnerIdentifier } from '../OwnerIdentifier.js';

const ACCOUNT = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';
const CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

describe('NFT owner identifier (task 0376)', () => {
  it('links an account owner to the account', () => {
    renderWithProviders(<OwnerIdentifier value={ACCOUNT} />);
    expect(screen.getByRole('link')).toHaveAttribute(
      'href',
      routes.account(ACCOUNT)
    );
  });

  it('links a contract owner to the contract', () => {
    renderWithProviders(<OwnerIdentifier value={CONTRACT} />);
    expect(screen.getByRole('link')).toHaveAttribute(
      'href',
      routes.contract(CONTRACT)
    );
  });
});
