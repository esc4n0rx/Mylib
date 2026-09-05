import { Box, Button, Stack, Typography } from '@mui/material';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { session } from '@/api';

export default function NotFoundPage() {
  const { t } = useTranslation('common');
  const navigate = useNavigate();
  return (
    <Box
      sx={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: (th) => th.tokens.externalBackground,
      }}
    >
      <Stack spacing={2} alignItems="center">
        <Typography variant="h1">404</Typography>
        <Typography variant="body1" color="text.secondary">
          {t('states.notFoundTitle')}
        </Typography>
        <Button
          variant="contained"
          onClick={() => navigate(session.isAuthenticated ? '/home' : '/login')}
        >
          {t('states.backHome')}
        </Button>
      </Stack>
    </Box>
  );
}
