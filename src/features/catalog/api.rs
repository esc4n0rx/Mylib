use std::{
    collections::HashSet,
    path::{Path as FsPath, PathBuf},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Any, Executor, Row};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{AuthUser, hash_password, verify_password},
    db::{Database, now},
    errors::{AppError, AppResult},
    libraries::{
        CreateLibraryRequest, LibraryType, Privacy, UpdateLibraryRequest, inspect_path,
        paths_overlap, valid_language, validate_name, validate_path,
    },
    library_sync::calculate_next_sync,
    metadata::{MetadataProvider, SearchCandidate, TmdbMetadataProvider, confidence},
    scanner::{DiscoveredFile, ParsedName, discover, parse_media_path},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/libraries",
            get(list_libraries).post(create_library),
        )
        .route(
            "/api/v1/libraries/paths/validate",
            post(validate_library_path),
        )
        .route(
            "/api/v1/libraries/{id}",
            get(get_library)
                .patch(update_library)
                .delete(delete_library),
        )
        .route("/api/v1/libraries/{id}/unlock", post(unlock_library))
        .route("/api/v1/libraries/{id}/stats", get(get_library_stats))
        .route(
            "/api/v1/libraries/{id}/paths/status",
            get(get_path_statuses),
        )
        .route("/api/v1/libraries/{id}/paths", post(add_library_path))
        .route(
            "/api/v1/libraries/{id}/paths/{path_id}",
            delete(remove_library_path),
        )
        .route("/api/v1/libraries/{id}/scan", post(start_scan))
        .route("/api/v1/libraries/{id}/scans", get(list_scans))
        .route("/api/v1/libraries/{id}/scans/{scan_id}", get(get_scan))
        .route(
            "/api/v1/libraries/{id}/scans/{scan_id}/cancel",
            post(cancel_scan),
        )
        .route("/api/v1/libraries/{id}/items", get(list_items))
        .route("/api/v1/libraries/{id}/items/{item_id}", get(get_item))
        .route("/api/v1/libraries/{id}/unmatched", get(list_unmatched))
        .route("/api/v1/media/identify/search", get(identify_search))
        .route("/api/v1/media/identify", post(identify_manual))
        .route(
            "/api/v1/media/{media_file_id}/identification",
            delete(remove_identification),
        )
        .route("/api/v1/media/{media_file_id}/reidentify", post(reidentify))
        .route(
            "/api/v1/media/items/{item_id}/metadata/refresh",
            post(refresh_metadata),
        )
        .route("/api/v1/settings/metadata/tmdb/status", get(tmdb_status))
}

#[derive(Deserialize)]
struct PathRequest {
    path: String,
}
#[derive(Deserialize)]
struct DeleteQuery {
    #[serde(default)]
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanRequest {
    #[serde(default)]
    scan_type: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_page_size")]
    page_size: i64,
    #[serde(rename = "type")]
    media_type: Option<String>,
    identification_status: Option<String>,
}
fn default_page() -> i64 {
    1
}
fn default_page_size() -> i64 {
    50
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    library_id: String,
    media_file_id: String,
    query: String,
    year: Option<i32>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentifyRequest {
    media_file_id: String,
    provider: String,
    provider_id: i64,
}
#[derive(Deserialize)]
struct UnlockRequest {
    password: String,
}

async fn validate_library_path(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<PathRequest>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    Ok(Json(
        serde_json::to_value(inspect_path(&payload.path, &state.config.data_dir))
            .map_err(|_| AppError::config("Unable to encode path validation."))?,
    ))
}

async fn create_library(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<CreateLibraryRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    auth.require("libraries.manage")?;
    let value = payload.validate(&state.config.data_dir)?;
    let db = state.database().await;
    ensure_paths_available(&db, &value.paths, None).await?;
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    let mut tx = db.pool.begin().await?;
    sqlx::query("INSERT INTO libraries (id,name,description,library_type,privacy,password_hash,minimum_age,metadata_language,metadata_region,is_active,scan_enabled,created_by,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,1,1,?,?,?)").bind(&id).bind(&value.name).bind(&value.description).bind(value.library_type.as_str()).bind(value.privacy.as_str()).bind(&value.password_hash).bind(value.minimum_age).bind(&value.metadata_language).bind(&value.metadata_region).bind(&auth.id).bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
    for path in value.paths {
        insert_path(&mut *tx, &id, &path, &timestamp).await?;
    }
    sqlx::query("INSERT INTO profile_library_access(profile_id,library_id,is_allowed,created_at,updated_at) SELECT id,?,1,?,? FROM profiles WHERE user_id=? AND is_active=1")
        .bind(&id).bind(&timestamp).bind(&timestamp).bind(&auth.id).execute(&mut *tx).await?;
    insert_audit(&mut *tx,&auth.id,"LIBRARY_CREATED","library",&id,json!({"name":value.name,"type":value.library_type.as_str(),"privacy":value.privacy.as_str()})).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(library_json(&db, &id).await?)))
}

async fn list_libraries(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    let profile_id = auth.require_profile()?;
    let rows = sqlx::query("SELECT l.id FROM libraries l JOIN profile_library_access pla ON pla.library_id=l.id AND pla.profile_id=? AND pla.is_allowed=1 JOIN profiles p ON p.id=pla.profile_id WHERE l.deleted_at IS NULL AND l.is_active=1 AND p.user_id=? AND l.minimum_age<=p.max_age_rating AND (?=1 OR l.privacy='PUBLIC' OR EXISTS (SELECT 1 FROM user_library_access ula WHERE ula.library_id=l.id AND ula.user_id=? AND ula.can_view=1)) ORDER BY l.name")
        .bind(profile_id).bind(&auth.id).bind(if auth.is_admin(){1}else{0}).bind(&auth.id).fetch_all(&db.pool).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(redact_library_paths(
            library_json(&db, &row.get::<String, _>(0)).await?,
            &auth,
        ));
    }
    Ok(Json(json!({"items":items})))
}
async fn get_library(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let allowed: i64 = sqlx::query("SELECT COUNT(*) FROM libraries l JOIN profile_library_access pla ON pla.library_id=l.id AND pla.profile_id=? AND pla.is_allowed=1 JOIN profiles p ON p.id=pla.profile_id WHERE l.id=? AND l.deleted_at IS NULL AND l.is_active=1 AND p.user_id=? AND l.minimum_age<=p.max_age_rating AND (?=1 OR l.privacy='PUBLIC' OR EXISTS (SELECT 1 FROM user_library_access ula WHERE ula.library_id=l.id AND ula.user_id=? AND ula.can_view=1))")
        .bind(auth.require_profile()?).bind(&id).bind(&auth.id).bind(if auth.is_admin(){1}else{0}).bind(&auth.id).fetch_one(&state.database().await.pool).await?.get(0);
    if allowed == 0 {
        return Err(AppError::not_found(
            "LIBRARY_NOT_FOUND",
            "Library was not found.",
        ));
    }
    Ok(Json(redact_library_paths(
        library_json(&state.database().await, &id).await?,
        &auth,
    )))
}

async fn get_library_stats(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    library_row(&db, &id).await?;
    refresh_library_stats(&db, &id).await?;
    Ok(Json(library_stats_json(&db, &id).await?))
}

async fn get_path_statuses(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    library_row(&db, &id).await?;
    let rows = sqlx::query(
        "SELECT id,path,status,last_available_at FROM library_paths WHERE library_id=? AND is_active=1 ORDER BY path",
    )
    .bind(&id)
    .fetch_all(&db.pool)
    .await?;
    let timestamp = now();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let path_id: String = row.get("id");
        let path: String = row.get("path");
        let previous: String = row.get("status");
        let previous_available = row
            .try_get::<Option<String>, _>("last_available_at")
            .ok()
            .flatten();
        let inspection = inspect_path(&path, &state.config.data_dir);
        let (status, error) = if !inspection.exists {
            ("PATH_UNAVAILABLE", Some("Path does not exist."))
        } else if !inspection.directory {
            ("PATH_UNAVAILABLE", Some("Path is not a directory."))
        } else if !inspection.readable {
            ("PATH_UNAVAILABLE", Some("Path is not readable."))
        } else {
            ("AVAILABLE", None)
        };
        let last_available = if status == "AVAILABLE" {
            Some(timestamp.clone())
        } else {
            previous_available
        };
        sqlx::query("UPDATE library_paths SET status=?,last_checked_at=?,last_available_at=?,last_error=?,updated_at=? WHERE id=?")
            .bind(status)
            .bind(&timestamp)
            .bind(&last_available)
            .bind(error)
            .bind(&timestamp)
            .bind(&path_id)
            .execute(&db.pool)
            .await?;
        if previous != status {
            db.audit(
                Some(&auth.id),
                "LIBRARY_PATH_STATUS_CHANGED",
                "library_path",
                Some(&path_id),
                json!({"libraryId":id,"from":previous,"to":status}),
                None,
            )
            .await?;
        }
        items.push(json!({"id":path_id,"path":path,"status":status,"exists":inspection.exists,"readable":inspection.readable,"lastCheckedAt":timestamp,"lastAvailableAt":last_available,"lastError":error}));
    }
    Ok(Json(json!({"items":items})))
}

