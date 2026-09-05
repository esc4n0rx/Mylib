//! Google Drive remote source: OAuth (authorization code + PKCE), folder
//! browsing, metadata-only scanning and authenticated stream proxying. All
//! tokens live sealed on the server and never reach the client.

use std::{collections::HashMap, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthUser,
    db::{Database, now},
    errors::{AppError, AppResult},
    infrastructure::secrets,
};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
const DRIVE_FILES: &str = "https://www.googleapis.com/drive/v3/files";
const DRIVE_ABOUT: &str = "https://www.googleapis.com/drive/v3/about";
pub const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "m4v", "avi", "mov", "ts", "m2ts", "webm"];

/// Server-side half of an in-flight OAuth exchange. Expires quickly.
#[derive(Clone)]
pub struct PendingOAuth {
    pub user_id: String,
    pub pkce_verifier: String,
    pub created_at: std::time::Instant,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/remote-sources/google-drive/connect", post(connect))
        .route(
            "/api/v1/remote-sources/google-drive/callback",
            get(callback),
        )
        .route(
            "/api/v1/remote-sources/google-drive/connections",
            get(list_connections),
        )
        .route(
            "/api/v1/remote-sources/google-drive/connections/{id}",
            axum::routing::delete(disconnect),
        )
        .route(
            "/api/v1/remote-sources/google-drive/{connection_id}/browse",
            get(browse),
        )
}

fn oauth_config(state: &AppState) -> AppResult<(String, String, String)> {
    let client_id = state
        .config
        .google_oauth_client_id
        .clone()
        .ok_or_else(oauth_not_configured)?;
    let client_secret = state
        .config
        .google_oauth_client_secret
        .clone()
        .ok_or_else(oauth_not_configured)?;
    let redirect = state
        .config
        .google_oauth_redirect_url
        .clone()
        .ok_or_else(oauth_not_configured)?;
    Ok((client_id, client_secret, redirect))
}

fn oauth_not_configured() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "GOOGLE_OAUTH_NOT_CONFIGURED",
        "Google Drive integration is not configured on this server.",
    )
}

fn random_token(bytes: usize) -> String {
    use rand::RngCore;
    let mut buffer = vec![0_u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

fn code_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn connect(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    let (client_id, _secret, redirect) = oauth_config(&state)?;
    let verifier = random_token(48);
    let csrf_state = random_token(24);
    prune_pending(&state).await;
    state.google_oauth_pending.lock().await.insert(
        csrf_state.clone(),
        PendingOAuth {
            user_id: auth.id.clone(),
            pkce_verifier: verifier.clone(),
            created_at: std::time::Instant::now(),
        },
    );
    let authorization_url = format!(
        "{AUTH_ENDPOINT}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent&include_granted_scopes=true",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect),
        urlencoding::encode(SCOPE),
        urlencoding::encode(&csrf_state),
        urlencoding::encode(&code_challenge(&verifier)),
    );
    Ok(Json(json!({ "authorizationUrl": authorization_url })))
}

async fn prune_pending(state: &AppState) {
    state
        .google_oauth_pending
        .lock()
        .await
        .retain(|_, pending| pending.created_at.elapsed() < Duration::from_secs(600));
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> AppResult<Html<String>> {
    if let Some(error) = query.error {
        return Ok(bridge_page(&format!("google-drive-error:{error}")));
    }
    let csrf_state = query
        .state
        .ok_or_else(|| AppError::validation("INVALID_OAUTH_STATE", "Missing OAuth state."))?;
    let code = query
        .code
        .ok_or_else(|| AppError::validation("INVALID_OAUTH_CODE", "Missing authorization code."))?;
    prune_pending(&state).await;
    let pending = state
        .google_oauth_pending
        .lock()
        .await
        .remove(&csrf_state)
        .ok_or_else(|| {
            AppError::validation(
                "INVALID_OAUTH_STATE",
                "The OAuth state is unknown or expired.",
            )
        })?;

    let (client_id, client_secret, redirect) = oauth_config(&state)?;
    let tokens = exchange_code(
        &client_id,
        &client_secret,
        &redirect,
        &code,
        &pending.pkce_verifier,
    )
    .await?;
    let account = drive_account(&tokens.access_token).await?;
    let db = state.database().await;
    upsert_connection(&state, &db, &pending.user_id, &account, &tokens).await?;
    let _ = db
        .audit(
            Some(&pending.user_id),
            "GOOGLE_DRIVE_CONNECTED",
            "google_drive_connection",
            None,
            json!({ "accountEmail": account.email }),
            None,
        )
        .await;
    Ok(bridge_page("google-drive-connected"))
}

fn bridge_page(message: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>MyLib</title><script>try{{window.opener&&window.opener.postMessage({{source:'mylib',event:{message:?}}},'*');}}catch(e){{}}window.close();</script><p>Você já pode fechar esta janela.</p>"
    ))
}

