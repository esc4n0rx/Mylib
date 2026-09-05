# MyLib Server

MyLib Server is the self-hosted backend for managing a personal media library. It provides bootstrap, authentication and administration, local movie/TV libraries, asynchronous filesystem scans, a TMDB-backed catalog, playback, operational monitoring and personalized recommendations.

## Project structure

The Rust backend follows feature-oriented boundaries. Business capabilities live under
`src/features`; shared policies and adapters are kept outside those features so ownership is
clear and dependencies stay predictable.

```text
src/
├── app/                 # application state, startup and middleware composition
├── core/                # configuration, shared errors and transport models
├── features/            # auth, catalog, libraries, operations, playback, recommendations
├── http/                # top-level Axum router
├── infrastructure/      # database and embedded frontend adapters
├── bin/                 # optional maintenance and benchmark binaries
├── lib.rs               # public API and compatibility exports
└── main.rs              # executable entry point
```

See [Architecture](docs/ARCHITECTURE.md) for dependency rules and where new code belongs.
Contributions should follow [CONTRIBUTING.md](CONTRIBUTING.md).

## Quick install (prebuilt binary)

No Rust or Node toolchain required. The script downloads the prebuilt server for your OS/CPU
from [GitHub Releases](https://github.com/esc4n0rx/Mylib/releases), makes sure FFmpeg is
available, starts `mylib-server` and prints the URL to open — both `http://localhost:8096` and
the address reachable from other devices on the same network.

Linux / macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/esc4n0rx/Mylib/main/scripts/install.sh | bash
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/esc4n0rx/Mylib/main/scripts/install.ps1 | iex
```

The server keeps running after the script exits; re-run the same command later to update to the
latest release (data in `MYLIB_DATA_DIR` is preserved). Source: [scripts/install.sh](scripts/install.sh),
[scripts/install.ps1](scripts/install.ps1), release pipeline in
[.github/workflows/release.yml](.github/workflows/release.yml).

## Requirements and build

- Rust 1.98 or newer
- SQLite requires no separate service
- MySQL 8+ is optional

```bash
cargo build --release
./target/release/mylib-server
```

The server listens on `0.0.0.0:8096`. On first start it creates `./data/{config,logs,secrets}`, opens `./data/mylib.db`, runs the SQLite migrations, and waits for setup through the API. No default credentials are created.

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `MYLIB_HOST` | `0.0.0.0` | Listen address |
| `MYLIB_PORT` | `8096` | Listen port |
| `MYLIB_DATA_DIR` | `./data` | Persistent data root (`/data` in Docker) |
| `MYLIB_LOG_LEVEL` | `info,tower_http=info` | `tracing` filter |
| `MYLIB_ALLOWED_ORIGINS` | `http://localhost:3000` | Comma-separated CORS origins |
| `MYLIB_DATABASE_TYPE` | unset | Explicit `sqlite` or `mysql` provider |
| `MYLIB_DATABASE_URL` | unset | Explicit SQLx connection URL |
| `MYLIB_JWT_SECRET` | generated | At least 32 characters; injected secret takes precedence |
| `MYLIB_TOKEN_TTL_SECONDS` | `3600` | Access-token lifetime |
| `MYLIB_TMDB_API_KEY` | unset | TMDB v3 API key; scans still index files when unset |
| `MYLIB_TMDB_TIMEOUT_SECONDS` | `10` | Per-request TMDB timeout |
| `MYLIB_TMDB_MAX_CONCURRENCY` | `4` | Global in-flight TMDB request limit |
| `MYLIB_SCAN_MAX_CONCURRENT_LIBRARIES` | `2` | Simultaneous library scans |
| `MYLIB_SCAN_DISCOVERY_WORKERS` | `4` | Reserved discovery worker limit |
| `MYLIB_SCAN_PARSE_WORKERS` | `8` | Reserved filename parser worker limit |
| `MYLIB_SCAN_METADATA_WORKERS` | `4` | Metadata stage worker limit |
| `MYLIB_SCAN_BATCH_SIZE` | `250` | Database batch and bounded-channel sizing |
| `MYLIB_FFMPEG_PATH` | `./tools/ffmpeg/ffmpeg` (`.exe` on Windows) | FFmpeg executable used for remux and transcoding |
| `MYLIB_FFPROBE_PATH` | `./tools/ffmpeg/ffprobe` (`.exe` on Windows) | FFprobe executable used for persisted technical analysis |

Precedence is: explicitly defined database environment variables, encrypted persisted setup selection, then the default SQLite database. Environment values are never copied into logs or persisted by startup.

## Initial setup

Check status, then create the server and first administrator:

```bash
curl http://localhost:8096/api/v1/setup/status
curl -X POST http://localhost:8096/api/v1/setup \
  -H 'Content-Type: application/json' \
  -d '{"serverName":"My Home Server","database":{"type":"sqlite"},"administrator":{"username":"admin","password":"StrongPassword123!","displayName":"Administrator"}}'
```

Setup runs in one database transaction. It creates the persistent server UUID, system roles, current permissions, the Argon2id administrator credential, role links and audit records. A second setup request returns `409 SETUP_ALREADY_COMPLETED`.

### SQLite

SQLite is the recommended zero-configuration mode. The default file is `/data/mylib.db` in a container or `$MYLIB_DATA_DIR/mylib.db` locally. WAL and foreign keys are enabled. A custom setup path is accepted only when it contains no parent traversal.

### MySQL

Use the setup database-test endpoint before selecting MySQL:

```bash
curl -X POST http://localhost:8096/api/v1/setup/database/test \
  -H 'Content-Type: application/json' \
  -d '{"type":"mysql","host":"mysql.example","port":3306,"database":"mylib","username":"mylib","password":"secret","sslMode":"preferred"}'
```

The same object can be placed in the setup payload. `sslMode` supports `disabled`, `preferred`, and `required`. The connection URL is encrypted with ChaCha20-Poly1305 in `/data/secrets/database.enc`; its separately generated 256-bit key is in `/data/secrets/database.key`. Passwords, URLs, encryption keys and tokens are never logged. An external secret manager can instead inject `MYLIB_DATABASE_URL` and `MYLIB_DATABASE_TYPE=mysql`.

## API

Public routes:

- `GET /health`, `GET /api/v1/health`
- `GET /api/v1/setup/status`
- `POST /api/v1/setup` and `POST /api/v1/setup/database/test` (only before setup)
- `POST /api/v1/auth/login` (five failed attempts per source IP per minute)

Bearer-authenticated routes:

- `GET /api/v1/auth/me`
- `GET|PATCH /api/v1/server`
- `GET|POST /api/v1/users`
- `GET|PATCH /api/v1/users/{id}`
- `PUT /api/v1/users/{id}/password`
- `POST /api/v1/users/{id}/disable|enable`
- `PUT /api/v1/users/{id}/roles`
- `GET /api/v1/roles`

Authorization is evaluated against the database on every request, so role changes take effect immediately rather than waiting for JWT expiry. The last active administrator cannot be disabled or stripped of the Administrator role.

## Libraries and catalog

A library is explicitly either `MOVIE` or `TV_SHOW`, may contain multiple non-overlapping local paths, stores a metadata language/optional region and a future-facing minimum age from 0 through 21. `PUBLIC` libraries are available to authenticated viewers. `PRIVATE` libraries use an independent Argon2id password and issue a four-hour, user-and-library-scoped unlock token through `POST /api/v1/libraries/{id}/unlock`; send that token as `X-Library-Unlock` on content requests. Password hashes are never serialized.

Paths must exist, be readable directories and cannot contain parent traversal, overlap another active library, or point into `MYLIB_DATA_DIR`. Mounted NAS/NFS/SMB directories work as ordinary local paths; MyLib does not mount them. Deleting a library requires `?confirm=true`, soft-deletes catalog ownership and never deletes physical media.

The scanner recognizes `.mkv`, `.mp4`, `.m4v`, `.avi`, `.mov`, `.ts`, `.m2ts` and `.webm`, while skipping sidecars and common recycle/system directories. Movie parsing extracts titles and plausible years; TV parsing supports `S01E01`, `1x01`, multi-episode `S01E01E02`, and specials (`S00`). Resolution, source, codec, audio and release tokens are treated as extensible noise. No video content, full-file hashes, FFprobe process or external command is used.

Scans return `202` immediately and move through persisted job states. Discovery feeds a bounded channel, persistence uses short batches, library and TMDB concurrency are globally limited, and each library has an atomic single-scan guard. The light fingerprint is normalized path + size + modification time. Unchanged, already matched files avoid metadata work; unmatched files are reparsed and retried so matcher improvements apply without renaming them. Removed files become `MISSING` rather than being deleted. If a whole path is inaccessible it becomes `PATH_UNAVAILABLE`, the scan completes with warnings, and none of its files are marked missing.

TMDB search always follows the library type, language and region. Title, original title and year contribute to the safe `0.90` confidence threshold; when a release-folder year produces no safe result, the matcher retries without it. Once a TV show is matched, files with the same parsed series title reuse that association. Search/details responses use a persistent TTL cache, requests have connection/request timeouts, bounded concurrency and retry backoff for 429/5xx. Catalog records normalize media items, movies, shows, seasons, episodes, genres, people and credits while retaining only TMDB image paths. Ambiguous/unmatched files remain available for manual TMDB search; choosing a show associates all unmatched episodes with the same parsed title. A scan changes filesystem knowledge; metadata refresh is a separate operation.

Library/catalog routes (all under `/api/v1`) are:

- `GET|POST /libraries`, `GET|PATCH|DELETE /libraries/{id}`
- `POST /libraries/paths/validate`, `POST /libraries/{id}/paths`, `DELETE /libraries/{id}/paths/{pathId}`
- `POST /libraries/{id}/unlock`
- `POST /libraries/{id}/scan`, `GET /libraries/{id}/scans`, `GET /libraries/{id}/scans/{scanId}`, `POST /libraries/{id}/scans/{scanId}/cancel`
- `GET /libraries/{id}/items`, `GET /libraries/{id}/items/{itemId}`, `GET /libraries/{id}/unmatched`
- `GET /media/identify/search`, `POST /media/identify`, `DELETE /media/{mediaFileId}/identification`, `POST /media/{mediaFileId}/reidentify`
- `POST /media/items/{itemId}/metadata/refresh`, `GET /settings/metadata/tmdb/status`

Potentially large lists default to 50 rows and cap `pageSize` at 200. The new permissions are `libraries.view/manage/scan/unlock` and `media.view/identify/manage`; administrators receive all of them, while the standard User role receives the two view permissions.

## Web frontend (`web/`)

The MyLib web interface lives in `web/` and is part of this repository — one product, one
binary. Stack: React + Vite + TypeScript + Material UI (Emotion), React Router, TanStack
Query (server state), Zustand (UI state only), React Hook Form + Zod, i18next. The MUI theme
in `web/src/theme` implements the Design System (`design/`) with hand-tuned light **and**
dark token sets; the theme mode is `system` by default and follows `prefers-color-scheme`
live. The UI language is **pt-BR** by default with the i18n structure ready for more locales.

### Development

Backend and frontend run as two dev processes; Vite proxies `/api` and `/health` to the
backend so the app always uses relative URLs.

```bash
cargo run                     # backend  -> http://localhost:8096
npm --prefix web install
npm --prefix web run dev      # frontend -> http://localhost:5173
# or: ./scripts/dev.sh
```

Set `MYLIB_BACKEND_URL` to point the Vite proxy elsewhere.

### Production build (single artifact)

```bash
./scripts/build-release.sh          # bash
# or  pwsh scripts/build-release.ps1
```

`npm run build` emits `web/dist/`, which `rust-embed` compiles into the server binary
(`src/infrastructure/web_assets.rs`). At runtime Axum serves `/api/*` as the JSON API and every other path
from the embedded bundle, with SPA fallback to `index.html` (never for `/api`), long-lived
immutable caching for hashed `/assets/*` and `no-cache` for `index.html`. The whole app is
reachable on the single backend port (`8096`). If the server is built without running the
web build first, non-API routes return a short "frontend not embedded" message.

### Frontend quality gates

```bash
cd web
npm run lint
npm run typecheck
npm run test          # Vitest + Testing Library
npm run build
npm run test:e2e      # Playwright (needs a running server at MYLIB_E2E_URL)
```

## Persistence and migrations

Provider-specific schemas live in `migrations/sqlite` and `migrations/mysql`; the runner embeds versioned, idempotent DDL in the single binary. Task 02 adds `libraries`, `library_paths`, `scan_jobs`, `media_files`, `media_items`, movie/TV detail tables, normalized genres/credits and `metadata_cache`, with indexes on library/path/status/fingerprint/TMDB and episode keys. SQLite uses WAL, foreign keys and a controlled batch writer; MySQL uses the same provider-neutral repository boundary.

## Security

- Argon2id password hashes, HS256 access tokens with minimal claims, and generic login errors
- Immediate active-user and permission checks
- Basic per-IP login throttling
- Encrypted MySQL credentials and generated local secrets
- Explicit CORS allow-list, `nosniff`, frame denial and no-referrer headers
- UUID `X-Request-ID`, structured JSON errors, tracing, and audit events
- Graceful Ctrl+C/SIGTERM shutdown and pool closure

Terminate TLS at a trusted reverse proxy for production and provide secrets through mounted files or environment injection appropriate to the deployment platform.

## Docker

```bash
docker build -t mylib/server:0.1.0 .
docker run --rm -p 8096:8096 -v ./mylib-data:/data mylib/server:0.1.0
```

Then open `http://localhost:8096` — setup wizard, admin, database, libraries, scan, login/home,
all from the one container. The multi-stage image builds the web bundle (`node:22`), embeds it
into the release binary (`rust:1.98`), and ships only the binary plus CA certificates on
`debian:bookworm-slim` — no node, npm, vite or Rust toolchain — running as the unprivileged
`mylib` user. For MySQL, place the database anywhere reachable by the container and select it through setup or environment variables; no same-host assumption is made.

## Validation

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

The integration suite additionally exercises path validation, public/private library creation, Argon2id unlock, controlled deletion, asynchronous full/incremental scans, ignored extensions and safe missing-file detection. TMDB is not contacted by automated tests. MySQL migrations and connectivity require an integration MySQL service and can be checked with the public pre-setup database-test route.

For a local scanner throughput smoke benchmark (no TMDB), run `cargo run --release --bin scanner_benchmark -- 10000`. It creates an isolated temporary fake tree, reports files/second, then removes only that generated tree. Try `50000` or `100000` for larger hosts.

## Known limitations

Incremental scans still enumerate directory metadata to discover removals; they avoid reparsing, database replacement and TMDB work for unchanged fingerprints. Rename recognition is deliberately conservative, age enforcement awaits a user birth-date model, TMDB availability status reports configuration without spending an API request, and provider integration currently covers TMDB only. Deep media analysis and cloud/storage mounting are not included.

Large libraries still require a complete metadata enumeration to detect removals. Playback
compatibility depends on the codecs exposed by FFmpeg and the target browser. Additional
metadata providers and remote storage adapters are future extension points.
