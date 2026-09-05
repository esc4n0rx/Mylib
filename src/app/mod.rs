use std::{
    collections::{HashMap, HashSet},
    fs,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use axum::{
    Router,
    body::Body,
    http::{HeaderName, HeaderValue, Request, header},
    middleware::Next,
    response::Response,
};
use base64::Engine;
use chacha20poly1305::{
    AeadCore, ChaCha20Poly1305, KeyInit,
    aead::{Aead, OsRng},
};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::{AllowOrigin, CorsLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use uuid::Uuid;

use crate::{
    api,
    auth::TokenService,
    config::{Config, PersistedDatabaseConfig},
    db::{Database, DatabaseKind},
    errors::{AppError, AppResult},
    operational::SystemMetricsService,
    playback::PlaybackRuntime,
    profiles::{AvatarCatalog, PinWindow},
    recommendations::RecommendationService,
};

#[derive(Clone)]
pub struct AppState {
    database: Arc<RwLock<Database>>,
    pub config: Arc<Config>,
    pub tokens: TokenService,
    pub started_at: SystemTime,
    pub login_limiter: Arc<Mutex<HashMap<IpAddr, LoginWindow>>>,
    pub scan_cancellations: Arc<Mutex<HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub scanning_libraries: Arc<Mutex<HashSet<String>>>,
    pub scan_slots: Arc<Semaphore>,
    pub metadata_slots: Arc<Semaphore>,
    pub playback: PlaybackRuntime,
    pub metrics: SystemMetricsService,
    pub recommendations: RecommendationService,
    pub avatar_catalog: AvatarCatalog,
    pub profile_pin_limiter: Arc<Mutex<HashMap<String, PinWindow>>>,
    pub remote_http_slots: Arc<Semaphore>,
    pub syncing_sources: Arc<Mutex<HashSet<String>>>,
    pub remote_cache: crate::features::remote_sources::cache::RemoteCache,
    pub google_oauth_pending:
        Arc<Mutex<HashMap<String, crate::features::remote_sources::google_drive::PendingOAuth>>>,
}

#[derive(Clone, Copy)]
pub struct LoginWindow {
    pub started: Instant,
    pub attempts: u8,
}

impl AppState {
    pub async fn initialize(config: Config) -> AppResult<Self> {
        let (kind, url) = resolve_database(&config)?;
        let database = Database::connect(kind, &url).await?;
        database.migrate().await?;
        let tokens = TokenService::new(config.jwt_secret.as_bytes(), config.token_ttl_seconds);
        let remote_http_slots = Arc::new(Semaphore::new(config.remote_http_max_concurrency.max(1)));
        let remote_cache = crate::features::remote_sources::cache::RemoteCache::new(&config)?;
        let scan_slots = Arc::new(Semaphore::new(config.scan_max_concurrent_libraries));
        let metadata_slots = Arc::new(Semaphore::new(config.tmdb_max_concurrency));
        let playback = PlaybackRuntime::new(&config)?;
        let metrics = SystemMetricsService::new(&config);
        let recommendations = RecommendationService::new();
        let avatar_root = config.data_dir.join("avatars");
        for category in ["dp", "nf", "pop", "pp", "pv"] {
            fs::create_dir_all(avatar_root.join(category))?;
        }
        let avatar_catalog = AvatarCatalog::new(avatar_root);
        Ok(Self {
            database: Arc::new(RwLock::new(database)),
            config: Arc::new(config),
            tokens,
            started_at: SystemTime::now(),
            login_limiter: Arc::new(Mutex::new(HashMap::new())),
            scan_cancellations: Arc::new(Mutex::new(HashMap::new())),
            scanning_libraries: Arc::new(Mutex::new(HashSet::new())),
            scan_slots,
            metadata_slots,
            playback,
            metrics,
            recommendations,
            avatar_catalog,
            profile_pin_limiter: Arc::new(Mutex::new(HashMap::new())),
            remote_http_slots,
            syncing_sources: Arc::new(Mutex::new(HashSet::new())),
            remote_cache,
            google_oauth_pending: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    pub async fn database(&self) -> Database {
        self.database.read().await.clone()
    }
    pub async fn replace_database(&self, database: Database) {
        let old = {
            let mut guard = self.database.write().await;
            std::mem::replace(&mut *guard, database)
        };
        old.pool.close().await;
    }
    pub async fn close(&self) {
        self.database().await.pool.close().await;
    }
    pub async fn allow_login(&self, ip: IpAddr) -> bool {
        let mut limiter = self.login_limiter.lock().await;
        let entry = limiter.entry(ip).or_insert(LoginWindow {
            started: Instant::now(),
            attempts: 0,
        });
        if entry.started.elapsed() >= Duration::from_secs(60) {
            *entry = LoginWindow {
                started: Instant::now(),
                attempts: 0,
            };
        }
        if entry.attempts >= 5 {
            false
        } else {
            entry.attempts += 1;
            true
        }
    }
    pub async fn clear_login_failures(&self, ip: IpAddr) {
        self.login_limiter.lock().await.remove(&ip);
    }
}

fn resolve_database(config: &Config) -> AppResult<(DatabaseKind, String)> {
    if config.database_type.is_some() || config.database_url.is_some() {
        let kind = DatabaseKind::parse(config.database_type.as_deref().unwrap_or("sqlite"))?;
        let url = config
            .database_url
            .clone()
            .unwrap_or_else(|| config.sqlite_url());
        return Ok((kind, url));
    }
    if config.persisted_config_path().exists() {
        let persisted: PersistedDatabaseConfig =
            serde_json::from_slice(&fs::read(config.persisted_config_path())?)
                .map_err(|_| AppError::config("invalid persisted database configuration"))?;
        let kind = DatabaseKind::parse(&persisted.database_type)?;
        if kind == DatabaseKind::MySql {
            let encrypted = persisted
                .encrypted_url_file
                .ok_or_else(|| AppError::config("missing encrypted database configuration"))?;
            return Ok((kind, decrypt_secret(config, &encrypted)?));
        }
        return Ok((kind, config.sqlite_url()));
    }
    Ok((DatabaseKind::Sqlite, config.sqlite_url()))
}

pub fn persist_database(config: &Config, kind: DatabaseKind, url: Option<&str>) -> AppResult<()> {
    let encrypted_url_file = if let Some(url) = url {
        Some(encrypt_secret(config, url)?)
    } else {
        None
    };
    let persisted = PersistedDatabaseConfig {
        database_type: kind.as_str().into(),
        encrypted_url_file,
    };
    fs::write(
        config.persisted_config_path(),
        serde_json::to_vec_pretty(&persisted)
            .map_err(|_| AppError::config("unable to encode database configuration"))?,
    )?;
    Ok(())
}

fn encryption_key(config: &Config) -> AppResult<[u8; 32]> {
    let path = config.data_dir.join("secrets/database.key");
    if path.exists() {
        let bytes = fs::read(path)?;
        return bytes
            .try_into()
            .map_err(|_| AppError::config("invalid database encryption key"));
    }
    use rand::RngCore;
    let mut key = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    fs::write(path, key)?;
    Ok(key)
}

fn encrypt_secret(config: &Config, plaintext: &str) -> AppResult<String> {
    let cipher = ChaCha20Poly1305::new((&encryption_key(config)?).into());
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let encrypted = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| AppError::config("unable to encrypt database credentials"))?;
    let mut payload = nonce.to_vec();
    payload.extend(encrypted);
    let filename = "database.enc";
    fs::write(
        config.data_dir.join("secrets").join(filename),
        base64::engine::general_purpose::STANDARD.encode(payload),
    )?;
    Ok(filename.into())
}

fn decrypt_secret(config: &Config, filename: &str) -> AppResult<String> {
    let encoded = fs::read_to_string(config.data_dir.join("secrets").join(filename))?;
    let payload = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| AppError::config("invalid encrypted database configuration"))?;
    if payload.len() < 13 {
        return Err(AppError::config("invalid encrypted database configuration"));
    }
    let (nonce, ciphertext) = payload.split_at(12);
    let cipher = ChaCha20Poly1305::new((&encryption_key(config)?).into());
    let plaintext = cipher
        .decrypt(nonce.into(), ciphertext)
        .map_err(|_| AppError::config("unable to decrypt database configuration"))?;
    String::from_utf8(plaintext)
        .map_err(|_| AppError::config("invalid database configuration encoding"))
}

pub fn build_app(state: AppState) -> AppResult<Router> {
    let origins: Vec<HeaderValue> = state
        .config
        .allowed_origins
        .iter()
        .map(|origin| {
            origin
                .parse()
                .map_err(|_| AppError::config("invalid MYLIB_ALLOWED_ORIGINS"))
        })
        .collect::<AppResult<_>>()?;
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-request-id"),
            HeaderName::from_static("x-library-unlock"),
        ]);
    Ok(api::router()
        .fallback(crate::web_assets::serve)
        .with_state(state)
        .layer(CatchPanicLayer::new())
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(request_id)))
}

async fn request_id(mut request: Request<Body>, next: Next) -> Response {
    let id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| Uuid::parse_str(v).is_ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    request.extensions_mut().insert(id.clone());
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}
