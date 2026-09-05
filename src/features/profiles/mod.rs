use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{StatusCode, header},
    response::Response,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthUser,
    db::now,
    errors::{AppError, AppResult},
};

const CATEGORIES: [(&str, &str); 5] = [
    ("dp", "Disney+"),
    ("nf", "Netflix"),
    ("pop", "Pop"),
    ("pp", "Famosos"),
    ("pv", "Prime Video"),
];
const AGE_RATINGS: [i64; 6] = [0, 10, 12, 14, 16, 18];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarEntry {
    pub id: String,
    pub category: String,
    pub name: String,
    pub url: String,
}

#[derive(Clone)]
pub struct AvatarCatalog {
    root: PathBuf,
    index: Arc<RwLock<Option<Vec<AvatarEntry>>>>,
}

impl AvatarCatalog {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            index: Arc::new(RwLock::new(None)),
        }
    }

    async fn entries(&self) -> AppResult<Vec<AvatarEntry>> {
        if let Some(entries) = self.index.read().await.as_ref() {
            return Ok(entries.clone());
        }
        let root = self.root.clone();
        let entries = tokio::task::spawn_blocking(move || scan_avatars(&root))
            .await
            .map_err(|_| AppError::config("Unable to index avatar catalog."))??;
        *self.index.write().await = Some(entries.clone());
        Ok(entries)
    }

    pub async fn contains(&self, id: &str) -> AppResult<bool> {
        if matches!(id, "default.png" | "kids.png") {
            return Ok(true);
        }
        Ok(self.entries().await?.iter().any(|entry| entry.id == id))
    }

    fn file_path(&self, category: &str, filename: &str) -> AppResult<PathBuf> {
        validate_category(category)?;
        validate_filename(filename)?;
        Ok(self.root.join(category).join(filename))
    }
}

fn scan_avatars(root: &Path) -> AppResult<Vec<AvatarEntry>> {
    let mut entries = vec![
        AvatarEntry {
            id: "default.png".into(),
            category: "pop".into(),
            name: "default.png".into(),
            url: "/api/v1/avatars/fallback/default.png".into(),
        },
        AvatarEntry {
            id: "kids.png".into(),
            category: "pop".into(),
            name: "kids.png".into(),
            url: "/api/v1/avatars/fallback/kids.png".into(),
        },
    ];
    for (category, _) in CATEGORIES {
        let directory = root.join(category);
        let Ok(files) = std::fs::read_dir(directory) else {
            continue;
        };
        for file in files.flatten() {
            let Ok(kind) = file.file_type() else { continue };
            if !kind.is_file() {
                continue;
            }
            let name = file.file_name().to_string_lossy().into_owned();
            if validate_filename(&name).is_err() {
                continue;
            }
            let id = format!("{category}/{name}");
            entries.push(AvatarEntry {
                url: format!("/api/v1/avatars/{id}"),
                id,
                category: category.into(),
                name,
            });
        }
    }
    entries.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    Ok(entries)
}

#[derive(Clone, Copy)]
pub struct PinWindow {
    started: Instant,
    attempts: u8,
}

