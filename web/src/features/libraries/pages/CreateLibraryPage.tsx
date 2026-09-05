import { Box, Button, Stack } from '@mui/material';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ApiError } from '@/api';
import { useToast } from '@/app/ToastProvider';
import { PageHeader } from '@/components/PageHeader';
import { LibraryForm } from '../components/LibraryForm';
import { useCreateLibrary } from '../hooks';

export default function CreateLibraryPage() {
  const { t } = useTranslation('libraries');
  const { t: tc } = useTranslation('common');
  const navigate = useNavigate();
  const { notify } = useToast();
  const createLibrary = useCreateLibrary();

  return (
    <Box sx={{ maxWidth: 760 }}>
      <PageHeader title={t('createLibrary')} />
      <LibraryForm
        formId="create-library-form"
        onSubmit={async (request) => {
          try {
            const library = await createLibrary.mutateAsync(request);
            notify(t('toast.created'), 'success');
            navigate(`/libraries/${library.id}`);
          } catch (err) {
            notify(
              err instanceof ApiError ? err.localizedMessage : t('toast.created'),
              'error',
            );
          }
        }}
      />
      <Stack direction="row" spacing={1} justifyContent="flex-end" sx={{ mt: 4 }}>
        <Button onClick={() => navigate('/libraries')}>{tc('actions.cancel')}</Button>
        <Button
          type="submit"
          form="create-library-form"
          variant="contained"
          disabled={createLibrary.isPending}
        >
          {t('createLibrary')}
        </Button>
      </Stack>
    </Box>
  );
}
