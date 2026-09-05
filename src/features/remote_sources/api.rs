use std::path::PathBuf;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::io::BufReader;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthUser,
    config::Config,
    db::{Database, now},
    errors::{AppError, AppResult},
    infrastructure::secrets,
};

use super::{
    m3u,
    models::{
        CreateRemoteSourceRequest, ProviderType, RemoteAutoSyncRequest, UpdateRemoteSourceRequest,
        validate_name,
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/libraries/{id}/remote-sources",
            get(list_sources).post(create_source),
        )
        .route(
            "/api/v1/remote-sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/api/v1/remote-sources/{id}/status", get(source_status))
        .route(
            "/api/v1/remote-sources/{id}/selections",
            get(get_selections).put(put_selections),
        )
        .route("/api/v1/remote-sources/{id}/sync", post(trigger_sync))
        .route("/api/v1/remote-sources/{id}/entries", get(list_entries))
        .route(
            "/api/v1/remote-sources/m3u/upload",
            // Playlists routinely exceed axum's 2 MB default; the handler still
            // enforces MYLIB_M3U_MAX_BYTES against the buffered body.
            post(upload_m3u).layer(DefaultBodyLimit::max(M3U_UPLOAD_LIMIT_BYTES)),
        )
        .route("/api/v1/remote-sources/m3u/preview", post(preview_m3u))
}

/// Hard ceiling for a buffered M3U upload (the file is held in memory before the
/// structural check). Streamed URL fetches use the larger `m3u_max_bytes`.
const M3U_UPLOAD_LIMIT_BYTES: usize = 256 * 1024 * 1024;

pub(crate) fn m3u_dir(config: &Config) -> PathBuf {
    config.data_dir.join("remote/m3u")
}
fn m3u_upload_dir(config: &Config) -> PathBuf {
    m3u_dir(config).join("uploads")
}
fn safe_component(value: &str) -> AppResult<&str> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(AppError::validation(
            "INVALID_UPLOAD_ID",
            "Invalid upload id.",
        ));
    }
    Ok(value)
}

const SOURCE_SELECT: &str = "SELECT id,library_id,provider_type,name,is_active,config,status,auto_sync_enabled,auto_sync_interval_minutes,last_sync_at,last_successful_sync_at,next_sync_at,last_error,last_error_at,created_at,updated_at FROM remote_sources";

