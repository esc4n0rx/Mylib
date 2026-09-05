import { Box } from '@mui/material';
import { useTranslation } from 'react-i18next';
import { PageHeader } from '@/components/PageHeader';
import { EmptyState } from '@/components/states/EmptyState';

export default function UsersPage() {
  const { t } = useTranslation('common');
  return (
    <Box>
      <PageHeader title={t('nav.users')} />
      <EmptyState title={t('states.comingSoon')} body={t('topbar.searchComingSoon')} />
    </Box>
  );
}
