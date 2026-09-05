import { Box, Stack, Step, StepLabel, Stepper, Typography, useMediaQuery } from '@mui/material';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { BrandMark } from '@/components/BrandMark';

interface SetupShellProps {
  activeStep: number;
  steps: string[];
  children: ReactNode;
  footer: ReactNode;
  aside?: ReactNode;
}

export function SetupShell({ activeStep, steps, children, footer, aside }: SetupShellProps) {
  const { t } = useTranslation('setup');
  const compact = useMediaQuery('(max-width:900px)');

  return (
    <Box
      sx={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: { xs: 'stretch', md: 'center' },
        justifyContent: 'center',
        backgroundColor: (th) => th.tokens.externalBackground,
        p: { xs: 0, md: 3 },
      }}
    >
      <Box
        sx={{
          width: '100%',
          maxWidth: 1040,
          display: 'flex',
          flexDirection: 'column',
          borderRadius: { xs: 0, md: (th) => `${th.ds.radius.card + 4}px` },
          overflow: 'hidden',
          boxShadow: { xs: 'none', md: (th) => th.appShadow },
          backgroundColor: (th) => th.tokens.surfaceContainerLowest,
        }}
      >
        <Stack
          direction="row"
          alignItems="center"
          spacing={1.5}
          sx={{
            px: 4,
            py: 2.5,
            borderBottom: (th) => `1px solid ${th.tokens.outlineVariant}`,
            backgroundColor: (th) => th.tokens.surface,
          }}
        >
          <BrandMark />
          <Typography variant="h1">{t('wizardTitle')}</Typography>
        </Stack>

        <Box
          sx={{
            px: 4,
            py: 2,
            borderBottom: (th) => `1px solid ${th.tokens.outlineVariant}`,
            backgroundColor: (th) => th.tokens.surfaceContainerLow,
          }}
        >
          <Stepper
            activeStep={activeStep}
            alternativeLabel={!compact}
            orientation={compact ? 'vertical' : 'horizontal'}
          >
            {steps.map((label) => (
              <Step key={label}>
                <StepLabel>{label}</StepLabel>
              </Step>
            ))}
          </Stepper>
        </Box>

        <Box sx={{ display: 'flex', flex: 1, flexDirection: { xs: 'column', md: 'row' } }}>
          <Box sx={{ flex: aside ? '0 0 58%' : 1, p: { xs: 3, md: 5 }, overflowY: 'auto' }}>
            {children}
          </Box>
          {aside ? (
            <Box
              sx={{
                flex: '0 0 42%',
                p: { xs: 3, md: 5 },
                borderLeft: { md: (th) => `1px solid ${th.tokens.outlineVariant}` },
                backgroundColor: (th) => th.tokens.surfaceContainerLow,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              {aside}
            </Box>
          ) : null}
        </Box>

        <Stack
          direction="row"
          justifyContent="space-between"
          alignItems="center"
          sx={{
            px: 3,
            py: 2,
            borderTop: (th) => `1px solid ${th.tokens.outlineVariant}`,
            backgroundColor: (th) => th.tokens.surface,
          }}
        >
          {footer}
        </Stack>
      </Box>
    </Box>
  );
}
