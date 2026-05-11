import { getTransactionOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

export const useTransactionDetail = (hash: string) =>
  useQuery({
    ...getTransactionOptions({ path: { hash } }),
    ...detailPolicy,
    enabled: hash.length > 0,
  });
