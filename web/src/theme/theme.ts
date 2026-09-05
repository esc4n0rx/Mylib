import { createTheme, type Theme } from '@mui/material';
import { buildPalette } from './palette';
import { buildComponents } from './components';
import { typography } from './typography';
import {
  appShadow,
  darkTokens,
  lightTokens,
  midnightTokens,
  oceanTokens,
  roseTokens,
  sunsetTokens,
  violetTokens,
  radius,
  spacing,
  type SurfaceTokens,
} from './tokens';

declare module '@mui/material/styles' {
  interface Theme {
    tokens: SurfaceTokens;
    appShadow: string;
    ds: { radius: typeof radius; spacing: typeof spacing };
  }
  interface ThemeOptions {
    tokens?: SurfaceTokens;
    appShadow?: string;
    ds?: { radius: typeof radius; spacing: typeof spacing };
  }
}

export type ThemeMode = 'system' | 'light' | 'dark' | 'ocean' | 'violet' | 'sunset' | 'rose' | 'midnight';
export type ResolvedTheme = Exclude<ThemeMode, 'system'>;

export function createAppTheme(resolved: ResolvedTheme): Theme {
  const tokensByTheme: Record<ResolvedTheme, SurfaceTokens> = {
    light: lightTokens,
    dark: darkTokens,
    ocean: oceanTokens,
    violet: violetTokens,
    sunset: sunsetTokens,
    rose: roseTokens,
    midnight: midnightTokens,
  };
  const tokens = tokensByTheme[resolved];
  const paletteMode = resolved === 'dark' || resolved === 'midnight' ? 'dark' : 'light';
  return createTheme({
    palette: buildPalette(tokens, paletteMode),
    typography,
    shape: { borderRadius: radius.button },
    spacing: 4,
    components: buildComponents(tokens),
    tokens,
    appShadow,
    ds: { radius, spacing },
  });
}
