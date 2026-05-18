import InfoIcon from '@mui/icons-material/InfoOutlined';
import { Box, Card, Stack, Typography } from '@mui/material';
import { TableSectionHeader } from '@rumblefish/soroban-block-explorer-ui';

interface NftMetadataProps {
  /**
   * Off-chain JSON metadata. `null` means runtime enrichment could not
   * resolve it (IPFS timeout / unsupported content-type, ADR 0043) — this
   * is distinct from an NFT that simply has no attributes.
   */
  metadata?: Record<string, unknown> | null;
}

interface Attribute {
  label: string;
  value: string;
}

const MAX_VALUE_LEN = 120;

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** Render any JSON value as a single readable string, capped in length. */
function stringifyValue(v: unknown): string {
  let text: string;
  if (v === null || v === undefined) text = 'N/A';
  else if (typeof v === 'string') text = v;
  else if (typeof v === 'number' || typeof v === 'boolean') text = String(v);
  else text = JSON.stringify(v);
  return text.length > MAX_VALUE_LEN
    ? `${text.slice(0, MAX_VALUE_LEN)}…`
    : text;
}

/** Extract trait objects from an OpenSea-style `attributes` array. */
function parseAttributes(value: unknown): Attribute[] {
  if (!Array.isArray(value)) return [];
  return value.map((entry, index) => {
    if (isPlainObject(entry)) {
      const label = stringifyValue(entry.trait_type ?? `Trait ${index + 1}`);
      return { label, value: stringifyValue(entry.value ?? '') };
    }
    return { label: `Trait ${index + 1}`, value: stringifyValue(entry) };
  });
}

function Empty({ label }: { label: string }) {
  return (
    <Stack spacing={1} alignItems="center" sx={{ py: 6 }}>
      <InfoIcon sx={{ fontSize: 32, color: 'text.tertiary' }} />
      <Typography variant="bodySmRegular" sx={{ color: 'text.tertiary' }}>
        {label}
      </Typography>
    </Stack>
  );
}

function TraitCard({ label, value }: Attribute) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: 0.5,
        p: 2,
        borderRadius: `${theme.shape.radius.s}px`,
        backgroundColor: theme.palette.surface.grayMainAlt,
      })}
    >
      <Typography
        variant="bodySmRegular"
        sx={{ color: 'text.secondary', textAlign: 'center' }}
      >
        {label}
      </Typography>
      <Typography
        variant="heading5SemiBold"
        sx={{
          color: 'text.primary',
          textAlign: 'center',
          overflowWrap: 'anywhere',
        }}
      >
        {value || 'N/A'}
      </Typography>
    </Box>
  );
}

/**
 * The "Traits" card on the NFT detail page. Renders OpenSea-style
 * `attributes` as a grid of trait cards; tolerates a missing metadata blob
 * and NFTs with no attributes.
 */
export function NftMetadata({ metadata }: NftMetadataProps) {
  const attributes =
    metadata == null ? [] : parseAttributes(metadata.attributes);

  let body;
  if (metadata == null) {
    body = <Empty label="Metadata unavailable" />;
  } else if (attributes.length === 0) {
    body = <Empty label="No traits for this NFT" />;
  } else {
    body = (
      <Box
        sx={{
          display: 'grid',
          gridTemplateColumns: {
            xs: '1fr',
            sm: 'repeat(2, 1fr)',
            md: 'repeat(3, 1fr)',
          },
          gap: 1.5,
          p: 2,
        }}
      >
        {attributes.map((attr, index) => (
          <TraitCard key={`${attr.label}-${index}`} {...attr} />
        ))}
      </Box>
    );
  }

  return (
    <Card>
      <TableSectionHeader
        title="Traits"
        description={
          attributes.length > 0 ? `${attributes.length} attributes` : undefined
        }
      />
      {body}
    </Card>
  );
}
