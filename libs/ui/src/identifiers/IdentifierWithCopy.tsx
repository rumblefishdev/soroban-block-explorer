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
        // Snug: the round button carries 6px of its own inner padding, so
        // even a small flex gap reads as a visual gulf next to the text.
        gap: 0.25,
        maxWidth: '100%',
      }}
    >
      <IdentifierDisplay value={value} {...displayProps} />
      <CopyButton value={value} ariaLabel={copyAriaLabel} />
    </Box>
  );
}
