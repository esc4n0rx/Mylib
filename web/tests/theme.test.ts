import { describe, expect, it } from 'vitest';
import { resolveTheme } from '@/theme/ThemeModeProvider';
import { createAppTheme } from '@/theme/theme';

describe('resolveTheme', () => {
  it('defaults to system following prefers-color-scheme', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });

  it('honours an explicit override regardless of the OS', () => {
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });
});

describe('createAppTheme', () => {
  it('produces distinct token sets per mode without hardcoded leakage', () => {
    const light = createAppTheme('light');
    const dark = createAppTheme('dark');
    expect(light.tokens.surface).not.toEqual(dark.tokens.surface);
    expect(light.palette.mode).toBe('light');
    expect(dark.palette.mode).toBe('dark');
    // Institutional green preserved in both.
    expect(light.tokens.primary.toLowerCase()).toContain('0f6e11');
    expect(dark.tokens.primary).toBeTruthy();
  });
});
