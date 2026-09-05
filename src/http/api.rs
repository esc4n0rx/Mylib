use std::{net::SocketAddr, time::Instant};

use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    app::{AppState, persist_database},
    auth::{AuthUser, hash_password, validate_username, verify_password},
    config::normalize_sqlite_url,
    db::{Database, DatabaseKind, PERMISSIONS, create_user, now},
    errors::{AppError, AppResult},
    models::{
        CreateUserRequest, LibraryAccessEntry, LibraryAccessRequest, PasswordRequest, RolesRequest,
        UpdateUserRequest, UserResponse,
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/health", get(api_health))
        .route("/api/v1/setup/status", get(setup_status))
        .route("/api/v1/setup", post(setup))
        .route("/api/v1/setup/database/test", post(test_database))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/server", get(server_info).patch(update_server))
        .route("/api/v1/users", get(list_users).post(add_user))
        .route("/api/v1/users/{id}", get(get_user).patch(update_user))
        .route(
            "/api/v1/users/{id}/password",
            axum::routing::put(change_password),
        )
        .route("/api/v1/users/{id}/disable", post(disable_user))
        .route("/api/v1/users/{id}/enable", post(enable_user))
        .route("/api/v1/users/{id}/roles", axum::routing::put(change_roles))
        .route(
            "/api/v1/users/{id}/library-access",
            get(get_library_access).put(update_library_access),
        )
        .route("/api/v1/roles", get(list_roles))
        .merge(crate::catalog_api::router())
        .merge(crate::media_api::router())
        .merge(crate::playback::router())
        .merge(crate::profiles::router())
        .merge(crate::operational::router())
        .merge(crate::recommendations::router())
        .merge(crate::features::remote_sources::api::router())
        .merge(crate::features::remote_sources::google_drive::router())
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

async fn api_health(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let db = state.database().await;
    db.ping().await?;
    Ok(Json(
        json!({"status":"ok","database":"ok","databaseType":db.kind.as_str(),"version":env!("CARGO_PKG_VERSION")}),
    ))
}

async fn setup_status(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let db = state.database().await;
    match db.server_config().await? {
        Some(config) if config.setup_completed != 0 => Ok(Json(
            json!({"setupRequired":false,"configured":true,"serverName":config.server_name,"databaseType":config.database_type}),
        )),
        _ => Ok(Json(json!({"setupRequired":true,"configured":false}))),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupRequest {
    server_name: String,
    database: DatabaseRequest,
    administrator: AdministratorRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdministratorRequest {
    username: String,
    password: String,
    display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum DatabaseRequest {
    Sqlite {
        path: Option<String>,
    },
    Mysql {
        host: String,
        #[serde(default = "default_mysql_port")]
        port: u16,
        database: String,
        username: String,
        password: String,
        #[serde(default, rename = "sslMode")]
        ssl_mode: Option<String>,
    },
}
fn default_mysql_port() -> u16 {
    3306
}

impl DatabaseRequest {
    fn connection(&self, state: &AppState) -> AppResult<(DatabaseKind, String)> {
        match self {
            Self::Sqlite { path: None } => Ok((DatabaseKind::Sqlite, state.config.sqlite_url())),
            Self::Sqlite { path: Some(path) } => {
                Ok((DatabaseKind::Sqlite, normalize_sqlite_url(path)?))
            }
            Self::Mysql {
                host,
                port,
                database,
                username,
                password,
                ssl_mode,
            } => {
                if host.trim().is_empty()
                    || database.trim().is_empty()
                    || username.trim().is_empty()
                    || password.is_empty()
                {
                    return Err(AppError::validation(
                        "INVALID_DATABASE_CONFIGURATION",
                        "All MySQL connection fields are required.",
                    ));
                }
                let ssl = match ssl_mode.as_deref().unwrap_or("preferred") {
                    "disabled" => "disabled",
                    "required" => "required",
                    "preferred" => "preferred",
                    _ => {
                        return Err(AppError::validation(
                            "INVALID_SSL_MODE",
                            "SSL mode must be disabled, preferred or required.",
                        ));
                    }
                };
                Ok((
                    DatabaseKind::MySql,
                    format!(
                        "mysql://{}:{}@{}:{}/{}?ssl-mode={}",
                        urlencoding::encode(username),
                        urlencoding::encode(password),
                        host,
                        port,
                        urlencoding::encode(database),
                        ssl
                    ),
                ))
            }
        }
    }
}

async fn setup(
    State(state): State<AppState>,
    Json(payload): Json<SetupRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    if payload.server_name.trim().is_empty() || payload.server_name.chars().count() > 128 {
        return Err(AppError::validation(
            "INVALID_SERVER_NAME",
            "Server name must contain 1-128 characters.",
        ));
    }
    validate_username(&payload.administrator.username)?;
    let password_hash = hash_password(&payload.administrator.password)?;
    if state
        .database()
        .await
        .server_config()
        .await?
        .is_some_and(|c| c.setup_completed != 0)
    {
        return Err(AppError::conflict(
            "SETUP_ALREADY_COMPLETED",
            "MyLib server has already been configured.",
        ));
    }
    let (kind, url) = payload.database.connection(&state)?;
    let target = Database::connect(kind, &url).await?;
    target.migrate().await?;
    if target
        .server_config()
        .await?
        .is_some_and(|c| c.setup_completed != 0)
    {
        return Err(AppError::conflict(
            "SETUP_ALREADY_COMPLETED",
            "MyLib server has already been configured.",
        ));
    }

    // Persist before committing the selected database. If the transaction fails, restart still
    // reaches the same empty provider and can safely retry setup.
    persist_database(
        &state.config,
        kind,
        (kind == DatabaseKind::MySql).then_some(url.as_str()),
    )?;
    let mut tx = target.pool.begin().await?;
    let timestamp = now();
    let server_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO server_config (id,server_id,server_name,setup_completed,database_type,server_version,created_at,updated_at) VALUES (?,?,?,1,?,?,?,?)")
        .bind(Uuid::new_v4().to_string()).bind(&server_id).bind(payload.server_name.trim()).bind(kind.as_str()).bind(env!("CARGO_PKG_VERSION")).bind(&timestamp).bind(&timestamp).execute(&mut *tx).await.map_err(map_setup_conflict)?;
    let admin_role = Uuid::new_v4().to_string();
    let user_role = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO roles (id,name,description,is_system,created_at,updated_at) VALUES (?,?,?,1,?,?)").bind(&admin_role).bind("Administrator").bind("Full access to current server capabilities").bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO roles (id,name,description,is_system,created_at,updated_at) VALUES (?,?,?,1,?,?)").bind(&user_role).bind("User").bind("Standard user with no administrative permissions").bind(&timestamp).bind(&timestamp).execute(&mut *tx).await?;
    for permission in PERMISSIONS {
        let permission_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO permissions (id,name,description,created_at) VALUES (?,?,?,?)")
            .bind(&permission_id)
            .bind(permission)
            .bind(format!("Permission {permission}"))
            .bind(&timestamp)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO role_permissions (role_id,permission_id) VALUES (?,?)")
            .bind(&admin_role)
            .bind(&permission_id)
            .execute(&mut *tx)
            .await?;
        if matches!(
            *permission,
            "libraries.view"
                | "media.view"
                | "media.play"
                | "playback.view_own"
                | "playback.history.view_own"
        ) {
            sqlx::query("INSERT INTO role_permissions (role_id,permission_id) VALUES (?,?)")
                .bind(&user_role)
                .bind(&permission_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    let admin_request = CreateUserRequest {
        username: payload.administrator.username,
        password: String::new(),
        display_name: payload.administrator.display_name,
        email: None,
        roles: vec![],
        library_access: vec![],
    };
    let admin_id = create_user(&mut tx, &admin_request, &password_hash).await?;
    sqlx::query("INSERT INTO user_roles (user_id,role_id) VALUES (?,?)")
        .bind(&admin_id)
        .bind(&admin_role)
        .execute(&mut *tx)
        .await?;
    insert_audit(
        &mut tx,
        Some(&admin_id),
        "DATABASE_CONFIGURATION_SELECTED",
        "server",
        Some(&server_id),
        json!({"databaseType":kind.as_str()}),
    )
    .await?;
    insert_audit(
        &mut tx,
        Some(&admin_id),
        "SERVER_SETUP_COMPLETED",
        "server",
        Some(&server_id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    state.replace_database(target).await;
    tracing::info!(
        server_id,
        database_type = kind.as_str(),
        "server setup completed"
    );
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"configured":true,"serverId":server_id,"serverName":payload.server_name,"databaseType":kind.as_str()}),
        ),
    ))
}

fn map_setup_conflict(error: sqlx::Error) -> AppError {
    let lower = error.to_string().to_ascii_lowercase();
    if lower.contains("unique") || lower.contains("duplicate") {
        AppError::conflict(
            "SETUP_ALREADY_COMPLETED",
            "MyLib server has already been configured.",
        )
    } else {
        AppError::from(error)
    }
}

async fn insert_audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    actor: Option<&str>,
    action: &str,
    entity_type: &str,
    entity_id: Option<&str>,
    metadata: Value,
) -> AppResult<()> {
    sqlx::query("INSERT INTO audit_log (id,actor_user_id,action,entity_type,entity_id,metadata,ip_address,created_at) VALUES (?,?,?,?,?,?,NULL,?)").bind(Uuid::new_v4().to_string()).bind(actor).bind(action).bind(entity_type).bind(entity_id).bind(metadata.to_string()).bind(now()).execute(&mut **tx).await?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseTestResponse {
    success: bool,
    database_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

async fn test_database(
    State(state): State<AppState>,
    Json(request): Json<DatabaseRequest>,
) -> AppResult<Json<DatabaseTestResponse>> {
    if state
        .database()
        .await
        .server_config()
        .await?
        .is_some_and(|c| c.setup_completed != 0)
    {
        return Err(AppError::not_found("NOT_FOUND", "Resource not found."));
    }
    let (kind, url) = request.connection(&state)?;
    let started = Instant::now();
    match Database::connect(kind, &url).await {
        Ok(db) => {
            let result = db.ping().await;
            db.pool.close().await;
            match result {
                Ok(()) => Ok(Json(DatabaseTestResponse {
                    success: true,
                    database_type: kind.as_str().into(),
                    latency_ms: Some(started.elapsed().as_millis()),
                    error: None,
                })),
                Err(_) => Ok(database_test_failure(kind)),
            }
        }
        Err(_) => Ok(database_test_failure(kind)),
    }
}
fn database_test_failure(kind: DatabaseKind) -> Json<DatabaseTestResponse> {
    Json(DatabaseTestResponse {
        success: false,
        database_type: kind.as_str().into(),
        latency_ms: None,
        error: Some(
            json!({"code":"DATABASE_CONNECTION_FAILED","message":"Unable to connect to the configured database."}),
        ),
    })
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}
async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<Value>> {
    let ip = peer.ip();
    if !state.allow_login(ip).await {
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            "Too many login attempts. Try again later.",
        ));
    }
    let db = state.database().await;
    let user = db.user_by_username(&payload.username).await?;
    let valid = user
        .as_ref()
        .is_some_and(|u| u.is_active != 0 && verify_password(&payload.password, &u.password_hash));
    if !valid {
        // A dummy Argon2 verification narrows the timing difference for unknown users.
        if user.is_none()
            && let Ok(dummy) = hash_password("dummy-login-password")
        {
            let _ = verify_password(&payload.password, &dummy);
        }
        db.audit(
            None,
            "LOGIN_FAILED",
            "user",
            None,
            json!({"username":payload.username.to_ascii_lowercase()}),
            Some(&ip.to_string()),
        )
        .await?;
        return Err(AppError::new(
            StatusCode::UNAUTHORIZED,
            "INVALID_CREDENTIALS",
            "Invalid username or password.",
        ));
    }
    let user = user.ok_or_else(AppError::unauthorized)?;
    state.clear_login_failures(ip).await;
    sqlx::query("UPDATE users SET last_login_at=?,updated_at=? WHERE id=?")
        .bind(now())
        .bind(now())
        .bind(&user.id)
        .execute(&db.pool)
        .await?;
    db.audit(
        Some(&user.id),
        "LOGIN_SUCCESS",
        "user",
        Some(&user.id),
        json!({}),
        Some(&ip.to_string()),
    )
    .await?;
    let profiles = sqlx::query(
        "SELECT id,pin_hash FROM profiles WHERE user_id=? AND is_active=1 ORDER BY is_default DESC,created_at",
    )
    .bind(&user.id)
    .fetch_all(&db.pool)
    .await?;
    let automatically_selected =
        profiles.len() == 1 && profiles[0].try_get::<String, _>("pin_hash").is_err();
    let selected_profile_id = automatically_selected.then(|| profiles[0].get::<String, _>("id"));
    let token = if let Some(profile_id) = &selected_profile_id {
        state
            .tokens
            .issue_for_profile(&user.id, &user.username, profile_id)?
    } else {
        state.tokens.issue(&user.id, &user.username)?
    };
    let is_admin = db.is_admin(&user.id).await?;
    Ok(Json(
        json!({"accessToken":token,"tokenType":"Bearer","expiresIn":state.tokens.ttl(),"profileSelectionRequired":!automatically_selected,"selectedProfileId":selected_profile_id,"user":{"id":user.id,"username":user.username,"displayName":user.display_name,"isAdmin":is_admin}}),
    ))
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let db = state.database().await;
    let user = db
        .user_by_id(&auth.id)
        .await?
        .ok_or_else(|| AppError::not_found("USER_NOT_FOUND", "User not found."))?;
    Ok(Json(
        json!({"id":user.id,"username":user.username,"displayName":user.display_name,"isAdmin":auth.is_admin(),"roles":auth.roles,"permissions":auth.permissions,"profileId":auth.profile_id}),
    ))
}

async fn server_info(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let db = state.database().await;
    let config = db
        .server_config()
        .await?
        .ok_or_else(|| AppError::not_found("SETUP_REQUIRED", "Server setup is required."))?;
    let uptime = state.started_at.elapsed().unwrap_or_default().as_secs();
    Ok(Json(
        json!({"id":config.server_id,"name":config.server_name,"version":config.server_version,"status":"online","databaseType":config.database_type,"setupCompleted":config.setup_completed!=0,"startedAt":chrono::DateTime::<chrono::Utc>::from(state.started_at).to_rfc3339(),"uptimeSeconds":uptime}),
    ))
}

#[derive(Deserialize)]
struct ServerUpdate {
    name: String,
}
async fn update_server(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<ServerUpdate>,
) -> AppResult<Json<Value>> {
    auth.require("server.manage")?;
    if payload.name.trim().is_empty() || payload.name.chars().count() > 128 {
        return Err(AppError::validation(
            "INVALID_SERVER_NAME",
            "Server name must contain 1-128 characters.",
        ));
    }
    let db = state.database().await;
    sqlx::query("UPDATE server_config SET server_name=?,updated_at=?")
        .bind(payload.name.trim())
        .bind(now())
        .execute(&db.pool)
        .await?;
    db.audit(
        Some(&auth.id),
        "SERVER_UPDATED",
        "server",
        None,
        json!({"name":payload.name.trim()}),
        None,
    )
    .await?;
    server_info(State(state), auth).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsersQuery {
    #[serde(default = "default_users_page")]
    page: i64,
    #[serde(default = "default_users_page_size")]
    page_size: i64,
    search: Option<String>,
    status: Option<String>,
    role: Option<String>,
}
fn default_users_page() -> i64 {
    1
}
fn default_users_page_size() -> i64 {
    20
}

async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<UsersQuery>,
) -> AppResult<Json<Value>> {
    auth.require("users.view")?;
    let db = state.database().await;
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 100);
    let mut filters = String::from(" WHERE 1=1");
    let mut values: Vec<String> = Vec::new();
    if let Some(search) = query.search.filter(|value| !value.trim().is_empty()) {
        filters.push_str(" AND (LOWER(u.username) LIKE ? OR LOWER(u.display_name) LIKE ? OR LOWER(COALESCE(u.email,'')) LIKE ?)");
        let pattern = format!("%{}%", search.trim().to_ascii_lowercase());
        values.extend([pattern.clone(), pattern.clone(), pattern]);
    }
    if let Some(status) = query.status.filter(|value| value != "all") {
        filters.push_str(" AND u.is_active=?");
        values.push(if status == "active" { "1" } else { "0" }.into());
    }
    if let Some(role) = query.role.filter(|value| value != "all") {
        filters.push_str(" AND EXISTS (SELECT 1 FROM user_roles fur JOIN roles fr ON fr.id=fur.role_id WHERE fur.user_id=u.id AND fr.name=?)");
        values.push(role);
    }
    let count_sql = format!("SELECT COUNT(*) FROM users u{filters}");
    let mut count_query = sqlx::query(&count_sql);
    for value in &values {
        count_query = count_query.bind(value);
    }
    let total: i64 = count_query.fetch_one(&db.pool).await?.get(0);
    let list_sql = format!(
        "SELECT u.id,u.username,u.display_name,u.email,u.is_active,u.last_login_at,u.created_at,u.updated_at,(SELECT COUNT(*) FROM user_library_access ula WHERE ula.user_id=u.id AND ula.can_view=1) AS library_access_count,CASE WHEN EXISTS (SELECT 1 FROM user_roles aur JOIN roles ar ON ar.id=aur.role_id WHERE aur.user_id=u.id AND ar.name='Administrator') THEN 1 ELSE 0 END AS is_admin FROM users u{filters} ORDER BY LOWER(u.username) LIMIT ? OFFSET ?"
    );
    let mut list_query = sqlx::query(&list_sql);
    for value in &values {
        list_query = list_query.bind(value);
    }
    let rows = list_query
        .bind(page_size)
        .bind((page - 1) * page_size)
        .fetch_all(&db.pool)
        .await?;
    let items = rows.iter().map(|row| json!({
        "id": row.get::<String,_>("id"), "username": row.get::<String,_>("username"),
        "displayName": row.get::<String,_>("display_name"), "email": row.try_get::<String,_>("email").ok(),
        "isActive": row.get::<i64,_>("is_active") != 0, "isAdmin": row.get::<i64,_>("is_admin") != 0,
        "roles": if row.get::<i64,_>("is_admin") != 0 { vec!["Administrator"] } else { vec!["User"] },
        "lastLoginAt": row.try_get::<String,_>("last_login_at").ok(), "createdAt": row.get::<String,_>("created_at"),
        "updatedAt": row.get::<String,_>("updated_at"), "libraryAccessCount": row.get::<i64,_>("library_access_count")
    })).collect::<Vec<_>>();
    Ok(Json(
        json!({"items":items,"page":page,"pageSize":page_size,"total":total,"totalPages":if total==0{0}else{(total+page_size-1)/page_size}}),
    ))
}
async fn get_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<UserResponse>> {
    auth.require("users.view")?;
    let db = state.database().await;
    let user = db
        .user_by_id(&id)
        .await?
        .ok_or_else(|| AppError::not_found("USER_NOT_FOUND", "User not found."))?;
    Ok(Json(db.user_response(user).await?))
}

async fn add_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<UserResponse>)> {
    auth.require("users.create")?;
    validate_username(&payload.username)?;
    let hash = hash_password(&payload.password)?;
    let db = state.database().await;
    let role_names = if payload.roles.is_empty() {
        vec!["User".into()]
    } else {
        payload.roles.clone()
    };
    let role_ids = db.role_ids(&role_names).await?;
    let mut tx = db.pool.begin().await?;
    let id = create_user(&mut tx, &payload, &hash).await?;
    for role_id in role_ids {
        sqlx::query("INSERT INTO user_roles (user_id,role_id) VALUES (?,?)")
            .bind(&id)
            .bind(role_id)
            .execute(&mut *tx)
            .await?;
    }
    for access in &payload.library_access {
        let exists: i64 =
            sqlx::query("SELECT COUNT(*) FROM libraries WHERE id=? AND deleted_at IS NULL")
                .bind(&access.library_id)
                .fetch_one(&mut *tx)
                .await?
                .get(0);
        if exists == 0 {
            return Err(AppError::validation(
                "INVALID_LIBRARY",
                "A selected library does not exist.",
            ));
        }
        sqlx::query("INSERT INTO user_library_access(id,user_id,library_id,can_view,can_play,granted_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string()).bind(&id).bind(&access.library_id)
            .bind(if access.can_view { 1 } else { 0 }).bind(if access.can_view && access.can_play { 1 } else { 0 })
            .bind(&auth.id).bind(now()).bind(now()).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO profile_library_access(profile_id,library_id,is_allowed,created_at,updated_at) SELECT p.id,l.id,1,?,? FROM profiles p JOIN libraries l ON l.deleted_at IS NULL AND l.is_active=1 WHERE p.user_id=? AND p.is_default=1 AND (?=1 OR l.privacy='PUBLIC' OR EXISTS(SELECT 1 FROM user_library_access ula WHERE ula.user_id=p.user_id AND ula.library_id=l.id AND ula.can_view=1))")
        .bind(now()).bind(now()).bind(&id)
        .bind(if role_names.iter().any(|role| role == "Administrator") {1} else {0})
        .execute(&mut *tx).await?;
    insert_audit(
        &mut tx,
        Some(&auth.id),
        "USER_CREATED",
        "user",
        Some(&id),
        json!({"roles":role_names,"libraryAccessCount":payload.library_access.len()}),
    )
    .await?;
    tx.commit().await?;
    let user = db
        .user_by_id(&id)
        .await?
        .ok_or_else(|| AppError::not_found("USER_NOT_FOUND", "User not found."))?;
    Ok((StatusCode::CREATED, Json(db.user_response(user).await?)))
}

async fn update_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateUserRequest>,
) -> AppResult<Json<UserResponse>> {
    auth.require("users.update")?;
    let db = state.database().await;
    if db.user_by_id(&id).await?.is_none() {
        return Err(AppError::not_found("USER_NOT_FOUND", "User not found."));
    }
    if let Some(username) = payload.username {
        validate_username(&username)?;
        sqlx::query("UPDATE users SET username=?,username_normalized=?,updated_at=? WHERE id=?")
            .bind(username.trim())
            .bind(username.trim().to_ascii_lowercase())
            .bind(now())
            .bind(&id)
            .execute(&db.pool)
            .await
            .map_err(|error| {
                let value = error.to_string().to_ascii_lowercase();
                if value.contains("unique") || value.contains("duplicate") {
                    AppError::conflict("USERNAME_ALREADY_EXISTS", "Username is already in use.")
                } else {
                    AppError::from(error)
                }
            })?;
    }
    if let Some(name) = payload.display_name {
        if name.trim().is_empty() {
            return Err(AppError::validation(
                "INVALID_DISPLAY_NAME",
                "Display name may not be empty.",
            ));
        }
        sqlx::query("UPDATE users SET display_name=?,updated_at=? WHERE id=?")
            .bind(name.trim())
            .bind(now())
            .bind(&id)
            .execute(&db.pool)
            .await?;
    }
    if let Some(email) = payload.email {
        sqlx::query("UPDATE users SET email=?,updated_at=? WHERE id=?")
            .bind(if email.trim().is_empty() {
                None
            } else {
                Some(email.trim())
            })
            .bind(now())
            .bind(&id)
            .execute(&db.pool)
            .await?;
    }
    if let Some(active) = payload.is_active {
        if !active && db.is_admin(&id).await? && db.active_admin_count().await? <= 1 {
            return Err(AppError::conflict(
                "LAST_ADMIN_PROTECTION",
                "The last active administrator cannot be disabled.",
            ));
        }
        sqlx::query("UPDATE users SET is_active=?,updated_at=? WHERE id=?")
            .bind(if active { 1 } else { 0 })
            .bind(now())
            .bind(&id)
            .execute(&db.pool)
            .await?;
    }
    db.audit(
        Some(&auth.id),
        "USER_UPDATED",
        "user",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    get_user(State(state), auth, Path(id)).await
}

async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<PasswordRequest>,
) -> AppResult<StatusCode> {
    auth.require("users.update")?;
    let password = payload
        .value()
        .ok_or_else(|| AppError::validation("PASSWORD_REQUIRED", "A new password is required."))?;
    let hash = hash_password(password)?;
    let db = state.database().await;
    let result = sqlx::query("UPDATE users SET password_hash=?,updated_at=? WHERE id=?")
        .bind(hash)
        .bind(now())
        .bind(&id)
        .execute(&db.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("USER_NOT_FOUND", "User not found."));
    }
    db.audit(
        Some(&auth.id),
        "USER_PASSWORD_RESET",
        "user",
        Some(&id),
        json!({}),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn disable_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    set_active(state, auth, id, false).await
}
async fn enable_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    set_active(state, auth, id, true).await
}
async fn set_active(
    state: AppState,
    auth: AuthUser,
    id: String,
    active: bool,
) -> AppResult<StatusCode> {
    auth.require("users.disable")?;
    let db = state.database().await;
    let user = db
        .user_by_id(&id)
        .await?
        .ok_or_else(|| AppError::not_found("USER_NOT_FOUND", "User not found."))?;
    if !active
        && user.is_active != 0
        && db.is_admin(&id).await?
        && db.active_admin_count().await? <= 1
    {
        return Err(AppError::conflict(
            "LAST_ADMIN_PROTECTION",
            "The last active administrator cannot be disabled.",
        ));
    }
    sqlx::query("UPDATE users SET is_active=?,updated_at=? WHERE id=?")
        .bind(if active { 1 } else { 0 })
        .bind(now())
        .bind(&id)
        .execute(&db.pool)
        .await?;
    let action = if active {
        "USER_ENABLED"
    } else {
        "USER_DISABLED"
    };
    db.audit(Some(&auth.id), action, "user", Some(&id), json!({}), None)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn change_roles(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<RolesRequest>,
) -> AppResult<StatusCode> {
    auth.require("users.permissions.manage")?;
    let db = state.database().await;
    let user = db
        .user_by_id(&id)
        .await?
        .ok_or_else(|| AppError::not_found("USER_NOT_FOUND", "User not found."))?;
    let role_ids = db.role_ids(&payload.roles).await?;
    let removing_admin =
        db.is_admin(&id).await? && !payload.roles.iter().any(|r| r == "Administrator");
    if user.is_active != 0 && removing_admin && db.active_admin_count().await? <= 1 {
        return Err(AppError::conflict(
            "LAST_ADMIN_PROTECTION",
            "The Administrator role cannot be removed from the last active administrator.",
        ));
    }
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM user_roles WHERE user_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    for role_id in role_ids {
        sqlx::query("INSERT INTO user_roles (user_id,role_id) VALUES (?,?)")
            .bind(&id)
            .bind(role_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    db.audit(
        Some(&auth.id),
        "USER_ROLES_CHANGED",
        "user",
        Some(&id),
        json!({"roles":payload.roles}),
        None,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_roles(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("users.view")?;
    Ok(Json(json!(state.database().await.roles().await?)))
}

async fn get_library_access(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require("users.view")?;
    let db = state.database().await;
    if db.user_by_id(&id).await?.is_none() {
        return Err(AppError::not_found("USER_NOT_FOUND", "User not found."));
    }
    let rows = sqlx::query("SELECT ula.library_id,ula.can_view,ula.can_play,l.name,l.library_type,l.privacy FROM user_library_access ula JOIN libraries l ON l.id=ula.library_id WHERE ula.user_id=? AND l.deleted_at IS NULL ORDER BY l.name")
        .bind(&id).fetch_all(&db.pool).await?;
    Ok(Json(json!({"libraries":rows.iter().map(|row|json!({
        "libraryId":row.get::<String,_>("library_id"),"name":row.get::<String,_>("name"),
        "type":row.get::<String,_>("library_type"),"privacy":row.get::<String,_>("privacy"),
        "canView":row.get::<i64,_>("can_view")!=0,"canPlay":row.get::<i64,_>("can_play")!=0
    })).collect::<Vec<_>>() })))
}

async fn update_library_access(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<LibraryAccessRequest>,
) -> AppResult<Json<Value>> {
    auth.require("users.permissions.manage")?;
    let db = state.database().await;
    if db.user_by_id(&id).await?.is_none() {
        return Err(AppError::not_found("USER_NOT_FOUND", "User not found."));
    }
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM user_library_access WHERE user_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    for LibraryAccessEntry {
        library_id,
        can_view,
        can_play,
    } in &payload.libraries
    {
        let exists: i64 =
            sqlx::query("SELECT COUNT(*) FROM libraries WHERE id=? AND deleted_at IS NULL")
                .bind(library_id)
                .fetch_one(&mut *tx)
                .await?
                .get(0);
        if exists == 0 {
            return Err(AppError::validation(
                "INVALID_LIBRARY",
                "A selected library does not exist.",
            ));
        }
        sqlx::query("INSERT INTO user_library_access(id,user_id,library_id,can_view,can_play,granted_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string()).bind(&id).bind(library_id)
            .bind(if *can_view {1}else{0}).bind(if *can_view && *can_play {1}else{0})
            .bind(&auth.id).bind(now()).bind(now()).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM profile_library_access WHERE profile_id IN(SELECT id FROM profiles WHERE user_id=?) AND library_id IN(SELECT l.id FROM libraries l WHERE l.privacy='PRIVATE' AND NOT EXISTS(SELECT 1 FROM user_library_access ula WHERE ula.user_id=? AND ula.library_id=l.id AND ula.can_view=1))")
        .bind(&id).bind(&id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO profile_library_access(profile_id,library_id,is_allowed,created_at,updated_at) SELECT p.id,ula.library_id,1,?,? FROM profiles p JOIN user_library_access ula ON ula.user_id=p.user_id AND ula.can_view=1 LEFT JOIN profile_library_access pla ON pla.profile_id=p.id AND pla.library_id=ula.library_id WHERE p.user_id=? AND p.is_default=1 AND pla.profile_id IS NULL")
        .bind(now()).bind(now()).bind(&id).execute(&mut *tx).await?;
    insert_audit(
        &mut tx,
        Some(&auth.id),
        "USER_LIBRARY_ACCESS_CHANGED",
        "user",
        Some(&id),
        json!({"libraryCount":payload.libraries.len()}),
    )
    .await?;
    tx.commit().await?;
    state.recommendations.invalidate_all().await;
    get_library_access(State(state), auth, Path(id)).await
}
