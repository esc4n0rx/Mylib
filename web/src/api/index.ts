import { request } from './client';
import { session } from './session';
import { ApiError, kindForStatus } from './errors';
import type {
  CreateLibraryRequest,
  CurrentUser,
  DatabaseConfig,
  DatabaseTestResponse,
  HealthResponse,
  Library,
  LibraryPath,
  LibraryStats,
  AutoSyncConfig,
  LoginResponse,
  Paginated,
  PathValidationResult,
  ScanJob,
  ServerInfo,
  SetupRequest,
  SetupStatus,
  StartScanResponse,
  UnmatchedItem,
  MediaCard,
  MediaDetails,
  Genre,
  Season,
  Episode,
  PlaybackStartRequest,
  PlaybackStartResponse,
  ContinueWatchingItem,
  PlaybackProgressRequest,
  IdentificationCandidate,
  ManualIdentificationResult,
  ManagedUser,
  UserLibraryAccess,
  CreateManagedUserRequest,
  UpdateManagedUserRequest,
  ServerHealth, ServerMetrics, StorageOverview, PlaybackSession, TranscodesResponse, ServerJob, ActivityEvent, ServerAlert, PlaybackCapabilities, HomeRecommendations, ForYouRecommendations,
  Profile, AvatarItem, ProfileLibraryAccess,
  RemoteSource, RemoteProviderType, M3uEntry, M3uPreview, RemoteSelection, RemoteSyncOutcome,
  GoogleDriveConnection, GoogleDriveItem,
} from './types';

