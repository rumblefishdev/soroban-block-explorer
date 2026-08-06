import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Box, Grid } from '@mui/material';
import { useMemo } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';

import { buildOperationEntries } from './operationEntries.js';
import { OperationCard } from '../op-card/OperationCard.js';
import { OperationPicker } from './OperationPicker.js';
import { StatusStrip } from '../shared/StatusStrip.js';

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
  const selected = entries[selectedIndex] ?? entries[0];

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

  // Two causes, one symptom: no heavy block at all, or a heavy block whose
  // operations came back empty (the archive answered but this transaction's
  // envelope was missing). Either way the picker falls back to the folded light
  // rows — shorter than `count` — and the card's execution trace, authorized
  // calls, events and route strip all resolve empty, each vanishing with no
  // hint, so an invoke reads as "made no sub-calls and emitted no events".
  // Testing the operations array rather than `heavy` catches both (0377 F7).
  const executionDetailMissing = (tx.heavy?.operations?.length ?? 0) === 0;

  return (
    <SectionCard
      title="Operations"
      meta={`${count} Operation${count === 1 ? '' : 's'}`}
    >
      {executionDetailMissing && (
        <StatusStrip tone="warning">
          Execution detail unavailable — sub-calls, events and raw data could
          not be read from the Stellar archive
          {entries.length < count
            ? `, and only ${entries.length} of ${count} operations can be listed`
            : ''}
        </StatusStrip>
      )}
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
