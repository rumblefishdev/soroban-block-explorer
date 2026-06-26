import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Box, Grid } from '@mui/material';
import { useMemo } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';
import { AdvancedRightPanel } from '../advanced/AdvancedRightPanel.js';
import { NormalRightPanel } from '../normal/NormalRightPanel.js';
import type { DetailMode } from '../useDetailMode.js';

import { buildOperationEntries } from './operationEntries.js';
import { OperationPicker } from './OperationPicker.js';

interface OperationsSectionProps {
  tx: E3ResponseTransactionDetailLight;
  mode: DetailMode;
  selectedIndex: number;
  onSelect: (index: number) => void;
}

export function OperationsSection({
  tx,
  mode,
  selectedIndex,
  onSelect,
}: OperationsSectionProps) {
  const entries = useMemo(() => buildOperationEntries(tx), [tx]);
  const rows = useMemo(() => entries.map((e) => e.row), [entries]);
  // Header count from operation_count (always present, never folded) — the
  // picker list (rows) may be shorter when heavy is unavailable (task 0329).
  const count = tx.operation_count;
  const selected = entries[selectedIndex] ?? entries[0];
  const selectedLightOp = selected?.light;
  const selectedHeavyOp = selected?.heavy ?? null;

  const rightPanel =
    mode === 'normal' ? (
      <NormalRightPanel
        tx={tx}
        lightOp={selectedLightOp}
        heavyOp={selectedHeavyOp}
      />
    ) : (
      <AdvancedRightPanel lightOp={selectedLightOp} heavyOp={selectedHeavyOp} />
    );

  return (
    <SectionCard
      title="Operations"
      meta={`${count} Operation${count === 1 ? '' : 's'}`}
    >
      <Box sx={{ p: 2 }}>
        <Grid container spacing={2}>
          <Grid size={{ xs: 12, md: 6 }}>
            <OperationPicker
              operations={rows}
              selectedIndex={selectedIndex}
              onSelect={onSelect}
            />
          </Grid>
          <Grid size={{ xs: 12, md: 6 }}>{rightPanel}</Grid>
        </Grid>
      </Box>
    </SectionCard>
  );
}
