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

/**
 * `GBEEFP…CT56` on mainnet, as the API answered on 2026-10-08: the account,
 * two signers and the USDC trustline paid by one sponsor, a third signer by
 * another — 6 reserves.
 */
const GBEEFP: AccountSponsorshipResponse = {
  num_sponsored: 6,
  entries: [
    { kind: 'account', reserves: 2, sponsor: SPONSOR_A },
    {
      kind: 'signer',
      signer: 'GASJKOU7K4WASDH53W6PLXYAZPIFGIK6XSMIMYWK3XNQRICATUDFCM5P',
      reserves: 1,
      sponsor: SPONSOR_A,
    },
    {
      kind: 'signer',
      signer: 'GA2GNZF7ABYUBWEXAM6XES6URPHJ7XYXCB7OSEG6B4HIUZZYJ4CXEK6C',
      reserves: 1,
      sponsor: SPONSOR_A,
    },
    {
      kind: 'signer',
      signer: 'GBZJ3BGSGMR7PLYD6GTCTO543GTZHJVAF5NSQ4U3RA3N66I5IVACRVWS',
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
    expect(screen.getAllByText('Signer')).toHaveLength(3);
    // Sponsor A pays account 2 + two signers + the trustline.
    expect(screen.getByText('5 reserves')).toBeInTheDocument();
    expect(screen.getByText('1 reserve')).toBeInTheDocument();
  });

  it('counts what is listed against what the chain holds', () => {
    expect(sponsorshipMeta(GBEEFP)).toBe('6 of 6 reserves paid by 2 sponsors');
    // An offer or data entry the list cannot name.
    expect(sponsorshipMeta({ ...GBEEFP, num_sponsored: 7 })).toBe(
      '6 of 7 reserves paid by 2 sponsors'
    );
  });

  it('is not shown for an account nobody sponsors', () => {
    const { container } = renderWithProviders(
      <AccountSponsorship accountId="GA" numSponsored={0} />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('says so when the network no longer holds sponsored entries', () => {
    renderWithProviders(
      <SponsorshipList data={{ num_sponsored: 0, entries: [] }} />
    );
    expect(
      screen.getByText(/no sponsored entries for this account now/)
    ).toBeInTheDocument();
  });
});
