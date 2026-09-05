// Hand-maintained mirror of the Rust DTOs. When the backend adds an OpenAPI
// document (utoipa), replace this file with generated types.

export interface SetupStatus {
  setupRequired: boolean;
  configured: boolean;
  serverName?: string;
  databaseType?: string;
}

export interface HealthResponse {
  status: string;
  database?: string;
  databaseType?: string;
  version?: string;
}

export type DatabaseConfig =
  | { type: 'sqlite'; path?: string }
  | {
      type: 'mysql';
      host: string;
      port: number;
      database: string;
      username: string;
      password: string;
      sslMode?: 'disabled' | 'preferred' | 'required';
    };

export interface SetupRequest {
  serverName: string;
  database: DatabaseConfig;
  administrator: {
    username: string;
    password: string;
    displayName: string;
  };
  /** Optional. Ignored server-side when MYLIB_TMDB_API_KEY is already set via env var. */
  tmdbApiKey?: string;
}

export interface DatabaseTestResponse {
  success: boolean;
  databaseType: string;
  latencyMs?: number;
  error?: { code: string; message: string };
}

export interface LoginResponse {
  accessToken: string;
  tokenType: string;
  expiresIn: number;
  user: { id: string; username: string; displayName: string; isAdmin: boolean };
  profileSelectionRequired: boolean;
  selectedProfileId?: string;
}

export interface CurrentUser {
  id: string;
  username: string;
  displayName: string;
  isAdmin: boolean;
  roles: string[];
  permissions: string[];
  profileId?: string;
}

export interface Profile {
  id: string;
  userId: string;
  name: string;
  avatarId: string;
  avatarUrl: string;
  isDefault: boolean;
  isKids: boolean;
  isActive: boolean;
  pinProtected: boolean;
  maxAgeRating: 0 | 10 | 12 | 14 | 16 | 18;
  lastUsedAt?: string;
}

export interface AvatarItem {
  id: string;
  category: 'dp' | 'nf' | 'pop' | 'pp' | 'pv';
  name: string;
  url: string;
}

export interface ProfileLibraryAccess {
  libraryId: string;
  name: string;
  type: LibraryType;
  minimumAge: number;
  isAllowed: boolean;
}

export interface ManagedUser {
  id: string;
  username: string;
  displayName: string;
  email?: string;
  isActive: boolean;
  isAdmin: boolean;
  roles: string[];
  lastLoginAt?: string;
  createdAt: string;
  updatedAt: string;
  libraryAccessCount: number;
}

export interface UserLibraryAccess {
  libraryId: string;
  name?: string;
  type?: LibraryType;
  privacy?: LibraryPrivacy;
  canView: boolean;
  canPlay: boolean;
}

export interface CreateManagedUserRequest {
  username: string;
  displayName: string;
  email?: string;
  password: string;
  libraryAccess: UserLibraryAccess[];
}

export interface UpdateManagedUserRequest {
  username?: string;
  displayName?: string;
  email?: string;
  isActive?: boolean;
}

export interface ServerInfo {
  id: string;
  name: string;
  version: string;
  status: string;
  databaseType: string;
  setupCompleted: boolean;
  startedAt: string;
  uptimeSeconds: number;
}

