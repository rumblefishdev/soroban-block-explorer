import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';

import { QueryProvider } from './api/index.js';
import { App } from './app.js';
import { markTestnetTab, network } from './network.js';
import './styles/fonts.css';

markTestnetTab(network);

const rootElement = document.getElementById('root');

if (!rootElement) {
  throw new Error(
    'Root element not found. Ensure index.html contains <div id="root"></div>.'
  );
}

createRoot(rootElement).render(
  <StrictMode>
    <ExplorerThemeProvider>
      <QueryProvider>
        <App />
      </QueryProvider>
    </ExplorerThemeProvider>
  </StrictMode>
);
