import type { AccountDetailResponse } from '@rumblefish/api-types';
import {
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';

/**
 * Account summary card — account ID (copyable), sequence number, and the
 * first / last seen ledgers, each linked to its ledger detail page.
 */
export function AccountSummary({
  account,
}: {
  account: AccountDetailResponse;
}) {
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
                truncate={false}
              />
            ),
          },
        ]}
      />
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
