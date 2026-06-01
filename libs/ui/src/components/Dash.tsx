import { Typography } from '@mui/material';

/** Em-dash placeholder for a missing or not-applicable cell value. */
export function Dash() {
  return (
    <Typography
      component="span"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      —
    </Typography>
  );
}
