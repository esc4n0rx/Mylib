import { Box, Button } from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { PageHeader } from '@/components/PageHeader';
import { QueryBoundary } from '@/components/QueryBoundary';
import { EmptyState } from '@/components/states/EmptyState';
import { useLibraries } from '../hooks';
import { LibraryCard } from '../components/LibraryCard';

export default function LibrariesPage() {
  const { t } = useTranslation('libraries');
  const navigate = useNavigate();
  const query = useLibraries();

  return (
    <Box>
      <PageHeader
        title={t('pageTitle')}
        actions={
          <Button
            variant="contained"
            startIcon={<AddIcon />}
            onClick={() => navigate('/libraries/new')}
          >
            {t('addLibrary')}
          </Button>
        }
      />
      <QueryBoundary query={query}>
        {(data) =>
          data.items.length === 0 ? (
            <EmptyState
              title={t('empty.title')}
              body={t('empty.body')}
              action={{ label: t('addLibrary'), onClick: () => navigate('/libraries/new') }}
            />
          ) : (
            <Box
              sx={{
                display: 'grid',
                gap: 2,
                gridTemplateColumns: { xs: '1fr', md: '1fr 1fr', xl: '1fr 1fr 1fr' },
              }}
            >
              {data.items.map((library) => (
                <LibraryCard key={library.id} library={library} />
              ))}
            </Box>
          )
        }
      </QueryBoundary>
    </Box>
  );
}
