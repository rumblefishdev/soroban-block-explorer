import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { Box, Typography } from '@mui/material';

import { OperationJsonDetail } from './OperationJsonDetail.js';

interface AdvancedRightPanelProps {
  lightOp: OperationItem | undefined;
  heavyOp: XdrOperationDto | null;
}

export function AdvancedRightPanel({
  lightOp,
  heavyOp,
}: AdvancedRightPanelProps) {
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
  return <OperationJsonDetail light={lightOp} heavy={heavyOp} />;
}
