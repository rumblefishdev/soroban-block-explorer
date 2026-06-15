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
  const hasIcon = Boolean(asset.icon_url || asset.asset_code);
  const hasName = Boolean(asset.name);

  const iconNameRow: SummaryCell[] = [];
  if (hasIcon) {
    iconNameRow.push({
      label: 'Icon',
      value: (
        <Box sx={{ py: 0.5 }}>
          <AssetIcon
            code={asset.asset_code}
            iconUrl={asset.icon_url}
            size={32}
          />
        </Box>
      ),
    });
  }
  if (hasName) {
    iconNameRow.push({ label: 'Name', value: asset.name });
  }

  const safeUrl = safeHttpUrl(asset.home_page);
  const domain = safeUrl
    ? new URL(safeUrl).hostname.replace(/^www\./, '')
    : null;

  const homepageCell: SummaryCell | null = asset.home_page
    ? {
        label: 'Homepage',
        value: safeUrl ? (
          <Link
            href={safeUrl}
            target="_blank"
            rel="noopener noreferrer"
            variant="bodySmMedium"
            sx={(theme) => ({
              color: theme.palette.text.accent,
              wordBreak: 'break-all',
            })}
          >
            {asset.home_page}
          </Link>
        ) : (
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({
              color: theme.palette.text.primary,
              wordBreak: 'break-all',
            })}
          >
            {asset.home_page}
          </Typography>
        ),
      }
    : null;

  const domainHomepageRow: SummaryCell[] = [];
  if (domain) {
    domainHomepageRow.push({ label: 'Domain', value: domain });
  }
  if (homepageCell) {
    domainHomepageRow.push(homepageCell);
  }

  const hasAny =
    iconNameRow.length > 0 || asset.description || domainHomepageRow.length > 0;

  return (
    <SectionCard title="Metadata" meta="From TOML">
      {!hasAny ? (
        <Box sx={{ p: 3, textAlign: 'center' }}>
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            No metadata available for this asset
          </Typography>
        </Box>
      ) : (
        <>
          {iconNameRow.length > 0 && <SummaryRow cells={iconNameRow} />}
          {asset.description && (
            <SummaryRow
              cells={[{ label: 'Description', value: asset.description }]}
            />
          )}
          {domainHomepageRow.length > 0 && (
            <SummaryRow cells={domainHomepageRow} />
          )}
        </>
      )}
    </SectionCard>
  );
}
