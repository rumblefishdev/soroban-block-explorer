import type { AccountSponsorshipResponse } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PAGE_SIZE } from '../../../api/polling.js';
import { renderWithProviders } from '../../../test-utils.js';
import {
  AccountSponsorship,
  SponsorshipList,
  sponsorshipMeta,
} from '../AccountSponsorship.js';

const hook = vi.hoisted(() => ({ useAccountSponsorship: vi.fn() }));
vi.mock('../../../api/index.js', () => hook);

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
    hook.useAccountSponsorship.mockReturnValue({ isLoading: false });
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

  it('says the network holds no entry when the API answers 404', () => {
    hook.useAccountSponsorship.mockReturnValue({
      isLoading: false,
      isError: true,
      error: { status: 404 },
      data: undefined,
    });
    renderWithProviders(<AccountSponsorship accountId="GA" numSponsored={3} />);
    expect(
      screen.getByText(/holds no entry for this account now/)
    ).toBeInTheDocument();
  });

  it('shows the shared error state with a retry on any other failure', () => {
    hook.useAccountSponsorship.mockReturnValue({
      isLoading: false,
      isError: true,
      error: { status: 500 },
      data: undefined,
    });
    renderWithProviders(<AccountSponsorship accountId="GA" numSponsored={3} />);
    // The shared section error state, with its retry, as every other card.
    expect(screen.getByText('Something went wrong')).toBeInTheDocument();
    expect(screen.getByText('Try again')).toBeInTheDocument();
  });

  it('names offers and data entries when nothing listable is sponsored', () => {
    renderWithProviders(
      <SponsorshipList data={{ num_sponsored: 1, entries: [] }} />
    );
    expect(screen.getByText(/can be listed here/)).toBeInTheDocument();
  });

  it('pages long lists like the Assets card, totals per sponsor kept', () => {
    const many: AccountSponsorshipResponse = {
      num_sponsored: PAGE_SIZE + 2 + 1,
      entries: [
        { kind: 'account', reserves: 2, sponsor: SPONSOR_A },
        ...Array.from({ length: PAGE_SIZE + 1 }, (_, i) => ({
          kind: 'trustline',
          asset: `T${i}-${USDC_ISSUER}`,
          reserves: 1,
          sponsor: SPONSOR_A,
        })),
      ],
    };
    renderWithProviders(<SponsorshipList data={many} />);

    expect(
      screen.getByText(`1–${PAGE_SIZE} of ${PAGE_SIZE + 2}`)
    ).toBeInTheDocument();
    // The sponsor's header counts all its reserves, not just this page's.
    expect(screen.getByText(`${PAGE_SIZE + 3} reserves`)).toBeInTheDocument();
  });
});
