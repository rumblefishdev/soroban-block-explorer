import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
  XdrOperationDto,
} from '@rumblefish/api-types';
import { Box, Typography } from '@mui/material';
import { OperationFlowTree } from '@rumblefish/soroban-block-explorer-ui';

import { toFlowNodes } from './toFlowNodes.js';

interface NormalRightPanelProps {
  tx: E3ResponseTransactionDetailLight;
  lightOp: OperationItem | undefined;
  heavyOp: XdrOperationDto | null;
}

export function NormalRightPanel({
  tx,
  lightOp,
  heavyOp,
}: NormalRightPanelProps) {
  if (lightOp == null) {
    return (
      <Box sx={{ p: 2 }}>
        <Typography
          variant="bodySmRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          No operation selected.
        </Typography>
      </Box>
    );
  }

  const nodes = toFlowNodes({ tx, light: lightOp, heavy: heavyOp });
  return <OperationFlowTree nodes={nodes} />;
}
