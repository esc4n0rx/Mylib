import { describe, expect, it } from 'vitest';
import { ApiError, kindForStatus } from '@/api/errors';
import '@/i18n';

describe('ApiError', () => {
  it('maps status codes to kinds', () => {
    expect(kindForStatus(401)).toBe('unauthorized');
    expect(kindForStatus(429)).toBe('rateLimit');
    expect(kindForStatus(503)).toBe('serverError');
  });

  it('localises known backend codes to pt-BR', () => {
    const err = new ApiError({
      status: 401,
      code: 'INVALID_CREDENTIALS',
      message: 'Invalid username or password.',
      kind: 'unauthorized',
    });
    expect(err.localizedMessage).toBe('Usuário ou senha inválidos.');
  });

  it('falls back to a generic localized message for unknown codes', () => {
    const err = new ApiError({ status: 500, message: 'boom', kind: 'serverError' });
    expect(err.localizedMessage).toBe('Erro interno do servidor.');
  });
});