export const api = {
  health: () => request<HealthResponse>('/health', { raw: true }),

  setup: {
    status: () => request<SetupStatus>('/setup/status'),
    submit: (body: SetupRequest) =>
      request<{ configured: boolean; serverId: string }>('/setup', { body }),
    testDatabase: (body: DatabaseConfig) =>
      request<DatabaseTestResponse>('/setup/database/test', { body }),
  },

  auth: {
    login: (username: string, password: string) =>
      request<LoginResponse>('/auth/login', { body: { username, password } }),
    me: () => request<CurrentUser>('/auth/me'),
  },

  profiles: {
    list: (userId?: string) => request<{ items: Profile[] }>('/profiles', { query: { userId } }),
    current: () => request<Profile>('/profiles/current'),
    create: (body: { name: string; avatarId?: string; isKids: boolean; maxAgeRating: number; userId?: string }) =>
      request<Profile>('/profiles', { body }),
    update: (id: string, body: Partial<Pick<Profile, 'name' | 'avatarId' | 'isKids' | 'maxAgeRating' | 'isActive'>>) =>
      request<Profile>(`/profiles/${id}`, { method: 'PATCH', body }),
    disable: (id: string) => request<void>(`/profiles/${id}`, { method: 'DELETE' }),
    select: (id: string, pin?: string) => request<{ accessToken: string; profile: Profile }>(`/profiles/${id}/select`, { method: 'POST', ...(pin ? { body: { pin } } : {}) }),
    setPin: (id: string, pin: string) => request<void>(`/profiles/${id}/pin`, { method: 'PUT', body: { pin } }),
    removePin: (id: string) => request<void>(`/profiles/${id}/pin`, { method: 'DELETE' }),
    libraryAccess: (id: string) => request<{ libraries: ProfileLibraryAccess[] }>(`/profiles/${id}/library-access`),
    updateLibraryAccess: (id: string, libraryIds: string[]) => request<{ libraries: ProfileLibraryAccess[] }>(`/profiles/${id}/library-access`, { method: 'PUT', body: { libraryIds } }),
  },

  avatars: {
    categories: () => request<Array<{ id: AvatarItem['category']; name: string }>>('/avatars/categories'),
    list: (query: { category?: AvatarItem['category']; page?: number; pageSize?: number }) => request<Paginated<AvatarItem>>('/avatars', { query }),
  },

  users: {
    list: (query?: Record<string, string | number | boolean | undefined>) =>
      request<Paginated<ManagedUser>>('/users', { query }),
    get: (id: string) => request<ManagedUser>(`/users/${id}`),
    create: (body: CreateManagedUserRequest) => request<ManagedUser>('/users', { body }),
    update: (id: string, body: UpdateManagedUserRequest) =>
      request<ManagedUser>(`/users/${id}`, { method: 'PATCH', body }),
    enable: (id: string) => request<void>(`/users/${id}/enable`, { method: 'POST' }),
    disable: (id: string) => request<void>(`/users/${id}/disable`, { method: 'POST' }),
    resetPassword: (id: string, newPassword: string) =>
      request<void>(`/users/${id}/password`, { method: 'PUT', body: { newPassword } }),
    libraryAccess: (id: string) =>
      request<{ libraries: UserLibraryAccess[] }>(`/users/${id}/library-access`),
    updateLibraryAccess: (id: string, libraries: UserLibraryAccess[]) =>
      request<{ libraries: UserLibraryAccess[] }>(`/users/${id}/library-access`, {
        method: 'PUT', body: { libraries },
      }),
  },

  server: {
    info: () => request<ServerInfo>('/server'),
    update: (name: string) =>
      request<ServerInfo>('/server', { method: 'PATCH', body: { name } }),
    health: () => request<ServerHealth>('/server/health'),
    metrics: () => request<ServerMetrics>('/server/metrics'),
    storage: () => request<StorageOverview>('/server/storage'),
    alerts: () => request<{ items: ServerAlert[] }>('/server/alerts'),
  },

  libraries: {
    list: () => request<Paginated<Library>>('/libraries'),
    get: (id: string) => request<Library>(`/libraries/${id}`),
    stats: (id: string) => request<LibraryStats>(`/libraries/${id}/stats`),
    pathStatuses: (id: string) =>
      request<{ items: LibraryPath[] }>(`/libraries/${id}/paths/status`),
    update: (id: string, body: { autoSync: AutoSyncConfig }) =>
      request<Library>(`/libraries/${id}`, { method: 'PATCH', body }),
    create: (body: CreateLibraryRequest) => request<Library>('/libraries', { body }),
    validatePath: (path: string) =>
      request<PathValidationResult>('/libraries/paths/validate', { body: { path } }),
  },

  scans: {
    start: (libraryId: string, scanType?: 'FULL' | 'INCREMENTAL') =>
      request<StartScanResponse>(`/libraries/${libraryId}/scan`, {
        body: scanType ? { scanType } : {},
      }),
    get: (libraryId: string, scanId: string) =>
      request<ScanJob>(`/libraries/${libraryId}/scans/${scanId}`),
    list: (libraryId: string) =>
      request<Paginated<ScanJob>>(`/libraries/${libraryId}/scans`, {
        query: { pageSize: 20 },
      }),
    cancel: (libraryId: string, scanId: string) =>
      request<{ cancellationRequested: boolean }>(
        `/libraries/${libraryId}/scans/${scanId}/cancel`,
        { method: 'POST' },
      ),
  },

  settings: {
    tmdbStatus: () => request<{ configured: boolean; available: boolean }>(
      '/settings/metadata/tmdb/status',
    ),
    updateTmdbKey: (apiKey: string | null) =>
      request<{ configured: boolean; available: boolean }>('/settings/metadata/tmdb', {
        method: 'PUT',
        body: { apiKey },
      }),
  },

  media: {
    unmatched: (libraryId: string) =>
      request<Paginated<UnmatchedItem>>(`/libraries/${libraryId}/unmatched`),
    identifySearch: (params: {
      libraryId: string;
      mediaFileId: string;
      query: string;
      year?: number;
    }) =>
      request<{ items: IdentificationCandidate[] }>('/media/identify/search', {
        query: params,
      }),
    identifyManual: (mediaFileId: string, providerId: number) =>
      request<ManualIdentificationResult>('/media/identify', {
        body: { mediaFileId, provider: 'TMDB', providerId },
      }),
    reidentify: (mediaFileId: string) =>
      request<{ mediaFileId: string; status: string }>(
        `/media/${mediaFileId}/reidentify`,
        { method: 'POST' },
      ),
    recent: (query?: Record<string, string | number | boolean | undefined>) =>
      request<Paginated<MediaCard>>('/media/recent', { query }),
    movies: (query?: Record<string, string | number | boolean | undefined>) =>
      request<Paginated<MediaCard>>('/media/movies', { query }),
    tvShows: (query?: Record<string, string | number | boolean | undefined>) =>
      request<Paginated<MediaCard>>('/media/tv-shows', { query }),
    movieGenres: () => request<Genre[]>('/media/movies/genres'),
    tvGenres: () => request<Genre[]>('/media/tv-shows/genres'),
    byGenre: (
      type: 'MOVIE' | 'TV_SHOW',
      genreId: string,
      query?: Record<string, string | number | boolean | undefined>,
    ) =>
      request<Paginated<MediaCard>>(
        `/media/${type === 'MOVIE' ? 'movies' : 'tv-shows'}/by-genre/${genreId}`,
        { query },
      ),
    details: (id: string) => request<MediaDetails>(`/media/items/${id}`),
    tvDetails: (id: string) => request<MediaDetails>(`/media/tv-shows/${id}`),
    seasons: (id: string) => request<Season[]>(`/media/tv-shows/${id}/seasons`),
    episodes: (id: string, season: number) =>
      request<Paginated<Episode>>(`/media/tv-shows/${id}/seasons/${season}/episodes`),
    similar: (id: string) => request<Paginated<MediaCard>>(`/media/items/${id}/similar`),
    favorites: (query?: Record<string, string | number | boolean | undefined>) =>
      request<Paginated<MediaCard>>('/media/favorites', { query }),
    addFavorite: (id: string) =>
      request<{ id: string; isFavorite: boolean }>(`/media/items/${id}/favorite`, {
        method: 'POST',
      }),
    removeFavorite: (id: string) =>
      request<void>(`/media/items/${id}/favorite`, { method: 'DELETE' }),
    refreshMetadata: (id: string) =>
      request<unknown>(`/media/items/${id}/metadata/refresh`, { method: 'POST' }),
  },
  playback: {
    start: (body: PlaybackStartRequest) =>
      request<PlaybackStartResponse>('/playback/start', { body }),
    progress: (sessionId: string, body: PlaybackProgressRequest) =>
      request<{ saved: boolean; completed: boolean; percentage: number; recommendationsInvalidated: boolean }>(
        `/playback/${sessionId}/progress`,
        { body },
      ),
    stop: (sessionId: string) =>
      request<{ stopped: boolean }>(`/playback/${sessionId}/stop`, { method: 'POST' }),
    continueWatching: () =>
      request<{ items: ContinueWatchingItem[] }>('/playback/continue-watching'),
    sessions: () => request<{ items: PlaybackSession[] }>('/playback/sessions'),
    transcodes: () => request<TranscodesResponse>('/playback/transcodes'),
    capabilities: () => request<PlaybackCapabilities>('/playback/capabilities'),
  },
  jobs: {
    list: (query?: Record<string, string | number | boolean | undefined>) => request<Paginated<ServerJob>>('/jobs', { query }),
  },
  activity: {
    list: (query?: Record<string, string | number | boolean | undefined>) => request<Paginated<ActivityEvent>>('/activity', { query }),
  },
  recommendations: {
    home: () => request<HomeRecommendations>('/recommendations/home'),
    forYou: (query?: { limit?: number; type?: 'MOVIE' | 'TV_SHOW' }) => request<ForYouRecommendations>('/recommendations/for-you', { query }),
    becauseYouWatched: (id: string, limit = 12) => request<{ sourceMediaId: string; title: string; items: MediaCard[] }>(`/recommendations/because-you-watched/${id}`, { query: { limit } }),
    genres: () => request<Array<{ genreId: string; name: string; score: number }>>('/recommendations/genres'),
  },
  remoteSources: {
    list: (libraryId: string) =>
      request<{ items: RemoteSource[] }>(`/libraries/${libraryId}/remote-sources`),
    create: (
      libraryId: string,
      body: {
        name: string;
        providerType: RemoteProviderType;
        config: Record<string, unknown>;
        autoSync?: { enabled: boolean; intervalMinutes?: number };
      },
    ) => request<RemoteSource>(`/libraries/${libraryId}/remote-sources`, { body }),
    get: (id: string) => request<RemoteSource>(`/remote-sources/${id}`),
    update: (
      id: string,
      body: Partial<{
        name: string;
        isActive: boolean;
        config: Record<string, unknown>;
        autoSync: { enabled: boolean; intervalMinutes?: number };
      }>,
    ) => request<RemoteSource>(`/remote-sources/${id}`, { method: 'PATCH', body }),
    remove: (id: string) => request<void>(`/remote-sources/${id}`, { method: 'DELETE' }),
    status: (id: string) => request<Record<string, unknown>>(`/remote-sources/${id}/status`),
    entries: (id: string, query?: Record<string, string | number | boolean | undefined>) =>
      request<Paginated<M3uEntry>>(`/remote-sources/${id}/entries`, { query }),
    selections: (id: string) =>
      request<{ selections: RemoteSelection[] }>(`/remote-sources/${id}/selections`),
    setSelections: (id: string, selections: RemoteSelection[]) =>
      request<{ selections: RemoteSelection[] }>(`/remote-sources/${id}/selections`, {
        method: 'PUT',
        body: { selections },
      }),
    sync: (id: string, wait = false) =>
      request<RemoteSyncOutcome | { status: string }>(`/remote-sources/${id}/sync`, {
        method: 'POST',
        query: wait ? { wait: true } : undefined,
      }),
    uploadM3u: async (file: File | Blob): Promise<{ uploadId: string; sizeBytes: number }> => {
      const response = await fetch('/api/v1/remote-sources/m3u/upload', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/octet-stream',
          ...(session.token ? { Authorization: `Bearer ${session.token}` } : {}),
        },
        body: file,
      });
      if (!response.ok) {
        const detail = await response.json().catch(() => null);
        throw new ApiError({
          status: response.status,
          code: detail?.error?.code,
          message: detail?.error?.message ?? 'Upload failed',
          kind: kindForStatus(response.status),
        });
      }
      return response.json();
    },
    previewM3u: (body: { type: 'url'; url: string } | { type: 'upload'; uploadId: string }) =>
      request<M3uPreview>('/remote-sources/m3u/preview', { body }),
  },
  googleDrive: {
    connect: () =>
      request<{ authorizationUrl: string }>('/remote-sources/google-drive/connect', {
        method: 'POST',
      }),
    connections: () =>
      request<{ items: GoogleDriveConnection[] }>('/remote-sources/google-drive/connections'),
    disconnect: (id: string) =>
      request<void>(`/remote-sources/google-drive/connections/${id}`, { method: 'DELETE' }),
    browse: (
      connectionId: string,
      query?: { folderId?: string; pageToken?: string; pageSize?: number },
    ) =>
      request<{ items: GoogleDriveItem[]; nextPageToken?: string }>(
        `/remote-sources/google-drive/${connectionId}/browse`,
        { query },
      ),
  },
};

export { session };
export * from './types';
export { ApiError } from './errors';
