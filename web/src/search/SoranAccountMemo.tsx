import { Button, Stack, Typography } from '@mui/material';
import { CopyButton } from '@rumblefish/soroban-block-explorer-ui';
import { useQuery } from '@tanstack/react-query';
import { useSearchParams } from 'react-router-dom';

import { network } from '../network.js';
import { SummaryRow } from '../pages/detail/SummaryRow.js';
import {
  parseSoranName,
  resolveSoranPayment,
  resolveSoranPrimary,
} from './soran.js';

/** Payment instructions belong to a name, not to every user of its account. */
export function SoranAccountMemo({ address }: { address: string }) {
  const [params] = useSearchParams();
  const requested = params.get('soran');
  const supported = network === 'testnet' && address.startsWith('G');
  const primary = useQuery({
    queryKey: ['soranPrimary', network, address],
    queryFn: ({ signal }) => resolveSoranPrimary(address, network, signal),
    enabled: supported && requested == null,
    staleTime: 60_000,
    gcTime: 60_000,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const name =
    requested == null
      ? primary.data ?? null
      : parseSoranName(requested)?.name ?? null;
  const destination = useQuery({
    queryKey: ['soranAccountPayment', network, address, name],
    queryFn: ({ signal }) => resolveSoranPayment(name ?? '', network, signal),
    enabled: supported && name != null,
    staleTime: 0,
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });

  if (!supported || (requested == null && name == null)) return null;

  // Re-read on entry and never apply cached instructions while revalidating.
  const result =
    destination.fetchStatus === 'idle' ? destination.data : undefined;
  const payment =
    result?.kind === 'resolved' && result.payment.address === address
      ? result.payment
      : null;
  const failure = destination.isError
    ? 'Unable to load the memo.'
    : result?.kind === 'failed'
    ? result.reason
    : result?.kind === 'resolved' && payment == null
    ? 'This name no longer resolves to this account.'
    : null;

  return (
    <SummaryRow
      cells={[
        {
          label: 'Memo',
          value: (
            <Stack spacing={0.5} sx={{ py: 0.5 }}>
              {name != null && (
                <Typography variant="bodySmMedium">{name}</Typography>
              )}
              {payment ? (
                payment.memo.type === 'none' ? (
                  <Typography variant="bodySmRegular">
                    No memo required for this name.
                  </Typography>
                ) : (
                  <Stack direction="row" spacing={0.5} alignItems="center">
                    <Typography
                      variant="bodySmRegular"
                      sx={{ overflowWrap: 'anywhere', whiteSpace: 'pre-wrap' }}
                    >
                      Required memo ({payment.memo.type}): {payment.memo.value}
                    </Typography>
                    <CopyButton
                      value={payment.memo.value}
                      ariaLabel="Copy required Soran memo"
                    />
                  </Stack>
                )
              ) : (
                <Typography variant="bodySmRegular" role="status">
                  {name == null
                    ? 'Invalid Soran name.'
                    : destination.fetchStatus === 'paused'
                    ? 'Offline. Reconnect to load the memo.'
                    : failure ?? 'Loading memo…'}
                </Typography>
              )}
              {failure && (
                <Button
                  size="small"
                  onClick={() => void destination.refetch()}
                  sx={{ alignSelf: 'flex-start' }}
                >
                  Retry memo
                </Button>
              )}
            </Stack>
          ),
        },
      ]}
    />
  );
}
