import type { ReactNode } from 'react';
import { Navigate, Outlet } from 'react-router-dom';
import { useSetupStatus } from '@/app/hooks';
import { useAuth } from '@/app/AuthProvider';
import { StartupScreen } from '@/components/StartupScreen';
import { ApiError } from '@/api';
import { ErrorState } from '@/components/states/ErrorState';
import { useTranslation } from 'react-i18next';

function ServerUnreachable({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation('common');
  return (
    <ErrorState
      title={t('states.serverOfflineTitle')}
      body={t('states.serverOfflineBody')}
      onRetry={onRetry}
      fullHeight
    />
  );
}

/** Blocks the app until setup is complete; sends unconfigured servers to /setup. */
export function SetupGuard({ children }: { children?: ReactNode }) {
  const status = useSetupStatus();

  if (status.isLoading) return <StartupScreen />;
  if (status.isError) {
    if (status.error instanceof ApiError && status.error.kind === 'network') {
      return <ServerUnreachable onRetry={() => void status.refetch()} />;
    }
    return <ServerUnreachable onRetry={() => void status.refetch()} />;
  }
  if (status.data?.setupRequired) return <Navigate to="/setup" replace />;
  return <>{children ?? <Outlet />}</>;
}

/** Requires an authenticated session. */
export function AuthGuard() {
  const { isAuthenticated, isLoading } = useAuth();
  if (isLoading) return <StartupScreen />;
  if (!isAuthenticated) return <Navigate to="/login" replace />;
  return <Outlet />;
}

/** Media routes require a server-validated profile session in the JWT. */
export function ProfileGuard() {
  const { user, isLoading } = useAuth();
  if (isLoading) return <StartupScreen />;
  if (!user?.profileId) return <Navigate to="/profiles" replace />;
  return <Outlet />;
}

/** Keeps authenticated users away from /login. */
export function GuestGuard() {
  const { isAuthenticated } = useAuth();
  if (isAuthenticated) return <Navigate to="/home" replace />;
  return <Outlet />;
}

/** Ensures /setup is only reachable while the server is unconfigured. */
export function SetupOnlyGuard() {
  const status = useSetupStatus();
  if (status.isLoading) return <StartupScreen />;
  if (status.data && !status.data.setupRequired) return <Navigate to="/home" replace />;
  return <Outlet />;
}
