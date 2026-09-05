use std::{env, fs, net::IpAddr, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    pub log_level: String,
    pub allowed_origins: Vec<String>,
    pub database_type: Option<String>,
    pub database_url: Option<String>,
    pub jwt_secret: String,
    pub token_ttl_seconds: i64,
    pub tmdb_api_key: Option<String>,
    pub tmdb_timeout_seconds: u64,
    pub tmdb_max_concurrency: usize,
    pub scan_max_concurrent_libraries: usize,
    pub scan_discovery_workers: usize,
    pub scan_parse_workers: usize,
    pub scan_metadata_workers: usize,
    pub scan_batch_size: usize,
    pub transcode_max_concurrent: usize,
    pub transcode_max_queue: usize,
    pub transcode_cache_gb: u64,
    pub transcode_cache_ttl_seconds: u64,
    pub playback_completion_percent: u64,
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: PathBuf,
    pub remote_cache_gb: u64,
    pub remote_cache_ttl_seconds: u64,
    pub m3u_max_bytes: u64,
    pub m3u_fetch_timeout_seconds: u64,
    pub remote_http_max_concurrency: usize,
    pub remote_sync_interval_seconds: u64,
    pub google_oauth_client_id: Option<String>,
    pub google_oauth_client_secret: Option<String>,
    pub google_oauth_redirect_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedDatabaseConfig {
    pub database_type: String,
    pub encrypted_url_file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedTmdbConfig {
    pub encrypted_key_file: Option<String>,
}

impl Config {
    pub fn load() -> AppResult<Self> {
        let data_dir =
            PathBuf::from(env::var("MYLIB_DATA_DIR").unwrap_or_else(|_| "./data".into()));
        for child in ["config", "logs", "secrets"] {
            fs::create_dir_all(data_dir.join(child))?;
        }
        let jwt_secret = match env::var("MYLIB_JWT_SECRET") {
            Ok(value) if value.len() >= 32 => value,
            Ok(_) => {
                return Err(AppError::config(
                    "MYLIB_JWT_SECRET must contain at least 32 characters",
                ));
            }
            Err(_) => load_or_create_secret(&data_dir.join("secrets/jwt.key"), 64)?,
        };
        Ok(Self {
            host: env::var("MYLIB_HOST")
                .unwrap_or_else(|_| "0.0.0.0".into())
                .parse()
                .map_err(|_| AppError::config("invalid MYLIB_HOST"))?,
            port: env::var("MYLIB_PORT")
                .unwrap_or_else(|_| "8096".into())
                .parse()
                .map_err(|_| AppError::config("invalid MYLIB_PORT"))?,
            log_level: env::var("MYLIB_LOG_LEVEL")
                .unwrap_or_else(|_| "info,tower_http=info".into()),
            allowed_origins: env::var("MYLIB_ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "http://localhost:3000".into())
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(String::from)
                .collect(),
            database_type: env::var("MYLIB_DATABASE_TYPE").ok(),
            database_url: env::var("MYLIB_DATABASE_URL").ok(),
            token_ttl_seconds: env::var("MYLIB_TOKEN_TTL_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            tmdb_api_key: env::var("MYLIB_TMDB_API_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            tmdb_timeout_seconds: positive_env("MYLIB_TMDB_TIMEOUT_SECONDS", 10),
            tmdb_max_concurrency: positive_env("MYLIB_TMDB_MAX_CONCURRENCY", 4),
            scan_max_concurrent_libraries: positive_env("MYLIB_SCAN_MAX_CONCURRENT_LIBRARIES", 2),
            scan_discovery_workers: positive_env("MYLIB_SCAN_DISCOVERY_WORKERS", 4),
            scan_parse_workers: positive_env("MYLIB_SCAN_PARSE_WORKERS", 8),
            scan_metadata_workers: positive_env("MYLIB_SCAN_METADATA_WORKERS", 4),
            scan_batch_size: positive_env("MYLIB_SCAN_BATCH_SIZE", 250),
            transcode_max_concurrent: positive_env("MYLIB_TRANSCODE_MAX_CONCURRENT", 2),
            transcode_max_queue: positive_env("MYLIB_TRANSCODE_MAX_QUEUE", 10),
            transcode_cache_gb: positive_env("MYLIB_TRANSCODE_CACHE_GB", 20),
            transcode_cache_ttl_seconds: positive_env("MYLIB_TRANSCODE_CACHE_TTL_SECONDS", 21600),
            playback_completion_percent: positive_env("MYLIB_PLAYBACK_COMPLETION_PERCENT", 92),
            ffmpeg_path: PathBuf::from(
                env::var("MYLIB_FFMPEG_PATH").unwrap_or_else(|_| default_tool_path("ffmpeg")),
            ),
            ffprobe_path: PathBuf::from(
                env::var("MYLIB_FFPROBE_PATH").unwrap_or_else(|_| default_tool_path("ffprobe")),
            ),
            remote_cache_gb: positive_env("MYLIB_REMOTE_CACHE_GB", 10),
            remote_cache_ttl_seconds: positive_env::<u64>("MYLIB_REMOTE_CACHE_TTL_HOURS", 24)
                .saturating_mul(3600),
            m3u_max_bytes: positive_env("MYLIB_M3U_MAX_BYTES", 512 * 1024 * 1024),
            m3u_fetch_timeout_seconds: positive_env("MYLIB_M3U_FETCH_TIMEOUT_SECONDS", 30),
            remote_http_max_concurrency: positive_env("MYLIB_REMOTE_HTTP_MAX_CONCURRENCY", 6),
            remote_sync_interval_seconds: positive_env("MYLIB_REMOTE_SYNC_INTERVAL_SECONDS", 60),
            google_oauth_client_id: env::var("MYLIB_GOOGLE_OAUTH_CLIENT_ID")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            google_oauth_client_secret: env::var("MYLIB_GOOGLE_OAUTH_CLIENT_SECRET")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            google_oauth_redirect_url: env::var("MYLIB_GOOGLE_OAUTH_REDIRECT_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            data_dir,
            jwt_secret,
        })
    }

    pub fn sqlite_url(&self) -> String {
        let root = fs::canonicalize(&self.data_dir).unwrap_or_else(|_| self.data_dir.clone());
        sqlite_connection_url(&root.join("mylib.db"))
    }

    pub fn persisted_config_path(&self) -> PathBuf {
        self.data_dir.join("config/database.json")
    }

    pub fn persisted_tmdb_config_path(&self) -> PathBuf {
        self.data_dir.join("config/tmdb.json")
    }
}

/// Default bundled location for `ffmpeg`/`ffprobe`, relative to the working directory.
/// Windows binaries carry the `.exe` suffix; Linux and macOS builds do not.
fn default_tool_path(name: &str) -> String {
    if cfg!(windows) {
        format!("./tools/ffmpeg/{name}.exe")
    } else {
        format!("./tools/ffmpeg/{name}")
    }
}

fn positive_env<T>(name: &str, default: T) -> T
where
    T: FromStr + Copy + PartialOrd + Default,
{
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<T>().ok())
        .filter(|value| *value > T::default())
        .unwrap_or(default)
}

fn load_or_create_secret(path: &PathBuf, length: usize) -> AppResult<String> {
    if path.exists() {
        let value = fs::read_to_string(path)?;
        if value.len() >= 32 {
            return Ok(value);
        }
        return Err(AppError::config("stored secret is invalid"));
    }
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = vec![0_u8; length];
    rand::thread_rng().fill_bytes(&mut bytes);
    let value = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    fs::write(path, &value)?;
    Ok(value)
}

pub fn normalize_sqlite_url(path: &str) -> AppResult<String> {
    let path = PathBuf::from_str(path)
        .map_err(|_| AppError::validation("INVALID_DATABASE_PATH", "Invalid SQLite path."))?;
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::validation(
            "INVALID_DATABASE_PATH",
            "SQLite path may not contain parent traversal.",
        ));
    }
    Ok(sqlite_connection_url(&path))
}

fn sqlite_connection_url(path: &std::path::Path) -> String {
    let rendered = path.to_string_lossy().replace('\\', "/");
    let rendered = rendered.strip_prefix("//?/").unwrap_or(&rendered);
    // The opaque `sqlite:` form survives `url::Url` normalization used by SQLx Any,
    // including Windows drive-letter paths.
    format!("sqlite:{}", urlencoding::encode(rendered))
}
