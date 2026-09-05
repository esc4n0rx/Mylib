export type PasswordStrength = 0 | 1 | 2 | 3 | 4;

/** Lightweight heuristic for UI feedback only; the backend enforces real rules. */
export function passwordStrength(value: string): PasswordStrength {
  if (!value) return 0;
  let score = 0;
  if (value.length >= 8) score++;
  if (value.length >= 12) score++;
  if (/[a-z]/.test(value) && /[A-Z]/.test(value)) score++;
  if (/\d/.test(value) && /[^A-Za-z0-9]/.test(value)) score++;
  return Math.min(score, 4) as PasswordStrength;
}

export const STRENGTH_KEYS = ['weak', 'weak', 'fair', 'good', 'strong'] as const;
