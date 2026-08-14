import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Box, Grid } from '@mui/material';
import { useMemo } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';

import { buildOperationEntries } from './operationEntries.js';
import { OperationCard } from '../op-card/OperationCard.js';
import { OperationPicker } from './OperationPicker.js';
import { UnavailableSection } from '../shared/Unavailable.js';
import { useSelectedOp } from '../useSelectedOp.js';

interface OperationsSectionProps {
  tx: E3ResponseTransactionDetailLight;
}

export function OperationsSection({ tx }: OperationsSectionProps) {
  const entries = useMemo(() => buildOperationEntries(tx), [tx]);
  // Header count from operation_count (always present, never folded) — the
  // picker list may be shorter when heavy is unavailable (task 0329).
  const count = tx.operation_count;
  // The `#op-N` fragment is resolved HERE because this is where the list it
  // has to be valid against already exists. Handing the count up to the page
  // and the answer back down as props derived the same number twice.
  const [selectedIndex, select] = useSelectedOp(entries.length);
  // `selectedIndex` always addresses an existing entry, so there is no range
  // guard here. Two attempts lived here before: falling back to `entries[0]`
  // while the picker beside it got the raw index and highlighted nothing, then
  // replacing the card with a message that pointed at a picker single-operation
  // transactions never render. Resolving the fragment against the list removes
  // the need for either.
  const selected = entries[selectedIndex];

  // 87 % of mainnet transactions have exactly one operation — an index with
  // one entry is pure width tax, so the card takes the full row (0460 #5,
  // the adaptive-index option deferred from 0453).
  const showPicker = entries.length > 1;

  const card = (
    /* Remount on op switch so disclosure state resets per operation. */
    <OperationCard
      key={selectedIndex}
      light={selected?.light}
      heavy={selected?.heavy ?? null}
      applied={tx.successful}
      fallbackOrder={selectedIndex + 1}
      txSourceAccount={tx.source_account ?? null}
      operationTree={tx.heavy?.operation_tree}
      contractEvents={tx.heavy?.contract_events ?? []}
      diagnosticEvents={tx.heavy?.diagnostic_events ?? []}
    />
  );

  // No archive data, no operation list. The DB's appearance index is NOT used
  // as a stand-in: it folds same-identity operations into one row, so it would
  // render "1" under a header that correctly says "4". The header count stays —
  // it comes from the transaction row and is true — and the body says plainly
  // that the operations could not be read (0377 F7).
  if (entries.length === 0) {
    return (
      <SectionCard
        title="Operations"
        meta={`${count} Operation${count === 1 ? '' : 's'}`}
      >
        <UnavailableSection what="Operations" />
      </SectionCard>
    );
  }

  return (
    <SectionCard
      title="Operations"
      meta={`${count} Operation${count === 1 ? '' : 's'}`}
    >
      <Box sx={{ p: 2 }}>
        {showPicker ? (
          <Grid container spacing={2}>
            {/* The card carries the substance; the picker is an index — keep
                it narrow (0460 #10). */}
            <Grid size={{ xs: 12, md: 5, lg: 4 }}>
              <OperationPicker
                entries={entries}
                txSourceAccount={tx.source_account ?? null}
                selectedIndex={selectedIndex}
                onSelect={select}
              />
            </Grid>
            <Grid size={{ xs: 12, md: 7, lg: 8 }}>{card}</Grid>
          </Grid>
        ) : (
          card
        )}
      </Box>
    </SectionCard>
  );
}
