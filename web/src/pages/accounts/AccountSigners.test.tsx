import type { AccountSigning } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import { AccountSigners } from './AccountSigners.js';

const ACCOUNT = 'GDXWIA4VF3GW2R5OSVIROD47W6AQHE33DSEG6TF7YZD3DYOVU54MYBEN';
const SIGNER_A = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';
const PREAUTH = 'TA7X4KFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75CCW67TSZV3SSS2H';

function signing(overrides: Partial<AccountSigning> = {}): AccountSigning {
  return {
    signers: [{ key: SIGNER_A, weight: 1, type: 'ed25519' }],
    master_weight: 1,
    threshold_low: 1,
    threshold_med: 2,
    threshold_high: 2,
    last_updated_ledger: 64_115_052,
    ...overrides,
  };
}

describe('AccountSigners', () => {
  it('puts the account’s own key first, badged, because the ledger omits it', () => {
    // The chain keeps the master key OUT of the signer list, so a page showing
    // only the list reads 3-of-4 where the chain says 3-of-5.
    renderWithProviders(
      <AccountSigners accountId={ACCOUNT} signing={signing()} deleted={false} />
    );

    expect(screen.getByText('master key')).toBeInTheDocument();
    expect(screen.getByText('master')).toBeInTheDocument();
    // Both keys are on screen, and the weights add up in the footer.
    expect(screen.getByText(/Total weight 2/)).toBeInTheDocument();
    expect(screen.getByText(/thresholds low 1/)).toBeInTheDocument();
  });

  it('says a weight-0 master key is DISABLED, not a low-weight signer', () => {
    // 703,871 accounts on pubnet are configured this way — the second most
    // common shape, not an edge case.
    renderWithProviders(
      <AccountSigners
        accountId={ACCOUNT}
        signing={signing({ master_weight: 0 })}
        deleted={false}
      />
    );

    expect(screen.getByText('master key — disabled')).toBeInTheDocument();
    expect(screen.queryByText('master key')).not.toBeInTheDocument();
  });

  it('flags an account no key can sign for', () => {
    // Master disabled AND no signers: 69,576 accounts. Deliberate, permanent,
    // and it must not read as an ordinary account.
    renderWithProviders(
      <AccountSigners
        accountId={ACCOUNT}
        signing={signing({ master_weight: 0, signers: [] })}
        deleted={false}
      />
    );

    expect(screen.getByText('No usable key')).toBeInTheDocument();
    expect(screen.queryByText('Single signature')).not.toBeInTheDocument();
  });

  it('calls an unobserved configuration unknown, never "single signature"', () => {
    // 3.7M of 14.6M accounts carry no row. Rendering that as a known answer
    // would be a security claim the data does not support.
    renderWithProviders(
      <AccountSigners accountId={ACCOUNT} signing={null} deleted={false} />
    );

    expect(screen.getByText('Not indexed')).toBeInTheDocument();
    expect(screen.queryByText('Single signature')).not.toBeInTheDocument();
    expect(screen.queryByText('Multisig')).not.toBeInTheDocument();
  });

  it('gives a merged account the right reason for the same missing data', () => {
    // A removed account has no ledger entry at all — "not indexed" would blame
    // our coverage for something the chain settled.
    renderWithProviders(
      <AccountSigners accountId={ACCOUNT} signing={null} deleted />
    );

    expect(screen.getByText('No account')).toBeInTheDocument();
    expect(screen.queryByText('Not indexed')).not.toBeInTheDocument();
  });

  it('links an ed25519 signer and leaves a pre-auth transaction unlinked', () => {
    // `preauth_tx` (`T…`) and `hash_x` (`X…`) are not accounts; linking one
    // would route to an account page that cannot exist.
    renderWithProviders(
      <AccountSigners
        accountId={ACCOUNT}
        signing={signing({
          signers: [
            { key: SIGNER_A, weight: 1, type: 'ed25519' },
            { key: PREAUTH, weight: 1, type: 'preauth_tx' },
          ],
        })}
        deleted={false}
      />
    );

    expect(
      screen.getByRole('link', { name: new RegExp(SIGNER_A.slice(0, 6)) })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: new RegExp(PREAUTH.slice(0, 6)) })
    ).not.toBeInTheDocument();
  });
});
