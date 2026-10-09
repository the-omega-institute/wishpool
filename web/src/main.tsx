import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import 'katex/dist/katex.min.css';
import './styles/tokens.css';
import './styles/app.css';
import { App } from './App';
import { createApiClient } from './api/client';
import { ApiProvider } from './api/context';
import { PolicyProvider } from './api/policy';
import { SessionProvider } from './auth/session';
import { RouterProvider } from './routing/router';

const container = document.getElementById('root');
if (!container) throw new Error('root element missing');

const client = createApiClient();

createRoot(container).render(
  <StrictMode>
    <ApiProvider client={client}>
      <SessionProvider>
        <PolicyProvider>
          <RouterProvider>
            <App />
          </RouterProvider>
        </PolicyProvider>
      </SessionProvider>
    </ApiProvider>
  </StrictMode>,
);
