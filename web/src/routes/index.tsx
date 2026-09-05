import { lazy, Suspense, type ReactNode } from 'react';
import { createBrowserRouter, Navigate } from 'react-router-dom';
import { AppShell } from '@/layouts/AppShell';
import { AuthGuard, GuestGuard, ProfileGuard, SetupGuard, SetupOnlyGuard } from './guards';
import { LoadingState } from '@/components/states/LoadingState';

const SetupPage = lazy(() => import('@/features/setup/pages/SetupPage'));
const LoginPage = lazy(() => import('@/features/auth/pages/LoginPage'));
const HomePage = lazy(() => import('@/features/home/pages/HomePage'));
const LibrariesPage = lazy(() => import('@/features/libraries/pages/LibrariesPage'));
const CreateLibraryPage = lazy(() => import('@/features/libraries/pages/CreateLibraryPage'));
const LibraryDetailPage = lazy(() => import('@/features/libraries/pages/LibraryDetailPage'));
const UsersPage = lazy(() => import('@/features/users/pages/UsersPage'));
const SettingsPage = lazy(() => import('@/features/settings/pages/SettingsPage'));
const MoviesPage = lazy(() => import('@/features/media/pages/MoviesPage'));
const TvPage = lazy(() => import('@/features/media/pages/TvPage'));
const MovieDetailsPage = lazy(() => import('@/features/media/pages/MovieDetailsPage'));
const TvDetailsPage = lazy(() => import('@/features/media/pages/TvDetailsPage'));
const FavoritesPage = lazy(() => import('@/features/media/pages/FavoritesPage'));
const NotFoundPage = lazy(() => import('@/features/misc/NotFoundPage'));
const PlayerPage = lazy(() => import('@/features/playback/pages/PlayerPage'));
const ActivityPage = lazy(() => import('@/features/activity/pages/ActivityPage'));
const ProfilesPage = lazy(() => import('@/features/profiles/pages/ProfilesPage'));

function Lazy({ children }: { children: ReactNode }) {
  return <Suspense fallback={<LoadingState rows={2} />}>{children}</Suspense>;
}

export const router = createBrowserRouter([
  { path: '/', element: <Navigate to="/home" replace /> },
  {
    element: <SetupOnlyGuard />,
    children: [{ path: '/setup', element: <Lazy><SetupPage /></Lazy> }],
  },
  {
    element: <SetupGuard />,
    children: [
      {
        element: <GuestGuard />,
        children: [{ path: '/login', element: <Lazy><LoginPage /></Lazy> }],
      },
      {
        element: <AuthGuard />,
        children: [
          { path: '/profiles', element: <Lazy><ProfilesPage /></Lazy> },
          {
            element: <ProfileGuard />,
            children: [
              { path: '/player/:sessionId', element: <Lazy><PlayerPage /></Lazy> },
              {
                element: <AppShell />,
                children: [
              { path: '/home', element: <Lazy><HomePage /></Lazy> },
              { path: '/movies', element: <Lazy><MoviesPage /></Lazy> },
              { path: '/tv', element: <Lazy><TvPage /></Lazy> },
              { path: '/media/:id', element: <Lazy><MovieDetailsPage /></Lazy> },
              { path: '/tv/:id', element: <Lazy><TvDetailsPage /></Lazy> },
              { path: '/favorites', element: <Lazy><FavoritesPage /></Lazy> },
              { path: '/libraries', element: <Lazy><LibrariesPage /></Lazy> },
              { path: '/libraries/new', element: <Lazy><CreateLibraryPage /></Lazy> },
              { path: '/libraries/:id', element: <Lazy><LibraryDetailPage /></Lazy> },
              { path: '/users', element: <Lazy><UsersPage /></Lazy> },
              { path: '/settings', element: <Lazy><SettingsPage /></Lazy> },
              { path: '/settings/users', element: <Lazy><SettingsPage /></Lazy> },
              { path: '/activity', element: <Lazy><ActivityPage /></Lazy> },
                ],
              },
            ],
          },
        ],
      },
    ],
  },
  { path: '*', element: <Lazy><NotFoundPage /></Lazy> },
]);