async fn update_library(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateLibraryRequest>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    let row = library_row(&db, &id).await?;
    let name = payload.name.unwrap_or_else(|| row.get("name"));
    validate_name(&name)?;
    let description = payload
        .description
        .or_else(|| row.try_get("description").ok());
    if description
        .as_ref()
        .is_some_and(|v| v.chars().count() > 500)
    {
        return Err(AppError::validation(
            "INVALID_LIBRARY_DESCRIPTION",
            "Description may contain at most 500 characters.",
        ));
    }
    let privacy = Privacy::parse(
        payload
            .privacy
            .as_deref()
            .unwrap_or_else(|| row.get("privacy")),
    )?;
    let minimum_age = payload
        .minimum_age
        .unwrap_or_else(|| row.get("minimum_age"));
    if !(0..=21).contains(&minimum_age) {
        return Err(AppError::validation(
            "INVALID_MINIMUM_AGE",
            "Minimum age must be between 0 and 21.",
        ));
    }
    let language = payload
        .metadata_language
        .unwrap_or_else(|| row.get("metadata_language"));
    if !valid_language(&language) {
        return Err(AppError::validation(
            "INVALID_METADATA_LANGUAGE",
            "Invalid metadata language.",
        ));
    }
    let old_hash: Option<String> = row.try_get("password_hash").ok();
    let password_hash = match (privacy, payload.password) {
        (Privacy::Private, Some(p)) => Some(hash_password(&p)?),
        (Privacy::Private, None) => Some(old_hash.ok_or_else(|| {
            AppError::validation(
                "LIBRARY_PASSWORD_REQUIRED",
                "A private library requires a password.",
            )
        })?),
        (Privacy::Public, _) => None,
    };
    let region = payload
        .metadata_region
        .or_else(|| row.try_get("metadata_region").ok());
    let active = payload
        .is_active
        .map_or_else(|| row.get::<i64, _>("is_active"), i64::from);
    let scan = payload
        .scan_enabled
        .map_or_else(|| row.get::<i64, _>("scan_enabled"), i64::from);
    let timezone = sqlx::query("SELECT server_timezone FROM server_config LIMIT 1")
        .fetch_optional(&db.pool)
        .await?
        .map_or_else(|| "America/Sao_Paulo".to_owned(), |value| value.get(0));
    let (auto_enabled, auto_mode, auto_interval, auto_hour, auto_minute, startup, next_sync) =
        if let Some(auto) = payload.auto_sync {
            if !matches!(auto.mode.as_str(), "INTERVAL" | "SCHEDULE") {
                return Err(AppError::validation(
                    "INVALID_AUTO_SYNC_MODE",
                    "Auto-sync mode must be INTERVAL or SCHEDULE.",
                ));
            }
            let interval = auto.interval_minutes.unwrap_or(60);
            if !(5..=10080).contains(&interval) {
                return Err(AppError::validation(
                    "INVALID_AUTO_SYNC_INTERVAL",
                    "Auto-sync interval must be between 5 and 10080 minutes.",
                ));
            }
            let (hour, minute) = auto
                .schedule
                .map_or((3, 0), |value| (value.hour, value.minute));
            if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
                return Err(AppError::validation(
                    "INVALID_AUTO_SYNC_SCHEDULE",
                    "Schedule must contain a valid hour and minute.",
                ));
            }
            let next = if auto.enabled {
                Some(
                    calculate_next_sync(
                        chrono::Utc::now(),
                        &auto.mode,
                        interval,
                        hour,
                        minute,
                        &timezone,
                    )?
                    .to_rfc3339(),
                )
            } else {
                None
            };
            (
                i64::from(auto.enabled),
                auto.mode,
                interval,
                hour,
                minute,
                i64::from(auto.scan_on_startup),
                next,
            )
        } else {
            (
                row.get("auto_sync_enabled"),
                row.get("auto_sync_mode"),
                row.get("auto_sync_interval_minutes"),
                row.get("auto_sync_hour"),
                row.get("auto_sync_minute"),
                row.get("scan_on_startup"),
                row.try_get::<Option<String>, _>("next_sync_at")
                    .ok()
                    .flatten(),
            )
        };
    sqlx::query("UPDATE libraries SET name=?,description=?,privacy=?,password_hash=?,minimum_age=?,metadata_language=?,metadata_region=?,is_active=?,scan_enabled=?,auto_sync_enabled=?,auto_sync_mode=?,auto_sync_interval_minutes=?,auto_sync_hour=?,auto_sync_minute=?,scan_on_startup=?,next_sync_at=?,updated_at=? WHERE id=? AND deleted_at IS NULL").bind(&name).bind(&description).bind(privacy.as_str()).bind(&password_hash).bind(minimum_age).bind(&language).bind(&region).bind(active).bind(scan).bind(auto_enabled).bind(&auto_mode).bind(auto_interval).bind(auto_hour).bind(auto_minute).bind(startup).bind(&next_sync).bind(now()).bind(&id).execute(&db.pool).await?;
    db.audit(
        Some(&auth.id),
        "LIBRARY_UPDATED",
        "library",
        Some(&id),
        json!({"autoSyncEnabled":auto_enabled != 0,"autoSyncMode":auto_mode}),
        None,
    )
    .await?;
    Ok(Json(library_json(&db, &id).await?))
}

async fn delete_library(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> AppResult<StatusCode> {
    auth.require("libraries.manage")?;
    if !query.confirm {
        return Err(AppError::validation(
            "LIBRARY_DELETE_CONFIRMATION_REQUIRED",
            "Pass confirm=true to deactivate the library.",
        ));
    }
    let db = state.database().await;
    let result=sqlx::query("UPDATE libraries SET is_active=0,scan_enabled=0,deleted_at=?,updated_at=? WHERE id=? AND deleted_at IS NULL").bind(now()).bind(now()).bind(&id).execute(&db.pool).await?;
    if result.rows_affected() == 0 {
        return Err(not_found_library());
    }
    db.audit(
        Some(&auth.id),
        "LIBRARY_DELETED",
        "library",
        Some(&id),
        json!({"physicalFilesDeleted":false}),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unlock_library(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UnlockRequest>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.unlock")?;
    let row = library_row(&state.database().await, &id).await?;
    let privacy: String = row.get("privacy");
    if privacy == "PUBLIC" {
        return Ok(Json(json!({"unlocked":true,"expiresIn":0})));
    }
    let hash: String = row
        .try_get("password_hash")
        .map_err(|_| AppError::config("Private library has no password."))?;
    if !verify_password(&payload.password, &hash) {
        return Err(AppError::new(
            StatusCode::UNAUTHORIZED,
            "INVALID_LIBRARY_PASSWORD",
            "The library password is incorrect.",
        ));
    }
    let ttl = 14400;
    let token = state.tokens.issue_library_unlock(&auth.id, &id, ttl)?;
    state
        .database()
        .await
        .audit(
            Some(&auth.id),
            "LIBRARY_UNLOCKED",
            "library",
            Some(&id),
            json!({}),
            None,
        )
        .await?;
    Ok(Json(
        json!({"unlocked":true,"expiresIn":ttl,"unlockToken":token}),
    ))
}

async fn add_library_path(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<PathRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    library_row(&db, &id).await?;
    let path = validate_path(&payload.path, &state.config.data_dir)?;
    ensure_paths_available(&db, std::slice::from_ref(&path), Some(&id)).await?;
    let timestamp = now();
    let mut tx = db.pool.begin().await?;
    let path_id = insert_path(&mut *tx, &id, &path, &timestamp).await?;
    insert_audit(
        &mut *tx,
        &auth.id,
        "LIBRARY_PATH_ADDED",
        "library_path",
        &path_id,
        json!({"libraryId":id}),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"id":path_id,"path":path.to_string_lossy(),"isActive":true,"status":"AVAILABLE"}),
        ),
    ))
}
async fn remove_library_path(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, path_id)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    auth.require("libraries.manage")?;
    let db = state.database().await;
    let count: i64 =
        sqlx::query("SELECT COUNT(*) FROM library_paths WHERE library_id=? AND is_active=1")
            .bind(&id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
    if count <= 1 {
        return Err(AppError::conflict(
            "LAST_LIBRARY_PATH",
            "A library must retain at least one active path.",
        ));
    }
    let result=sqlx::query("UPDATE library_paths SET is_active=0,updated_at=? WHERE id=? AND library_id=? AND is_active=1").bind(now()).bind(&path_id).bind(&id).execute(&db.pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found(
            "LIBRARY_PATH_NOT_FOUND",
            "Library path was not found.",
        ));
    }
    db.audit(
        Some(&auth.id),
        "LIBRARY_PATH_REMOVED",
        "library_path",
        Some(&path_id),
        json!({"libraryId":id}),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_scan(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    body: Option<Json<ScanRequest>>,
) -> AppResult<(StatusCode, Json<Value>)> {
    auth.require("libraries.scan")?;
    let requested = body.and_then(|Json(value)| value.scan_type);
    let job_id = enqueue_scan(&state, &id, requested.as_deref(), "MANUAL", &auth.id, false).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"jobId":job_id,"status":"QUEUED"})),
    ))
}

pub(crate) async fn enqueue_scan(
    state: &AppState,
    id: &str,
    requested_scan_type: Option<&str>,
    trigger_source: &str,
    actor_id: &str,
    skip_if_running: bool,
) -> AppResult<String> {
    let db = state.database().await;
    let library = library_row(&db, id).await?;
    if library.get::<i64, _>("scan_enabled") == 0 {
        return Err(AppError::conflict(
            "LIBRARY_SCAN_DISABLED",
            "Scanning is disabled for this library.",
        ));
    }
    let active:i64=sqlx::query("SELECT COUNT(*) FROM scan_jobs WHERE library_id=? AND status IN ('QUEUED','SCANNING','MATCHING','PERSISTING')").bind(id).fetch_one(&db.pool).await?.get(0);
    if active > 0 {
        if skip_if_running {
            let skipped_id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO scan_jobs (id,library_id,status,scan_type,trigger_source,created_by,created_at,finished_at) VALUES (?,?,'SKIPPED_ALREADY_RUNNING','INCREMENTAL',?,?,?,?)").bind(&skipped_id).bind(id).bind(trigger_source).bind(actor_id).bind(now()).bind(now()).execute(&db.pool).await?;
            db.audit(
                Some(actor_id),
                "LIBRARY_SYNC_SKIPPED",
                "scan_job",
                Some(&skipped_id),
                json!({"libraryId":id,"reason":"ALREADY_RUNNING","triggerSource":trigger_source}),
                None,
            )
            .await?;
            return Ok(skipped_id);
        }
        return Err(AppError::conflict(
            "LIBRARY_SCAN_ALREADY_RUNNING",
            "A scan is already running for this library.",
        ));
    }
    let requested = requested_scan_type.map(str::to_owned).unwrap_or_else(|| {
        if library
            .try_get::<Option<String>, _>("last_successful_scan_at")
            .ok()
            .flatten()
            .is_some()
        {
            "INCREMENTAL".into()
        } else {
            "FULL".into()
        }
    });
    if !matches!(requested.as_str(), "FULL" | "INCREMENTAL") {
        return Err(AppError::validation(
            "INVALID_SCAN_TYPE",
            "Scan type must be FULL or INCREMENTAL.",
        ));
    }
    {
        let mut active_libraries = state.scanning_libraries.lock().await;
        if !active_libraries.insert(id.to_owned()) {
            if skip_if_running {
                return Ok(String::new());
            }
            return Err(AppError::conflict(
                "LIBRARY_SCAN_ALREADY_RUNNING",
                "A scan is already running for this library.",
            ));
        }
    }
    let job_id = Uuid::new_v4().to_string();
    if let Err(error)=sqlx::query("INSERT INTO scan_jobs (id,library_id,status,scan_type,trigger_source,created_by,created_at) VALUES (?,?,'QUEUED',?,?,?,?)").bind(&job_id).bind(id).bind(&requested).bind(trigger_source).bind(actor_id).bind(now()).execute(&db.pool).await{state.scanning_libraries.lock().await.remove(id);return Err(error.into());}
    let audit_action = if trigger_source == "MANUAL" {
        "SCAN_QUEUED"
    } else {
        "LIBRARY_AUTO_SYNC_TRIGGERED"
    };
    db.audit(
        Some(actor_id),
        audit_action,
        "scan_job",
        Some(&job_id),
        json!({"libraryId":id,"triggerSource":trigger_source}),
        None,
    )
    .await?;
    let (sender, receiver) = watch::channel(false);
    state
        .scan_cancellations
        .lock()
        .await
        .insert(job_id.clone(), sender);
    let worker_state = state.clone();
    let worker_job = job_id.clone();
    let worker_library = id.to_owned();
    tokio::spawn(async move {
        run_scan(
            worker_state,
            worker_library,
            worker_job,
            requested,
            receiver,
        )
        .await;
    });
    Ok(job_id)
}