async fn list_sources(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(library_id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    ensure_library(&db, &library_id).await?;
    let rows = sqlx::query(&format!(
        "{SOURCE_SELECT} WHERE library_id=? ORDER BY created_at"
    ))
    .bind(&library_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(Json(
        json!({ "items": rows.iter().map(source_json).collect::<Vec<_>>() }),
    ))
}

async fn create_source(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(library_id): Path<String>,
    Json(payload): Json<CreateRemoteSourceRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    ensure_library(&db, &library_id).await?;
    validate_name(&payload.name)?;
    let provider = ProviderType::parse(&payload.provider_type)?;
    let id = Uuid::new_v4().to_string();
    let mut drive_link: Option<(String, Vec<Value>)> = None;

    let (config, secret_ref) = match provider {
        ProviderType::M3uUrl => {
            let url = payload
                .config
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AppError::validation("REMOTE_SOURCE_URL_REQUIRED", "An M3U URL is required.")
                })?;
            validate_m3u_url(url)?;
            let secret = secrets::seal(&state.config, &json!({ "url": url }).to_string())?;
            (json!({ "url": super::sanitize_url(url) }), Some(secret))
        }
        ProviderType::M3uFile => {
            let upload_id = payload
                .config
                .get("uploadId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::validation(
                        "REMOTE_SOURCE_UPLOAD_REQUIRED",
                        "Upload an M3U file before creating this source.",
                    )
                })?;
            let file_name = adopt_upload(&state.config, upload_id, &id).await?;
            (json!({ "storedFile": file_name }), None)
        }
        ProviderType::GoogleDrive => {
            let connection_id = payload
                .config
                .get("connectionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::validation(
                        "REMOTE_SOURCE_CONNECTION_REQUIRED",
                        "Connect a Google Drive account before creating this source.",
                    )
                })?;
            let exists: i64 =
                sqlx::query("SELECT COUNT(*) FROM google_drive_connections WHERE id=?")
                    .bind(connection_id)
                    .fetch_one(&db.pool)
                    .await?
                    .get(0);
            if exists == 0 {
                return Err(AppError::validation(
                    "GOOGLE_DRIVE_CONNECTION_NOT_FOUND",
                    "The Google Drive connection was not found.",
                ));
            }
            let folders: Vec<Value> = payload
                .config
                .get("folders")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if folders.is_empty() {
                return Err(AppError::validation(
                    "REMOTE_SOURCE_FOLDERS_REQUIRED",
                    "Select at least one Google Drive folder.",
                ));
            }
            drive_link = Some((connection_id.to_owned(), folders.clone()));
            (
                json!({ "connectionId": connection_id, "folderCount": folders.len() }),
                None,
            )
        }
    };

    let auto_sync = payload.auto_sync.unwrap_or(RemoteAutoSyncRequest {
        enabled: true,
        interval_minutes: None,
    });
    let interval = auto_sync.interval_minutes.unwrap_or(720).clamp(5, 43_200);
    let timestamp = now();
    let next_sync_at = auto_sync.enabled.then(|| timestamp.clone());

    let mut tx = db.pool.begin().await?;
    sqlx::query("INSERT INTO remote_sources (id,library_id,provider_type,name,is_active,config,status,auto_sync_enabled,auto_sync_interval_minutes,next_sync_at,created_by,created_at,updated_at) VALUES (?,?,?,?,1,?,'READY',?,?,?,?,?,?)")
        .bind(&id)
        .bind(&library_id)
        .bind(provider.as_str())
        .bind(payload.name.trim())
        .bind(config.to_string())
        .bind(i64::from(auto_sync.enabled))
        .bind(interval)
        .bind(&next_sync_at)
        .bind(&auth.id)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *tx)
        .await?;
    if let Some(secret) = secret_ref {
        sqlx::query(
            "INSERT INTO remote_source_secrets (source_id,secret_ref,updated_at) VALUES (?,?,?)",
        )
        .bind(&id)
        .bind(secret)
        .bind(&timestamp)
        .execute(&mut *tx)
        .await?;
    }
    if let Some((connection_id, folders)) = drive_link {
        sqlx::query(
            "INSERT INTO google_drive_source_connections (source_id,connection_id) VALUES (?,?)",
        )
        .bind(&id)
        .bind(&connection_id)
        .execute(&mut *tx)
        .await?;
        for folder in folders {
            let folder_id = folder
                .get("folderId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AppError::validation("INVALID_FOLDER", "Each folder needs a folderId.")
                })?;
            let display_name = folder
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(folder_id);
            sqlx::query("INSERT INTO google_drive_folders (id,source_id,folder_id,display_name,created_at) VALUES (?,?,?,?,?)")
                .bind(Uuid::new_v4().to_string())
                .bind(&id)
                .bind(folder_id)
                .bind(display_name)
                .bind(&timestamp)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    db.audit(
        Some(&auth.id),
        "REMOTE_SOURCE_CREATED",
        "remote_source",
        Some(&id),
        json!({"libraryId": library_id, "providerType": provider.as_str(), "name": payload.name.trim()}),
        None,
    )
    .await?;

    let row = source_row(&db, &id).await?;
    Ok((StatusCode::CREATED, Json(source_json(&row))))
}

async fn get_source(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    Ok(Json(source_json(&source_row(&db, &id).await?)))
}

