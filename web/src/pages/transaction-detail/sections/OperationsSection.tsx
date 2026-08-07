import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Box, Grid, Typography } from '@mui/material';
import { useMemo } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';

import { buildOperationEntries } from './operationEntries.js';
import { OperationCard } from '../op-card/OperationCard.js';
import { OperationPicker } from './OperationPicker.js';
import { UnavailableSection } from '../shared/Unavailable.js';

interface OperationsSectionProps {
  tx: E3ResponseTransactionDetailLight;
  selectedIndex: number;
  onSelect: (index: number) => void;
}

export function OperationsSection({
  tx,
  selectedIndex,
  onSelect,
}: OperationsSectionProps) {
  const entries = useMemo(() => buildOperationEntries(tx), [tx]);
  // Header count from operation_count (always present, never folded) — the
  // picker list may be shorter when heavy is unavailable (task 0329).
  const count = tx.operation_count;
  // `#op-N` is user-supplied, so the index can point past the end. It is NOT
  // silently swapped for the first operation: that showed a real operation
  // under a number the reader asked for and did not get, which reads as an
  // answer rather than a miss. Say what happened and leave the picker to
  // recover from.
  const outOfRange = selectedIndex < 0 || selectedIndex >= entries.length;
  const selected = entries[selectedIndex];

  // 87 % of mainnet transactions have exactly one operation — an index with
  // one entry is pure width tax, so the card takes the full row (0460 #5,
  // the adaptive-index option deferred from 0453).
  const showPicker = entries.length > 1;

  const card = outOfRange ? (
    <Typography
      variant="bodySmRegular"
      sx={(theme) => ({ color: theme.palette.text.tertiary, p: 2 })}
    >
      This transaction has no operation {selectedIndex + 1} — it has{' '}
      {entries.length}. Pick one from the list.
    </Typography>
  ) : (
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
                onSelect={onSelect}
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
