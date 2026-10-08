import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';
import type {
  AccountSponsoredEntry,
  AccountSponsorshipResponse,
} from '@rumblefish/api-types';
import {
  CardSkeleton,
  Chip,
  classifyError,
  formatAmount,
  IdentifierDisplay,
  isMissingResource,
  PaginationControls,
  QueryErrorState,
} from '@rumblefish/soroban-block-explorer-ui';
import { useSearchParams } from 'react-router-dom';

import { useAccountSponsorship } from '../../api/index.js';
import { PAGE_SIZE } from '../../api/polling.js';
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

/**
 * The card's body once the answer is in. Exported for tests.
 *
 * Paged like the Assets card: a page of `PAGE_SIZE` entries, sliced in the
 * order RPC listed them grouped by sponsor, its position in `?sponsors=`. A
 * sponsor whose entries straddle two pages heads both, with its full total.
 */
export function SponsorshipList({
  data,
}: {
  data: AccountSponsorshipResponse;
}) {
  const [params, setParams] = useSearchParams();
  // Our index counted sponsored reserves the network no longer holds: the
  // account changed since. The network is the newer word.
  if (data.num_sponsored === 0) {
    return (
      <Line>
        The network reports no sponsored entries for this account now.
      </Line>
    );
  }
  // Sponsored, but only in entries this list cannot name.
  if (data.entries.length === 0) {
    return (
      <Line>
        None of the sponsored entries can be listed here — they are offers, data
        entries, pool shares, or trustlines the network did not return.
      </Line>
    );
  }
  const groups = bySponsor(data.entries);
  const totals = new Map(
    groups.map(([sponsor, entries]) => [
      sponsor,
      entries.reduce((sum, e) => sum + e.reserves, 0),
    ])
  );
  const ordered = groups.flatMap(([, entries]) => entries);
  const paged = ordered.length > PAGE_SIZE;
  const lastPage = Math.max(0, Math.ceil(ordered.length / PAGE_SIZE) - 1);
  // Clamped, not trusted, like `?assets=`.
  const asked = Number(params.get('sponsors') ?? '1');
  const page = Number.isSafeInteger(asked)
    ? Math.min(Math.max(asked - 1, 0), lastPage)
    : 0;
  const goTo = (next: number) =>
    setParams(
      (prev) => {
        const p = new URLSearchParams(prev);
        if (next <= 0) p.delete('sponsors');
        else p.set('sponsors', String(next + 1));
        return p;
      },
      { replace: true }
    );
  const start = page * PAGE_SIZE;
  const shown = paged ? ordered.slice(start, start + PAGE_SIZE) : ordered;

  return (
    <Box>
      {bySponsor(shown).map(([sponsor, entries]) => {
        const reserves = totals.get(sponsor) ?? 0;
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
      {paged && (
        <PaginationControls
          caption={`${start + 1}–${start + shown.length} of ${ordered.length}`}
          canPrev={page > 0}
          canNext={page < lastPage}
          onPrev={() => goTo(page - 1)}
          onNext={() => goTo(page + 1)}
        />
      )}
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
  if (query.isPending) {
    body = <CardSkeleton />;
  } else if (query.isError && isMissingResource(classifyError(query.error))) {
    body = <Line>The network holds no entry for this account now.</Line>;
  } else if (query.isError || query.data == null) {
    body = (
      <QueryErrorState
        error={query.error}
        onRetry={() => void query.refetch()}
      />
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
