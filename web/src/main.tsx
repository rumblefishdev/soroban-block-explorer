import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { QueryProvider } from './api/index.js';
import { App } from './app.js';

const rootElement = document.getElementById('root');

if (!rootElement) {
  throw new Error(
    'Root element not found. Ensure index.html contains <div id="root"></div>.'
  );
}

createRoot(rootElement).render(
  <StrictMode>
    <QueryProvider>
      <App />
    </QueryProvider>
  </StrictMode>
);
