import { describe, expect, it } from 'vitest';
import i18n, { DEFAULT_LANGUAGE, FALLBACK_LANGUAGE } from '@/i18n';

describe('i18n', () => {
  it('defaults to pt-BR', () => {
    expect(DEFAULT_LANGUAGE).toBe('pt-BR');
    expect(FALLBACK_LANGUAGE).toBe('pt-BR');
    expect(i18n.language).toBe('pt-BR');
  });

  it('serves product copy in Portuguese', () => {
    expect(i18n.t('auth:signIn')).toBe('Entrar');
    expect(i18n.t('common:nav.home')).toBe('Início');
    expect(i18n.t('setup:steps.database')).toBe('Banco de dados');
    expect(i18n.t('profiles:whoIsWatching')).toBe('Quem está assistindo?');
    expect(i18n.t('profiles:switchProfile')).toBe('Trocar perfil');
  });
});
