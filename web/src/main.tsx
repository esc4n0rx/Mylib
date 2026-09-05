import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { RouterProvider } from 'react-router-dom';
import { QueryClientProvider } from '@tanstack/react-query';
import '@fontsource/geist-sans/400.css';
import '@fontsource/geist-sans/500.css';
import '@fontsource/geist-sans/600.css';
import '@fontsource/geist-sans/700.css';

import './i18n';
import { session } from './api';
import { queryClient } from './app/queryClient';
import { AuthProvider } from './app/AuthProvider';
import { ToastProvider } from './app/ToastProvider';
import { ErrorBoundary } from './app/ErrorBoundary';
import { ThemeModeProvider } from './theme/ThemeModeProvider';
import { router } from './routes';

session.load();

const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('#root not found');

createRoot(rootEl).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeModeProvider>
        <ToastProvider>
          <ErrorBoundary>
            <AuthProvider>
              <RouterProvider router={router} />
            </AuthProvider>
          </ErrorBoundary>
        </ToastProvider>
      </ThemeModeProvider>
    </QueryClientProvider>
  </StrictMode>,
);
