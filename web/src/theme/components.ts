import type { Components, Theme } from '@mui/material';
import type { SurfaceTokens } from './tokens';
import { modalShadow, radius } from './tokens';

export function buildComponents(t: SurfaceTokens): Components<Theme> {
  return {
    MuiCssBaseline: {
      styleOverrides: {
        body: { backgroundColor: t.surface, color: t.onSurface },
        '::selection': { background: t.primaryContainer, color: t.onPrimaryContainer },
      },
    },
    MuiPaper: {
      styleOverrides: {
        root: { backgroundImage: 'none' },
        outlined: { borderColor: t.outlineVariant },
      },
    },
    MuiCard: {
      defaultProps: { variant: 'outlined', elevation: 0 },
      styleOverrides: {
        root: {
          borderRadius: radius.card,
          borderColor: t.outlineVariant,
          backgroundColor: t.surfaceContainerLowest,
        },
      },
    },
    MuiButton: {
      defaultProps: { disableElevation: true },
      styleOverrides: {
        root: { borderRadius: radius.button, minHeight: 38, paddingInline: 16 },
        containedPrimary: {
          backgroundColor: t.primaryContainer,
          color: t.onPrimaryContainer,
          '&:hover': { backgroundColor: t.primaryContainer, filter: 'brightness(0.95)' },
        },
        outlined: { borderColor: t.outlineVariant, color: t.onSurface },
      },
    },
    MuiOutlinedInput: {
      styleOverrides: {
        root: {
          borderRadius: radius.input,
          backgroundColor: t.surfaceContainerLowest,
          '& .MuiOutlinedInput-notchedOutline': { borderColor: t.outlineVariant },
          '&.Mui-focused .MuiOutlinedInput-notchedOutline': {
            borderColor: t.primary,
            borderWidth: 1,
            boxShadow: `0 0 0 2px ${t.primary}33`,
          },
        },
        input: { height: 24, padding: '8px 12px' },
      },
    },
    MuiTextField: { defaultProps: { size: 'small', fullWidth: true } },
    MuiInputLabel: { styleOverrides: { root: { fontSize: 14, fontWeight: 500 } } },
    MuiDialog: {
      styleOverrides: {
        paper: {
          borderRadius: radius.card,
          boxShadow: modalShadow,
          backgroundColor: t.surfaceContainerLowest,
          backgroundImage: 'none',
        },
      },
    },
    MuiBackdrop: {
      styleOverrides: {
        root: { backgroundColor: 'rgba(0,0,0,0.32)', backdropFilter: 'blur(6px)' },
      },
    },
    MuiChip: {
      styleOverrides: {
        root: { borderRadius: radius.chip, fontWeight: 500 },
        outlined: { borderColor: t.outlineVariant },
      },
    },
    MuiTooltip: {
      styleOverrides: {
        tooltip: {
          backgroundColor: t.onSurface,
          color: t.surface,
          fontSize: 11,
          borderRadius: 6,
        },
      },
    },
    MuiDrawer: {
      styleOverrides: {
        paper: { backgroundColor: t.surfaceContainerLow, borderColor: t.outlineVariant },
      },
    },
    MuiLinearProgress: {
      styleOverrides: {
        root: { height: 8, borderRadius: 9999, backgroundColor: t.surfaceContainerHighest },
        bar: { borderRadius: 9999 },
      },
    },
    MuiSkeleton: {
      styleOverrides: { root: { backgroundColor: t.surfaceContainerHigh } },
    },
    MuiStepLabel: {
      styleOverrides: { label: { fontSize: 13, fontWeight: 500 } },
    },
    MuiSnackbarContent: {
      styleOverrides: {
        root: { backgroundColor: t.onSurface, color: t.surface, borderRadius: radius.button },
      },
    },
  };
}
