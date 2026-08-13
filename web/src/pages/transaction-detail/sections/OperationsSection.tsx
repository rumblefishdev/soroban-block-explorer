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
  /** 1-based number `#op-N` asked for when this transaction has no such
   *  operation — `useSelectedOp` owns that judgement (task 0482). */
  missingOp: number | null;
  onSelect: (index: number) => void;
}

export function OperationsSection({
  tx,
  selectedIndex,
  missingOp,
  onSelect,
}: OperationsSectionProps) {
  const entries = useMemo(() => buildOperationEntries(tx), [tx]);
  // Header count from operation_count (always present, never folded) — the
  // picker list may be shorter when heavy is unavailable (task 0329).
  const count = tx.operation_count;
  // `selectedIndex` always addresses an existing entry: `useSelectedOp`
  // resolved the user-supplied `#op-N` against this same list, the way
  // `useTableUrlState` resolves `sort`/`dir`. No range guard here — the one
  // that used to live here answered a bad fragment by replacing the operation
  // with a message, which for the ~85 % of transactions carrying a single
  // operation blanked the section and pointed at a picker they never get.
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

  /* The fragment named an operation that does not exist. Say so ABOVE the
     operation rather than instead of it: the reader still gets the page they
     came for, and the number they asked for and did not get is named instead
     of quietly becoming a different one. */
  const missingNotice = missingOp != null && (
    <Typography
      variant="bodySmRegular"
      sx={(theme) => ({ color: theme.palette.text.tertiary, mb: 2 })}
    >
      This transaction has no operation {missingOp} — it has {entries.length}.
      Showing operation {selectedIndex + 1}.
    </Typography>
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
        {missingNotice}
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
