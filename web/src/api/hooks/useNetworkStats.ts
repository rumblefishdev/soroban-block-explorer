import { getNetworkStatsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { homePolicy } from '../polling.js';

export const useNetworkStats = () =>
  useQuery({
    ...getNetworkStatsOptions(),
    ...homePolicy,
  });
