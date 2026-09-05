import type { ReactNode } from 'react';
import type { UseQueryResult } from '@tanstack/react-query';
import { ApiError } from '@/api';
import { useTranslation } from 'react-i18next';
import { LoadingState } from './states/LoadingState';
import { ErrorState } from './states/ErrorState';

interface QueryBoundaryProps<T> {
  query: UseQueryResult<T>;
  children: (data: T) => ReactNode;
  skeletonRows?: number;
}

/** Standard loading/error/success handling for a single query-backed view. */
export function QueryBoundary<T>({ query, children, skeletonRows }: QueryBoundaryProps<T>) {
  const { t } = useTranslation('common');
  if (query.isLoading) return <LoadingState rows={skeletonRows} />;
  if (query.isError) {
    const err = query.error;
    const isNetwork = err instanceof ApiError && err.kind === 'network';
    return (
      <ErrorState
        title={isNetwork ? t('states.serverOfflineTitle') : undefined}
        body={
          isNetwork
            ? t('states.serverOfflineBody')
            : err instanceof ApiError
              ? err.localizedMessage
              : undefined
        }
        onRetry={() => void query.refetch()}
      />
    );
  }
  if (query.data === undefined) return null;
  return <>{children(query.data)}</>;
}
