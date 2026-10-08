import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import {
  Accordion,
  AccordionDetails,
  AccordionSummary,
  Box,
  Stack,
  Typography,
} from '@mui/material';
import { useState } from 'react';
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
      <Stack direction="row" spacing={1} alignItems="baseline">
        <span>Trustline {code}</span>
        <IdentifierDisplay value={issuer} type="account" fontSize={12} />
      </Stack>
    );
  }
  if (entry.kind === 'signer' && entry.signer != null) {
    return (
      <Stack direction="row" spacing={1} alignItems="baseline">
        <span>Signer</span>
        {/* A signer is a key, not an account: most have no account entry
            on the ledger, so a link would land on a missing page. */}
        <IdentifierDisplay
          value={entry.signer}
          type="account"
          linked={false}
          fontSize={12}
        />
      </Stack>
    );
  }
  return <>{entry.kind}</>;
}

function Line({ children }: { children: ReactNode }) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        px: 2,
        py: 1.25,
        backgroundColor: theme.palette.surface.grayMain,
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
 * One sponsor: a header with its total, folding open to the entries it pays.
 * A sponsor can pay for up to 1,000 trustlines, so its entries are paged
 * `PAGE_SIZE` at a time.
 */
function SponsorGroup({
  sponsor,
  entries,
}: {
  sponsor: string;
  entries: AccountSponsoredEntry[];
}) {
  const [page, setPage] = useState(0);
  const reserves = entries.reduce((sum, e) => sum + e.reserves, 0);
  const lastPage = Math.max(0, Math.ceil(entries.length / PAGE_SIZE) - 1);
  const start = page * PAGE_SIZE;
  const shown = entries.slice(start, start + PAGE_SIZE);
  return (
    <Accordion
      disableGutters
      square
      elevation={0}
      sx={(theme) => ({
        backgroundColor: 'transparent',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&::before': { display: 'none' },
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <AccordionSummary
        expandIcon={<KeyboardArrowDownIcon fontSize="small" />}
        sx={(theme) => ({
          flexDirection: 'row-reverse',
          gap: 1,
          backgroundColor: theme.palette.surface.grayMainAlt,
          color: theme.palette.text.tertiary,
          '& .MuiAccordionSummary-content': {
            alignItems: 'baseline',
            justifyContent: 'space-between',
            gap: 2,
            minWidth: 0,
          },
        })}
      >
        <Stack direction="row" spacing={1} alignItems="baseline">
          <Typography variant="bodySmRegular" component="span">
            Paid by
          </Typography>
          <IdentifierDisplay value={sponsor} type="account" fontSize={13} />
        </Stack>
        <Typography variant="bodySmRegular" component="span">
          {formatAmount(reserves)} {reserves === 1 ? 'reserve' : 'reserves'}
        </Typography>
      </AccordionSummary>
      <AccordionDetails sx={{ p: 0 }}>
        {shown.map((e) => (
          <Line key={`${e.kind}-${e.asset ?? e.signer ?? ''}`}>
            <EntryLabel entry={e} />
            <Typography variant="bodyMedium" component="span">
              {e.reserves}
            </Typography>
          </Line>
        ))}
        {entries.length > PAGE_SIZE && (
          <PaginationControls
            caption={`${start + 1}–${start + shown.length} of ${
              entries.length
            }`}
            canPrev={page > 0}
            canNext={page < lastPage}
            onPrev={() => setPage(page - 1)}
            onNext={() => setPage(page + 1)}
          />
        )}
      </AccordionDetails>
    </Accordion>
  );
}

/**
 * The card's body once the answer is in. Exported for tests.
 *
 * One folding group per sponsor, folded at first: the sponsors and their
 * totals in view, the entries a click away.
 */
export function SponsorshipList({
  data,
}: {
  data: AccountSponsorshipResponse;
}) {
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
        entries or pool shares.
      </Line>
    );
  }
  const groups = bySponsor(data.entries);
  const unlisted = unlistedReserves(data);
  return (
    <Box>
      {groups.map(([sponsor, entries]) => (
        <SponsorGroup key={sponsor} sponsor={sponsor} entries={entries} />
      ))}
      {unlisted > 0 && (
        <Line>
          {`${formatAmount(unlisted)} more sponsored ${
            unlisted === 1 ? 'reserve is' : 'reserves are'
          } on offers, data entries or pool shares, which are not listed here.`}
        </Line>
      )}
    </Box>
  );
}

/** The card header: how many accounts pay this account's reserves. */
export function sponsorshipMeta(data: AccountSponsorshipResponse): string {
  const sponsors = new Set(data.entries.map((e) => e.sponsor)).size;
  return `${sponsors} ${sponsors === 1 ? 'sponsor' : 'sponsors'}`;
}

/** Sponsored reserves the list cannot name — offers, data entries, pool
 * shares. */
export function unlistedReserves(data: AccountSponsorshipResponse): number {
  const listed = data.entries.reduce((sum, e) => sum + e.reserves, 0);
  return Math.max(0, data.num_sponsored - listed);
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
