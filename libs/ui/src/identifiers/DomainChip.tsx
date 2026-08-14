import Box from '@mui/material/Box';

import { Chip } from '../components/Chip.js';

export interface DomainChipProps {
  /** On-chain `home_domain`, e.g. `centre.io`. Renders nothing when absent. */
  domain: string | null | undefined;
}

/**
 * An account's self-declared `home_domain`, as a chip linking to that site.
 *
 * Shared by the accounts list and the assets list so the two cannot drift; it
 * was inlined in the accounts table first (task 0450 lifted it out).
 *
 * **Not a verified identity.** The account holder sets `home_domain` itself and
 * nothing checks that the domain owns the account — hence a neutral chip, never
 * a tick or a "verified" colour.
 */
export function DomainChip({ domain }: DomainChipProps) {
  if (!domain) return null;
  // On-chain domains carry no scheme, so a bare href would resolve as a
  // relative path. Only prefix when the stored value lacks one.
  const href = /^https?:\/\//.test(domain) ? domain : `https://${domain}`;
  return (
    <Box
      component="a"
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      sx={{
        display: 'inline-flex',
        textDecoration: 'none',
        cursor: 'pointer',
        minWidth: 0,
      }}
    >
      <Chip size="sm" color="neutral" label={domain} />
    </Box>
  );
}
