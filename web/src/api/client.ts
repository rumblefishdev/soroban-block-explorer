import { client } from '@rumblefish/api-types';

import { apiBaseUrl } from './config.js';

client.setConfig({ baseUrl: apiBaseUrl });

client.interceptors.error.use((error, response) => {
  const status = response?.status;
  if (error && typeof error === 'object') {
    (error as { status?: number }).status = status;
    return error;
  }
  return { body: error, status };
});
