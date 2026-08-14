import { Box, Typography } from '@mui/material';

import { SectionCard } from '../../detail/SectionCard.js';

import { UnavailableSection } from '../shared/Unavailable.js';
import { XdrRow } from '../shared/XdrRow.js';

interface RawDataSectionProps {
  envelopeXdr: string | null | undefined;
  resultXdr: string | null | undefined;
  resultMetaXdr: string | null | undefined;
  /** `heavy` absent — the XDR was never loaded, not proven absent. */
  unavailable?: boolean;
}

interface XdrEntry {
  label: string;
  value: string;
  xdrType: string;
}

function present(value: string | null | undefined): value is string {
  return value != null && value.length > 0;
}

export function RawDataSection({
  envelopeXdr,
  resultXdr,
  resultMetaXdr,
  unavailable = false,
}: RawDataSectionProps) {
  // All three XDR blobs come from `heavy`; absent means unfetched, so the
  // section count below would assert a zero nothing measured (0377 F2).
  if (unavailable) {
    return (
      <SectionCard title="Raw data">
        <UnavailableSection what="Raw data" />
      </SectionCard>
    );
  }

  const entries: XdrEntry[] = [];
  if (present(envelopeXdr))
    entries.push({
      label: 'envelope_xdr',
      value: envelopeXdr,
      xdrType: 'TransactionEnvelope',
    });
  if (present(resultXdr))
    entries.push({
      label: 'result_xdr',
      value: resultXdr,
      xdrType: 'TransactionResult',
    });
  // The ledger-entry changes (0460 #13).
  if (present(resultMetaXdr))
    entries.push({
      label: 'result_meta_xdr',
      value: resultMetaXdr,
      xdrType: 'TransactionMeta',
    });

  return (
    <SectionCard
      title="Raw data"
      meta={`${entries.length} section${entries.length === 1 ? '' : 's'}`}
    >
      {entries.length === 0 ? (
        <Box sx={{ p: 3 }}>
          <Typography
            variant="bodySmRegular"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            No raw XDR available for this transaction.
          </Typography>
        </Box>
      ) : (
        <Box>
          {entries.map((entry) => (
            <XdrRow
              key={entry.label}
              label={entry.label}
              value={entry.value}
              xdrType={entry.xdrType}
            />
          ))}
        </Box>
      )}
    </SectionCard>
  );
}