struct OAuthTokens {
    access_token: String,
    refresh_token: Option<String>,
    expiry: String,
}

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("MyLib/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| AppError::config("Unable to initialize HTTP client."))
}

async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    redirect: &str,
    code: &str,
    verifier: &str,
) -> AppResult<OAuthTokens> {
    let response = http_client()?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect),
        ])
        .send()
        .await
        .map_err(|_| google_unavailable())?;
    parse_token_response(response).await
}

async fn refresh_access_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> AppResult<OAuthTokens> {
    let response = http_client()?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|_| google_unavailable())?;
    parse_token_response(response).await
}

async fn parse_token_response(response: reqwest::Response) -> AppResult<OAuthTokens> {
    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::BAD_REQUEST
    {
        return Err(AppError::new(
            StatusCode::BAD_GATEWAY,
            "GOOGLE_AUTH_REJECTED",
            "Google rejected the authorization.",
        ));
    }
    if !response.status().is_success() {
        return Err(google_unavailable());
    }
    let body: Value = response.json().await.map_err(|_| google_unavailable())?;
    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(google_unavailable)?
        .to_owned();
    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
    Ok(OAuthTokens {
        access_token,
        refresh_token: body["refresh_token"].as_str().map(str::to_owned),
        expiry: (chrono::Utc::now() + chrono::Duration::seconds(expires_in.max(60) - 30))
            .to_rfc3339(),
    })
}

fn google_unavailable() -> AppError {
    AppError::new(
        StatusCode::BAD_GATEWAY,
        "GOOGLE_DRIVE_UNAVAILABLE",
        "Google Drive is currently unavailable.",
    )
}

struct DriveAccount {
    id: String,
    email: String,
}

