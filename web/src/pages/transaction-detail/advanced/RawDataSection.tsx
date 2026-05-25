import { Box, Typography } from '@mui/material';

import { SectionCard } from '../../detail/SectionCard.js';

import { XdrRow } from './XdrRow.js';

interface RawDataSectionProps {
  envelopeXdr: string | null | undefined;
  resultXdr: string | null | undefined;
  resultsMetaXdr: string | null | undefined;
}

interface XdrEntry {
  label: string;
  value: string;
}

function present(value: string | null | undefined): value is string {
  return value != null && value.length > 0;
}

export function RawDataSection({
  envelopeXdr,
  resultXdr,
  resultsMetaXdr,
}: RawDataSectionProps) {
  const entries: XdrEntry[] = [];
  if (present(envelopeXdr))
    entries.push({ label: 'envelope_xdr', value: envelopeXdr });
  if (present(resultXdr))
    entries.push({ label: 'result_xdr', value: resultXdr });
  if (present(resultsMetaXdr))
    entries.push({ label: 'results_meta_xdr', value: resultsMetaXdr });

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
            <XdrRow key={entry.label} label={entry.label} value={entry.value} />
          ))}
        </Box>
      )}
    </SectionCard>
  );
}
