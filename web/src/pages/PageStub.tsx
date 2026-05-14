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
          sx={{ color: 'text.tertiary' }}
        >
          {path}
        </Typography>
      </Box>
      {body}
      {!body && rows && rows.length > 0 && (
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
                sx={{ color: 'text.secondary', minWidth: 120 }}
              >
                {label}
              </Typography>
              <Typography
                variant="bodyMonoSmRegular"
                sx={{ color: 'text.primary' }}
              >
                {value}
              </Typography>
            </Stack>
          ))}
        </Stack>
      )}
      {!body && (!rows || rows.length === 0) && (
        <Typography variant="bodyRegular" sx={{ color: 'text.tertiary' }}>
          Page implementation pending. Routing skeleton only.
        </Typography>
      )}
    </Stack>
  );
}
