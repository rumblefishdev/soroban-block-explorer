import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

interface PageStubProps {
  title: string;
  path: string;
  rows?: Array<{ label: string; value: ReactNode }>;
  body?: ReactNode;
}

export function PageStub({ title, path, rows, body }: PageStubProps) {
  return (
    <Stack spacing={2} sx={{ py: 2 }}>
      <Box>
        <Typography variant="heading3SemiBold">{title}</Typography>
        <Typography
          variant="bodyMonoSmRegular"
          component="span"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          {path}
        </Typography>
      </Box>
      {body}
      {body === undefined && rows && rows.length > 0 && (
        <Stack spacing={0.5}>
          {rows.map(({ label, value }) => (
            <Stack
              key={label}
              direction="row"
              spacing={2}
              alignItems="baseline"
            >
              <Typography
                variant="bodySmMedium"
                sx={(theme) => ({
                  color: theme.palette.text.secondary,
                  minWidth: 120,
                })}
              >
                {label}
              </Typography>
              <Typography
                variant="bodyMonoSmRegular"
                sx={(theme) => ({ color: theme.palette.text.primary })}
              >
                {value}
              </Typography>
            </Stack>
          ))}
        </Stack>
      )}
      {body === undefined && (!rows || rows.length === 0) && (
        <Typography
          variant="bodyRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          Page implementation pending. Routing skeleton only.
        </Typography>
      )}
    </Stack>
  );
}
