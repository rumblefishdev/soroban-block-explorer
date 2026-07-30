import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Box, Grid } from '@mui/material';
import { useMemo } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';

import { buildOperationEntries } from './operationEntries.js';
import { OperationCard } from '../op-card/OperationCard.js';
import { OperationPicker } from './OperationPicker.js';

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

  return (
    <SectionCard
      title="Operations"
      meta={`${count} Operation${count === 1 ? '' : 's'}`}
    >
      <Box sx={{ p: 2 }}>
        <Grid container spacing={2}>
          <Grid size={{ xs: 12, md: 6 }}>
            <OperationPicker
              operations={entries.map((entry) => entry.row)}
              selectedIndex={selectedIndex}
              onSelect={onSelect}
            />
          </Grid>
          <Grid size={{ xs: 12, md: 6 }}>
            {/* Remount on op switch so disclosure state resets per operation. */}
            <OperationCard
              key={selectedIndex}
              light={selected?.light}
              heavy={selected?.heavy ?? null}
              applied={tx.successful}
              fallbackOrder={selectedIndex + 1}
              txSourceAccount={tx.source_account ?? null}
              operationTree={tx.heavy?.operation_tree}
              contractEvents={tx.heavy?.contract_events ?? []}
            />
          </Grid>
        </Grid>
      </Box>
    </SectionCard>
  );
}
