import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, session, type CurrentUser, type Profile } from '@/api';

interface AuthContextValue {
  isAuthenticated: boolean;
  user: CurrentUser | undefined;
  profile: Profile | undefined;
  isLoading: boolean;
  login: (token: string) => void;
  logout: () => void;
  selectProfile: (token: string) => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const token = useSyncExternalStore(
    session.subscribe,
    () => session.token,
    () => null,
  );
  const isAuthenticated = token !== null;

  const meQuery = useQuery({
    queryKey: ['auth', 'me', token],
    queryFn: api.auth.me,
    enabled: isAuthenticated,
    staleTime: 5 * 60_000,
    retry: false,
  });

  const login = useCallback((token: string) => {
    session.set(token);
  }, []);

  const selectProfile = useCallback((nextToken: string) => {
    queryClient.clear();
    session.set(nextToken);
  }, [queryClient]);

  const profileQuery = useQuery({
    queryKey: ['profiles', 'current', meQuery.data?.profileId],
    queryFn: api.profiles.current,
    enabled: Boolean(meQuery.data?.profileId),
    staleTime: 5 * 60_000,
    retry: false,
  });

  const logout = useCallback(() => {
    session.clear();
    queryClient.clear();
  }, [queryClient]);

  const value = useMemo<AuthContextValue>(
    () => ({
      isAuthenticated,
      user: meQuery.data,
      profile: profileQuery.data,
      isLoading: isAuthenticated && meQuery.isLoading,
      login,
      logout,
      selectProfile,
    }),
    [isAuthenticated, meQuery.data, meQuery.isLoading, profileQuery.data, login, logout, selectProfile],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}
