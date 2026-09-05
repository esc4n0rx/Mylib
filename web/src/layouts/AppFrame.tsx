import { Box } from '@mui/material';
import type { ReactNode } from 'react';

/**
 * The "floating" application surface: 24px margin over the external background
 * with a 22px radius and the ambient app shadow on desktop; edge-to-edge on
 * mobile (external frame removed).
 */
export function AppFrame({ children }: { children: ReactNode }) {
  return (
    <Box
      sx={{
        minHeight: '100vh',
        backgroundColor: (t) => t.tokens.externalBackground,
        p: { xs: 0, md: `${24}px` },
      }}
    >
      <Box
        sx={{
          backgroundColor: (t) => t.tokens.surface,
          borderRadius: (t) => ({ xs: 0, md: `${t.ds.radius.appContainer}px` }),
          boxShadow: (t) => ({ xs: 'none', md: t.appShadow }),
          overflow: 'hidden',
          minHeight: { xs: '100vh', md: 'calc(100vh - 48px)' },
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {children}
      </Box>
    </Box>
  );
}
