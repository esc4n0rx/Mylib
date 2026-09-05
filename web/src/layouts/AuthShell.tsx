import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { BrandMark } from '@/components/BrandMark';

/** Two-pane login layout: 45% branding, 55% form (branding hidden on mobile). */
export function AuthShell({ children }: { children: ReactNode }) {
  const { t } = useTranslation('auth');
  return (
    <Box
      sx={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: (th) => th.tokens.externalBackground,
        p: { xs: 2, md: 6 },
      }}
    >
      <Box
        sx={{
          width: '100%',
          maxWidth: 1120,
          minHeight: 640,
          display: 'flex',
          borderRadius: (th) => `${th.ds.radius.card + 4}px`,
          overflow: 'hidden',
          boxShadow: (th) => th.appShadow,
          backgroundColor: (th) => th.tokens.surface,
        }}
      >
        <Box
          sx={{
            display: { xs: 'none', md: 'flex' },
            width: '45%',
            position: 'relative',
            flexDirection: 'column',
            justifyContent: 'center',
            p: 8,
            backgroundColor: (th) => th.tokens.surfaceContainerLow,
            overflow: 'hidden',
          }}
        >
          <PosterGrid />
          <Stack spacing={1} sx={{ position: 'relative', zIndex: 1 }}>
            <BrandMark size={48} />
            <Typography variant="h1" sx={{ mt: 3 }}>
              {t('brandTagline')}
            </Typography>
            <Typography variant="body1" color="text.secondary" sx={{ maxWidth: 320 }}>
              {t('brandSubtitle')}
            </Typography>
          </Stack>
        </Box>
        <Box
          sx={{
            width: { xs: '100%', md: '55%' },
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            p: { xs: 4, sm: 8 },
          }}
        >
          <Box sx={{ width: '100%', maxWidth: 360 }}>{children}</Box>
        </Box>
      </Box>
    </Box>
  );
}

function PosterGrid() {
  return (
    <Box
      aria-hidden
      sx={{
        position: 'absolute',
        inset: 0,
        opacity: 0.18,
        filter: 'blur(2px)',
        display: 'flex',
        flexWrap: 'wrap',
        gap: 2,
        p: 4,
        maskImage: 'linear-gradient(to right, black 45%, transparent 100%)',
        WebkitMaskImage: 'linear-gradient(to right, black 45%, transparent 100%)',
      }}
    >
      {Array.from({ length: 12 }).map((_, i) => (
        <Box
          key={i}
          sx={{
            width: 120,
            height: 180,
            borderRadius: '8px',
            backgroundColor: (t) =>
              [t.tokens.primaryContainer, t.tokens.surfaceContainerHighest, t.tokens.outlineVariant][
                i % 3
              ],
          }}
        />
      ))}
    </Box>
  );
}
