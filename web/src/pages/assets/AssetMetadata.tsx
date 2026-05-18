import { Box, Link, Typography } from '@mui/material';
import type { AssetDetailResponse } from '@rumblefish/api-types';

import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow, type SummaryCell } from '../detail/SummaryRow.js';
import { safeHttpUrl } from '../url.js';

import { AssetIcon } from './AssetIcon.js';

/**
 * Asset metadata card — the optional TOML-sourced name, icon, description and
 * homepage. Missing fields are tolerated gracefully: only available rows are
 * shown, and an empty section renders a short placeholder.
 */
export function AssetMetadata({ asset }: { asset: AssetDetailResponse }) {
  const rows: SummaryCell[] = [];

  if (asset.icon_url || asset.asset_code) {
    rows.push({
      label: 'Icon',
      value: <AssetIcon code={asset.asset_code} iconUrl={asset.icon_url} />,
    });
  }
  if (asset.name) {
    rows.push({ label: 'Name', value: asset.name });
  }
  if (asset.description) {
    rows.push({ label: 'Description', value: asset.description });
  }
  if (asset.home_page) {
    const safeUrl = safeHttpUrl(asset.home_page);
    rows.push({
      label: 'Homepage',
      value: safeUrl ? (
        <Link
          href={safeUrl}
          target="_blank"
          rel="noopener noreferrer"
          variant="bodySmRegular"
          sx={{ color: 'text.accent', wordBreak: 'break-all' }}
        >
          {asset.home_page}
        </Link>
      ) : (
        // Non-http(s) value — show as plain text, never as a clickable href.
        <Typography
          variant="bodySmRegular"
          sx={{ color: 'text.primary', wordBreak: 'break-all' }}
        >
          {asset.home_page}
        </Typography>
      ),
    });
  }

  return (
    <SectionCard title="Metadata" meta="From TOML">
      {rows.length === 0 ? (
        <Box sx={{ p: 3, textAlign: 'center' }}>
          <Typography variant="bodySmRegular" sx={{ color: 'text.tertiary' }}>
            No metadata available for this asset
          </Typography>
        </Box>
      ) : (
        rows.map((cell) => <SummaryRow key={cell.label} cells={[cell]} />)
      )}
    </SectionCard>
  );
}
