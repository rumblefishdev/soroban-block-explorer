import {
  Box,
  Chip as MuiChip,
  type ChipProps as MuiChipProps,
} from '@mui/material';

function ChipDot() {
  return (
    <Box
      component="span"
      sx={{
        width: 8,
        height: 8,
        borderRadius: '50%',
        backgroundColor: 'currentColor',
        flexShrink: 0,
      }}
    />
  );
}

export interface ChipProps extends MuiChipProps {
  dot?: boolean;
}

export function Chip({ dot, icon, ...rest }: ChipProps) {
  return <MuiChip icon={dot ? <ChipDot /> : icon} {...rest} />;
}
