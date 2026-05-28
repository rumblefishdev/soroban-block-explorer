import type { SignatureDto } from '@rumblefish/api-types';
import {
  Box,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material';
import {
  CopyButton,
  IdentifierWithCopy,
  monoFontFamily,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../../detail/SectionCard.js';

export interface SignatureRow extends SignatureDto {
  signer?: string | null;
  weight?: number | null;
}

interface SignaturesTableProps {
  signatures: readonly SignatureRow[];
}

function Dash() {
  return (
    <Typography
      component="span"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      —
    </Typography>
  );
}

export function SignaturesTable({ signatures }: SignaturesTableProps) {
  const count = signatures.length;
  return (
    <SectionCard
      title="Signatures"
      meta={`${count} signature${count === 1 ? '' : 's'}`}
    >
      {count === 0 ? (
        <Box sx={{ p: 3 }}>
          <Typography
            variant="bodySmRegular"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            No signatures recorded.
          </Typography>
        </Box>
      ) : (
        <Box sx={{ overflowX: 'auto' }}>
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell
                  sx={(theme) => ({
                    backgroundColor: theme.palette.surface.backgroundAlt,
                  })}
                >
                  Signer
                </TableCell>
                <TableCell
                  sx={(theme) => ({
                    width: 110,
                    backgroundColor: theme.palette.surface.backgroundAlt,
                  })}
                >
                  Weight
                </TableCell>
                <TableCell
                  sx={(theme) => ({
                    backgroundColor: theme.palette.surface.backgroundAlt,
                  })}
                >
                  Signature hex
                </TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {signatures.map((sig) => (
                <TableRow key={`${sig.hint}-${sig.signature}`}>
                  <TableCell>
                    {sig.signer != null ? (
                      <IdentifierWithCopy value={sig.signer} type="account" />
                    ) : (
                      <Dash />
                    )}
                  </TableCell>
                  <TableCell>
                    {sig.weight != null ? sig.weight : <Dash />}
                  </TableCell>
                  <TableCell>
                    <Stack direction="row" spacing={1} alignItems="center">
                      <Typography
                        component="span"
                        sx={(theme) => ({
                          fontFamily: monoFontFamily,
                          fontSize: 14,
                          color: theme.palette.text.primary,
                        })}
                      >
                        {truncateMiddle(sig.signature, {
                          prefix: 12,
                          suffix: 12,
                        })}
                      </Typography>
                      <CopyButton
                        value={sig.signature}
                        ariaLabel="Copy signature"
                      />
                    </Stack>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Box>
      )}
    </SectionCard>
  );
}
