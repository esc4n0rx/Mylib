import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from '@mui/material';
import { describe, expect, it } from 'vitest';
import { ToastProvider } from '@/app/ToastProvider';
import { MediaPosterCard } from '@/features/media/components/MediaPosterCard';
import { formatFileSize, formatRuntime, imageUrl } from '@/features/media/utils';
import { createAppTheme } from '@/theme/theme';

describe('media catalog UI', () => {
  it('formats media metadata in pt-BR', () => {
    expect(formatRuntime(126)).toBe('2h 06min');
    expect(formatFileSize(1_073_741_824)).toBe('1 GB');
    expect(imageUrl('/poster.jpg')).toContain('/w500/poster.jpg');
  });

  it('renders a reusable TV poster card with favorite action', () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <MemoryRouter>
        <ThemeProvider theme={createAppTheme('light')}>
          <QueryClientProvider client={client}>
            <ToastProvider>
              <MediaPosterCard item={{ id: 'show', title: 'Estação Final', year: 2023, rating: 8.1, genres: [], mediaType: 'TV_SHOW', libraryId: 'library', addedAt: '2026-01-01', isFavorite: false, numberOfSeasons: 2 }} />
            </ToastProvider>
          </QueryClientProvider>
        </ThemeProvider>
      </MemoryRouter>,
    );
    expect(screen.getByText('Estação Final')).toBeInTheDocument();
    expect(screen.getByText('2023 • 2 temp.')).toBeInTheDocument();
    expect(screen.getByLabelText('Adicionar à Minha Lista')).toBeInTheDocument();
  });
});
