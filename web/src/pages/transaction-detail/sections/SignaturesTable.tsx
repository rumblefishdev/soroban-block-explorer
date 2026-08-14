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
  Dash,
  DEFAULT_TRUNCATION,
  IdentifierWithCopy,
  monoFontFamily,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../../detail/SectionCard.js';
import { UnavailableSection } from '../shared/Unavailable.js';

export interface SignatureRow extends SignatureDto {
  signer?: string | null;
  weight?: number | null;
}

interface SignaturesTableProps {
  signatures: readonly SignatureRow[];
}

export function SignaturesTable({ signatures }: SignaturesTableProps) {
  // An empty list is rendered as "could not read", never as "has none",
  // because this component cannot tell those apart (0377 F1):
  //
  //  * `heavy` absent — the archive fetch failed, so nothing was decoded;
  //  * `heavy` present but its envelope missing — `align_envelopes` yields
  //    `None` per transaction on a hash miss and `extract_e3_heavy` still
  //    returns a heavy block, with `signatures: []`
  //    (`stellar_archive/extractors.rs`). A null check would MISS this one.
  //
  // A genuinely unsigned envelope may also be representable (a pre-authorised
  // transaction carries no signature at submission), which is the other reason
  // not to assert a cause here — we hold the same empty array either way.
  if (signatures.length === 0) {
    return (
      <SectionCard title="Signatures">
        {/* Carries its own explanation, unlike the sibling sections: those only
            appear when the whole heavy block is absent, which also raises the
            operations strip that explains it. Signatures can be empty while
            operations are present (they come from tx meta, the signatures from
            the envelope), so this can be the page's only sign of trouble. */}
        <UnavailableSection
          what="Signatures"
          description="This transaction's envelope could not be read from the Stellar archive."
        />
      </SectionCard>
    );
  }

  const count = signatures.length;
  return (
    <SectionCard
      title="Signatures"
      meta={`${count} signature${count === 1 ? '' : 's'}`}
    >
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
                      {truncateMiddle(sig.signature, DEFAULT_TRUNCATION)}
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
    </SectionCard>
  );
}