export interface ServerHealth {
  status: 'HEALTHY' | 'DEGRADED' | 'ERROR';
  version: string; startedAt: string; uptimeSeconds: number;
  databaseType: string; databaseStatus: string;
  ffmpegAvailable: boolean; ffprobeAvailable: boolean;
  operatingSystem?: string; architecture: string; dataDirectory: string; host: string; port: number;
}
export interface MetricPoint { capturedAt: string; cpuUsagePercent: number; memoryUsagePercent: number; activePlaybackSessions: number }
export interface ServerMetrics {
  capturedAt: string; cpuUsagePercent: number; memoryUsedBytes: number; memoryTotalBytes: number; memoryUsagePercent: number;
  processMemoryBytes: number; diskTotalBytes: number; diskUsedBytes: number; diskFreeBytes: number; dataDirectorySizeBytes: number;
  databaseSizeBytes: number; logsSizeBytes: number; transcodeCacheSizeBytes: number; activePlaybackSessions: number;
  activeTranscodes: number; queuedTranscodes: number; activeScanJobs: number; queuedJobs: number; transcodeLimit: number; transcodeQueueLimit: number;
  history: MetricPoint[];
}
export interface StorageOverview {
  systemStorage: { path: string; totalBytes: number; usedBytes: number; freeBytes: number; usagePercent: number; status: string };
  dataDirectory: { sizeBytes: number };
  libraryStorage: Array<{ id: string; name: string; type: string; sizeBytes: number; fileCount: number; contentCount: number; status: string }>;
  database: { type: string; sizeBytes: number }; transcodeCache: { sizeBytes: number; maxBytes: number }; logs: { sizeBytes: number };
}
export interface PlaybackSession {
  sessionId: string; user: { id: string; username: string; displayName: string };
  media: { title: string; mediaType: string; episode?: string; seasonNumber?: number; episodeNumber?: number };
  clientName?: string; ipAddress?: string; playbackMode: PlaybackMode; quality: string; bitrate?: number; width?: number; height?: number;
  position: number; duration: number; startedAt: string; lastActivityAt: string; status: string;
}
export interface TranscodePipeline { pipelineId: string; media: string; sourceCodec: string; sourceResolution: string; targetCodec: string; targetResolution: string; qualityProfile: string; hardwareAccelerator: string; speed?: number; fps?: number; bitrate?: number; activeViewers: number; cacheHitRate?: number; startedAt: string; status: string }
export interface TranscodesResponse { items: TranscodePipeline[]; active: number; queued: number; limit: number; queueLimit: number }
export interface PlaybackCapabilities { ffmpegAvailable: boolean; ffprobeAvailable: boolean; ffmpegPath: string; ffprobePath: string; hardwareAcceleration: string[]; softwareFallback: boolean; qualityProfiles: string[]; maxConcurrentTranscodes: number }
export interface ServerJob { id: string; type: string; status: string; progress: number; createdAt: string; startedAt?: string; finishedAt?: string; duration?: number; source: string; library: { id: string; name: string }; message?: string; errorCode?: string; actions: { cancellable: boolean; retryable: boolean } }
export interface ActivityEvent { id: string; type: string; title: string; message: string; createdAt: string; entityType: string; entityId?: string }
export interface ServerAlert { id: string; severity: 'INFO'|'WARNING'|'CRITICAL'; type: string; title: string; message: string; createdAt: string; resolved: boolean }

export type LibraryType = 'MOVIE' | 'TV_SHOW';
export type LibraryPrivacy = 'PUBLIC' | 'PRIVATE';

export interface LibraryPath {
  id: string;
  path: string;
  isActive: boolean;
  status: string;
  lastScanAt?: string;
  lastCheckedAt?: string;
  lastAvailableAt?: string;
  lastError?: string;
}

export interface LibraryStats {
  totalSizeBytes: number;
  fileCount: number;
  mediaItemCount: number;
  movieCount: number;
  tvShowCount: number;
  seasonCount: number;
  episodeCount: number;
  unmatchedCount: number;
  missingCount: number;
  pathCount: number;
  updatedAt?: string;
}

export interface AutoSyncConfig {
  enabled: boolean;
  mode: 'INTERVAL' | 'SCHEDULE';
  intervalMinutes: number;
  schedule: { hour: number; minute: number };
  scanOnStartup: boolean;
}

export interface Library {
  id: string;
  name: string;
  description?: string;
  type: LibraryType;
  privacy: LibraryPrivacy;
  minimumAge: number;
  metadataLanguage: string;
  metadataRegion?: string;
  isActive: boolean;
  scanEnabled: boolean;
  createdAt: string;
  updatedAt: string;
  lastScanAt?: string;
  lastSuccessfulScanAt?: string;
  operationalStatus: 'READY' | 'SCANNING' | 'SYNCING' | 'PATH_UNAVAILABLE' | 'ERROR' | 'DISABLED';
  nextSyncAt?: string;
  lastAutoSyncAt?: string;
  lastError?: string;
  autoSync: AutoSyncConfig;
  stats: LibraryStats;
  paths?: LibraryPath[];
}

export interface CreateLibraryRequest {
  name: string;
  description?: string;
  type: LibraryType;
  privacy: LibraryPrivacy;
  password?: string;
  minimumAge: number;
  metadataLanguage: string;
  metadataRegion?: string;
  paths: string[];
}

export type PathValidationStatus = 'VALID' | 'NOT_FOUND' | 'NOT_READABLE' | 'INVALID';

export interface PathValidationResult {
  valid: boolean;
  exists: boolean;
  readable: boolean;
  directory: boolean;
}

export function pathStatus(result: PathValidationResult): PathValidationStatus {
  if (result.valid) return 'VALID';
  if (!result.exists) return 'NOT_FOUND';
  if (!result.readable) return 'NOT_READABLE';
  return 'INVALID';
}

export type ScanStatus =
  | 'QUEUED'
  | 'SCANNING'
  | 'MATCHING'
  | 'PERSISTING'
  | 'COMPLETED'
  | 'COMPLETED_WITH_WARNINGS'
  | 'FAILED'
  | 'CANCELLED'
  | 'SKIPPED_ALREADY_RUNNING';

