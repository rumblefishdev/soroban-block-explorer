import { Box, Skeleton, Stack, Tooltip } from '@mui/material';

import type { AccountDetailResponse } from '@rumblefish/api-types';
import {
  Chip,
  CopyButton,
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { useFederatedName } from '../../search/useFederation.js';
import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';

/**
 * Account summary card — account ID (copyable), sequence number, and the
 * first / last seen ledgers, each linked to its ledger detail page.
 *
 * Also the SEP-2 federated address, when the account's home domain claims
 * one (task 0443 scope B). Resolved in the browser, not stored: the value is
 * the domain's to change at any moment, and a cached copy would be a stale
 * claim presented as a current one.
 */
export function AccountSummary({
  account,
}: {
  account: AccountDetailResponse;
}) {
  // The lookup lives in a named hook with the other queries' seam, so this
  // card stays presentation over its props. `federationPolicy` carries the
  // reasoning for the cache window, the missing retry and the suppressed
  // window-focus refetch, in one place both directions share.
  const homeDomain = account.home_domain ?? '';
  const federated = useFederatedName(account.account_id, homeDomain);

  return (
    <SectionCard title="Summary">
      <SummaryRow
        cells={[
          {
            label: 'Account ID',
            value: (
              <IdentifierWithCopy
                value={account.account_id}
                type="account"
                linked={false}
                truncate={false}
              />
            ),
          },
        ]}
      />
      {/* The row holds its place while the lookup runs, then goes for good if
          there is nothing to show. Rendering it only on success made the card
          grow a line under the reader's eye, seconds after the page settled.
          Its absence claims nothing — unlike the search path, where the user
          asked a question and silence would answer it wrongly. */}
      {(federated.isPending || federated.data != null) && (
        <SummaryRow
          cells={[
            {
              label: 'Federated address',
              value:
                federated.data == null ? (
                  <Skeleton variant="text" width={180} />
                ) : (
                  <Stack direction="row" spacing={1} alignItems="center">
                    <Box component="span">{federated.data}</Box>
                    {/* The human-readable form is the one people paste into a
                      wallet, so it gets the same copy affordance as the key. */}
                    <CopyButton value={federated.data} />
                    {/* Self-declared, and the caveat has to be visible rather
                      than hidden behind a hover: the domain publishes this,
                      we only check that the account named the domain first.
                      Never a verified identity, and never the copyable
                      canonical value — that stays the StrKey above. */}
                    <Tooltip
                      title={`${homeDomain} publishes this name for the account, and the account names ${homeDomain} as its home domain. Neither statement is verified by the network.`}
                    >
                      <Box component="span" sx={{ display: 'inline-flex' }}>
                        <Chip size="sm" color="neutral" label="self-declared" />
                      </Box>
                    </Tooltip>
                  </Stack>
                ),
            },
          ]}
        />
      )}
      <SummaryRow
        cells={[
          {
            label: 'Sequence number',
            value: formatAmount(account.sequence_number),
          },
        ]}
      />
      <SummaryRow
        cells={[
          {
            label: 'First seen ledger',
            value: (
              <IdentifierDisplay
                value={String(account.first_seen_ledger)}
                type="ledger"
              />
            ),
          },
          {
            label: 'Last seen ledger',
            value: (
              <IdentifierDisplay
                value={String(account.last_seen_ledger)}
                type="ledger"
              />
            ),
          },
        ]}
      />
    </SectionCard>
  );
}
