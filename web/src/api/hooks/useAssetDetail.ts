import { getAssetOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * Fetches asset detail (`GET /assets/:id`) — code, issuer/contract, type,
 * supply, holders, and TOML metadata. Disabled until an id is present.
 */
export const useAssetDetail = (id: string) =>
  useQuery({
    ...getAssetOptions({ path: { id } }),
    ...detailPolicy,
    enabled: id.length > 0,
  });
