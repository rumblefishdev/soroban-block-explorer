import { Box } from '@mui/material';
import { useQuery } from '@tanstack/react-query';

import type { AccountDetailResponse } from '@rumblefish/api-types';
import {
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { resolveFederatedName } from '../../search/federation.js';
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
  const homeDomain = account.home_domain ?? '';
  const federated = useQuery({
    queryKey: ['federatedName', account.account_id, homeDomain],
    queryFn: () => resolveFederatedName(account.account_id, homeDomain),
    enabled: homeDomain.length > 0,
    retry: false,
    staleTime: 10 * 60_000,
  });

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
      {/* Only when it resolved. The row is an attribute the account may or
          may not have, so its absence claims nothing — unlike the search
          path, where the user asked a question and silence would answer it
          wrongly. */}
      {federated.data != null && (
        <SummaryRow
          cells={[
            {
              label: 'Federated address',
              value: (
                <Box
                  component="span"
                  // Self-declared: the domain publishes this, we only check
                  // that the account named the domain first. Never a
                  // verified identity, and never the copyable canonical
                  // value — that stays the StrKey above.
                  title={`Published by ${homeDomain}, which this account names as its home domain. Not a verified identity.`}
                >
                  {federated.data}
                </Box>
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
