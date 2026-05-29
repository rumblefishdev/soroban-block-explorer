import { Box, Typography } from '@mui/material';

/**
 * Small green-dot "LIVE" indicator — shown on the chain-overview panel and
 * the activity-table section headers to signal the page auto-refreshes.
 */
export function LiveIndicator() {
  return (
    <Box
      component="span"
      sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}
    >
      <Box
        component="span"
        sx={(theme) => ({
          width: 6,
          height: 6,
          borderRadius: '50%',
          backgroundColor: theme.palette.stroke.success,
        })}
      />
      <Typography
        component="span"
        variant="bodyXsMedium"
        sx={(theme) => ({
          color: theme.palette.text.secondary,
          letterSpacing: '0.06em',
        })}
      >
        LIVE
      </Typography>
    </Box>
  );
}
