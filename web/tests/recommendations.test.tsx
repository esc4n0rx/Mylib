import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from '@mui/material';
import { ToastProvider } from '@/app/ToastProvider';
import { RecommendationSection, RecommendationSkeleton } from '@/features/recommendations/components/RecommendationSection';
import { createAppTheme } from '@/theme/theme';

function Location() { return <span data-testid="location">{useLocation().pathname}</span>; }

function Providers({ children, mode = 'light' }: { children: React.ReactNode; mode?: 'light'|'dark' }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <MemoryRouter><ThemeProvider theme={createAppTheme(mode)}><QueryClientProvider client={client}><ToastProvider>{children}<Location /></ToastProvider></QueryClientProvider></ThemeProvider></MemoryRouter>;
}

const section = { key: 'for_you', title: 'Recomendado para Você', items: [{ id: 'movie', title: 'Interestelar', year: 2014, rating: 8.7, genres: [], mediaType: 'MOVIE' as const, libraryId: 'movies', addedAt: '2026-01-01', isFavorite: false, recommendationReason: 'Porque você gosta de Ficção científica' }] };

describe('recomendações na Home', () => {
  it('renderiza título, motivo em pt-BR e navega para o conteúdo', () => {
    render(<Providers><RecommendationSection section={section} /></Providers>);
    expect(screen.getByText('Recomendado para Você')).toBeInTheDocument();
    expect(screen.getByText('Porque você gosta de Ficção científica')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Interestelar'));
    expect(screen.getByTestId('location')).toHaveTextContent('/media/movie');
  });

  it('exibe loading e funciona no tema escuro', () => {
    render(<Providers mode="dark"><RecommendationSkeleton /></Providers>);
    expect(screen.getByLabelText('Carregando recomendações')).toBeInTheDocument();
    expect(screen.getByText('Recomendado para Você')).toBeInTheDocument();
  });
});