export interface ScanJob {
  id: string;
  libraryId: string;
  status: ScanStatus;
  scanType: 'FULL' | 'INCREMENTAL';
  triggerSource: 'MANUAL' | 'AUTO_INTERVAL' | 'AUTO_SCHEDULE' | 'STARTUP';
  startedAt?: string;
  finishedAt?: string;
  discoveredFiles: number;
  processedFiles: number;
  matchedFiles: number;
  unmatchedFiles: number;
  skippedFiles: number;
  removedFiles: number;
  failedFiles: number;
  progress: number;
  errorMessage?: string;
  createdAt: string;
}

export interface StartScanResponse {
  jobId: string;
  status: ScanStatus;
}

export interface UnmatchedItem {
  mediaFileId: string;
  filename: string;
  relativePath: string;
  normalizedTitle?: string;
  year?: number;
  season?: number;
  episode?: number;
  status: string;
}
export interface IdentificationCandidate {
  provider: 'TMDB';
  providerId: number;
  type: MediaType;
  title: string;
  originalTitle?: string;
  year?: number;
  overview?: string;
  posterPath?: string;
  rating?: number;
}
export interface ManualIdentificationResult {
  mediaFileId: string;
  mediaItemId: string;
  identificationStatus: 'MATCHED_MANUAL';
  associatedFiles: number;
}

export interface Paginated<T> {
  items: T[];
  page?: number;
  pageSize?: number;
  total?: number;
  totalPages?: number;
}

export type MediaType = 'MOVIE' | 'TV_SHOW';
export interface Genre {
  id: string;
  name: string;
  count?: number;
}
export interface MediaCard {
  id: string;
  title: string;
  originalTitle?: string;
  year?: number;
  posterPath?: string;
  backdropPath?: string;
  rating?: number;
  popularity?: number;
  genres: Genre[];
  mediaType: MediaType;
  libraryId: string;
  addedAt: string;
  isFavorite: boolean;
  numberOfSeasons?: number;
  numberOfEpisodes?: number;
  recommendationScore?: number;
  recommendationReason?: string;
}
export interface RecommendationSectionData { key: string; title: string; sourceMediaId?: string; coldStart?: boolean; items: MediaCard[] }
export interface HomeRecommendations { sections: RecommendationSectionData[]; meta: { cacheHit: boolean; generatedAt: string; generationDurationMs?: number; candidateCount?: number; finalItemCount?: number } }
export interface ForYouRecommendations { items: MediaCard[]; affinities: Array<{ genreId: string; name: string; score: number }>; coldStart: boolean; meta: { cacheHit: boolean; generatedAt: string } }
export interface PersonCredit {
  id: string;
  name: string;
  profilePath?: string;
  character?: string;
  job?: string;
  department?: string;
}
export interface MediaFile {
  id: string;
  filename: string;
  relativePath?: string;
  fileSize: number;
  extension?: string;
  modifiedAt?: string;
  identificationStatus?: string;
}
export interface MediaDetails extends MediaCard {
  overview?: string;
  tagline?: string;
  releaseDate?: string;
  firstAirDate?: string;
  lastAirDate?: string;
  runtime?: number;
  status?: string;
  voteCount?: number;
  originalLanguage?: string;
  tmdbId: number;
  metadataLanguage: string;
  metadataFetchedAt: string;
  cast: PersonCredit[];
  crew: PersonCredit[];
  files?: MediaFile[];
  library: { id: string; name: string };
}
export interface Season {
  id: string;
  seasonNumber: number;
  name: string;
  overview?: string;
  posterPath?: string;
  episodeCount: number;
}
export interface Episode {
  id: string;
  episodeNumber: number;
  seasonNumber: number;
  name?: string;
  overview?: string;
  airDate?: string;
  stillPath?: string;
  rating?: number;
  runtime?: number;
  mediaFileId?: string;
  filename?: string;
  fileSize?: number;
}

export type PlaybackMode = 'DIRECT_PLAY' | 'DIRECT_STREAM' | 'TRANSCODE';
export type PlaybackQuality =
  'AUTO' | 'ORIGINAL' | '4K' | '1080P_HIGH' | '1080P' | '720P' | '480P';