async fn update_source(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateRemoteSourceRequest>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    let row = source_row(&db, &id).await?;
    let provider = ProviderType::parse(&row.get::<String, _>("provider_type"))?;
    let timestamp = now();

    if let Some(name) = &payload.name {
        validate_name(name)?;
        sqlx::query("UPDATE remote_sources SET name=?,updated_at=? WHERE id=?")
            .bind(name.trim())
            .bind(&timestamp)
            .bind(&id)
            .execute(&db.pool)
            .await?;
    }
    if let Some(active) = payload.is_active {
        let status = if active { "READY" } else { "DISABLED" };
        sqlx::query("UPDATE remote_sources SET is_active=?,status=?,updated_at=? WHERE id=?")
            .bind(i64::from(active))
            .bind(status)
            .bind(&timestamp)
            .bind(&id)
            .execute(&db.pool)
            .await?;
    }
    if let Some(config) = &payload.config
        && provider == ProviderType::M3uUrl
    {
        let url = config
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::validation("REMOTE_SOURCE_URL_REQUIRED", "An M3U URL is required.")
            })?;
        validate_m3u_url(url)?;
        let secret = secrets::seal(&state.config, &json!({ "url": url }).to_string())?;
        let stored = json!({ "url": super::sanitize_url(url) }).to_string();
        sqlx::query("UPDATE remote_sources SET config=?,updated_at=? WHERE id=?")
            .bind(&stored)
            .bind(&timestamp)
            .bind(&id)
            .execute(&db.pool)
            .await?;
        let updated = sqlx::query(
            "UPDATE remote_source_secrets SET secret_ref=?,updated_at=? WHERE source_id=?",
        )
        .bind(&secret)
        .bind(&timestamp)
        .bind(&id)
        .execute(&db.pool)
        .await?
        .rows_affected();
        if updated == 0 {
            sqlx::query(
                "INSERT INTO remote_source_secrets (source_id,secret_ref,updated_at) VALUES (?,?,?)",
            )
            .bind(&id)
            .bind(&secret)
            .bind(&timestamp)
            .execute(&db.pool)
            .await?;
        }
    }
    if let Some(auto_sync) = &payload.auto_sync {
        let interval = auto_sync.interval_minutes.unwrap_or(720).clamp(5, 43_200);
        let next_sync_at = auto_sync.enabled.then(|| timestamp.clone());
        sqlx::query("UPDATE remote_sources SET auto_sync_enabled=?,auto_sync_interval_minutes=?,next_sync_at=?,updated_at=? WHERE id=?")
            .bind(i64::from(auto_sync.enabled))
            .bind(interval)
            .bind(&next_sync_at)
            .bind(&timestamp)
            .bind(&id)
            .execute(&db.pool)
            .await?;
    }

    db.audit(
        Some(&auth.id),
        "REMOTE_SOURCE_UPDATED",
        "remote_source",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    Ok(Json(source_json(&source_row(&db, &id).await?)))
}