async fn list_scans(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(page): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let db = state.database().await;
    library_row(&db, &id).await?;
    let (limit, offset) = page_bounds(page.page, page_size(page.page_size));
    let statement =
        SCAN_SELECT.to_owned() + " WHERE library_id=? ORDER BY created_at DESC LIMIT ? OFFSET ?";
    let rows = sqlx::query(&statement)
        .bind(&id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&db.pool)
        .await?;
    Ok(Json(
        json!({"items":rows.iter().map(scan_json).collect::<Vec<_>>(),"page":page.page.max(1),"pageSize":limit}),
    ))
}
async fn get_scan(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, scan_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.view")?;
    let row = sqlx::query(&(SCAN_SELECT.to_owned() + " WHERE id=? AND library_id=?"))
        .bind(&scan_id)
        .bind(&id)
        .fetch_optional(&state.database().await.pool)
        .await?
        .ok_or_else(|| AppError::not_found("SCAN_NOT_FOUND", "Scan was not found."))?;
    Ok(Json(scan_json(&row)))
}
async fn cancel_scan(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, scan_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    auth.require("libraries.scan")?;
    let db = state.database().await;
    let exists:i64=sqlx::query("SELECT COUNT(*) FROM scan_jobs WHERE id=? AND library_id=? AND status IN ('QUEUED','SCANNING','MATCHING','PERSISTING')").bind(&scan_id).bind(&id).fetch_one(&db.pool).await?.get(0);
    if exists == 0 {
        return Err(AppError::conflict("SCAN_NOT_ACTIVE", "Scan is not active."));
    }
    if let Some(sender) = state.scan_cancellations.lock().await.get(&scan_id) {
        let _ = sender.send(true);
    }
    Ok(Json(json!({"id":scan_id,"cancellationRequested":true})))
}

