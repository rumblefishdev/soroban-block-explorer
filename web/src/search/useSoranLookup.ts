import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import { network } from '../network.js';
import { parseSoranName, resolveSoranPayment } from './soran.js';

const LOOKUP_DELAY_MS = 350;

/** Resolve after typing pauses; Enter can submit the current name immediately. */
export function useSoranLookup(q: string, askOnMount = false) {
  const trimmed = q.trim();
  const name = parseSoranName(trimmed)?.name ?? null;
  const inputKey = `${network}:${trimmed}`;
  const [askedFor, setAskedFor] = useState<string | null>(
    askOnMount ? inputKey : null
  );
  const supported = network === 'testnet';
  const armed = supported && name != null && askedFor === inputKey;

  useEffect(() => {
    // An edit hides the previous answer immediately and starts a fresh delay.
    // Clear the submitted key too, so returning to an old query still waits.
    setAskedFor((previous) => (previous === inputKey ? previous : null));
    if (!supported || name == null) return;
    const timer = setTimeout(() => setAskedFor(inputKey), LOOKUP_DELAY_MS);
    return () => clearTimeout(timer);
  }, [inputKey, name, supported]);

  const query = useQuery({
    queryKey: ['soranPayment', network, name],
    queryFn: ({ signal }) => resolveSoranPayment(name ?? '', network, signal),
    enabled: supported && name != null && armed,
    staleTime: 0,
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });

  return {
    name,
    supported,
    armed,
    isFetching: armed && query.isFetching,
    isPaused: armed && query.fetchStatus === 'paused',
    result: armed && query.fetchStatus === 'idle' ? query.data : undefined,
    ask: () => {
      if (!supported || name == null) return;
      if (armed) {
        if (!query.isFetching) void query.refetch();
      } else setAskedFor(inputKey);
    },
  };
}

export type SoranLookup = ReturnType<typeof useSoranLookup>;