async fn delete_source(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    let row = source_row(&db, &id).await?;
    let library_id: String = row.get("library_id");
    let provider = ProviderType::parse(&row.get::<String, _>("provider_type"))?;
    let config: Value =
        serde_json::from_str(&row.get::<String, _>("config")).unwrap_or_else(|_| json!({}));

    let sentinel = format!("remote://{id}");
    let mut tx = db.pool.begin().await?;
    // Remove the catalog footprint before the source: the synthetic library path
    // owns every media file this source produced.
    sqlx::query("DELETE FROM media_files WHERE library_path_id IN (SELECT id FROM library_paths WHERE library_id=? AND normalized_path=?)")
        .bind(&library_id)
        .bind(&sentinel)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM library_paths WHERE library_id=? AND normalized_path=?")
        .bind(&library_id)
        .bind(&sentinel)
        .execute(&mut *tx)
        .await?;
    // Drop catalog items that no longer have any file (local or remote).
    sqlx::query("DELETE FROM media_items WHERE library_id=? AND NOT EXISTS (SELECT 1 FROM media_files WHERE media_files.media_item_id=media_items.id)")
        .bind(&library_id)
        .execute(&mut *tx)
        .await?;
    // Foreign keys cascade the secret, entries and remote media rows.
    sqlx::query("DELETE FROM remote_sources WHERE id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    if provider == ProviderType::M3uFile
        && let Some(file_name) = config.get("storedFile").and_then(Value::as_str)
        && let Ok(name) = safe_component(file_name.trim_end_matches(".m3u"))
    {
        let _ = tokio::fs::remove_file(m3u_dir(&state.config).join(format!("{name}.m3u"))).await;
    }
    let _ = crate::features::catalog::api::refresh_library_stats(&db, &library_id).await;
    state.recommendations.invalidate_all().await;
    db.audit(
        Some(&auth.id),
        "REMOTE_SOURCE_DELETED",
        "remote_source",
        Some(&id),
        json!({ "libraryId": library_id }),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn source_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    let row = source_row(&db, &id).await?;
    Ok(Json(json!({
        "id": row.get::<String, _>("id"),
        "status": row.get::<String, _>("status"),
        "isActive": row.get::<i64, _>("is_active") != 0,
        "lastSyncAt": row.try_get::<Option<String>, _>("last_sync_at").ok().flatten(),
        "lastSuccessfulSync": row.try_get::<Option<String>, _>("last_successful_sync_at").ok().flatten(),
        "nextSyncAt": row.try_get::<Option<String>, _>("next_sync_at").ok().flatten(),
        "lastError": row.try_get::<Option<String>, _>("last_error").ok().flatten(),
        "lastErrorAt": row.try_get::<Option<String>, _>("last_error_at").ok().flatten(),
    })))
}

pub(crate) async fn ensure_library(db: &Database, library_id: &str) -> AppResult<()> {
    let exists: i64 =
        sqlx::query("SELECT COUNT(*) FROM libraries WHERE id=? AND deleted_at IS NULL")
            .bind(library_id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
    if exists == 0 {
        return Err(AppError::not_found(
            "LIBRARY_NOT_FOUND",
            "Library was not found.",
        ));
    }
    Ok(())
}

pub(crate) async fn source_row(db: &Database, id: &str) -> AppResult<sqlx::any::AnyRow> {
    sqlx::query(&format!("{SOURCE_SELECT} WHERE id=?"))
        .bind(id)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| {
            AppError::not_found("REMOTE_SOURCE_NOT_FOUND", "Remote source was not found.")
        })
}

pub(crate) fn source_json(row: &sqlx::any::AnyRow) -> Value {
    let config: Value = row
        .try_get::<String, _>("config")
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    json!({
        "id": row.get::<String, _>("id"),
        "libraryId": row.get::<String, _>("library_id"),
        "providerType": row.get::<String, _>("provider_type"),
        "name": row.get::<String, _>("name"),
        "isActive": row.get::<i64, _>("is_active") != 0,
        "status": row.get::<String, _>("status"),
        "config": config,
        "autoSync": {
            "enabled": row.get::<i64, _>("auto_sync_enabled") != 0,
            "intervalMinutes": row.get::<i64, _>("auto_sync_interval_minutes"),
        },
        "lastSyncAt": row.try_get::<Option<String>, _>("last_sync_at").ok().flatten(),
        "lastSuccessfulSyncAt": row.try_get::<Option<String>, _>("last_successful_sync_at").ok().flatten(),
        "nextSyncAt": row.try_get::<Option<String>, _>("next_sync_at").ok().flatten(),
        "lastError": row.try_get::<Option<String>, _>("last_error").ok().flatten(),
        "lastErrorAt": row.try_get::<Option<String>, _>("last_error_at").ok().flatten(),
        "createdAt": row.get::<String, _>("created_at"),
        "updatedAt": row.get::<String, _>("updated_at"),
    })
}

fn validate_m3u_url(url: &str) -> AppResult<()> {
    if url.chars().count() > 2048 {
        return Err(AppError::validation(
            "INVALID_REMOTE_SOURCE_URL",
            "URL is too long.",
        ));
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    match rest {
        Some(rest) if !rest.trim_start_matches('/').is_empty() => Ok(()),
        _ => Err(AppError::validation(
            "INVALID_REMOTE_SOURCE_URL",
            "URL must be an absolute http(s) address.",
        )),
    }
}

async fn upload_m3u(
    State(state): State<AppState>,
    auth: AuthUser,
    body: Bytes,
) -> AppResult<(StatusCode, Json<Value>)> {
    auth.require("libraries.manage")?;
    if body.len() as u64 > state.config.m3u_max_bytes {
        return Err(AppError::validation(
            "M3U_TOO_LARGE",
            "The playlist exceeds the configured maximum size.",
        ));
    }
    let head = &body[..body.len().min(64)];
    if !m3u::looks_like_m3u(head) {
        return Err(AppError::validation(
            "INVALID_M3U",
            "The file is not a valid M3U playlist.",
        ));
    }
    let dir = m3u_upload_dir(&state.config);
    tokio::fs::create_dir_all(&dir).await?;
    let upload_id = Uuid::new_v4().to_string();
    tokio::fs::write(dir.join(format!("{upload_id}.m3u")), &body).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "uploadId": upload_id, "sizeBytes": body.len() })),
    ))
}

