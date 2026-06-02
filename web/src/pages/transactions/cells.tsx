import { Box, Typography } from '@mui/material';
import { Chip, Dash } from '@rumblefish/soroban-block-explorer-ui';

import { formatOperationType } from './operationTypes.js';

/**
 * Operation-type cell shared by every transaction table: the first operation
 * type as a chip, with a `+N` count when a transaction has more than one.
 */
export function OperationCell({ types }: { types: readonly string[] }) {
  if (types.length === 0) return <Dash />;
  const [first, ...rest] = types;
  return (
    <Box sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}>
      <Chip size="sm" color="neutral" label={formatOperationType(first)} />
      {rest.length > 0 && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          +{rest.length}
        </Typography>
      )}
    </Box>
  );
}
