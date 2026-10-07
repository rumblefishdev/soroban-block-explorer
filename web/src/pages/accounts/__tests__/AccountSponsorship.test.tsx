import type { AccountSponsorshipResponse } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import {
  AccountSponsorship,
  SponsorshipList,
  sponsorshipMeta,
} from '../AccountSponsorship.js';

const SPONSOR_A = 'GCUISJEWU2TZ4QIJNGNVU4BSZ5CQS3KE6A3N3ETOV7XHCBVO4GLTLGOQ';
const SPONSOR_B = 'GCNPDPJLTEAPL2FOAYXSSYF6VB6EPP2KFUMXGAJKLIIC3XQLWIY6GQLX';
const USDC_ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

/** `GBEEFP…CT56` on mainnet, 2026-10-07: 6 reserves, two sponsors. */
const GBEEFP: AccountSponsorshipResponse = {
  num_sponsored: 6,
  entries: [
    { kind: 'account', reserves: 2, sponsor: SPONSOR_A },
    {
      kind: 'signer',
      signer: 'GASJKOU7K4HZ3SV7WNYJCUJ4HC2UZK2G3SQDKN6K5MOTGZW5OBF7XQB2',
      reserves: 1,
      sponsor: SPONSOR_A,
    },
    {
      kind: 'signer',
      signer: 'GBZJ3BGSGMUJQ3YPVR2GTXU6Q5R3TVRMXGJT6ZRF6YCXJPOJ3UDN3BGS',
      reserves: 1,
      sponsor: SPONSOR_B,
    },
    {
      kind: 'trustline',
      asset: `USDC-${USDC_ISSUER}`,
      reserves: 1,
      sponsor: SPONSOR_A,
    },
  ],
};

describe('AccountSponsorship', () => {
  it('groups the entries by the account paying them', () => {
    renderWithProviders(<SponsorshipList data={GBEEFP} />);

    expect(screen.getAllByText('Paid by')).toHaveLength(2);
    expect(screen.getByText('Account (base reserve)')).toBeInTheDocument();
    expect(screen.getByText('Trustline USDC')).toBeInTheDocument();
    expect(screen.getAllByText('Signer')).toHaveLength(2);
    // Sponsor A pays account 2 + signer 1 + trustline 1.
    expect(screen.getByText('4 reserves')).toBeInTheDocument();
    expect(screen.getByText('1 reserve')).toBeInTheDocument();
  });

  it('counts what is listed against what the chain holds', () => {
    expect(sponsorshipMeta(GBEEFP)).toBe('5 of 6 reserves paid by 2 sponsors');
    expect(sponsorshipMeta({ ...GBEEFP, num_sponsored: 5 })).toBe(
      '5 of 5 reserves paid by 2 sponsors'
    );
  });

  it('is not shown for an account nobody sponsors', () => {
    const { container } = renderWithProviders(
      <AccountSponsorship accountId="GA" numSponsored={0} />
    );
    expect(container).toBeEmptyDOMElement();
  });
});