async fn drive_account(access_token: &str) -> AppResult<DriveAccount> {
    let body: Value = http_client()?
        .get(format!("{DRIVE_ABOUT}?fields=user"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| google_unavailable())?
        .json()
        .await
        .map_err(|_| google_unavailable())?;
    Ok(DriveAccount {
        id: body["user"]["permissionId"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        email: body["user"]["emailAddress"]
            .as_str()
            .unwrap_or("conta@google")
            .to_owned(),
    })
}

async fn upsert_connection(
    state: &AppState,
    db: &Database,
    user_id: &str,
    account: &DriveAccount,
    tokens: &OAuthTokens,
) -> AppResult<String> {
    let sealed = secrets::seal(
        &state.config,
        &json!({
            "accessToken": tokens.access_token,
            "refreshToken": tokens.refresh_token,
            "expiry": tokens.expiry,
        })
        .to_string(),
    )?;
    let timestamp = now();
    if let Some(row) = sqlx::query(
        "SELECT id FROM google_drive_connections WHERE owner_user_id=? AND account_email=?",
    )
    .bind(user_id)
    .bind(&account.email)
    .fetch_optional(&db.pool)
    .await?
    {
        let id: String = row.get("id");
        sqlx::query("UPDATE google_drive_connections SET credentials_ref=?,status='CONNECTED',last_refresh_at=?,last_error=NULL,updated_at=? WHERE id=?")
            .bind(&sealed)
            .bind(&timestamp)
            .bind(&timestamp)
            .bind(&id)
            .execute(&db.pool)
            .await?;
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO google_drive_connections (id,owner_user_id,account_email,account_id,credentials_ref,scopes,status,last_refresh_at,created_at,updated_at) VALUES (?,?,?,?,?,?,'CONNECTED',?,?,?)")
        .bind(&id)
        .bind(user_id)
        .bind(&account.email)
        .bind(&account.id)
        .bind(&sealed)
        .bind(SCOPE)
        .bind(&timestamp)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&db.pool)
        .await?;
    Ok(id)
}

async fn list_connections(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    let rows = sqlx::query("SELECT id,account_email,account_id,status,last_refresh_at,last_error,created_at FROM google_drive_connections ORDER BY created_at")
        .fetch_all(&db.pool)
        .await?;
    Ok(Json(json!({
        "items": rows.iter().map(|row| json!({
            "id": row.get::<String, _>("id"),
            "accountEmail": row.get::<String, _>("account_email"),
            "status": row.get::<String, _>("status"),
            "lastRefreshAt": row.try_get::<Option<String>, _>("last_refresh_at").ok().flatten(),
            "lastError": row.try_get::<Option<String>, _>("last_error").ok().flatten(),
            "createdAt": row.get::<String, _>("created_at"),
        })).collect::<Vec<_>>()
    })))
}

async fn disconnect(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    let result = sqlx::query("DELETE FROM google_drive_connections WHERE id=?")
        .bind(&id)
        .execute(&db.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found(
            "GOOGLE_DRIVE_CONNECTION_NOT_FOUND",
            "Connection was not found.",
        ));
    }
    let _ = db
        .audit(
            Some(&auth.id),
            "GOOGLE_DRIVE_DISCONNECTED",
            "google_drive_connection",
            Some(&id),
            json!({}),
            None,
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Returns a valid access token for `connection_id`, refreshing and re-sealing
/// when the stored token is stale.
pub async fn access_token(
    state: &AppState,
    db: &Database,
    connection_id: &str,
) -> AppResult<String> {
    let row = sqlx::query("SELECT credentials_ref FROM google_drive_connections WHERE id=?")
        .bind(connection_id)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| {
            AppError::not_found(
                "GOOGLE_DRIVE_CONNECTION_NOT_FOUND",
                "Connection was not found.",
            )
        })?;
    let stored: Value = serde_json::from_str(&secrets::open(
        &state.config,
        &row.get::<String, _>("credentials_ref"),
    )?)
    .map_err(|_| AppError::config("Stored Google credentials are invalid."))?;
    let expiry = stored["expiry"].as_str().unwrap_or_default();
    let fresh = chrono::DateTime::parse_from_rfc3339(expiry)
        .map(|time| time > chrono::Utc::now())
        .unwrap_or(false);
    if fresh && let Some(token) = stored["accessToken"].as_str() {
        return Ok(token.to_owned());
    }
    let refresh_token = stored["refreshToken"].as_str().ok_or_else(|| {
        AppError::new(
            StatusCode::BAD_GATEWAY,
            "GOOGLE_AUTH_REQUIRED",
            "Reconnect the Google Drive account.",
        )
    })?;
    let (client_id, client_secret, _redirect) = oauth_config(state)?;
    let refreshed = match refresh_access_token(&client_id, &client_secret, refresh_token).await {
        Ok(tokens) => tokens,
        Err(error) => {
            sqlx::query("UPDATE google_drive_connections SET status='AUTH_REQUIRED',last_error=?,updated_at=? WHERE id=?")
                .bind(error.message.clone())
                .bind(now())
                .bind(connection_id)
                .execute(&db.pool)
                .await?;
            return Err(error);
        }
    };
    let sealed = secrets::seal(
        &state.config,
        &json!({
            "accessToken": refreshed.access_token,
            "refreshToken": refreshed.refresh_token.or_else(|| Some(refresh_token.to_owned())),
            "expiry": refreshed.expiry,
        })
        .to_string(),
    )?;
    sqlx::query("UPDATE google_drive_connections SET credentials_ref=?,status='CONNECTED',last_refresh_at=?,last_error=NULL,updated_at=? WHERE id=?")
        .bind(&sealed)
        .bind(now())
        .bind(now())
        .bind(connection_id)
        .execute(&db.pool)
        .await?;
    Ok(refreshed.access_token)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowseQuery {
    folder_id: Option<String>,
    page_token: Option<String>,
    #[serde(default = "default_page_size")]
    page_size: i64,
}

fn default_page_size() -> i64 {
    100
}

async fn browse(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(connection_id): Path<String>,
    Query(query): Query<BrowseQuery>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    let token = access_token(&state, &db, &connection_id).await?;
    let folder = query.folder_id.as_deref().unwrap_or("root");
    let page_size = query.page_size.clamp(10, 1000);
    let _permit = state.remote_http_slots.acquire().await.ok();
    let mut request = http_client()?.get(DRIVE_FILES).bearer_auth(&token).query(&[
        (
            "q",
            format!(
                "'{}' in parents and trashed=false",
                folder.replace('\'', "")
            ),
        ),
        (
            "fields",
            "nextPageToken,files(id,name,mimeType,size,modifiedTime)".to_owned(),
        ),
        ("pageSize", page_size.to_string()),
        ("orderBy", "folder,name".to_owned()),
        ("supportsAllDrives", "true".to_owned()),
        ("includeItemsFromAllDrives", "true".to_owned()),
    ]);
    if let Some(page_token) = &query.page_token {
        request = request.query(&[("pageToken", page_token)]);
    }
    let body: Value = request
        .send()
        .await
        .map_err(|_| google_unavailable())?
        .json()
        .await
        .map_err(|_| google_unavailable())?;
    let items = body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| {
            let mime = file["mimeType"].as_str().unwrap_or_default();
            json!({
                "id": file["id"].as_str().unwrap_or_default(),
                "name": file["name"].as_str().unwrap_or_default(),
                "mimeType": mime,
                "isFolder": mime == "application/vnd.google-apps.folder",
                "size": file["size"].as_str().and_then(|value| value.parse::<i64>().ok()),
                "modifiedTime": file["modifiedTime"].as_str(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "items": items,
        "nextPageToken": body["nextPageToken"].as_str(),
    })))
}

/// Resolves a `drive:<connectionId>:<fileId>` reference to an authenticated
/// download request the playback proxy can forward.
pub async fn resolve_stream(
    state: &AppState,
    db: &Database,
    reference: &str,
) -> AppResult<(String, String)> {
    let rest = reference
        .strip_prefix("drive:")
        .ok_or_else(|| AppError::config("Invalid Google Drive stream reference."))?;
    let (connection_id, file_id) = rest
        .split_once(':')
        .ok_or_else(|| AppError::config("Invalid Google Drive stream reference."))?;
    let token = access_token(state, db, connection_id).await?;
    Ok((
        format!("{DRIVE_FILES}/{file_id}?alt=media&supportsAllDrives=true"),
        token,
    ))
}

pub fn stream_reference(connection_id: &str, file_id: &str) -> String {
    format!("drive:{connection_id}:{file_id}")
}

/// Lists every supported video file beneath the given folders, keyed by file id.
/// Metadata only — no downloads.
pub async fn list_video_files(
    state: &AppState,
    db: &Database,
    connection_id: &str,
    folders: &[(String, String)],
) -> AppResult<HashMap<String, DriveFile>> {
    let token = access_token(state, db, connection_id).await?;
    let mut found: HashMap<String, DriveFile> = HashMap::new();
    let mut queue: Vec<(String, String)> = folders.to_vec();
    let mut visited = std::collections::HashSet::new();
    while let Some((folder_id, path_prefix)) = queue.pop() {
        if !visited.insert(folder_id.clone()) {
            continue;
        }
        let mut page_token: Option<String> = None;
        loop {
            let _permit = state.remote_http_slots.acquire().await.ok();
            let mut request = http_client()?.get(DRIVE_FILES).bearer_auth(&token).query(&[
                (
                    "q",
                    format!(
                        "'{}' in parents and trashed=false",
                        folder_id.replace('\'', "")
                    ),
                ),
                (
                    "fields",
                    "nextPageToken,files(id,name,mimeType,size,modifiedTime,parents)".to_owned(),
                ),
                ("pageSize", "1000".to_owned()),
                ("supportsAllDrives", "true".to_owned()),
                ("includeItemsFromAllDrives", "true".to_owned()),
            ]);
            if let Some(token) = &page_token {
                request = request.query(&[("pageToken", token)]);
            }
            let body: Value = request
                .send()
                .await
                .map_err(|_| google_unavailable())?
                .json()
                .await
                .map_err(|_| google_unavailable())?;
            for file in body["files"].as_array().into_iter().flatten() {
                let id = file["id"].as_str().unwrap_or_default().to_owned();
                let name = file["name"].as_str().unwrap_or_default().to_owned();
                let mime = file["mimeType"].as_str().unwrap_or_default();
                if mime == "application/vnd.google-apps.folder" {
                    queue.push((id, format!("{path_prefix}/{name}")));
                    continue;
                }
                let extension = name
                    .rsplit_once('.')
                    .map(|(_, ext)| ext.to_ascii_lowercase())
                    .unwrap_or_default();
                if !VIDEO_EXTENSIONS.contains(&extension.as_str()) {
                    continue;
                }
                found.insert(
                    id.clone(),
                    DriveFile {
                        id,
                        name,
                        relative_path: path_prefix.trim_start_matches('/').to_owned(),
                        size: file["size"].as_str().and_then(|value| value.parse().ok()),
                        modified_time: file["modifiedTime"].as_str().unwrap_or_default().to_owned(),
                    },
                );
            }
            match body["nextPageToken"].as_str() {
                Some(next) => page_token = Some(next.to_owned()),
                None => break,
            }
        }
    }
    Ok(found)
}

#[derive(Debug, Clone)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub size: Option<i64>,
    pub modified_time: String,
}

/// Synchronizes a Google Drive source: list metadata, diff against
/// `google_drive_files`, catalog the new/changed files through the shared
/// resolver. Never downloads media.
pub async fn sync_inner(state: &AppState, source_id: &str) -> AppResult<super::sync::SyncOutcome> {
    use std::path::Path as FsPath;

    use crate::{
        features::catalog::{api::refresh_library_stats, resolver::identify_pending},
        libraries::LibraryType,
        scanner::{ParsedName, parse_media_path},
    };

    let started = std::time::Instant::now();
    let db = state.database().await;
    let source = super::api::source_row(&db, source_id).await?;
    let library_id: String = source.get("library_id");
    let config: Value =
        serde_json::from_str(&source.get::<String, _>("config")).unwrap_or(json!({}));
    let connection_id = config["connectionId"].as_str().ok_or_else(|| {
        AppError::validation(
            "REMOTE_SOURCE_CONNECTION_REQUIRED",
            "This Google Drive source has no linked connection.",
        )
    })?;

    let library = sqlx::query(
        "SELECT library_type,metadata_language,metadata_region FROM libraries WHERE id=? AND deleted_at IS NULL",
    )
    .bind(&library_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("LIBRARY_NOT_FOUND", "Library was not found."))?;
    let kind = LibraryType::parse(&library.get::<String, _>("library_type"))?;
    let language: String = library.get("metadata_language");
    let region: Option<String> = library.try_get("metadata_region").ok();

    let folder_rows =
        sqlx::query("SELECT folder_id,display_name FROM google_drive_folders WHERE source_id=?")
            .bind(source_id)
            .fetch_all(&db.pool)
            .await?;
    let folders: Vec<(String, String)> = folder_rows
        .iter()
        .map(|row| {
            (
                row.get::<String, _>("folder_id"),
                row.get::<String, _>("display_name"),
            )
        })
        .collect();

    sqlx::query(
        "UPDATE remote_sources SET status='SYNCING',last_sync_at=?,updated_at=? WHERE id=?",
    )
    .bind(now())
    .bind(now())
    .bind(source_id)
    .execute(&db.pool)
    .await?;

    let files = list_video_files(state, &db, connection_id, &folders).await?;
    let path_id = super::sync::ensure_remote_path(&db, &library_id, source_id).await?;
    let started_at = now();
    let provider = crate::features::catalog::api::provider(state)?;
    let (_cancel_tx, cancel) = tokio::sync::watch::channel(false);
    let mut outcome = super::sync::SyncOutcome::default();
    let mut pending: Vec<(String, ParsedName)> = Vec::new();

    for file in files.values() {
        outcome.scanned += 1;
        let timestamp = now();
        let existing = sqlx::query(
            "SELECT id,modified_time FROM google_drive_files WHERE source_id=? AND file_id=?",
        )
        .bind(source_id)
        .bind(&file.id)
        .fetch_optional(&db.pool)
        .await?;
        let changed = match &existing {
            Some(row) => row.get::<String, _>("modified_time") != file.modified_time,
            None => true,
        };
        match &existing {
            Some(row) => {
                sqlx::query("UPDATE google_drive_files SET name=?,size=?,modified_time=?,parents=?,missing_since=NULL,last_seen_at=?,updated_at=? WHERE id=?")
                    .bind(&file.name)
                    .bind(file.size)
                    .bind(&file.modified_time)
                    .bind(&file.relative_path)
                    .bind(&timestamp)
                    .bind(&timestamp)
                    .bind(row.get::<String, _>("id"))
                    .execute(&db.pool)
                    .await?;
                if changed {
                    outcome.updated += 1;
                } else {
                    outcome.unchanged += 1;
                }
            }
            None => {
                sqlx::query("INSERT INTO google_drive_files (id,source_id,file_id,name,size,mime_type,modified_time,parents,last_seen_at,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
                    .bind(Uuid::new_v4().to_string())
                    .bind(source_id)
                    .bind(&file.id)
                    .bind(&file.name)
                    .bind(file.size)
                    .bind(Option::<String>::None)
                    .bind(&file.modified_time)
                    .bind(&file.relative_path)
                    .bind(&timestamp)
                    .bind(&timestamp)
                    .bind(&timestamp)
                    .execute(&db.pool)
                    .await?;
                outcome.new += 1;
            }
        }

        let relative = if file.relative_path.is_empty() {
            file.name.clone()
        } else {
            format!("{}/{}", file.relative_path, file.name)
        };
        let parsed = parse_media_path(&file.name, FsPath::new(&relative), kind);
        let file_id = upsert_drive_media(
            &db,
            state,
            source_id,
            &library_id,
            &path_id,
            connection_id,
            kind,
            file,
            &parsed,
            &timestamp,
        )
        .await?;
        sqlx::query(
            "UPDATE google_drive_files SET media_file_id=? WHERE source_id=? AND file_id=?",
        )
        .bind(&file_id)
        .bind(source_id)
        .bind(&file.id)
        .execute(&db.pool)
        .await
        .ok();
        if changed {
            pending.push((file_id, parsed));
        }
    }

    if !pending.is_empty() {
        let (matched, unmatched, _failed) = identify_pending(
            &db,
            &provider,
            &library_id,
            kind,
            &language,
            region.as_deref(),
            &pending,
            &cancel,
        )
        .await?;
        outcome.matched += matched as u64;
        outcome.unmatched += unmatched as u64;
        sqlx::query("UPDATE remote_media_sources SET media_item_id=(SELECT media_item_id FROM media_files WHERE media_files.remote_media_source_id=remote_media_sources.id),episode_id=(SELECT tv_episode_id FROM media_files WHERE media_files.remote_media_source_id=remote_media_sources.id),updated_at=? WHERE remote_source_id=?")
            .bind(now())
            .bind(source_id)
            .execute(&db.pool)
            .await?;
    }

    let missing = sqlx::query("UPDATE google_drive_files SET missing_since=COALESCE(missing_since,?),updated_at=? WHERE source_id=? AND last_seen_at<?")
        .bind(&started_at)
        .bind(now())
        .bind(source_id)
        .bind(&started_at)
        .execute(&db.pool)
        .await?
        .rows_affected();
    outcome.missing = missing;
    sqlx::query("UPDATE media_files SET scan_status='MISSING',missing_since=COALESCE(missing_since,?),updated_at=? WHERE remote_media_source_id IN (SELECT id FROM remote_media_sources WHERE remote_source_id=? AND last_seen_at<?)")
        .bind(&started_at)
        .bind(now())
        .bind(source_id)
        .bind(&started_at)
        .execute(&db.pool)
        .await?;
    sqlx::query("UPDATE remote_media_sources SET is_active=0,updated_at=? WHERE remote_source_id=? AND last_seen_at<?")
        .bind(now())
        .bind(source_id)
        .bind(&started_at)
        .execute(&db.pool)
        .await?;

    refresh_library_stats(&db, &library_id).await?;
    state.recommendations.invalidate_all().await;
    outcome.duration_ms = started.elapsed().as_millis();
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
async fn upsert_drive_media(
    db: &Database,
    state: &AppState,
    source_id: &str,
    library_id: &str,
    path_id: &str,
    connection_id: &str,
    kind: crate::libraries::LibraryType,
    file: &DriveFile,
    parsed: &crate::scanner::ParsedName,
    timestamp: &str,
) -> AppResult<String> {
    let sealed = secrets::seal(&state.config, &stream_reference(connection_id, &file.id))?;
    let rms_id = match sqlx::query(
        "SELECT id FROM remote_media_sources WHERE remote_source_id=? AND external_key=?",
    )
    .bind(source_id)
    .bind(&file.id)
    .fetch_optional(&db.pool)
    .await?
    {
        Some(row) => {
            let id: String = row.get("id");
            sqlx::query("UPDATE remote_media_sources SET stream_ref=?,stream_sealed=1,is_active=1,last_seen_at=?,updated_at=? WHERE id=?")
                .bind(&sealed)
                .bind(timestamp)
                .bind(timestamp)
                .bind(&id)
                .execute(&db.pool)
                .await?;
            id
        }
        None => {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO remote_media_sources (id,remote_source_id,provider_type,external_key,stream_ref,stream_sealed,is_active,last_seen_at,created_at,updated_at) VALUES (?,?,?,?,?,1,1,?,?,?)")
                .bind(&id)
                .bind(source_id)
                .bind("GOOGLE_DRIVE")
                .bind(&file.id)
                .bind(&sealed)
                .bind(timestamp)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&db.pool)
                .await?;
            id
        }
    };

    let absolute_path = format!("gdrive://{source_id}/{}", file.id);
    let extension = file
        .name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let existing = sqlx::query(
        "SELECT id,identification_status FROM media_files WHERE library_id=? AND absolute_path=?",
    )
    .bind(library_id)
    .bind(&absolute_path)
    .fetch_optional(&db.pool)
    .await?;
    match existing {
        Some(row) => {
            let id: String = row.get("id");
            let status: String = row.get("identification_status");
            let next_status = if matches!(status.as_str(), "MATCHED_AUTO" | "MATCHED_MANUAL") {
                status
            } else {
                "PENDING".to_string()
            };
            sqlx::query("UPDATE media_files SET library_path_id=?,filename=?,extension=?,file_size=?,content_type=?,scan_status='PRESENT',identification_status=?,normalized_title=?,parsed_year=?,parsed_season=?,parsed_episode=?,storage_kind='REMOTE',remote_media_source_id=?,missing_since=NULL,last_seen_at=?,updated_at=? WHERE id=?")
                .bind(path_id)
                .bind(&file.name)
                .bind(&extension)
                .bind(file.size.unwrap_or(0))
                .bind(kind.as_str())
                .bind(&next_status)
                .bind(&parsed.title)
                .bind(parsed.year)
                .bind(parsed.season)
                .bind(parsed.episodes.first().copied())
                .bind(&rms_id)
                .bind(timestamp)
                .bind(timestamp)
                .bind(&id)
                .execute(&db.pool)
                .await?;
            Ok(id)
        }
        None => {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO media_files (id,library_id,library_path_id,absolute_path,relative_path,filename,extension,file_size,modified_at,content_type,scan_status,identification_status,normalized_title,parsed_year,parsed_season,parsed_episode,storage_kind,remote_media_source_id,created_at,updated_at,last_seen_at) VALUES (?,?,?,?,?,?,?,?,?,?,'PRESENT','PENDING',?,?,?,?,'REMOTE',?,?,?,?)")
                .bind(&id)
                .bind(library_id)
                .bind(path_id)
                .bind(&absolute_path)
                .bind(&file.id)
                .bind(&file.name)
                .bind(&extension)
                .bind(file.size.unwrap_or(0))
                .bind(&file.modified_time)
                .bind(kind.as_str())
                .bind(&parsed.title)
                .bind(parsed.year)
                .bind(parsed.season)
                .bind(parsed.episodes.first().copied())
                .bind(&rms_id)
                .bind(timestamp)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&db.pool)
                .await?;
            Ok(id)
        }
    }
}
