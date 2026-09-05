# Remote media sources

MyLib libraries can be fed from remote origins in addition to local paths. The feature lives in
`src/features/remote_sources/` and reuses the existing catalog, TMDB, playback and auto-sync
machinery.

Supported providers:

| Provider       | Added via                    | Sync                                    |
| -------------- | ---------------------------- | -------------------------------------- |
| `M3U_URL`      | playlist URL                 | re-fetch (honours `ETag`/`Last-Modified`, `304` skips) |
| `M3U_FILE`     | uploaded `.m3u` / `.m3u8`     | reprocess the stored file (replace to update) |
| `GOOGLE_DRIVE` | OAuth + folder selection     | Drive metadata listing (`files.list`) |

The architecture is prepared for `S3`, `WebDAV`, `OneDrive`, `SMB` and `NFS` but they are **not**
implemented.

## Pipeline

```text
RemoteSource → discovery → normalization → selection filter → catalog resolver → TMDB → MyLib catalog
```

- `m3u.rs` – streaming parser (`M3uParser` line state machine, `analyze_stream` preview
  aggregator). Memory stays bounded by the number of distinct categories, never entry count.
- `normalize.rs` – `group-title` → category/subcategory, accent/case folding, and
  `analyze_entry` which reuses `catalog::scanner::parse_filename` for year and `SxxExx`.
- `sync.rs` – diffs `m3u_entries` (`NEW`/`UPDATED`/`UNCHANGED`/`MISSING`), creates
  `media_files` (`storage_kind='REMOTE'`) + `remote_media_sources`, then calls
  `catalog::resolver::identify_pending`. Missing entries keep their rows and gain a
  `missing_since`; a later reappearance clears it.
- `google_drive.rs` – OAuth (authorization code + PKCE), folder browser, recursive
  metadata-only listing, `sync_inner`, and `resolve_stream` for playback.
- `scheduler.rs` – background loop (mirrors `libraries::sync`) that runs due sources.
- `cache.rs` – bounded on-disk cache (`data/cache/remote/`) with TTL + total-size LRU.

## Security model

- Credentials embedded in M3U URLs and every per-entry stream URL are sealed with
  ChaCha20-Poly1305 (`infrastructure::secrets`); the key lives in
  `data/secrets/remote-sources.key`.
- Google OAuth access/refresh tokens are sealed the same way in
  `google_drive_connections.credentials_ref` and never returned by any endpoint or written to
  logs.
- API responses only ever expose `sanitize_url()` output (no userinfo, no secret query params).
- Playback never receives an origin URL: `GET /api/v1/playback/{session}/remote` resolves the
  origin server-side, forwards the client `Range`, and streams the body back. Google Drive
  downloads attach `Authorization: Bearer` server-side.
- The scanner never mutates user media; remote entries use an `is_active=0` synthetic
  `library_paths` row so the local scanner ignores them.

## Configuration

| Variable | Default | Purpose |
| -------- | ------- | ------- |
| `MYLIB_REMOTE_CACHE_GB` | `10` | Max size of the remote chunk cache |
| `MYLIB_REMOTE_CACHE_TTL_HOURS` | `24` | Cache entry TTL |
| `MYLIB_M3U_MAX_BYTES` | `536870912` | Playlist size ceiling (fetch + upload) |
| `MYLIB_M3U_FETCH_TIMEOUT_SECONDS` | `30` | Preview fetch timeout |
| `MYLIB_REMOTE_HTTP_MAX_CONCURRENCY` | `6` | Concurrent outbound requests to origins/Drive |
| `MYLIB_REMOTE_SYNC_INTERVAL_SECONDS` | `60` | Scheduler tick |
| `MYLIB_GOOGLE_OAUTH_CLIENT_ID` | – | Google OAuth client id |
| `MYLIB_GOOGLE_OAUTH_CLIENT_SECRET` | – | Google OAuth client secret |
| `MYLIB_GOOGLE_OAUTH_REDIRECT_URL` | – | Must match the console; points at `/api/v1/remote-sources/google-drive/callback` |

Google Drive scope requested: `https://www.googleapis.com/auth/drive.readonly` only.

## Endpoints

```text
GET    /api/v1/libraries/{id}/remote-sources
POST   /api/v1/libraries/{id}/remote-sources
GET    /api/v1/remote-sources/{id}
PATCH  /api/v1/remote-sources/{id}
DELETE /api/v1/remote-sources/{id}
GET    /api/v1/remote-sources/{id}/status
GET    /api/v1/remote-sources/{id}/entries
GET    /api/v1/remote-sources/{id}/selections
PUT    /api/v1/remote-sources/{id}/selections
POST   /api/v1/remote-sources/{id}/sync            (?wait=true runs inline)
POST   /api/v1/remote-sources/m3u/upload
POST   /api/v1/remote-sources/m3u/preview
POST   /api/v1/remote-sources/google-drive/connect
GET    /api/v1/remote-sources/google-drive/callback
GET    /api/v1/remote-sources/google-drive/connections
DELETE /api/v1/remote-sources/google-drive/connections/{id}
GET    /api/v1/remote-sources/google-drive/{connectionId}/browse
```

## Benchmark

```bash
cargo run --release --bin m3u_benchmark -- 10000 50000 100000 250000
```

Reports parse time and entries/second for the streaming parser + normalizer (no TMDB, no DB).
Database and catalog timings for a real sync are reported per run in the `SyncOutcome`
(`durationMs`) and the `REMOTE_SOURCE_SYNCED` activity entry.
