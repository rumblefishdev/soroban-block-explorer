import { Box, CircularProgress } from '@mui/material';

interface SearchSpinnerProps {
  size?: number;
  minHeight?: number;
}

export function SearchSpinner({
  size = 24,
  minHeight = 80,
}: SearchSpinnerProps) {
  return (
    <Box
      sx={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        minHeight,
        width: '100%',
      }}
    >
      <CircularProgress size={size} />
    </Box>
  );
}
