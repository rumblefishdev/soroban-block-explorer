import { Typography } from '@mui/material';
import type { AccountSponsorship } from '@rumblefish/api-types';
import { formatAmount } from '@rumblefish/soroban-block-explorer-ui';

import { SummaryRow } from '../detail/SummaryRow.js';

/**
 * XLM the network locks per reserve — the base reserve, 0.5 XLM since 2019.
 * Validators can vote it, so the XLM figure is a reading aid beside the
 * reserve count, which is the ledger's own number.
 */
const BASE_RESERVE_XLM = 0.5;

function Reserves({ count, xlmNote }: { count: number; xlmNote: string }) {
  return (
    <>
      {formatAmount(count)} {count === 1 ? 'reserve' : 'reserves'}
      {count > 0 && (
        <Typography
          variant="bodySmRegular"
          component="span"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          {` · ${formatAmount(count * BASE_RESERVE_XLM)} XLM ${xlmNote}`}
        </Typography>
      )}
    </>
  );
}

/**
 * Sponsored reserves (CAP-33, issue #454): how many reserves this account
 * pays for others, and how many of its own others pay. Both are counters the
 * network keeps on the account, copied as stored.
 */
export function SponsoredReserves({
  sponsorship,
}: {
  sponsorship: AccountSponsorship;
}) {
  return (
    <SummaryRow
      cells={[
        {
          label: 'Sponsoring',
          value: (
            <Reserves
              count={sponsorship.num_sponsoring}
              xlmNote="locked for others"
            />
          ),
        },
        {
          label: 'Sponsored by others',
          value: (
            <Reserves
              count={sponsorship.num_sponsored}
              xlmNote="paid by sponsors"
            />
          ),
        },
      ]}
    />
  );
}
