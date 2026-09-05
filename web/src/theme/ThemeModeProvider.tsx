import { useEffect, useMemo, type ReactNode } from 'react';
import { CssBaseline, ThemeProvider } from '@mui/material';
import { useUiStore } from '@/stores/uiStore';
import { createAppTheme, type ResolvedTheme, type ThemeMode } from './theme';

const DARK_QUERY = '(prefers-color-scheme: dark)';

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' && window.matchMedia(DARK_QUERY).matches;
}

export function resolveTheme(
  mode: ThemeMode,
  prefersDark: boolean,
): ResolvedTheme {
  if (mode === 'system') return prefersDark ? 'dark' : 'light';
  return mode;
}

/**
 * Owns theme resolution: reads the persisted preference, follows the OS when the
 * preference is "system", keeps `color-scheme` in sync, and feeds the MUI theme.
 */
export function ThemeModeProvider({ children }: { children: ReactNode }) {
  const themeMode = useUiStore((s) => s.themeMode);
  const resolvedTheme = useUiStore((s) => s.resolvedTheme);
  const setResolvedTheme = useUiStore((s) => s.setResolvedTheme);

  useEffect(() => {
    const media = window.matchMedia(DARK_QUERY);
    const apply = () => setResolvedTheme(resolveTheme(themeMode, media.matches));
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [themeMode, setResolvedTheme]);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme;
    root.style.colorScheme = resolvedTheme === 'dark' || resolvedTheme === 'midnight' ? 'dark' : 'light';
  }, [resolvedTheme]);

  const effective = themeMode === 'system'
    ? resolveTheme('system', systemPrefersDark())
    : themeMode;
  const theme = useMemo(() => createAppTheme(effective), [effective]);

  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      {children}
    </ThemeProvider>
  );
}
