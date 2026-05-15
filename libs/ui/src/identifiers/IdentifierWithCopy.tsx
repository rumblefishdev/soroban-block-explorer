import Box from '@mui/material/Box';

import { CopyButton } from './CopyButton.js';
import {
  IdentifierDisplay,
  type IdentifierDisplayProps,
} from './IdentifierDisplay.js';

export interface IdentifierWithCopyProps extends IdentifierDisplayProps {
  copyAriaLabel?: string;
}

export function IdentifierWithCopy({
  value,
  copyAriaLabel,
  ...displayProps
}: IdentifierWithCopyProps) {
  return (
    <Box
      sx={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 1,
        maxWidth: '100%',
      }}
    >
      <IdentifierDisplay value={value} {...displayProps} />
      <CopyButton value={value} ariaLabel={copyAriaLabel} />
    </Box>
  );
}