/// Moves a validated upload into the source's permanent slot. Returns the stored
/// file name (relative to the M3U directory).
async fn adopt_upload(config: &Config, upload_id: &str, source_id: &str) -> AppResult<String> {
    let upload_id = safe_component(upload_id)?;
    let source = m3u_upload_dir(config).join(format!("{upload_id}.m3u"));
    if !source.exists() {
        return Err(AppError::validation(
            "UPLOAD_NOT_FOUND",
            "The uploaded playlist was not found. Upload it again.",
        ));
    }
    let dir = m3u_dir(config);
    tokio::fs::create_dir_all(&dir).await?;
    let file_name = format!("{source_id}.m3u");
    tokio::fs::rename(&source, dir.join(&file_name)).await?;
    Ok(file_name)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum M3uPreviewRequest {
    Url {
        url: String,
    },
    Upload {
        #[serde(rename = "uploadId")]
        upload_id: String,
    },
}

async fn preview_m3u(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(request): Json<M3uPreviewRequest>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    let max = state.config.m3u_max_bytes;
    let summary = match request {
        M3uPreviewRequest::Url { url } => {
            validate_m3u_url(url.trim())?;
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(
                    state.config.m3u_fetch_timeout_seconds,
                ))
                .user_agent(concat!("MyLib/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| AppError::config("Unable to initialize HTTP client."))?;
            let response = client.get(url.trim()).send().await.map_err(|error| {
                tracing::warn!(url = %super::sanitize_url(url.trim()), %error, "M3U fetch failed");
                AppError::new(
                    StatusCode::BAD_GATEWAY,
                    "M3U_FETCH_FAILED",
                    "The playlist URL could not be reached.",
                )
            })?;
            if !response.status().is_success() {
                return Err(AppError::new(
                    StatusCode::BAD_GATEWAY,
                    "M3U_FETCH_FAILED",
                    "The playlist URL returned an error.",
                ));
            }
            let stream = response.bytes_stream().map_err(std::io::Error::other);
            let reader = BufReader::new(tokio_util::io::StreamReader::new(stream));
            m3u::analyze_stream(reader, max).await?
        }
        M3uPreviewRequest::Upload { upload_id } => {
            let upload_id = safe_component(upload_id.trim())?;
            let path = m3u_upload_dir(&state.config).join(format!("{upload_id}.m3u"));
            let file = tokio::fs::File::open(&path).await.map_err(|_| {
                AppError::validation("UPLOAD_NOT_FOUND", "The uploaded playlist was not found.")
            })?;
            m3u::analyze_stream(BufReader::new(file), max).await?
        }
    };
    Ok(Json(
        serde_json::to_value(summary).unwrap_or_else(|_| json!({})),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionInput {
    media_type: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    subcategory: Option<String>,
    #[serde(default)]
    include_all: bool,
    #[serde(default = "default_true")]
    is_enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct SelectionsRequest {
    selections: Vec<SelectionInput>,
}

async fn get_selections(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    source_row(&db, &id).await?;
    let rows = sqlx::query("SELECT media_type,category,subcategory,include_all,is_enabled FROM m3u_source_selections WHERE source_id=? ORDER BY media_type,category,subcategory")
        .bind(&id)
        .fetch_all(&db.pool)
        .await?;
    Ok(Json(json!({
        "selections": rows.iter().map(|row| json!({
            "mediaType": row.get::<String, _>("media_type"),
            "category": row.try_get::<Option<String>, _>("category").ok().flatten(),
            "subcategory": row.try_get::<Option<String>, _>("subcategory").ok().flatten(),
            "includeAll": row.get::<i64, _>("include_all") != 0,
            "isEnabled": row.get::<i64, _>("is_enabled") != 0,
        })).collect::<Vec<_>>()
    })))
}

async fn put_selections(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<SelectionsRequest>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    source_row(&db, &id).await?;
    if payload.selections.len() > 2000 {
        return Err(AppError::validation(
            "TOO_MANY_SELECTIONS",
            "At most 2000 selection rules are allowed.",
        ));
    }
    for selection in &payload.selections {
        if !matches!(selection.media_type.as_str(), "MOVIE" | "TV_SHOW" | "ALL") {
            return Err(AppError::validation(
                "INVALID_SELECTION_MEDIA_TYPE",
                "Selection media type must be MOVIE, TV_SHOW or ALL.",
            ));
        }
    }
    let timestamp = now();
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM m3u_source_selections WHERE source_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    for selection in &payload.selections {
        sqlx::query("INSERT INTO m3u_source_selections (id,source_id,media_type,category,subcategory,include_all,is_enabled,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string())
            .bind(&id)
            .bind(&selection.media_type)
            .bind(
                selection
                    .category
                    .as_deref()
                    .filter(|value| !value.is_empty()),
            )
            .bind(
                selection
                    .subcategory
                    .as_deref()
                    .filter(|value| !value.is_empty()),
            )
            .bind(i64::from(selection.include_all))
            .bind(i64::from(selection.is_enabled))
            .bind(&timestamp)
            .bind(&timestamp)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    db.audit(
        Some(&auth.id),
        "REMOTE_SOURCE_SELECTIONS_UPDATED",
        "remote_source",
        Some(&id),
        json!({ "count": payload.selections.len() }),
        None,
    )
    .await?;
    get_selections(State(state), auth, Path(id)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncQuery {
    #[serde(default)]
    wait: bool,
}

async fn trigger_sync(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<SyncQuery>,
) -> AppResult<(StatusCode, Json<Value>)> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    let row = source_row(&db, &id).await?;
    if row.get::<i64, _>("is_active") == 0 {
        return Err(AppError::conflict(
            "REMOTE_SOURCE_DISABLED",
            "This source is disabled.",
        ));
    }
    ProviderType::parse(&row.get::<String, _>("provider_type"))?;
    if query.wait {
        let outcome = super::sync::run_sync(&state, &id, "MANUAL").await?;
        return Ok((
            StatusCode::OK,
            Json(serde_json::to_value(outcome).unwrap_or_else(|_| json!({}))),
        ));
    }
    let worker = state.clone();
    let source_id = id.clone();
    tokio::spawn(async move {
        if let Err(error) = super::sync::run_sync(&worker, &source_id, "MANUAL").await {
            tracing::warn!(source_id = %source_id, code = error.code, "remote source sync failed");
        }
    });
    Ok((StatusCode::ACCEPTED, Json(json!({ "status": "SYNCING" }))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntriesQuery {
    #[serde(default = "default_entries_page")]
    page: i64,
    #[serde(default = "default_entries_page_size")]
    page_size: i64,
    media_type: Option<String>,
    category: Option<String>,
    subcategory: Option<String>,
    sync_status: Option<String>,
    selected: Option<bool>,
    search: Option<String>,
}

fn default_entries_page() -> i64 {
    1
}
fn default_entries_page_size() -> i64 {
    50
}

async fn list_entries(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<EntriesQuery>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    source_row(&db, &id).await?;
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 200);
    let mut filters = String::from(" WHERE source_id=?");
    let mut values: Vec<String> = Vec::new();
    if let Some(media_type) = query.media_type.filter(|value| !value.is_empty()) {
        filters.push_str(" AND media_type=?");
        values.push(media_type);
    }
    if let Some(category) = query.category.filter(|value| !value.is_empty()) {
        filters.push_str(" AND category=?");
        values.push(category);
    }
    if let Some(subcategory) = query.subcategory.filter(|value| !value.is_empty()) {
        filters.push_str(" AND subcategory=?");
        values.push(subcategory);
    }
    if let Some(sync_status) = query.sync_status.filter(|value| !value.is_empty()) {
        filters.push_str(" AND sync_status=?");
        values.push(sync_status);
    }
    if let Some(selected) = query.selected {
        filters.push_str(" AND is_selected=?");
        values.push(if selected { "1" } else { "0" }.into());
    }
    if let Some(search) = query.search.filter(|value| !value.trim().is_empty()) {
        filters.push_str(" AND LOWER(clean_title) LIKE ?");
        values.push(format!("%{}%", search.trim().to_ascii_lowercase()));
    }

    let count_sql = format!("SELECT COUNT(*) FROM m3u_entries{filters}");
    let mut count_query = sqlx::query(&count_sql).bind(&id);
    for value in &values {
        count_query = count_query.bind(value);
    }
    let total: i64 = count_query.fetch_one(&db.pool).await?.get(0);

    let list_sql = format!(
        "SELECT id,external_key,raw_name,clean_title,year,media_type,category,subcategory,season_number,episode_number,tvg_logo,is_selected,sync_status,missing_since,last_seen_at,media_file_id FROM m3u_entries{filters} ORDER BY clean_title LIMIT ? OFFSET ?"
    );
    let mut list_query = sqlx::query(&list_sql).bind(&id);
    for value in &values {
        list_query = list_query.bind(value);
    }
    let rows = list_query
        .bind(page_size)
        .bind((page - 1) * page_size)
        .fetch_all(&db.pool)
        .await?;
    let items = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<String, _>("id"),
                "externalKey": row.get::<String, _>("external_key"),
                "rawName": row.get::<String, _>("raw_name"),
                "cleanTitle": row.get::<String, _>("clean_title"),
                "year": row.try_get::<Option<i64>, _>("year").ok().flatten(),
                "mediaType": row.get::<String, _>("media_type"),
                "category": row.try_get::<Option<String>, _>("category").ok().flatten(),
                "subcategory": row.try_get::<Option<String>, _>("subcategory").ok().flatten(),
                "seasonNumber": row.try_get::<Option<i64>, _>("season_number").ok().flatten(),
                "episodeNumber": row.try_get::<Option<i64>, _>("episode_number").ok().flatten(),
                "tvgLogo": row.try_get::<Option<String>, _>("tvg_logo").ok().flatten(),
                "isSelected": row.get::<i64, _>("is_selected") != 0,
                "syncStatus": row.get::<String, _>("sync_status"),
                "missingSince": row.try_get::<Option<String>, _>("missing_since").ok().flatten(),
                "lastSeenAt": row.get::<String, _>("last_seen_at"),
                "mediaFileId": row.try_get::<Option<String>, _>("media_file_id").ok().flatten(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "items": items,
        "page": page,
        "pageSize": page_size,
        "total": total,
        "totalPages": if total == 0 { 0 } else { (total + page_size - 1) / page_size },
    })))
}
