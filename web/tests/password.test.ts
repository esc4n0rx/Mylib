import { describe, expect, it } from 'vitest';
import { passwordStrength } from '@/utils/password';

describe('passwordStrength', () => {
  it('rates empty and trivial passwords as weak', () => {
    expect(passwordStrength('')).toBe(0);
    expect(passwordStrength('abc')).toBe(0);
  });

  it('rewards length and character variety', () => {
    expect(passwordStrength('abcdefgh')).toBeGreaterThanOrEqual(1);
    expect(passwordStrength('Abcdef1!ghij')).toBe(4);
  });
});
