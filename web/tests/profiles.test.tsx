import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider } from '@mui/material';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AuthProvider } from '@/app/AuthProvider';
import ProfilesPage from '@/features/profiles/pages/ProfilesPage';
import { createAppTheme } from '@/theme/theme';
import { session } from '@/api';

const profiles = [
  { id: 'adult', userId: 'user', name: 'Paulo', avatarId: 'default.png', avatarUrl: '/api/v1/avatars/fallback/default.png', isDefault: true, isKids: false, isActive: true, pinProtected: true, maxAgeRating: 18 },
  { id: 'kids', userId: 'user', name: 'Kids', avatarId: 'kids.png', avatarUrl: '/api/v1/avatars/fallback/kids.png', isDefault: false, isKids: true, isActive: true, pinProtected: false, maxAgeRating: 10 },
];

afterEach(() => { session.clear(); vi.restoreAllMocks(); });

describe('profiles screen', () => {
  it('renders pt-BR cards and opens profile management', async () => {
    session.set('account-token');
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const payload = url.endsWith('/auth/me')
        ? { id: 'user', username: 'paulo', displayName: 'Paulo', isAdmin: true, roles: ['Administrator'], permissions: [] }
        : url.endsWith('/profiles') ? { items: profiles } : { items: [] };
      return new Response(JSON.stringify(payload), { status: 200, headers: { 'Content-Type': 'application/json' } });
    }));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(<MemoryRouter><ThemeProvider theme={createAppTheme('dark')}><QueryClientProvider client={client}><AuthProvider><ProfilesPage /></AuthProvider></QueryClientProvider></ThemeProvider></MemoryRouter>);

    expect(await screen.findByText('Quem está assistindo?')).toBeInTheDocument();
    expect(await screen.findByText('Paulo')).toBeInTheDocument();
    expect(screen.getByText('Kids')).toBeInTheDocument();
    const images = document.querySelectorAll('img');
    expect(images.length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole('button', { name: 'Gerenciar perfis' }));
    await waitFor(() => expect(screen.getByText('Adicionar perfil')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Adicionar perfil'));
    expect(await screen.findByRole('dialog')).toHaveTextContent('Adicionar perfil');
    expect(screen.getByLabelText('Perfil infantil')).toBeInTheDocument();
    expect(screen.getByLabelText('Classificação máxima')).toBeInTheDocument();
  });
});
