import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { routes } from '../../../router/routes.js';
import { renderWithProviders } from '../../../test-utils.js';
import { CallerCell } from '../ContractInvocations.js';

const ACCOUNT = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';
const CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

describe('Invocations tab Caller cell (task 0487)', () => {
  it('links an account caller to the account', () => {
    renderWithProviders(
      <CallerCell row={{ caller_account: ACCOUNT, caller_contract: null }} />
    );
    expect(screen.getByRole('link')).toHaveAttribute(
      'href',
      routes.account(ACCOUNT)
    );
  });

  it('links a contract caller to the contract instead of a dash', () => {
    renderWithProviders(
      <CallerCell row={{ caller_account: null, caller_contract: CONTRACT }} />
    );
    expect(screen.getByRole('link')).toHaveAttribute(
      'href',
      routes.contract(CONTRACT)
    );
  });

  it('renders no link when neither caller is known', () => {
    renderWithProviders(
      <CallerCell row={{ caller_account: null, caller_contract: null }} />
    );
    expect(screen.queryByRole('link')).toBeNull();
  });
});