pub async fn allow_pin_attempt(
    limiter: &Mutex<std::collections::HashMap<String, PinWindow>>,
    profile_id: &str,
) -> bool {
    let mut limiter = limiter.lock().await;
    let entry = limiter.entry(profile_id.into()).or_insert(PinWindow {
        started: Instant::now(),
        attempts: 0,
    });
    if entry.started.elapsed() >= Duration::from_secs(60) {
        *entry = PinWindow {
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/profiles", get(list_profiles).post(create_profile))
        .route("/api/v1/profiles/current", get(current_profile))
        .route(
            "/api/v1/profiles/{id}",
            get(get_profile)
                .patch(update_profile)
                .delete(disable_profile),
        )
        .route("/api/v1/profiles/{id}/select", post(select_profile))
        .route("/api/v1/profiles/{id}/pin", put(set_pin).delete(remove_pin))
        .route("/api/v1/profiles/{id}/unlock", post(select_profile))
        .route(
            "/api/v1/profiles/{id}/library-access",
            get(get_library_access).put(update_library_access),
        )
        .route("/api/v1/avatars", get(list_avatars))
        .route("/api/v1/avatars/categories", get(avatar_categories))
        .route("/api/v1/avatars/{category}/{filename}", get(serve_avatar))
        .route(
            "/api/v1/parental-controls/settings",
            get(parental_settings).put(update_parental_settings),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProfileRequest {
    name: String,
    avatar_id: Option<String>,
    #[serde(default)]
    is_kids: bool,
    max_age_rating: Option<i64>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProfileRequest {
    name: Option<String>,
    avatar_id: Option<String>,
    is_kids: Option<bool>,
    max_age_rating: Option<i64>,
    is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PinRequest {
    pin: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvatarQuery {
    category: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileLibraryAccessRequest {
    library_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListProfilesQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParentalSettingsRequest {
    unknown_kids_policy: String,
}

async fn list_profiles(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListProfilesQuery>,
) -> AppResult<Json<Value>> {
    let db = state.database().await;
    let user_id = query.user_id.as_deref().unwrap_or(&auth.id);
    if user_id != auth.id {
        auth.require("users.view")?;
    }
    let rows = sqlx::query("SELECT id,user_id,name,avatar_id,is_default,is_kids,is_active,pin_hash,max_age_rating,created_at,updated_at,last_used_at FROM profiles WHERE user_id=? AND is_active=1 ORDER BY is_default DESC,created_at")
        .bind(user_id).fetch_all(&db.pool).await?;
    Ok(Json(
        json!({"items": rows.iter().map(profile_json).collect::<Vec<_>>() }),
    ))
}

async fn parental_settings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let row = sqlx::query(
        "SELECT unknown_kids_policy,updated_at FROM parental_control_settings WHERE id=1",
    )
    .fetch_one(&state.database().await.pool)
    .await?;
    Ok(Json(
        json!({"unknownKidsPolicy":row.get::<String,_>("unknown_kids_policy"),"updatedAt":row.get::<String,_>("updated_at")}),
    ))
}

async fn update_parental_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<ParentalSettingsRequest>,
) -> AppResult<Json<Value>> {
    auth.require("server.manage")?;
    if !matches!(
        payload.unknown_kids_policy.as_str(),
        "ALLOW" | "BLOCK_FOR_KIDS"
    ) {
        return Err(AppError::validation(
            "INVALID_UNKNOWN_RATING_POLICY",
            "Unknown content policy must be ALLOW or BLOCK_FOR_KIDS.",
        ));
    }
    let db = state.database().await;
    sqlx::query(
        "UPDATE parental_control_settings SET unknown_kids_policy=?,updated_at=? WHERE id=1",
    )
    .bind(&payload.unknown_kids_policy)
    .bind(now())
    .execute(&db.pool)
    .await?;
    db.audit(
        Some(&auth.id),
        "PROFILE_PARENTAL_CONTROL_CHANGED",
        "parental_control_settings",
        Some("1"),
        json!({"unknownKidsPolicy":payload.unknown_kids_policy}),
        None,
    )
    .await?;
    parental_settings(State(state), auth).await
}

async fn get_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Json<Value>> {
    let row = owned_profile(&state, &auth, &id).await?;
    Ok(Json(profile_json(&row)))
}

async fn current_profile(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let id = auth.require_profile()?.to_owned();
    get_profile(State(state), auth, AxumPath(id)).await
}

async fn create_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<CreateProfileRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    require_profile_management(&auth)?;
    validate_name(&payload.name)?;
    let user_id = payload.user_id.as_deref().unwrap_or(&auth.id);
    if user_id != auth.id {
        auth.require("users.update")?;
    }
    let age = payload
        .max_age_rating
        .unwrap_or(if payload.is_kids { 12 } else { 18 });
    validate_age(age)?;
    let avatar = payload.avatar_id.unwrap_or_else(|| {
        if payload.is_kids {
            "kids.png"
        } else {
            "default.png"
        }
        .into()
    });
    validate_avatar(&state, &avatar).await?;
    let db = state.database().await;
    if db.user_by_id(user_id).await?.is_none() {
        return Err(AppError::not_found("USER_NOT_FOUND", "User not found."));
    }
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    let mut tx = db.pool.begin().await?;
    sqlx::query("INSERT INTO profiles(id,user_id,name,avatar_id,is_default,is_kids,is_active,max_age_rating,created_at,updated_at) VALUES(?,?,?,?,0,?,1,?,?,?)")
        .bind(&id).bind(user_id).bind(payload.name.trim()).bind(&avatar)
        .bind(if payload.is_kids {1}else{0}).bind(age).bind(&timestamp).bind(&timestamp)
        .execute(&mut *tx).await?;
    let is_admin = db.is_admin(user_id).await?;
    let libraries = sqlx::query("SELECT l.id FROM libraries l WHERE l.deleted_at IS NULL AND l.is_active=1 AND (?=1 OR l.privacy='PUBLIC' OR EXISTS(SELECT 1 FROM user_library_access ula WHERE ula.user_id=? AND ula.library_id=l.id AND ula.can_view=1))")
        .bind(if is_admin {1}else{0}).bind(user_id).fetch_all(&mut *tx).await?;
    for library in libraries {
        sqlx::query("INSERT INTO profile_library_access(profile_id,library_id,is_allowed,created_at,updated_at) VALUES(?,?,1,?,?)")
            .bind(&id).bind(library.get::<String,_>("id")).bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    db.audit(
        Some(&auth.id),
        "PROFILE_CREATED",
        "profile",
        Some(&id),
        json!({"userId":user_id,"isKids":payload.is_kids}),
        None,
    )
    .await?;
    let row = owned_or_admin_profile(&state, &auth, &id).await?;
    Ok((StatusCode::CREATED, Json(profile_json(&row))))
}

async fn update_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
    Json(payload): Json<UpdateProfileRequest>,
) -> AppResult<Json<Value>> {
    require_profile_management(&auth)?;
    let current = owned_or_admin_profile(&state, &auth, &id).await?;
    if let Some(name) = &payload.name {
        validate_name(name)?;
    }
    if let Some(age) = payload.max_age_rating {
        validate_age(age)?;
    }
    if let Some(avatar) = &payload.avatar_id {
        validate_avatar(&state, avatar).await?;
    }
    if payload.is_active == Some(false) {
        ensure_not_last_profile(&state, &current.get::<String, _>("user_id"), &id).await?;
    }
    let db = state.database().await;
    sqlx::query("UPDATE profiles SET name=COALESCE(?,name),avatar_id=COALESCE(?,avatar_id),is_kids=COALESCE(?,is_kids),max_age_rating=COALESCE(?,max_age_rating),is_active=COALESCE(?,is_active),updated_at=? WHERE id=?")
        .bind(payload.name.as_deref().map(str::trim)).bind(&payload.avatar_id)
        .bind(payload.is_kids.map(|v|if v {1}else{0})).bind(payload.max_age_rating)
        .bind(payload.is_active.map(|v|if v {1}else{0})).bind(now()).bind(&id).execute(&db.pool).await?;
    let action = if payload.is_active == Some(false) {
        "PROFILE_DISABLED"
    } else {
        "PROFILE_UPDATED"
    };
    db.audit(Some(&auth.id),action,"profile",Some(&id),json!({"parentalControlsChanged":payload.is_kids.is_some()||payload.max_age_rating.is_some(),"avatarChanged":payload.avatar_id.is_some()}),None).await?;
    if payload.avatar_id.is_some() {
        db.audit(
            Some(&auth.id),
            "PROFILE_AVATAR_CHANGED",
            "profile",
            Some(&id),
            json!({}),
            None,
        )
        .await?;
    }
    if payload.is_kids.is_some() || payload.max_age_rating.is_some() {
        db.audit(
            Some(&auth.id),
            "PROFILE_PARENTAL_CONTROL_CHANGED",
            "profile",
            Some(&id),
            json!({}),
            None,
        )
        .await?;
    }
    let row = owned_or_admin_profile(&state, &auth, &id).await?;
    Ok(Json(profile_json(&row)))
}

async fn disable_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
) -> AppResult<StatusCode> {
    require_profile_management(&auth)?;
    let current = owned_or_admin_profile(&state, &auth, &id).await?;
    let user_id = current.get::<String, _>("user_id");
    ensure_not_last_profile(&state, &user_id, &id).await?;
    let db = state.database().await;
    sqlx::query("UPDATE profiles SET is_active=0,is_default=0,updated_at=? WHERE id=?")
        .bind(now())
        .bind(&id)
        .execute(&db.pool)
        .await?;
    db.audit(
        Some(&auth.id),
        "PROFILE_DISABLED",
        "profile",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn select_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
    payload: Option<Json<PinRequest>>,
) -> AppResult<Json<Value>> {
    let row = owned_profile(&state, &auth, &id).await?;
    let pin_hash = row.try_get::<String, _>("pin_hash").ok();
    if let Some(hash) = pin_hash {
        if !allow_pin_attempt(&state.profile_pin_limiter, &id).await {
            return Err(AppError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "PROFILE_PIN_RATE_LIMITED",
                "Too many PIN attempts. Try again later.",
            ));
        }
        let valid = payload
            .as_ref()
            .and_then(|Json(value)| value.pin.as_deref())
            .is_some_and(|pin| verify_pin(pin, &hash));
        if !valid {
            state
                .database()
                .await
                .audit(
                    Some(&auth.id),
                    "PROFILE_PIN_FAILED",
                    "profile",
                    Some(&id),
                    json!({}),
                    None,
                )
                .await?;
            return Err(AppError::new(
                StatusCode::UNAUTHORIZED,
                "INVALID_PROFILE_PIN",
                "Invalid profile PIN.",
            ));
        }
        state.profile_pin_limiter.lock().await.remove(&id);
    }
    let db = state.database().await;
    sqlx::query("UPDATE profiles SET last_used_at=?,updated_at=? WHERE id=?")
        .bind(now())
        .bind(now())
        .bind(&id)
        .execute(&db.pool)
        .await?;
    db.audit(
        Some(&auth.id),
        "PROFILE_SELECTED",
        "profile",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    let token = state
        .tokens
        .issue_for_profile(&auth.id, &auth.username, &id)?;
    Ok(Json(
        json!({"accessToken":token,"tokenType":"Bearer","expiresIn":state.tokens.ttl(),"profile":profile_json(&row)}),
    ))
}

async fn set_pin(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
    Json(payload): Json<PinRequest>,
) -> AppResult<StatusCode> {
    require_profile_management(&auth)?;
    owned_or_admin_profile(&state, &auth, &id).await?;
    let pin = payload.pin.as_deref().ok_or_else(|| {
        AppError::validation("INVALID_PROFILE_PIN", "PIN must contain 4-6 digits.")
    })?;
    let hash = hash_pin(pin)?;
    let db = state.database().await;
    sqlx::query("UPDATE profiles SET pin_hash=?,updated_at=? WHERE id=?")
        .bind(hash)
        .bind(now())
        .bind(&id)
        .execute(&db.pool)
        .await?;
    db.audit(
        Some(&auth.id),
        "PROFILE_PIN_SET",
        "profile",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_pin(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
) -> AppResult<StatusCode> {
    require_profile_management(&auth)?;
    owned_or_admin_profile(&state, &auth, &id).await?;
    let db = state.database().await;
    sqlx::query("UPDATE profiles SET pin_hash=NULL,updated_at=? WHERE id=?")
        .bind(now())
        .bind(&id)
        .execute(&db.pool)
        .await?;
    db.audit(
        Some(&auth.id),
        "PROFILE_PIN_REMOVED",
        "profile",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_library_access(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Json<Value>> {
    require_profile_management(&auth)?;
    owned_or_admin_profile(&state, &auth, &id).await?;
    let db = state.database().await;
    let rows=sqlx::query("SELECT l.id,l.name,l.library_type,l.minimum_age,CASE WHEN pla.is_allowed=1 THEN 1 ELSE 0 END is_allowed FROM profiles p JOIN libraries l ON l.deleted_at IS NULL AND l.is_active=1 LEFT JOIN profile_library_access pla ON pla.profile_id=p.id AND pla.library_id=l.id WHERE p.id=? AND (?=1 OR l.privacy='PUBLIC' OR EXISTS(SELECT 1 FROM user_library_access ula WHERE ula.user_id=p.user_id AND ula.library_id=l.id AND ula.can_view=1)) ORDER BY l.name")
        .bind(&id).bind(if auth.is_admin(){1}else{0}).fetch_all(&db.pool).await?;
    Ok(Json(
        json!({"libraries":rows.iter().map(|row|json!({"libraryId":row.get::<String,_>("id"),"name":row.get::<String,_>("name"),"type":row.get::<String,_>("library_type"),"minimumAge":row.get::<i64,_>("minimum_age"),"isAllowed":row.get::<i64,_>("is_allowed")!=0})).collect::<Vec<_>>() }),
    ))
}

async fn update_library_access(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
    Json(payload): Json<ProfileLibraryAccessRequest>,
) -> AppResult<Json<Value>> {
    require_profile_management(&auth)?;
    let profile = owned_or_admin_profile(&state, &auth, &id).await?;
    let user_id = profile.get::<String, _>("user_id");
    let db = state.database().await;
    let is_owner_admin = db.is_admin(&user_id).await?;
    let mut tx = db.pool.begin().await?;
    for library_id in &payload.library_ids {
        let allowed:i64=sqlx::query("SELECT COUNT(*) FROM libraries l WHERE l.id=? AND l.deleted_at IS NULL AND l.is_active=1 AND (?=1 OR l.privacy='PUBLIC' OR EXISTS(SELECT 1 FROM user_library_access ula WHERE ula.user_id=? AND ula.library_id=l.id AND ula.can_view=1))")
            .bind(library_id).bind(if is_owner_admin {1}else{0}).bind(&user_id).fetch_one(&mut *tx).await?.get(0);
        if allowed == 0 {
            return Err(AppError::forbidden());
        }
    }
    sqlx::query("DELETE FROM profile_library_access WHERE profile_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let timestamp = now();
    for library_id in &payload.library_ids {
        sqlx::query("INSERT INTO profile_library_access(profile_id,library_id,is_allowed,created_at,updated_at) VALUES(?,?,1,?,?)")
            .bind(&id).bind(library_id).bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    db.audit(
        Some(&auth.id),
        "PROFILE_LIBRARY_ACCESS_CHANGED",
        "profile",
        Some(&id),
        json!({"libraryCount":payload.library_ids.len()}),
        None,
    )
    .await?;
    get_library_access(State(state), auth, AxumPath(id)).await
}

async fn avatar_categories() -> Json<Value> {
    Json(json!(
        CATEGORIES
            .iter()
            .map(|(id, name)| json!({"id":id,"name":name}))
            .collect::<Vec<_>>()
    ))
}

async fn list_avatars(
    State(state): State<AppState>,
    Query(query): Query<AvatarQuery>,
) -> AppResult<Json<Value>> {
    if let Some(category) = query.category.as_deref() {
        validate_category(category)?;
    }
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(40).clamp(1, 100);
    let entries = state.avatar_catalog.entries().await?;
    let filtered = entries
        .into_iter()
        .filter(|entry| {
            query
                .category
                .as_ref()
                .is_none_or(|category| entry.category == *category)
        })
        .collect::<Vec<_>>();
    let total = filtered.len() as i64;
    let start = ((page - 1) * page_size) as usize;
    let items = filtered
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"items":items,"page":page,"pageSize":page_size,"total":total,"totalPages":if total==0{0}else{(total+page_size-1)/page_size}}),
    ))
}

async fn serve_avatar(
    State(state): State<AppState>,
    AxumPath((category, filename)): AxumPath<(String, String)>,
) -> AppResult<Response> {
    if category == "fallback" && matches!(filename.as_str(), "default.png" | "kids.png") {
        return fallback_avatar(filename == "kids.png");
    }
    let path = state.avatar_catalog.file_path(&category, &filename)?;
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(metadata) => metadata,
        Err(_) => {
            tracing::warn!(avatar_id=%format!("{category}/{filename}"), "saved avatar is missing; serving fallback");
            return fallback_avatar(false);
        }
    };
    if !metadata.is_file() {
        return Err(AppError::not_found("AVATAR_NOT_FOUND", "Avatar not found."));
    }
    let bytes = tokio::fs::read(&path).await?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |value| value.as_secs());
    let etag = format!("\"{:x}-{:x}\"", metadata.len(), modified);
    let content_type = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header(header::ETAG, etag)
        .body(Body::from(bytes))
        .map_err(|_| AppError::config("Unable to serve avatar."))
}

fn fallback_avatar(kids: bool) -> AppResult<Response> {
    let (background, glyph, id) = if kids {
        ("#5B8DEF", "K", "kids")
    } else {
        ("#7257D5", "M", "default")
    };
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='256' height='256' viewBox='0 0 256 256'><rect width='256' height='256' rx='40' fill='{background}'/><text x='128' y='158' text-anchor='middle' font-family='sans-serif' font-size='96' font-weight='700' fill='white'>{glyph}</text></svg>"
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header(header::ETAG, format!("\"fallback-{id}\""))
        .body(Body::from(svg))
        .map_err(|_| AppError::config("Unable to serve avatar fallback."))
}

async fn owned_profile(
    state: &AppState,
    auth: &AuthUser,
    id: &str,
) -> AppResult<sqlx::any::AnyRow> {
    let db = state.database().await;
    sqlx::query("SELECT id,user_id,name,avatar_id,is_default,is_kids,is_active,pin_hash,max_age_rating,created_at,updated_at,last_used_at FROM profiles WHERE id=? AND user_id=? AND is_active=1").bind(id).bind(&auth.id).fetch_optional(&db.pool).await?.ok_or_else(||AppError::not_found("PROFILE_NOT_FOUND","Profile not found."))
}

async fn owned_or_admin_profile(
    state: &AppState,
    auth: &AuthUser,
    id: &str,
) -> AppResult<sqlx::any::AnyRow> {
    let db = state.database().await;
    let row=sqlx::query("SELECT id,user_id,name,avatar_id,is_default,is_kids,is_active,pin_hash,max_age_rating,created_at,updated_at,last_used_at FROM profiles WHERE id=?").bind(id).fetch_optional(&db.pool).await?.ok_or_else(||AppError::not_found("PROFILE_NOT_FOUND","Profile not found."))?;
    if row.get::<String, _>("user_id") != auth.id && !auth.is_admin() {
        return Err(AppError::forbidden());
    }
    Ok(row)
}

async fn ensure_not_last_profile(state: &AppState, user_id: &str, id: &str) -> AppResult<()> {
    let db = state.database().await;
    let count: i64 =
        sqlx::query("SELECT COUNT(*) FROM profiles WHERE user_id=? AND is_active=1 AND id<>?")
            .bind(user_id)
            .bind(id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
    if count == 0 {
        Err(AppError::conflict(
            "LAST_PROFILE_PROTECTION",
            "The last active profile cannot be disabled.",
        ))
    } else {
        Ok(())
    }
}

fn profile_json(row: &sqlx::any::AnyRow) -> Value {
    let avatar_id = row.get::<String, _>("avatar_id");
    let avatar_url = if avatar_id.contains('/') {
        format!("/api/v1/avatars/{avatar_id}")
    } else {
        format!("/api/v1/avatars/fallback/{avatar_id}")
    };
    json!({"id":row.get::<String,_>("id"),"userId":row.get::<String,_>("user_id"),"name":row.get::<String,_>("name"),"avatarId":avatar_id,"avatarUrl":avatar_url,"isDefault":row.get::<i64,_>("is_default")!=0,"isKids":row.get::<i64,_>("is_kids")!=0,"isActive":row.get::<i64,_>("is_active")!=0,"pinProtected":row.try_get::<String,_>("pin_hash").is_ok(),"maxAgeRating":row.get::<i64,_>("max_age_rating"),"createdAt":row.get::<String,_>("created_at"),"updatedAt":row.get::<String,_>("updated_at"),"lastUsedAt":row.try_get::<String,_>("last_used_at").ok()})
}

fn validate_name(name: &str) -> AppResult<()> {
    let length = name.trim().chars().count();
    if (1..=40).contains(&length) {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_PROFILE_NAME",
            "Profile name must contain 1-40 characters.",
        ))
    }
}
fn require_profile_management(auth: &AuthUser) -> AppResult<()> {
    if auth.profile_is_kids {
        Err(AppError::forbidden())
    } else {
        Ok(())
    }
}
fn validate_age(age: i64) -> AppResult<()> {
    if AGE_RATINGS.contains(&age) {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_AGE_RATING",
            "Maximum age rating must be 0, 10, 12, 14, 16 or 18.",
        ))
    }
}
fn validate_category(category: &str) -> AppResult<()> {
    if CATEGORIES.iter().any(|(id, _)| *id == category) {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_AVATAR_CATEGORY",
            "Invalid avatar category.",
        ))
    }
}
fn validate_filename(filename: &str) -> AppResult<()> {
    let path = Path::new(filename);
    let one_component = path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)));
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if one_component
        && extension
            .is_some_and(|value| matches!(value.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif"))
    {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_AVATAR_FILENAME",
            "Invalid avatar filename.",
        ))
    }
}
async fn validate_avatar(state: &AppState, id: &str) -> AppResult<()> {
    if id.contains("..")
        || id.starts_with('/')
        || id.starts_with('\\')
        || !state.avatar_catalog.contains(id).await?
    {
        Err(AppError::validation(
            "INVALID_AVATAR",
            "Avatar is not part of the local catalog.",
        ))
    } else {
        Ok(())
    }
}
fn hash_pin(pin: &str) -> AppResult<String> {
    if !(4..=6).contains(&pin.len()) || !pin.bytes().all(|value| value.is_ascii_digit()) {
        return Err(AppError::validation(
            "INVALID_PROFILE_PIN",
            "PIN must contain 4-6 digits.",
        ));
    }
    Argon2::default()
        .hash_password(pin.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|_| AppError::config("Unable to secure profile PIN."))
}
fn verify_pin(pin: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded).ok().is_some_and(|hash| {
        Argon2::default()
            .verify_password(pin.as_bytes(), &hash)
            .is_ok()
    })
}