export interface ClientCapabilities {
  containers: string[];
  videoCodecs: string[];
  audioCodecs: string[];
  maxWidth: number;
  maxHeight: number;
  estimatedBandwidthKbps?: number;
  maxAudioChannels?: number;
}
export interface PlaybackStartRequest {
  mediaItemId: string;
  mediaFileId?: string;
  episodeId?: string;
  clientCapabilities: ClientCapabilities;
  quality: PlaybackQuality;
  clientId?: string;
  clientName?: string;
}
export interface PlaybackTechnicalMetadata {
  container?: string;
  durationMs?: number;
  overallBitrate?: number;
  videoCodec?: string;
  audioCodec?: string;
  width?: number;
  height?: number;
}
export interface PlaybackStartResponse {
  sessionId: string;
  playbackMode: PlaybackMode;
  streamUrl: string;
  duration: number;
  resumePosition: number;
  quality: PlaybackQuality;
  reason: string[];
  availableQualities: PlaybackQuality[];
  metadata: PlaybackTechnicalMetadata;
  content: {
    mediaItemId: string;
    mediaFileId: string;
    episodeId?: string;
    title: string;
    year?: number;
    episodeName?: string;
    seasonNumber?: number;
    episodeNumber?: number;
  };
  nextEpisode?: {
    episodeId: string;
    mediaFileId: string;
    seasonNumber: number;
    episodeNumber: number;
    name?: string;
  };
}
export interface ContinueWatchingItem {
  mediaItemId: string;
  episodeId?: string;
  positionMs: number;
  durationMs: number;
  percentage: number;
  updatedAt: string;
  title: string;
  posterPath?: string;
  backdropPath?: string;
  mediaType: MediaType;
  episodeName?: string;
  seasonNumber?: number;
  episodeNumber?: number;
  stillPath?: string;
}
export interface PlaybackProgressRequest {
  positionMs: number;
  durationMs: number;
  state: 'PLAYING' | 'PAUSED' | 'ENDED';
  bufferEvents?: number;
}

export const TERMINAL_SCAN_STATUSES: ScanStatus[] = [
  'COMPLETED',
  'COMPLETED_WITH_WARNINGS',
  'FAILED',
  'CANCELLED',
  'SKIPPED_ALREADY_RUNNING',
];

export type RemoteProviderType = 'M3U_URL' | 'M3U_FILE' | 'GOOGLE_DRIVE';
export type RemoteSourceStatus =
  | 'READY'
  | 'SYNCING'
  | 'WARNING'
  | 'AUTH_REQUIRED'
  | 'UNAVAILABLE'
  | 'ERROR'
  | 'DISABLED';

export interface RemoteSource {
  id: string;
  libraryId: string;
  providerType: RemoteProviderType;
  name: string;
  isActive: boolean;
  status: RemoteSourceStatus;
  config: Record<string, unknown>;
  autoSync: { enabled: boolean; intervalMinutes: number };
  lastSyncAt?: string;
  lastSuccessfulSyncAt?: string;
  nextSyncAt?: string;
  lastError?: string;
  lastErrorAt?: string;
  createdAt: string;
  updatedAt: string;
}

export interface M3uSubcategory {
  name: string;
  count: number;
}
export interface M3uCategory {
  name: string;
  mediaType: 'MOVIE' | 'TV_SHOW' | 'UNKNOWN';
  count: number;
  subcategories: M3uSubcategory[];
}
export interface M3uPreview {
  totalEntries: number;
  movieCandidates: number;
  tvCandidates: number;
  unknownCandidates: number;
  categories: M3uCategory[];
}

export interface RemoteSelection {
  mediaType: 'MOVIE' | 'TV_SHOW' | 'ALL';
  category: string | null;
  subcategory: string | null;
  includeAll: boolean;
  isEnabled: boolean;
}

export interface M3uEntry {
  id: string;
  externalKey: string;
  rawName: string;
  cleanTitle: string;
  year?: number;
  mediaType: string;
  category?: string;
  subcategory?: string;
  seasonNumber?: number;
  episodeNumber?: number;
  tvgLogo?: string;
  isSelected: boolean;
  syncStatus: string;
  missingSince?: string;
  lastSeenAt: string;
  mediaFileId?: string;
}

export interface RemoteSyncOutcome {
  scanned: number;
  new: number;
  updated: number;
  unchanged: number;
  missing: number;
  matched: number;
  unmatched: number;
  notModified: boolean;
  durationMs: number;
}

export interface GoogleDriveConnection {
  id: string;
  accountEmail: string;
  status: 'CONNECTED' | 'AUTH_REQUIRED' | 'EXPIRED' | 'ERROR' | 'DISABLED';
  lastRefreshAt?: string;
  lastError?: string;
  createdAt: string;
}

export interface GoogleDriveItem {
  id: string;
  name: string;
  mimeType: string;
  isFolder: boolean;
  size?: number;
  modifiedTime?: string;
}