async fn list_items(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(page): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    ensure_library_access(&state, &auth, &headers, &id).await?;
    let db = state.database().await;
    let (limit, offset) = page_bounds(page.page, page.page_size);
    let mut sql="SELECT id,media_type,tmdb_id,title,original_title,overview,release_date,year,poster_path,backdrop_path,rating,vote_count,popularity FROM media_items WHERE library_id=?".to_owned();
    if page.media_type.is_some() {
        sql.push_str(" AND media_type=?");
    }
    sql.push_str(" ORDER BY title LIMIT ? OFFSET ?");
    let mut query = sqlx::query(&sql).bind(&id);
    if let Some(kind) = page.media_type {
        query = query.bind(kind);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(&db.pool).await?;
    Ok(Json(
        json!({"items":rows.iter().map(item_json).collect::<Vec<_>>(),"page":page.page.max(1),"pageSize":limit}),
    ))
}
async fn get_item(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path((id, item_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    ensure_library_access(&state, &auth, &headers, &id).await?;
    let row=sqlx::query("SELECT id,media_type,tmdb_id,title,original_title,overview,release_date,year,poster_path,backdrop_path,rating,vote_count,popularity FROM media_items WHERE id=? AND library_id=?").bind(&item_id).bind(&id).fetch_optional(&state.database().await.pool).await?.ok_or_else(||AppError::not_found("MEDIA_ITEM_NOT_FOUND","Media item was not found."))?;
    Ok(Json(item_json(&row)))
}
async fn list_unmatched(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(page): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    auth.require("media.identify")?;
    ensure_library_access(&state, &auth, &headers, &id).await?;
    let (limit, offset) = page_bounds(page.page, page.page_size);
    let mut sql="SELECT id,filename,relative_path,normalized_title,parsed_year,parsed_season,parsed_episode,identification_status FROM media_files WHERE library_id=? AND identification_status IN ('PENDING','UNMATCHED','AMBIGUOUS','ERROR')".to_owned();
    if page.identification_status.is_some() {
        sql.push_str(" AND identification_status=?");
    }
    sql.push_str(" ORDER BY filename LIMIT ? OFFSET ?");
    let mut query = sqlx::query(&sql).bind(&id);
    if let Some(status) = page.identification_status {
        query = query.bind(status);
    }
    let rows = query
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.database().await.pool)
        .await?;
    Ok(Json(
        json!({"items":rows.iter().map(|r|json!({"mediaFileId":r.get::<String,_>("id"),"filename":r.get::<String,_>("filename"),"relativePath":r.get::<String,_>("relative_path"),"normalizedTitle":r.try_get::<String,_>("normalized_title").ok(),"year":r.try_get::<i64,_>("parsed_year").ok(),"season":r.try_get::<i64,_>("parsed_season").ok(),"episode":r.try_get::<i64,_>("parsed_episode").ok(),"status":r.get::<String,_>("identification_status")})).collect::<Vec<_>>(),"page":page.page.max(1),"pageSize":limit}),
    ))
}

async fn identify_search(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    auth.require("media.identify")?;
    ensure_library_access(&state, &auth, &headers, &query.library_id).await?;
    let db = state.database().await;
    let library = library_row(&db, &query.library_id).await?;
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM media_files WHERE id=? AND library_id=?")
        .bind(&query.media_file_id)
        .bind(&query.library_id)
        .fetch_one(&db.pool)
        .await?
        .get(0);
    if count == 0 {
        return Err(AppError::not_found(
            "MEDIA_FILE_NOT_FOUND",
            "Media file was not found.",
        ));
    }
    let provider = provider(&state)?;
    let items = cached_search(
        &db,
        &provider,
        LibraryType::parse(&library.get::<String, _>("library_type"))?,
        &query.query,
        query.year,
        &library.get::<String, _>("metadata_language"),
        library
            .try_get::<String, _>("metadata_region")
            .ok()
            .as_deref(),
    )
    .await?;
    Ok(Json(json!({"items":items})))
}
async fn identify_manual(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(payload): Json<IdentifyRequest>,
) -> AppResult<Json<Value>> {
    auth.require("media.identify")?;
    if payload.provider != "TMDB" {
        return Err(AppError::validation(
            "UNSUPPORTED_METADATA_PROVIDER",
            "Only TMDB is supported.",
        ));
    }
    let db = state.database().await;
    let (library_id, kind, language, parsed) = file_context(&db, &payload.media_file_id).await?;
    ensure_library_access(&state, &auth, &headers, &library_id).await?;
    let metadata_provider = provider(&state)?;
    let details = cached_details(
        &db,
        &metadata_provider,
        kind,
        payload.provider_id,
        &language,
    )
    .await?;
    let item_id = persist_metadata(&db, &library_id, kind, &language, &details).await?;
    let mut associated_files = 1_usize;
    if kind == LibraryType::TvShow {
        let rows = sqlx::query("SELECT id,parsed_year,parsed_season,parsed_episode FROM media_files WHERE library_id=? AND normalized_title=? AND missing_since IS NULL AND (media_item_id IS NULL OR id=?)")
            .bind(&library_id).bind(&parsed.title).bind(&payload.media_file_id).fetch_all(&db.pool).await?;
        let mut seasons = HashSet::new();
        for row in &rows {
            if let Ok(season) = row.try_get::<i64, _>("parsed_season") {
                seasons.insert(season as i32);
            }
        }
        for season in seasons {
            let season_details = cached_season(
                &db,
                &metadata_provider,
                payload.provider_id,
                season,
                &language,
            )
            .await?;
            persist_tv_season(&db, &item_id, &season_details).await?;
        }
        associated_files = rows.len();
        for row in rows {
            let episode_parsed = ParsedName {
                title: parsed.title.clone(),
                year: row.try_get::<i64, _>("parsed_year").ok().map(|v| v as i32),
                season: row
                    .try_get::<i64, _>("parsed_season")
                    .ok()
                    .map(|v| v as i32),
                episodes: row
                    .try_get::<i64, _>("parsed_episode")
                    .ok()
                    .map(|v| vec![v as i32])
                    .unwrap_or_default(),
                noise: vec![],
            };
            associate_file(
                &db,
                &row.get::<String, _>("id"),
                &item_id,
                "MATCHED_MANUAL",
                kind,
                &episode_parsed,
            )
            .await?;
        }
    } else {
        associate_file(
            &db,
            &payload.media_file_id,
            &item_id,
            "MATCHED_MANUAL",
            kind,
            &parsed,
        )
        .await?;
    }
    db.audit(
        Some(&auth.id),
        "MEDIA_MANUALLY_MATCHED",
        "media_file",
        Some(&payload.media_file_id),
        json!({"provider":"TMDB","providerId":payload.provider_id,"associatedFiles":associated_files}),
        None,
    )
    .await?;
    Ok(Json(
        json!({"mediaFileId":payload.media_file_id,"mediaItemId":item_id,"identificationStatus":"MATCHED_MANUAL","associatedFiles":associated_files}),
    ))
}
async fn remove_identification(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    auth.require("media.manage")?;
    let result=sqlx::query("UPDATE media_files SET media_item_id=NULL,tv_episode_id=NULL,identification_status='PENDING',updated_at=? WHERE id=?").bind(now()).bind(&id).execute(&state.database().await.pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found(
            "MEDIA_FILE_NOT_FOUND",
            "Media file was not found.",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn reidentify(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    auth.require("media.identify")?;
    let db = state.database().await;
    let result=sqlx::query("UPDATE media_files SET media_item_id=NULL,tv_episode_id=NULL,identification_status='PENDING',updated_at=? WHERE id=?").bind(now()).bind(&id).execute(&db.pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found(
            "MEDIA_FILE_NOT_FOUND",
            "Media file was not found.",
        ));
    }
    db.audit(
        Some(&auth.id),
        "MEDIA_REIDENTIFIED",
        "media_file",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    let worker_state = state.clone();
    let worker_id = id.clone();
    tokio::spawn(async move {
        if let Err(error) = auto_identify_one(&worker_state, &worker_id).await {
            tracing::warn!(media_file_id=%worker_id,%error,"reidentification failed");
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"mediaFileId":id,"status":"PENDING"})),
    ))
}
async fn refresh_metadata(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("media.manage")?;
    let db = state.database().await;
    let row = sqlx::query(
        "SELECT library_id,media_type,tmdb_id,metadata_language FROM media_items WHERE id=?",
    )
    .bind(&id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("MEDIA_ITEM_NOT_FOUND", "Media item was not found."))?;
    let kind = LibraryType::parse(&row.get::<String, _>("media_type"))?;
    let details = provider(&state)?
        .details(
            kind,
            row.get("tmdb_id"),
            &row.get::<String, _>("metadata_language"),
        )
        .await?;
    persist_metadata(
        &db,
        &row.get::<String, _>("library_id"),
        kind,
        &row.get::<String, _>("metadata_language"),
        &details,
    )
    .await?;
    db.audit(
        Some(&auth.id),
        "METADATA_REFRESHED",
        "media_item",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    Ok(Json(json!({"id":id,"refreshed":true})))
}
async fn tmdb_status(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let configured = state.config.tmdb_api_key.is_some();
    Ok(Json(
        json!({"configured":configured,"available":configured}),
    ))
}

// Scanner worker. Discovery uses a blocking filesystem producer and a bounded Tokio channel;
// persistence drains fixed-size batches, providing backpressure without retaining the tree.
async fn run_scan(
    state: AppState,
    library_id: String,
    job_id: String,
    scan_type: String,
    cancel: watch::Receiver<bool>,
) {
    let result = run_scan_inner(&state, &library_id, &job_id, &scan_type, cancel.clone()).await;
    let db = state.database().await;
    match result {
        Ok(warnings) => {
            let status = if *cancel.borrow() {
                "CANCELLED"
            } else if warnings > 0 {
                "COMPLETED_WITH_WARNINGS"
            } else {
                "COMPLETED"
            };
            if status.starts_with("COMPLETED") {
                let _ = refresh_library_stats(&db, &library_id).await;
            }
            // Publish the terminal status only after all scan work (including the
            // statistics refresh) is done, so a following scan can start immediately.
            state.scanning_libraries.lock().await.remove(&library_id);
            let _ =
                sqlx::query("UPDATE scan_jobs SET status=?,progress=100,finished_at=? WHERE id=?")
                    .bind(status)
                    .bind(now())
                    .bind(&job_id)
                    .execute(&db.pool)
                    .await;
            let action = match status {
                "CANCELLED" => "SCAN_CANCELLED",
                _ => "SCAN_COMPLETED",
            };
            let _ = db
                .audit(
                    None,
                    action,
                    "scan_job",
                    Some(&job_id),
                    json!({"libraryId":library_id}),
                    None,
                )
                .await;
            if status.starts_with("COMPLETED") {
                let _ = sqlx::query(
                    "UPDATE libraries SET last_scan_at=?,last_successful_scan_at=?,last_error=NULL,last_error_at=NULL WHERE id=?",
                )
                .bind(now())
                .bind(now())
                .bind(&library_id)
                .execute(&db.pool)
                .await;
                state.recommendations.invalidate_all().await;
            }
            tracing::info!(scan_id=%job_id, library_id=%library_id, scan_status=status, warnings, "library scan finished");
        }
        Err(error) => {
            tracing::error!(scan_id=%job_id,library_id=%library_id,%error,"scan failed");
            let _ = sqlx::query(
                "UPDATE scan_jobs SET status='FAILED',error_message=?,finished_at=? WHERE id=?",
            )
            .bind(error.to_string())
            .bind(now())
            .bind(&job_id)
            .execute(&db.pool)
            .await;
            let _ = sqlx::query("UPDATE libraries SET last_error=?,last_error_at=? WHERE id=?")
                .bind(error.to_string())
                .bind(now())
                .bind(&library_id)
                .execute(&db.pool)
                .await;
            let _ = db
                .audit(
                    None,
                    "SCAN_FAILED",
                    "scan_job",
                    Some(&job_id),
                    json!({"libraryId":library_id}),
                    None,
                )
                .await;
        }
    }
    state.scan_cancellations.lock().await.remove(&job_id);
    state.scanning_libraries.lock().await.remove(&library_id);
}

async fn auto_identify_one(state: &AppState, file_id: &str) -> AppResult<()> {
    let db = state.database().await;
    let (library_id, kind, language, parsed) = file_context(&db, file_id).await?;
    let region: Option<String> = sqlx::query("SELECT metadata_region FROM libraries WHERE id=?")
        .bind(&library_id)
        .fetch_one(&db.pool)
        .await?
        .try_get("metadata_region")
        .ok();
    let provider = provider(state)?;
    if kind == LibraryType::TvShow
        && let Some(item_id) =
            existing_series_match(&db, &library_id, &parsed.title, file_id).await?
    {
        associate_file(&db, file_id, &item_id, "MATCHED_AUTO", kind, &parsed).await?;
        return Ok(());
    }
    if !provider.configured() {
        sqlx::query(
            "UPDATE media_files SET identification_status='UNMATCHED',updated_at=? WHERE id=?",
        )
        .bind(now())
        .bind(file_id)
        .execute(&db.pool)
        .await?;
        return Ok(());
    }
    let best = best_search_candidate(
        &db,
        &provider,
        kind,
        &parsed.title,
        parsed.year,
        &language,
        region.as_deref(),
    )
    .await?;
    if let Some((score, candidate)) = best {
        if score >= 0.90 {
            let details =
                cached_details(&db, &provider, kind, candidate.provider_id, &language).await?;
            let item_id = persist_metadata(&db, &library_id, kind, &language, &details).await?;
            if kind == LibraryType::TvShow
                && let Some(season) = parsed.season
            {
                let season_details =
                    cached_season(&db, &provider, candidate.provider_id, season, &language).await?;
                persist_tv_season(&db, &item_id, &season_details).await?;
            }
            associate_file(&db, file_id, &item_id, "MATCHED_AUTO", kind, &parsed).await?;
        } else {
            sqlx::query(
                "UPDATE media_files SET identification_status='AMBIGUOUS',updated_at=? WHERE id=?",
            )
            .bind(now())
            .bind(file_id)
            .execute(&db.pool)
            .await?;
        }
    } else {
        sqlx::query(
            "UPDATE media_files SET identification_status='UNMATCHED',updated_at=? WHERE id=?",
        )
        .bind(now())
        .bind(file_id)
        .execute(&db.pool)
        .await?;
    }
    Ok(())
}

async fn run_scan_inner(
    state: &AppState,
    library_id: &str,
    job_id: &str,
    _scan_type: &str,
    cancel: watch::Receiver<bool>,
) -> AppResult<u64> {
    let _slot = state
        .scan_slots
        .acquire()
        .await
        .map_err(|_| AppError::config("Scan queue is unavailable."))?;
    let db = state.database().await;
    let library = library_row(&db, library_id).await?;
    let kind = LibraryType::parse(&library.get::<String, _>("library_type"))?;
    let language: String = library.get("metadata_language");
    let region: Option<String> = library.try_get("metadata_region").ok();
    let started = Instant::now();
    sqlx::query("UPDATE scan_jobs SET status='SCANNING',started_at=? WHERE id=?")
        .bind(now())
        .bind(job_id)
        .execute(&db.pool)
        .await?;
    db.audit(
        None,
        "SCAN_STARTED",
        "scan_job",
        Some(job_id),
        json!({"libraryId":library_id}),
        None,
    )
    .await?;
    let paths = sqlx::query("SELECT id,path FROM library_paths WHERE library_id=? AND is_active=1")
        .bind(library_id)
        .fetch_all(&db.pool)
        .await?;
    let seen_at = now();
    let mut warnings = 0_u64;
    let mut discovered = 0_i64;
    let mut processed = 0_i64;
    let mut matched = 0_i64;
    let mut unmatched = 0_i64;
    let mut failed = 0_i64;
    let mut batches = 0_i64;
    let provider = provider(state)?;
    for path_row in paths {
        if *cancel.borrow() {
            break;
        }
        let path_id: String = path_row.get("id");
        let root = PathBuf::from(path_row.get::<String, _>("path"));
        if std::fs::read_dir(&root).is_err() {
            warnings += 1;
            sqlx::query(
                "UPDATE library_paths SET status='PATH_UNAVAILABLE',last_checked_at=?,last_error='Path is unavailable or unreadable.',updated_at=? WHERE id=?",
            )
            .bind(now())
            .bind(now())
            .bind(&path_id)
            .execute(&db.pool)
            .await?;
            continue;
        }
        sqlx::query("UPDATE library_paths SET status='AVAILABLE',last_checked_at=?,last_available_at=?,last_error=NULL WHERE id=?")
            .bind(now())
            .bind(now())
            .bind(&path_id)
            .execute(&db.pool)
            .await?;
        let (sender, mut receiver) = mpsc::channel(state.config.scan_batch_size.saturating_mul(2));
        let root_clone = root.clone();
        let cancel_discovery = cancel.clone();
        let discovery =
            tokio::spawn(async move { discover(root_clone, sender, cancel_discovery).await });
        let mut batch = Vec::with_capacity(state.config.scan_batch_size);
        while let Some(file) = receiver.recv().await {
            discovered += 1;
            batch.push(file);
            if batch.len() >= state.config.scan_batch_size {
                let outcome = process_batch(
                    &db,
                    &provider,
                    library_id,
                    &path_id,
                    kind,
                    &language,
                    region.as_deref(),
                    &seen_at,
                    std::mem::take(&mut batch),
                    cancel.clone(),
                )
                .await?;
                processed += outcome.0;
                matched += outcome.1;
                unmatched += outcome.2;
                failed += outcome.3;
                batches += 1;
                update_scan_counts(
                    &db, job_id, discovered, processed, matched, unmatched, failed, batches,
                )
                .await?;
            }
        }
        if !batch.is_empty() {
            let outcome = process_batch(
                &db,
                &provider,
                library_id,
                &path_id,
                kind,
                &language,
                region.as_deref(),
                &seen_at,
                batch,
                cancel.clone(),
            )
            .await?;
            processed += outcome.0;
            matched += outcome.1;
            unmatched += outcome.2;
            failed += outcome.3;
            batches += 1;
        }
        match discovery.await {
            Ok(Ok(_)) => {}
            _ => {
                warnings += 1;
            }
        }
        if !*cancel.borrow() {
            let removed=sqlx::query("UPDATE media_files SET scan_status='MISSING',missing_since=COALESCE(missing_since,?),updated_at=? WHERE library_path_id=? AND last_seen_at<? AND scan_status<>'MISSING'").bind(now()).bind(now()).bind(&path_id).bind(&seen_at).execute(&db.pool).await?.rows_affected();
            sqlx::query("UPDATE scan_jobs SET removed_files=removed_files+? WHERE id=?")
                .bind(removed as i64)
                .bind(job_id)
                .execute(&db.pool)
                .await?;
        }
        sqlx::query("UPDATE library_paths SET last_scan_at=?,updated_at=? WHERE id=?")
            .bind(now())
            .bind(now())
            .bind(&path_id)
            .execute(&db.pool)
            .await?;
    }
    update_scan_counts(
        &db, job_id, discovered, processed, matched, unmatched, failed, batches,
    )
    .await?;
    sqlx::query("UPDATE scan_jobs SET discovery_ms=? WHERE id=?")
        .bind(started.elapsed().as_millis() as i64)
        .bind(job_id)
        .execute(&db.pool)
        .await?;
    Ok(warnings + failed as u64)
}

#[allow(clippy::too_many_arguments)]
async fn process_batch(
    db: &Database,
    provider: &TmdbMetadataProvider,
    library_id: &str,
    path_id: &str,
    kind: LibraryType,
    language: &str,
    region: Option<&str>,
    seen_at: &str,
    files: Vec<DiscoveredFile>,
    cancel: watch::Receiver<bool>,
) -> AppResult<(i64, i64, i64, i64)> {
    let mut changed = Vec::new();
    let mut tx = db.pool.begin().await?;
    for file in files {
        let parsed = parse_media_path(&file.filename, &file.relative_path, kind);
        let absolute = file.absolute_path.to_string_lossy().to_string();
        let relative = file.relative_path.to_string_lossy().to_string();
        let unchanged=sqlx::query("UPDATE media_files SET last_seen_at=?,scan_status='PRESENT',missing_since=NULL,normalized_title=?,parsed_year=?,parsed_season=?,parsed_episode=?,updated_at=? WHERE library_id=? AND absolute_path=? AND file_size=? AND modified_at=?").bind(seen_at).bind(&parsed.title).bind(parsed.year).bind(parsed.season).bind(parsed.episodes.first().copied()).bind(now()).bind(library_id).bind(&absolute).bind(file.size as i64).bind(&file.modified_at).execute(&mut *tx).await?.rows_affected()>0;
        if unchanged {
            let row = sqlx::query("SELECT id,identification_status FROM media_files WHERE library_id=? AND absolute_path=?")
                .bind(library_id).bind(&absolute).fetch_one(&mut *tx).await?;
            let status: String = row.get("identification_status");
            if matches!(
                status.as_str(),
                "PENDING" | "UNMATCHED" | "AMBIGUOUS" | "ERROR"
            ) {
                changed.push((row.get("id"), parsed));
            }
            continue;
        }
        let existing=sqlx::query("UPDATE media_files SET library_path_id=?,relative_path=?,filename=?,extension=?,file_size=?,modified_at=?,content_type=?,scan_status='PRESENT',identification_status='PENDING',normalized_title=?,parsed_year=?,parsed_season=?,parsed_episode=?,last_seen_at=?,missing_since=NULL,updated_at=? WHERE library_id=? AND absolute_path=?").bind(path_id).bind(&relative).bind(&file.filename).bind(&file.extension).bind(file.size as i64).bind(&file.modified_at).bind(kind.as_str()).bind(&parsed.title).bind(parsed.year).bind(parsed.season).bind(parsed.episodes.first().copied()).bind(seen_at).bind(now()).bind(library_id).bind(&absolute).execute(&mut *tx).await?.rows_affected();
        let file_id = if existing > 0 {
            sqlx::query("SELECT id FROM media_files WHERE library_id=? AND absolute_path=?")
                .bind(library_id)
                .bind(&absolute)
                .fetch_one(&mut *tx)
                .await?
                .get(0)
        } else {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO media_files (id,library_id,library_path_id,absolute_path,relative_path,filename,extension,file_size,modified_at,content_type,scan_status,identification_status,normalized_title,parsed_year,parsed_season,parsed_episode,created_at,updated_at,last_seen_at) VALUES (?,?,?,?,?,?,?,?,?,?,'PRESENT','PENDING',?,?,?,?,?,?,?)").bind(&id).bind(library_id).bind(path_id).bind(&absolute).bind(&relative).bind(&file.filename).bind(&file.extension).bind(file.size as i64).bind(&file.modified_at).bind(kind.as_str()).bind(&parsed.title).bind(parsed.year).bind(parsed.season).bind(parsed.episodes.first().copied()).bind(now()).bind(now()).bind(seen_at).execute(&mut *tx).await?;
            id
        };
        changed.push((file_id, parsed));
    }
    tx.commit().await?;
    let (matched, unmatched, failed) = crate::features::catalog::resolver::identify_pending(
        db, provider, library_id, kind, language, region, &changed, &cancel,
    )
    .await?;
    Ok((changed.len() as i64, matched, unmatched, failed))
}

pub(crate) async fn persist_metadata(
    db: &Database,
    library_id: &str,
    kind: LibraryType,
    language: &str,
    value: &Value,
) -> AppResult<String> {
    let tmdb_id = value["id"].as_i64().ok_or_else(|| {
        AppError::new(
            StatusCode::BAD_GATEWAY,
            "TMDB_INVALID_RESPONSE",
            "TMDB details omitted the id.",
        )
    })?;
    let title = value[if kind == LibraryType::Movie {
        "title"
    } else {
        "name"
    }]
    .as_str()
    .unwrap_or("Untitled");
    let original = value[if kind == LibraryType::Movie {
        "original_title"
    } else {
        "original_name"
    }]
    .as_str();
    let date = value[if kind == LibraryType::Movie {
        "release_date"
    } else {
        "first_air_date"
    }]
    .as_str();
    let year = date
        .and_then(|v| v.get(..4))
        .and_then(|v| v.parse::<i32>().ok());
    let timestamp = now();
    let content_age_rating = content_age_rating(kind, value);
    let existing =
        sqlx::query("SELECT id FROM media_items WHERE library_id=? AND media_type=? AND tmdb_id=?")
            .bind(library_id)
            .bind(kind.as_str())
            .bind(tmdb_id)
            .fetch_optional(&db.pool)
            .await?;
    let id = existing
        .as_ref()
        .map(|r| r.get::<String, _>(0))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if existing.is_some() {
        sqlx::query("UPDATE media_items SET title=?,original_title=?,overview=?,original_language=?,release_date=?,year=?,poster_path=?,backdrop_path=?,rating=?,vote_count=?,popularity=?,adult=?,content_age_rating=?,metadata_language=?,metadata_fetched_at=?,updated_at=? WHERE id=?").bind(title).bind(original).bind(value["overview"].as_str()).bind(value["original_language"].as_str()).bind(date).bind(year).bind(value["poster_path"].as_str()).bind(value["backdrop_path"].as_str()).bind(value["vote_average"].as_f64()).bind(value["vote_count"].as_i64()).bind(value["popularity"].as_f64()).bind(i64::from(value["adult"].as_bool().unwrap_or(false))).bind(content_age_rating).bind(language).bind(&timestamp).bind(&timestamp).bind(&id).execute(&db.pool).await?;
    } else {
        sqlx::query("INSERT INTO media_items (id,library_id,media_type,tmdb_id,title,original_title,overview,original_language,release_date,year,poster_path,backdrop_path,rating,vote_count,popularity,adult,content_age_rating,metadata_language,metadata_source,metadata_fetched_at,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'TMDB',?,?,?)").bind(&id).bind(library_id).bind(kind.as_str()).bind(tmdb_id).bind(title).bind(original).bind(value["overview"].as_str()).bind(value["original_language"].as_str()).bind(date).bind(year).bind(value["poster_path"].as_str()).bind(value["backdrop_path"].as_str()).bind(value["vote_average"].as_f64()).bind(value["vote_count"].as_i64()).bind(value["popularity"].as_f64()).bind(i64::from(value["adult"].as_bool().unwrap_or(false))).bind(content_age_rating).bind(language).bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await?;
    }
    persist_genres_and_credits(db, &id, value).await?;
    if kind == LibraryType::Movie {
        let detail_exists: i64 = sqlx::query("SELECT COUNT(*) FROM movies WHERE media_item_id=?")
            .bind(&id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
        if detail_exists == 0 {
            sqlx::query("INSERT INTO movies (media_item_id,runtime,status,tagline,collection_tmdb_id,collection_name,production_companies,production_countries,spoken_languages,external_ids,keywords) VALUES (?,?,?,?,?,?,?,?,?,?,?)").bind(&id).bind(value["runtime"].as_i64()).bind(value["status"].as_str()).bind(value["tagline"].as_str()).bind(value["belongs_to_collection"]["id"].as_i64()).bind(value["belongs_to_collection"]["name"].as_str()).bind(value["production_companies"].to_string()).bind(value["production_countries"].to_string()).bind(value["spoken_languages"].to_string()).bind(value["external_ids"].to_string()).bind(value["keywords"].to_string()).execute(&db.pool).await?;
        }
    } else {
        let detail_exists: i64 = sqlx::query("SELECT COUNT(*) FROM tv_shows WHERE media_item_id=?")
            .bind(&id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
        if detail_exists == 0 {
            sqlx::query("INSERT INTO tv_shows (media_item_id,last_air_date,status,show_type,number_of_seasons,number_of_episodes,networks,creators,production_companies,external_ids) VALUES (?,?,?,?,?,?,?,?,?,?)").bind(&id).bind(value["last_air_date"].as_str()).bind(value["status"].as_str()).bind(value["type"].as_str()).bind(value["number_of_seasons"].as_i64()).bind(value["number_of_episodes"].as_i64()).bind(value["networks"].to_string()).bind(value["created_by"].to_string()).bind(value["production_companies"].to_string()).bind(value["external_ids"].to_string()).execute(&db.pool).await?;
        }
    }
    Ok(id)
}

fn content_age_rating(kind: LibraryType, value: &Value) -> Option<i64> {
    let ratings = if kind == LibraryType::Movie {
        value["release_dates"]["results"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|country| {
                let code = country["iso_3166_1"].as_str().unwrap_or_default();
                country["release_dates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(move |entry| {
                        entry["certification"]
                            .as_str()
                            .filter(|rating| !rating.trim().is_empty())
                            .map(|rating| (code, rating))
                    })
            })
            .collect::<Vec<_>>()
    } else {
        value["content_ratings"]["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| Some((entry["iso_3166_1"].as_str()?, entry["rating"].as_str()?)))
            .collect::<Vec<_>>()
    };
    ratings
        .iter()
        .find(|(country, _)| *country == "BR")
        .or_else(|| ratings.iter().find(|(country, _)| *country == "US"))
        .or_else(|| ratings.first())
        .and_then(|(_, rating)| normalize_age_rating(rating))
}

fn normalize_age_rating(value: &str) -> Option<i64> {
    let normalized = value.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "L" | "LIVRE" | "G" | "TV-Y" | "TV-G" => Some(0),
        "PG" | "TV-Y7" | "TV-PG" => Some(10),
        "PG-13" | "TV-14" => Some(14),
        "R" => Some(16),
        "NC-17" | "TV-MA" => Some(18),
        _ => [18, 16, 14, 12, 10, 0]
            .into_iter()
            .find(|age| normalized == age.to_string()),
    }
}

async fn persist_genres_and_credits(db: &Database, item_id: &str, value: &Value) -> AppResult<()> {
    for genre in value["genres"].as_array().into_iter().flatten() {
        if let (Some(tmdb), Some(name)) = (genre["id"].as_i64(), genre["name"].as_str()) {
            let row = sqlx::query("SELECT id FROM genres WHERE tmdb_id=?")
                .bind(tmdb)
                .fetch_optional(&db.pool)
                .await?;
            let id = row
                .map(|r| r.get(0))
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            sqlx::query("INSERT INTO genres (id,tmdb_id,name) SELECT ?,?,? WHERE NOT EXISTS (SELECT 1 FROM genres WHERE tmdb_id=?)").bind(&id).bind(tmdb).bind(name).bind(tmdb).execute(&db.pool).await?;
            sqlx::query("INSERT INTO media_genres (media_item_id,genre_id) SELECT ?,? WHERE NOT EXISTS (SELECT 1 FROM media_genres WHERE media_item_id=? AND genre_id=?)").bind(item_id).bind(&id).bind(item_id).bind(&id).execute(&db.pool).await?;
        }
    }
    for (credit_type, key) in [("CAST", "cast"), ("CREW", "crew")] {
        for credit in value["credits"][key]
            .as_array()
            .into_iter()
            .flatten()
            .take(100)
        {
            if let (Some(tmdb), Some(name)) = (credit["id"].as_i64(), credit["name"].as_str()) {
                let row = sqlx::query("SELECT id FROM people WHERE tmdb_id=?")
                    .bind(tmdb)
                    .fetch_optional(&db.pool)
                    .await?;
                let person = row
                    .map(|r| r.get(0))
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                sqlx::query("INSERT INTO people (id,tmdb_id,name,profile_path,known_for_department) SELECT ?,?,?,?,? WHERE NOT EXISTS (SELECT 1 FROM people WHERE tmdb_id=?)").bind(&person).bind(tmdb).bind(name).bind(credit["profile_path"].as_str()).bind(credit["known_for_department"].as_str()).bind(tmdb).execute(&db.pool).await?;
                let character = credit["character"].as_str();
                let job = credit["job"].as_str();
                let exists:i64=sqlx::query("SELECT COUNT(*) FROM credits WHERE media_item_id=? AND person_id=? AND credit_type=?").bind(item_id).bind(&person).bind(credit_type).fetch_one(&db.pool).await?.get(0);
                if exists == 0 {
                    let signature = format!(
                        "{tmdb}:{credit_type}:{}:{}",
                        character.unwrap_or(""),
                        job.unwrap_or("")
                    );
                    let sql = if db.kind == crate::db::DatabaseKind::MySql {
                        "INSERT INTO credits (id,media_item_id,person_id,credit_type,character_name,job,department,credit_order,signature) VALUES (?,?,?,?,?,?,?,?,?)"
                    } else {
                        "INSERT INTO credits (id,media_item_id,person_id,credit_type,character_name,job,department,credit_order) VALUES (?,?,?,?,?,?,?,?)"
                    };
                    let query = sqlx::query(sql)
                        .bind(Uuid::new_v4().to_string())
                        .bind(item_id)
                        .bind(&person)
                        .bind(credit_type)
                        .bind(character)
                        .bind(job)
                        .bind(credit["department"].as_str())
                        .bind(credit["order"].as_i64());
                    if db.kind == crate::db::DatabaseKind::MySql {
                        query.bind(signature).execute(&db.pool).await?;
                    } else {
                        query.execute(&db.pool).await?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) async fn associate_file(
    db: &Database,
    file_id: &str,
    item_id: &str,
    status: &str,
    kind: LibraryType,
    parsed: &ParsedName,
) -> AppResult<()> {
    let mut episode_id: Option<String> = None;
    if kind == LibraryType::TvShow
        && let (Some(season), Some(episode)) = (parsed.season, parsed.episodes.first())
    {
        let row = sqlx::query("SELECT id FROM tv_seasons WHERE tv_show_id=? AND season_number=?")
            .bind(item_id)
            .bind(season)
            .fetch_optional(&db.pool)
            .await?;
        let season_id = row
            .map(|r| r.get(0))
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        sqlx::query("INSERT INTO tv_seasons (id,tv_show_id,season_number,name) SELECT ?,?,?,? WHERE NOT EXISTS (SELECT 1 FROM tv_seasons WHERE tv_show_id=? AND season_number=?)").bind(&season_id).bind(item_id).bind(season).bind(format!("Season {season}" )).bind(item_id).bind(season).execute(&db.pool).await?;
        let row = sqlx::query("SELECT id FROM tv_episodes WHERE season_id=? AND episode_number=?")
            .bind(&season_id)
            .bind(episode)
            .fetch_optional(&db.pool)
            .await?;
        let id = row
            .map(|r| r.get(0))
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        sqlx::query("INSERT INTO tv_episodes (id,tv_show_id,season_id,season_number,episode_number) SELECT ?,?,?,?,? WHERE NOT EXISTS (SELECT 1 FROM tv_episodes WHERE season_id=? AND episode_number=?)").bind(&id).bind(item_id).bind(&season_id).bind(season).bind(episode).bind(&season_id).bind(episode).execute(&db.pool).await?;
        episode_id = Some(id);
    }
    sqlx::query("UPDATE media_files SET media_item_id=?,tv_episode_id=?,identification_status=?,updated_at=? WHERE id=?").bind(item_id).bind(episode_id).bind(status).bind(now()).bind(file_id).execute(&db.pool).await?;
    Ok(())
}

pub(crate) async fn existing_series_match(
    db: &Database,
    library_id: &str,
    normalized_title: &str,
    excluding_file_id: &str,
) -> AppResult<Option<String>> {
    Ok(sqlx::query("SELECT media_item_id FROM media_files WHERE library_id=? AND normalized_title=? AND id<>? AND media_item_id IS NOT NULL AND missing_since IS NULL LIMIT 1")
        .bind(library_id).bind(normalized_title).bind(excluding_file_id).fetch_optional(&db.pool).await?.map(|row| row.get(0)))
}

async fn cached_search(
    db: &Database,
    provider: &TmdbMetadataProvider,
    kind: LibraryType,
    query: &str,
    year: Option<i32>,
    language: &str,
    region: Option<&str>,
) -> AppResult<Vec<SearchCandidate>> {
    let key = format!(
        "search:{}:{}:{}:{}",
        kind.as_str(),
        query.to_lowercase(),
        year.map_or_else(String::new, |v| v.to_string()),
        region.unwrap_or("")
    );
    if let Some(row)=sqlx::query("SELECT payload FROM metadata_cache WHERE provider='TMDB' AND request_key=? AND language=? AND expires_at>?").bind(&key).bind(language).bind(now()).fetch_optional(&db.pool).await?
        && let Ok(items)=serde_json::from_str(&row.get::<String,_>(0)) { return Ok(items); }
    let items = provider.search(kind, query, year, language, region).await?;
    cache_value(
        db,
        &key,
        "search",
        None,
        language,
        &serde_json::to_value(&items).unwrap_or(Value::Array(vec![])),
        Duration::from_secs(86400),
    )
    .await?;
    Ok(items)
}

pub(crate) async fn best_search_candidate(
    db: &Database,
    provider: &TmdbMetadataProvider,
    kind: LibraryType,
    query: &str,
    year: Option<i32>,
    language: &str,
    region: Option<&str>,
) -> AppResult<Option<(f64, SearchCandidate)>> {
    let candidates = cached_search(db, provider, kind, query, year, language, region).await?;
    let mut best = candidates
        .into_iter()
        .map(|candidate| (confidence(query, year, &candidate), candidate))
        .max_by(|a, b| a.0.total_cmp(&b.0));

    // Release folders often contain the pack year instead of the original air year.
    // Retry broadly only when the year-constrained result is not safe to auto-match.
    if year.is_some() && best.as_ref().is_none_or(|(score, _)| *score < 0.90) {
        let broad = cached_search(db, provider, kind, query, None, language, region).await?;
        if let Some(candidate) = broad
            .into_iter()
            .map(|candidate| (confidence(query, None, &candidate), candidate))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            && best.as_ref().is_none_or(|(score, _)| candidate.0 > *score)
        {
            best = Some(candidate);
        }
    }
    Ok(best)
}
pub(crate) async fn cached_details(
    db: &Database,
    provider: &TmdbMetadataProvider,
    kind: LibraryType,
    id: i64,
    language: &str,
) -> AppResult<Value> {
    let key = format!("details:{}:{id}", kind.as_str());
    if let Some(row)=sqlx::query("SELECT payload FROM metadata_cache WHERE provider='TMDB' AND request_key=? AND language=? AND expires_at>?").bind(&key).bind(language).bind(now()).fetch_optional(&db.pool).await?
        && let Ok(value)=serde_json::from_str(&row.get::<String,_>(0)) { return Ok(value); }
    let value = provider.details(kind, id, language).await?;
    cache_value(
        db,
        &key,
        "details",
        Some(id),
        language,
        &value,
        Duration::from_secs(604800),
    )
    .await?;
    Ok(value)
}
pub(crate) async fn cached_season(
    db: &Database,
    provider: &TmdbMetadataProvider,
    show_id: i64,
    season: i32,
    language: &str,
) -> AppResult<Value> {
    let key = format!("season:{show_id}:{season}");
    if let Some(row)=sqlx::query("SELECT payload FROM metadata_cache WHERE provider='TMDB' AND request_key=? AND language=? AND expires_at>?").bind(&key).bind(language).bind(now()).fetch_optional(&db.pool).await?
        && let Ok(value)=serde_json::from_str(&row.get::<String,_>(0)){return Ok(value);}
    let value = provider.season_details(show_id, season, language).await?;
    cache_value(
        db,
        &key,
        "season",
        Some(show_id),
        language,
        &value,
        Duration::from_secs(604800),
    )
    .await?;
    Ok(value)
}

pub(crate) async fn persist_tv_season(
    db: &Database,
    show_id: &str,
    value: &Value,
) -> AppResult<()> {
    let number = value["season_number"].as_i64().unwrap_or_default();
    let existing = sqlx::query("SELECT id FROM tv_seasons WHERE tv_show_id=? AND season_number=?")
        .bind(show_id)
        .bind(number)
        .fetch_optional(&db.pool)
        .await?;
    let season_id = existing
        .as_ref()
        .map(|row| row.get::<String, _>(0))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if existing.is_some() {
        sqlx::query("UPDATE tv_seasons SET tmdb_id=?,name=?,overview=?,air_date=?,poster_path=?,episode_count=? WHERE id=?").bind(value["id"].as_i64()).bind(value["name"].as_str()).bind(value["overview"].as_str()).bind(value["air_date"].as_str()).bind(value["poster_path"].as_str()).bind(value["episodes"].as_array().map(|items|items.len() as i64)).bind(&season_id).execute(&db.pool).await?;
    } else {
        sqlx::query("INSERT INTO tv_seasons (id,tv_show_id,tmdb_id,season_number,name,overview,air_date,poster_path,episode_count) VALUES (?,?,?,?,?,?,?,?,?)").bind(&season_id).bind(show_id).bind(value["id"].as_i64()).bind(number).bind(value["name"].as_str()).bind(value["overview"].as_str()).bind(value["air_date"].as_str()).bind(value["poster_path"].as_str()).bind(value["episodes"].as_array().map(|items|items.len() as i64)).execute(&db.pool).await?;
    }
    for episode in value["episodes"].as_array().into_iter().flatten() {
        let episode_number = episode["episode_number"].as_i64().unwrap_or_default();
        let existing =
            sqlx::query("SELECT id FROM tv_episodes WHERE season_id=? AND episode_number=?")
                .bind(&season_id)
                .bind(episode_number)
                .fetch_optional(&db.pool)
                .await?;
        let id = existing
            .as_ref()
            .map(|row| row.get::<String, _>(0))
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        if existing.is_some() {
            sqlx::query("UPDATE tv_episodes SET tmdb_id=?,name=?,overview=?,air_date=?,still_path=?,rating=?,vote_count=?,runtime=? WHERE id=?").bind(episode["id"].as_i64()).bind(episode["name"].as_str()).bind(episode["overview"].as_str()).bind(episode["air_date"].as_str()).bind(episode["still_path"].as_str()).bind(episode["vote_average"].as_f64()).bind(episode["vote_count"].as_i64()).bind(episode["runtime"].as_i64()).bind(&id).execute(&db.pool).await?;
        } else {
            sqlx::query("INSERT INTO tv_episodes (id,tv_show_id,season_id,tmdb_id,season_number,episode_number,name,overview,air_date,still_path,rating,vote_count,runtime) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(&id).bind(show_id).bind(&season_id).bind(episode["id"].as_i64()).bind(number).bind(episode_number).bind(episode["name"].as_str()).bind(episode["overview"].as_str()).bind(episode["air_date"].as_str()).bind(episode["still_path"].as_str()).bind(episode["vote_average"].as_f64()).bind(episode["vote_count"].as_i64()).bind(episode["runtime"].as_i64()).execute(&db.pool).await?;
        }
    }
    Ok(())
}
async fn cache_value(
    db: &Database,
    key: &str,
    response_type: &str,
    entity_id: Option<i64>,
    language: &str,
    value: &Value,
    ttl: Duration,
) -> AppResult<()> {
    let expires =
        (chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default()).to_rfc3339();
    let updated=sqlx::query("UPDATE metadata_cache SET response_type=?,entity_id=?,payload=?,expires_at=?,created_at=? WHERE provider='TMDB' AND request_key=? AND language=?").bind(response_type).bind(entity_id).bind(value.to_string()).bind(&expires).bind(now()).bind(key).bind(language).execute(&db.pool).await?.rows_affected();
    if updated == 0 {
        sqlx::query("INSERT INTO metadata_cache (provider,request_key,response_type,entity_id,language,payload,expires_at,created_at) VALUES ('TMDB',?,?,?,?,?,?,?)").bind(key).bind(response_type).bind(entity_id).bind(language).bind(value.to_string()).bind(expires).bind(now()).execute(&db.pool).await?;
    }
    Ok(())
}

pub(crate) fn provider(state: &AppState) -> AppResult<TmdbMetadataProvider> {
    TmdbMetadataProvider::new(
        state.config.tmdb_api_key.clone(),
        state.config.tmdb_timeout_seconds,
        state.metadata_slots.clone(),
    )
}
async fn file_context(
    db: &Database,
    file_id: &str,
) -> AppResult<(String, LibraryType, String, ParsedName)> {
    let row=sqlx::query("SELECT f.library_id,f.normalized_title,f.parsed_year,f.parsed_season,f.parsed_episode,l.library_type,l.metadata_language FROM media_files f JOIN libraries l ON l.id=f.library_id WHERE f.id=?").bind(file_id).fetch_optional(&db.pool).await?.ok_or_else(||AppError::not_found("MEDIA_FILE_NOT_FOUND","Media file was not found."))?;
    let kind = LibraryType::parse(&row.get::<String, _>("library_type"))?;
    Ok((
        row.get("library_id"),
        kind,
        row.get("metadata_language"),
        ParsedName {
            title: row.try_get("normalized_title").unwrap_or_default(),
            year: row.try_get::<i64, _>("parsed_year").ok().map(|v| v as i32),
            season: row
                .try_get::<i64, _>("parsed_season")
                .ok()
                .map(|v| v as i32),
            episodes: row
                .try_get::<i64, _>("parsed_episode")
                .ok()
                .map(|v| vec![v as i32])
                .unwrap_or_default(),
            noise: vec![],
        },
    ))
}
async fn ensure_library_access(
    state: &AppState,
    auth: &AuthUser,
    headers: &HeaderMap,
    id: &str,
) -> AppResult<()> {
    let row = library_row(&state.database().await, id).await?;
    if row.get::<String, _>("privacy") == "PUBLIC" {
        return Ok(());
    }
    let valid = headers
        .get("x-library-unlock")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|token| state.tokens.verify_library_unlock(token, &auth.id, id));
    if valid {
        Ok(())
    } else {
        Err(AppError::new(
            StatusCode::LOCKED,
            "LIBRARY_LOCKED",
            "Unlock this private library before accessing its content.",
        ))
    }
}
async fn ensure_paths_available(
    db: &Database,
    paths: &[PathBuf],
    library_id: Option<&str>,
) -> AppResult<()> {
    let rows =
        sqlx::query("SELECT library_id,normalized_path FROM library_paths WHERE is_active=1")
            .fetch_all(&db.pool)
            .await?;
    for path in paths {
        for row in &rows {
            if library_id.is_some_and(|id| id == row.get::<String, _>("library_id")) {
                continue;
            }
            if paths_overlap(path, FsPath::new(&row.get::<String, _>("normalized_path"))) {
                return Err(AppError::conflict(
                    "OVERLAPPING_LIBRARY_PATH",
                    "The path overlaps an existing library path.",
                ));
            }
        }
    }
    Ok(())
}
async fn insert_path<'e, E: Executor<'e, Database = Any>>(
    executor: E,
    library_id: &str,
    path: &FsPath,
    timestamp: &str,
) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    let rendered = path.to_string_lossy().to_string();
    sqlx::query("INSERT INTO library_paths (id,library_id,path,normalized_path,is_active,status,created_at,updated_at) VALUES (?,?,?, ?,1,'AVAILABLE',?,?)").bind(&id).bind(library_id).bind(&rendered).bind(&rendered).bind(timestamp).bind(timestamp).execute(executor).await?;
    Ok(id)
}
async fn insert_audit<'e, E: Executor<'e, Database = Any>>(
    executor: E,
    actor: &str,
    action: &str,
    entity_type: &str,
    entity_id: &str,
    metadata: Value,
) -> AppResult<()> {
    sqlx::query("INSERT INTO audit_log (id,actor_user_id,action,entity_type,entity_id,metadata,created_at) VALUES (?,?,?,?,?,?,?)").bind(Uuid::new_v4().to_string()).bind(actor).bind(action).bind(entity_type).bind(entity_id).bind(metadata.to_string()).bind(now()).execute(executor).await?;
    Ok(())
}
async fn library_row(db: &Database, id: &str) -> AppResult<sqlx::any::AnyRow> {
    sqlx::query("SELECT id,name,description,library_type,privacy,password_hash,minimum_age,metadata_language,metadata_region,is_active,scan_enabled,created_by,created_at,updated_at,last_scan_at,last_successful_scan_at,auto_sync_enabled,auto_sync_mode,auto_sync_interval_minutes,auto_sync_hour,auto_sync_minute,scan_on_startup,next_sync_at,last_auto_sync_at,last_error,last_error_at FROM libraries WHERE id=? AND deleted_at IS NULL").bind(id).fetch_optional(&db.pool).await?.ok_or_else(not_found_library)
}
fn not_found_library() -> AppError {
    AppError::not_found("LIBRARY_NOT_FOUND", "Library was not found.")
}
async fn library_json(db: &Database, id: &str) -> AppResult<Value> {
    let row = library_row(db, id).await?;
    let paths=sqlx::query("SELECT id,path,is_active,status,created_at,updated_at,last_scan_at,last_checked_at,last_available_at,last_error FROM library_paths WHERE library_id=? AND is_active=1 ORDER BY path").bind(id).fetch_all(&db.pool).await?;
    let active_scan: Option<String> = sqlx::query("SELECT trigger_source FROM scan_jobs WHERE library_id=? AND status IN ('QUEUED','SCANNING','MATCHING','PERSISTING') ORDER BY created_at DESC LIMIT 1").bind(id).fetch_optional(&db.pool).await?.map(|value| value.get(0));
    let unavailable = paths
        .iter()
        .any(|path| path.get::<String, _>("status") != "AVAILABLE");
    let operational_status = if row.get::<i64, _>("is_active") == 0 {
        "DISABLED"
    } else if let Some(source) = active_scan {
        if source == "MANUAL" {
            "SCANNING"
        } else {
            "SYNCING"
        }
    } else if unavailable {
        "PATH_UNAVAILABLE"
    } else if row
        .try_get::<Option<String>, _>("last_error")
        .ok()
        .flatten()
        .is_some()
    {
        "ERROR"
    } else {
        "READY"
    };
    let stats = library_stats_json(db, id)
        .await
        .unwrap_or_else(|_| json!({"totalSizeBytes":0,"fileCount":0,"mediaItemCount":0}));
    Ok(
        json!({"id":row.get::<String,_>("id"),"name":row.get::<String,_>("name"),"description":row.try_get::<String,_>("description").ok(),"type":row.get::<String,_>("library_type"),"privacy":row.get::<String,_>("privacy"),"minimumAge":row.get::<i64,_>("minimum_age"),"metadataLanguage":row.get::<String,_>("metadata_language"),"metadataRegion":row.try_get::<String,_>("metadata_region").ok(),"isActive":row.get::<i64,_>("is_active")!=0,"scanEnabled":row.get::<i64,_>("scan_enabled")!=0,"operationalStatus":operational_status,"createdAt":row.get::<String,_>("created_at"),"updatedAt":row.get::<String,_>("updated_at"),"lastScanAt":row.try_get::<Option<String>,_>("last_scan_at").ok().flatten(),"lastSuccessfulScanAt":row.try_get::<Option<String>,_>("last_successful_scan_at").ok().flatten(),"nextSyncAt":row.try_get::<Option<String>,_>("next_sync_at").ok().flatten(),"lastAutoSyncAt":row.try_get::<Option<String>,_>("last_auto_sync_at").ok().flatten(),"lastError":row.try_get::<Option<String>,_>("last_error").ok().flatten(),"autoSync":{"enabled":row.get::<i64,_>("auto_sync_enabled")!=0,"mode":row.get::<String,_>("auto_sync_mode"),"intervalMinutes":row.get::<i64,_>("auto_sync_interval_minutes"),"schedule":{"hour":row.get::<i64,_>("auto_sync_hour"),"minute":row.get::<i64,_>("auto_sync_minute")},"scanOnStartup":row.get::<i64,_>("scan_on_startup")!=0},"stats":stats,"paths":paths.iter().map(|p|json!({"id":p.get::<String,_>("id"),"path":p.get::<String,_>("path"),"isActive":p.get::<i64,_>("is_active")!=0,"status":p.get::<String,_>("status"),"lastScanAt":p.try_get::<Option<String>,_>("last_scan_at").ok().flatten(),"lastCheckedAt":p.try_get::<Option<String>,_>("last_checked_at").ok().flatten(),"lastAvailableAt":p.try_get::<Option<String>,_>("last_available_at").ok().flatten(),"lastError":p.try_get::<Option<String>,_>("last_error").ok().flatten()})).collect::<Vec<_>>() }),
    )
}

pub(crate) async fn refresh_library_stats(db: &Database, id: &str) -> AppResult<()> {
    let files = sqlx::query("SELECT COALESCE(SUM(file_size),0) total_size,COUNT(*) file_count,COALESCE(SUM(CASE WHEN identification_status IN ('UNMATCHED','AMBIGUOUS') THEN 1 ELSE 0 END),0) unmatched_count,COALESCE(SUM(CASE WHEN missing_since IS NOT NULL THEN 1 ELSE 0 END),0) missing_count FROM media_files WHERE library_id=?").bind(id).fetch_one(&db.pool).await?;
    let items = sqlx::query("SELECT COUNT(*) item_count,COALESCE(SUM(CASE WHEN media_type='MOVIE' THEN 1 ELSE 0 END),0) movie_count,COALESCE(SUM(CASE WHEN media_type='TV_SHOW' THEN 1 ELSE 0 END),0) tv_count FROM media_items WHERE library_id=?").bind(id).fetch_one(&db.pool).await?;
    let seasons: i64 = sqlx::query("SELECT COUNT(*) FROM tv_seasons s JOIN media_items i ON i.id=s.tv_show_id WHERE i.library_id=?").bind(id).fetch_one(&db.pool).await?.get(0);
    let episodes: i64 = sqlx::query("SELECT COUNT(*) FROM tv_episodes e JOIN media_items i ON i.id=e.tv_show_id WHERE i.library_id=?").bind(id).fetch_one(&db.pool).await?.get(0);
    let paths: i64 =
        sqlx::query("SELECT COUNT(*) FROM library_paths WHERE library_id=? AND is_active=1")
            .bind(id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
    let timestamp = now();
    let values = (
        files.get::<i64, _>("total_size"),
        files.get::<i64, _>("file_count"),
        items.get::<i64, _>("item_count"),
        items.get::<i64, _>("movie_count"),
        items.get::<i64, _>("tv_count"),
        seasons,
        episodes,
        files.get::<i64, _>("unmatched_count"),
        files.get::<i64, _>("missing_count"),
        paths,
    );
    let updated = sqlx::query("UPDATE library_stats SET total_size_bytes=?,file_count=?,media_item_count=?,movie_count=?,tv_show_count=?,season_count=?,episode_count=?,unmatched_count=?,missing_count=?,path_count=?,updated_at=? WHERE library_id=?")
        .bind(values.0).bind(values.1).bind(values.2).bind(values.3).bind(values.4).bind(values.5).bind(values.6).bind(values.7).bind(values.8).bind(values.9).bind(&timestamp).bind(id).execute(&db.pool).await?;
    if updated.rows_affected() == 0 {
        sqlx::query("INSERT INTO library_stats (library_id,total_size_bytes,file_count,media_item_count,movie_count,tv_show_count,season_count,episode_count,unmatched_count,missing_count,path_count,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id).bind(values.0).bind(values.1).bind(values.2).bind(values.3).bind(values.4).bind(values.5).bind(values.6).bind(values.7).bind(values.8).bind(values.9).bind(timestamp).execute(&db.pool).await?;
    }
    Ok(())
}

async fn library_stats_json(db: &Database, id: &str) -> AppResult<Value> {
    let row = sqlx::query("SELECT total_size_bytes,file_count,media_item_count,movie_count,tv_show_count,season_count,episode_count,unmatched_count,missing_count,path_count,updated_at FROM library_stats WHERE library_id=?").bind(id).fetch_optional(&db.pool).await?;
    let Some(row) = row else {
        return Ok(
            json!({"totalSizeBytes":0,"fileCount":0,"mediaItemCount":0,"movieCount":0,"tvShowCount":0,"seasonCount":0,"episodeCount":0,"unmatchedCount":0,"missingCount":0,"pathCount":0,"updatedAt":null}),
        );
    };
    Ok(
        json!({"totalSizeBytes":row.get::<i64,_>("total_size_bytes"),"fileCount":row.get::<i64,_>("file_count"),"mediaItemCount":row.get::<i64,_>("media_item_count"),"movieCount":row.get::<i64,_>("movie_count"),"tvShowCount":row.get::<i64,_>("tv_show_count"),"seasonCount":row.get::<i64,_>("season_count"),"episodeCount":row.get::<i64,_>("episode_count"),"unmatchedCount":row.get::<i64,_>("unmatched_count"),"missingCount":row.get::<i64,_>("missing_count"),"pathCount":row.get::<i64,_>("path_count"),"updatedAt":row.get::<String,_>("updated_at")}),
    )
}
fn page_bounds(page: i64, size: i64) -> (i64, i64) {
    let limit = size.clamp(1, 200);
    (limit, (page.max(1) - 1) * limit)
}
fn redact_library_paths(mut value: Value, auth: &AuthUser) -> Value {
    if !auth
        .permissions
        .iter()
        .any(|permission| permission == "libraries.manage")
        && let Some(object) = value.as_object_mut()
    {
        object.remove("paths");
    }
    value
}
fn page_size(size: i64) -> i64 {
    size.clamp(1, 200)
}
const SCAN_SELECT: &str = "SELECT id,library_id,status,scan_type,trigger_source,started_at,finished_at,discovered_files,processed_files,matched_files,unmatched_files,skipped_files,removed_files,failed_files,progress,error_message,created_by,created_at,discovery_ms,matching_ms,persistence_ms,tmdb_requests,tmdb_cache_hits,db_batches FROM scan_jobs";
fn scan_json(row: &sqlx::any::AnyRow) -> Value {
    json!({"id":row.get::<String,_>("id"),"libraryId":row.get::<String,_>("library_id"),"status":row.get::<String,_>("status"),"scanType":row.get::<String,_>("scan_type"),"triggerSource":row.get::<String,_>("trigger_source"),"startedAt":row.try_get::<Option<String>,_>("started_at").ok().flatten(),"finishedAt":row.try_get::<Option<String>,_>("finished_at").ok().flatten(),"discoveredFiles":row.get::<i64,_>("discovered_files"),"processedFiles":row.get::<i64,_>("processed_files"),"matchedFiles":row.get::<i64,_>("matched_files"),"unmatchedFiles":row.get::<i64,_>("unmatched_files"),"skippedFiles":row.get::<i64,_>("skipped_files"),"removedFiles":row.get::<i64,_>("removed_files"),"failedFiles":row.get::<i64,_>("failed_files"),"progress":row.get::<f64,_>("progress"),"errorMessage":row.try_get::<Option<String>,_>("error_message").ok().flatten(),"createdAt":row.get::<String,_>("created_at"),"metrics":{"discoveryMs":row.get::<i64,_>("discovery_ms"),"matchingMs":row.get::<i64,_>("matching_ms"),"persistenceMs":row.get::<i64,_>("persistence_ms"),"tmdbRequests":row.get::<i64,_>("tmdb_requests"),"tmdbCacheHits":row.get::<i64,_>("tmdb_cache_hits"),"dbBatches":row.get::<i64,_>("db_batches")}})
}
fn item_json(row: &sqlx::any::AnyRow) -> Value {
    json!({"id":row.get::<String,_>("id"),"type":row.get::<String,_>("media_type"),"tmdbId":row.get::<i64,_>("tmdb_id"),"title":row.get::<String,_>("title"),"originalTitle":row.try_get::<String,_>("original_title").ok(),"overview":row.try_get::<String,_>("overview").ok(),"releaseDate":row.try_get::<String,_>("release_date").ok(),"year":row.try_get::<i64,_>("year").ok(),"posterPath":row.try_get::<String,_>("poster_path").ok(),"backdropPath":row.try_get::<String,_>("backdrop_path").ok(),"rating":row.try_get::<f64,_>("rating").ok(),"voteCount":row.try_get::<i64,_>("vote_count").ok(),"popularity":row.try_get::<f64,_>("popularity").ok()})
}
#[allow(clippy::too_many_arguments)]
async fn update_scan_counts(
    db: &Database,
    id: &str,
    discovered: i64,
    processed: i64,
    matched: i64,
    unmatched: i64,
    failed: i64,
    batches: i64,
) -> AppResult<()> {
    let progress = if discovered == 0 {
        0.0
    } else {
        processed as f64 / discovered as f64 * 90.0
    };
    sqlx::query("UPDATE scan_jobs SET discovered_files=?,processed_files=?,matched_files=?,unmatched_files=?,failed_files=?,db_batches=?,progress=? WHERE id=?").bind(discovered).bind(processed).bind(matched).bind(unmatched).bind(failed).bind(batches).bind(progress).bind(id).execute(&db.pool).await?;
    Ok(())
}
