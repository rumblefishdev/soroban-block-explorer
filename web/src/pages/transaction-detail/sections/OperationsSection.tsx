import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Box, Grid } from '@mui/material';

import { SectionCard } from '../../detail/SectionCard.js';
import { AdvancedRightPanel } from '../advanced/AdvancedRightPanel.js';
import { NormalRightPanel } from '../normal/NormalRightPanel.js';
import type { DetailMode } from '../useDetailMode.js';

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
  const ops = tx.operations;
  const count = ops.length;
  const heavyOps = tx.heavy?.operations ?? [];
  const selectedLightOp = ops[selectedIndex] ?? ops[0];
  const selectedHeavyOp =
    selectedLightOp != null
      ? heavyOps.find(
          (h) => h.application_order === selectedLightOp.application_order
        ) ?? null
      : null;

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
              operations={ops}
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
