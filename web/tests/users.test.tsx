import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { UsersSettingsSection } from '@/features/users/components/UsersSettingsSection';
import { ThemeProvider } from '@mui/material';
import { createAppTheme } from '@/theme/theme';
import '@/i18n';

const mocks = vi.hoisted(() => ({
  isAdmin: true,
  list: vi.fn(),
  libraries: vi.fn(),
}));

vi.mock('@/app/AuthProvider', () => ({ useAuth: () => ({ user: { id: 'admin', isAdmin: mocks.isAdmin } }) }));
vi.mock('@/app/ToastProvider', () => ({ useToast: () => ({ notify: vi.fn() }) }));
vi.mock('@/api', () => ({
  api: {
    users: {
      list: mocks.list,
      create: vi.fn(), update: vi.fn(), enable: vi.fn(), disable: vi.fn(),
      resetPassword: vi.fn(), libraryAccess: vi.fn(), updateLibraryAccess: vi.fn(),
    },
    libraries: { list: mocks.libraries },
  },
}));

function renderSection() {
  return render(<ThemeProvider theme={createAppTheme('light')}><QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><UsersSettingsSection /></QueryClientProvider></ThemeProvider>);
}

describe('gestão de usuários', () => {
  it('lista usuários com status, perfil e último acesso', async () => {
    mocks.isAdmin = true;
    mocks.list.mockResolvedValue({ items: [{ id: '1', username: 'admin', displayName: 'Administrador', isActive: true, isAdmin: true, roles: ['Administrator'], createdAt: '2026-01-01', updatedAt: '2026-01-01', lastLoginAt: '2026-01-02', libraryAccessCount: 0 }], page: 1, pageSize: 12, total: 1, totalPages: 1 });
    renderSection();
    expect((await screen.findAllByText('Administrador')).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText('Ativo')).toBeInTheDocument();
    expect(screen.getByText('Administrador', { selector: '.MuiChip-label' })).toBeInTheDocument();
    expect(screen.getByText(/Todas as bibliotecas/)).toBeInTheDocument();
  });

  it('abre criação e apresenta seleção de bibliotecas', async () => {
    mocks.list.mockResolvedValue({ items: [], page: 1, pageSize: 12, total: 0, totalPages: 0 });
    mocks.libraries.mockResolvedValue({ items: [{ id: 'lib', name: 'Filmes privados', type: 'MOVIE', privacy: 'PRIVATE' }] });
    renderSection();
    await userEvent.click((await screen.findAllByRole('button', { name: 'Adicionar usuário' }))[0]!);
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    expect(screen.getByLabelText('Nome de usuário')).toBeInTheDocument();
    expect(await screen.findByText('Filmes privados')).toBeInTheDocument();
    expect(screen.getByText(/Privada/)).toBeInTheDocument();
  });
});
