import type { PaletteOptions } from '@mui/material';
import type { SurfaceTokens } from './tokens';

export function buildPalette(t: SurfaceTokens, mode: 'light' | 'dark'): PaletteOptions {
  return {
    mode,
    primary: {
      main: t.primary,
      contrastText: t.onPrimary,
      light: t.primaryContainer,
      dark: t.onPrimaryContainer,
    },
    error: { main: t.error, contrastText: t.onError },
    background: {
      default: t.surface,
      paper: t.surfaceContainerLowest,
    },
    text: {
      primary: t.onSurface,
      secondary: t.onSurfaceVariant,
    },
    divider: t.outlineVariant,
  };
}
