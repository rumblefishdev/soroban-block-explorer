import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';
import type {
  AccountSponsoredEntry,
  AccountSponsorshipResponse,
} from '@rumblefish/api-types';
import {
  Chip,
  classifyError,
  formatAmount,
  IdentifierDisplay,
} from '@rumblefish/soroban-block-explorer-ui';

import { useAccountSponsorship } from '../../api/index.js';
import { SectionCard } from '../detail/SectionCard.js';

/** What the entry is, in words: the account itself, a trustline, a signer. */
function EntryLabel({ entry }: { entry: AccountSponsoredEntry }) {
  if (entry.kind === 'account') {
    return <>Account (base reserve)</>;
  }
  if (entry.kind === 'trustline' && entry.asset != null) {
    const [code, issuer = ''] = entry.asset.split('-');
    return (
      <Stack direction="row" spacing={1} alignItems="center">
        <span>Trustline {code}</span>
        <IdentifierDisplay value={issuer} type="account" fontSize={12} />
      </Stack>
    );
  }
  if (entry.kind === 'signer' && entry.signer != null) {
    return (
      <Stack direction="row" spacing={1} alignItems="center">
        <span>Signer</span>
        {/* Only `G…` signers are accounts; hash signers link nowhere. */}
        <IdentifierDisplay
          value={entry.signer}
          type="account"
          linked={entry.signer.startsWith('G')}
          fontSize={12}
        />
      </Stack>
    );
  }
  return <>{entry.kind}</>;
}

function Line({ children, header }: { children: ReactNode; header?: boolean }) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        px: 2,
        py: 1.25,
        backgroundColor: header
          ? theme.palette.surface.grayMainAlt
          : theme.palette.surface.grayMain,
        color: header ? theme.palette.text.tertiary : undefined,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      {children}
    </Box>
  );
}

/** Entries grouped by the account paying them, in the order RPC listed them. */
function bySponsor(entries: AccountSponsoredEntry[]) {
  const groups = new Map<string, AccountSponsoredEntry[]>();
  for (const e of entries) {
    groups.set(e.sponsor, [...(groups.get(e.sponsor) ?? []), e]);
  }
  return [...groups.entries()];
}

/** The card's body once the answer is in. Exported for tests. */
export function SponsorshipList({
  data,
}: {
  data: AccountSponsorshipResponse;
}) {
  // Our index counted sponsored reserves the network no longer holds: the
  // account changed since. The network is the newer word.
  if (data.entries.length === 0) {
    return (
      <Line>
        The network reports no sponsored entries for this account now.
      </Line>
    );
  }
  const groups = bySponsor(data.entries);
  return (
    <Box>
      {groups.map(([sponsor, entries]) => {
        const reserves = entries.reduce((sum, e) => sum + e.reserves, 0);
        return (
          <Box key={sponsor}>
            <Line header>
              <Stack direction="row" spacing={1} alignItems="center">
                <Typography variant="bodySmRegular" component="span">
                  Paid by
                </Typography>
                <IdentifierDisplay
                  value={sponsor}
                  type="account"
                  fontSize={13}
                />
              </Stack>
              <Typography variant="bodySmRegular" component="span">
                {formatAmount(reserves)}{' '}
                {reserves === 1 ? 'reserve' : 'reserves'}
              </Typography>
            </Line>
            {entries.map((e) => (
              <Line key={`${e.kind}-${e.asset ?? e.signer ?? ''}`}>
                <EntryLabel entry={e} />
                <Typography variant="bodyMedium" component="span">
                  {e.reserves}
                </Typography>
              </Line>
            ))}
          </Box>
        );
      })}
    </Box>
  );
}

/** `6 of 6 reserves paid by 2 sponsors` — and says when some are not listed. */
export function sponsorshipMeta(data: AccountSponsorshipResponse): string {
  const listed = data.entries.reduce((sum, e) => sum + e.reserves, 0);
  const sponsors = new Set(data.entries.map((e) => e.sponsor)).size;
  return `${formatAmount(listed)} of ${formatAmount(
    data.num_sponsored
  )} reserves paid by ${sponsors} ${sponsors === 1 ? 'sponsor' : 'sponsors'}`;
}

/**
 * Sponsored reserves (CAP-33, issue #454): which of this account's entries
 * another account pays for, and who. Read live from RPC, so it loads after
 * the rest of the page and says so when RPC does not answer.
 */
export function AccountSponsorship({
  accountId,
  numSponsored,
}: {
  accountId: string;
  /** From the account detail; the card shows only when it is above 0. */
  numSponsored: number;
}) {
  const query = useAccountSponsorship(accountId, numSponsored > 0);
  if (numSponsored === 0) {
    return null;
  }
  let body: ReactNode;
  if (query.isLoading) {
    body = <Line>Reading sponsors from the network…</Line>;
  } else if (classifyError(query.error) === 'not-found') {
    body = <Line>The network holds no entry for this account now.</Line>;
  } else if (query.isError || query.data == null) {
    body = (
      <Line>
        Sponsors are unavailable right now — the network did not answer.
      </Line>
    );
  } else {
    body = <SponsorshipList data={query.data} />;
  }
  return (
    <SectionCard
      title="Sponsored reserves"
      meta={
        query.data != null ? (
          <Chip size="sm" color="neutral" label={sponsorshipMeta(query.data)} />
        ) : undefined
      }
    >
      {body}
    </SectionCard>
  );
}
