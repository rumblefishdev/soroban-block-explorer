import { Typography } from '@mui/material';
import { useQuery } from '@tanstack/react-query';

import { network } from '../network.js';
import { resolveSoranPrimary } from './soran.js';

/** An onchain Soran Primary, shown beside the unchanged Testnet address. */
export function SoranPrimaryName({ address }: { address: string }) {
  const query = useQuery({
    queryKey: ['soranPrimary', network, address],
    queryFn: ({ signal }) => resolveSoranPrimary(address, network, signal),
    enabled: network === 'testnet',
    staleTime: 60_000,
    gcTime: 60_000,
    retry: false,
    refetchOnWindowFocus: false,
  });

  if (network !== 'testnet' || query.isError || query.data == null) return null;

  return (
    <Typography
      variant="bodySmRegular"
      component="span"
      title="Primary name read from Soran's Testnet contracts"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      {query.data}
    </Typography>
  );
}
