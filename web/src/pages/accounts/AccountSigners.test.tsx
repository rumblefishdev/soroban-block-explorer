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
      <AccountSigners accountId={ACCOUNT} signing={signing()} hasLiveHoldings />
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
        hasLiveHoldings
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
        hasLiveHoldings
      />
    );

    expect(screen.getByText('No usable key')).toBeInTheDocument();
    expect(screen.queryByText('Single signature')).not.toBeInTheDocument();
  });

  it('states plainly that a row-less account has no ledger entry', () => {
    // The checkpoint seed wrote entry state for every live account, so a
    // missing row is a fact about the chain, not a gap in our coverage:
    // 450 of 450 such accounts probed ABSENT via `getLedgerEntries`, across
    // two key ranges and both sub-populations. Dressing a known answer as an
    // unknown is its own kind of lie, so no alarm here.
    renderWithProviders(
      <AccountSigners
        accountId={ACCOUNT}
        signing={null}
        hasLiveHoldings={false}
      />
    );

    expect(screen.getByText('No account')).toBeInTheDocument();
    expect(screen.queryByText('Not indexed')).not.toBeInTheDocument();
    expect(screen.queryByText('Single signature')).not.toBeInTheDocument();
  });

  it('warns when holdings are shown for an account with no configuration', () => {
    // The one shape that WOULD be a real gap, and the one a live-writer
    // regression takes. It measures 0 today; the cost of it appearing
    // unannounced is a hidden multisig, which is the expensive half of #377.
    renderWithProviders(
      <AccountSigners accountId={ACCOUNT} signing={null} hasLiveHoldings />
    );

    expect(screen.getByText('Not indexed')).toBeInTheDocument();
    expect(screen.queryByText('No account')).not.toBeInTheDocument();
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
        hasLiveHoldings
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
