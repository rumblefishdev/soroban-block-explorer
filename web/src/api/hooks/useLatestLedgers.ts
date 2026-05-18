import { listLedgersOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { homePolicy } from '../polling.js';

/**
 * Latest 10 ledgers for the home page activity table. Polls on the home
 * cadence (10s stale / 12s refetch); no cursor pagination — always the
 * newest rows.
 */
export const useLatestLedgers = () =>
  useQuery({
    ...listLedgersOptions({ query: { limit: 10 } }),
    ...homePolicy,
  });
