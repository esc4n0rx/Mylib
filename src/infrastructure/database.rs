use std::{path::PathBuf, time::Duration};

use sqlx::{
    AnyConnection, AnyPool, Executor, Row, Sqlite, any::AnyPoolOptions, migrate::MigrateDatabase,
};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::{RoleResponse, ServerConfigRecord, UserRecord, UserResponse},
};

pub const PERMISSIONS: &[&str] = &[
    "server.view",
    "server.manage",
    "users.view",
    "users.create",
    "users.update",
    "users.disable",
    "users.permissions.manage",
    "libraries.view",
    "libraries.manage",
    "libraries.scan",
    "libraries.unlock",
    "media.view",
    "media.identify",
    "media.manage",
    "media.play",
    "playback.view_own",
    "playback.history.view_own",
    "playback.sessions.view",
    "playback.sessions.manage",
    "jobs.view",
    "jobs.manage",
    "storage.view",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseKind {
    Sqlite,
    MySql,
}

impl DatabaseKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::MySql => "mysql",
        }
    }
    pub fn parse(value: &str) -> AppResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            "sqlite" => Ok(Self::Sqlite),
            "mysql" => Ok(Self::MySql),
            _ => Err(AppError::validation(
                "INVALID_DATABASE_TYPE",
                "Database type must be sqlite or mysql.",
            )),
        }
    }
}

#[derive(Clone)]
pub struct Database {
    pub pool: AnyPool,
    pub kind: DatabaseKind,
}

impl Database {
    pub async fn connect(kind: DatabaseKind, url: &str) -> AppResult<Self> {
        sqlx::any::install_default_drivers();
        if kind == DatabaseKind::Sqlite {
            tracing::debug!(database_url = url, "opening SQLite database");
        }
        if kind == DatabaseKind::Sqlite && !Sqlite::database_exists(url).await.unwrap_or(false) {
            Sqlite::create_database(url).await.map_err(|error| {
                tracing::warn!(database_type = kind.as_str(), %error, "database creation failed");
                AppError::new(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "DATABASE_CONNECTION_FAILED",
                    "Unable to create the configured SQLite database.",
                )
            })?;
        }
        let pool = AnyPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|error| {
                tracing::warn!(database_type = kind.as_str(), %error, "database connection failed");
                AppError::new(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "DATABASE_CONNECTION_FAILED",
                    "Unable to connect to the configured database.",
                )
            })?;
        Ok(Self { pool, kind })
    }

    pub async fn migrate(&self) -> AppResult<()> {
        for statement in migration_statements(self.kind) {
            self.pool.execute(*statement).await?;
        }
        self.pool.execute("CREATE TABLE IF NOT EXISTS schema_migrations (version VARCHAR(64) PRIMARY KEY, applied_at VARCHAR(40) NOT NULL)").await?;
        let applied: i64 = sqlx::query("SELECT COUNT(*) FROM schema_migrations WHERE version=?")
            .bind("0002_libraries_catalog")
            .fetch_one(&self.pool)
            .await?
            .get(0);
        if applied == 0 {
            let script = match self.kind {
                DatabaseKind::Sqlite => {
                    include_str!("../../migrations/sqlite/0002_libraries_catalog.sql")
                }
                DatabaseKind::MySql => {
                    include_str!("../../migrations/mysql/0002_libraries_catalog.sql")
                }
            };
            for statement in script.split(';').map(str::trim).filter(|v| !v.is_empty()) {
                self.pool.execute(statement).await?;
            }
            sqlx::query("INSERT INTO schema_migrations (version,applied_at) VALUES (?,?)")
                .bind("0002_libraries_catalog")
                .bind(now())
                .execute(&self.pool)
                .await?;
        }
        let catalog_applied: i64 =
            sqlx::query("SELECT COUNT(*) FROM schema_migrations WHERE version=?")
                .bind("0003_content_catalog")
                .fetch_one(&self.pool)
                .await?
                .get(0);
        if catalog_applied == 0 {
            let script = match self.kind {
                DatabaseKind::Sqlite => {
                    include_str!("../../migrations/sqlite/0003_content_catalog.sql")
                }
                DatabaseKind::MySql => {
                    include_str!("../../migrations/mysql/0003_content_catalog.sql")
                }
            };
            for statement in script.split(';').map(str::trim).filter(|v| !v.is_empty()) {
                self.pool.execute(statement).await?;
            }
            sqlx::query("INSERT INTO schema_migrations (version,applied_at) VALUES (?,?)")
                .bind("0003_content_catalog")
                .bind(now())
                .execute(&self.pool)
                .await?;
        }
        let playback_applied: i64 =
            sqlx::query("SELECT COUNT(*) FROM schema_migrations WHERE version=?")
                .bind("0004_playback")
                .fetch_one(&self.pool)
                .await?
                .get(0);
        if playback_applied == 0 {
            let script = match self.kind {
                DatabaseKind::Sqlite => include_str!("../../migrations/sqlite/0004_playback.sql"),
                DatabaseKind::MySql => include_str!("../../migrations/mysql/0004_playback.sql"),
            };
            for statement in script.split(';').map(str::trim).filter(|v| !v.is_empty()) {
                self.pool.execute(statement).await?;
            }
            sqlx::query("INSERT INTO schema_migrations (version,applied_at) VALUES (?,?)")
                .bind("0004_playback")
                .bind(now())
                .execute(&self.pool)
                .await?;
        }
        let user_access_applied: i64 =
            sqlx::query("SELECT COUNT(*) FROM schema_migrations WHERE version=?")
                .bind("0005_user_library_access")
                .fetch_one(&self.pool)
                .await?
                .get(0);
        if user_access_applied == 0 {
            let script = match self.kind {
                DatabaseKind::Sqlite => {
                    include_str!("../../migrations/sqlite/0005_user_library_access.sql")
                }
                DatabaseKind::MySql => {
                    include_str!("../../migrations/mysql/0005_user_library_access.sql")
                }
            };
            for statement in script.split(';').map(str::trim).filter(|v| !v.is_empty()) {
                self.pool.execute(statement).await?;
            }
            sqlx::query("INSERT INTO schema_migrations (version,applied_at) VALUES (?,?)")
                .bind("0005_user_library_access")
                .bind(now())
                .execute(&self.pool)
                .await?;
        }
        let library_sync_applied: i64 =
            sqlx::query("SELECT COUNT(*) FROM schema_migrations WHERE version=?")
                .bind("0006_library_auto_sync")
                .fetch_one(&self.pool)
                .await?
                .get(0);
        if library_sync_applied == 0 {
            let script = match self.kind {
                DatabaseKind::Sqlite => {
                    include_str!("../../migrations/sqlite/0006_library_auto_sync.sql")
                }
                DatabaseKind::MySql => {
                    include_str!("../../migrations/mysql/0006_library_auto_sync.sql")
                }
            };
            for statement in script
                .split(';')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                self.pool.execute(statement).await?;
            }
            sqlx::query("INSERT INTO schema_migrations (version,applied_at) VALUES (?,?)")
                .bind("0006_library_auto_sync")
                .bind(now())
                .execute(&self.pool)
                .await?;
        }
        let profiles_applied: i64 =
            sqlx::query("SELECT COUNT(*) FROM schema_migrations WHERE version=?")
                .bind("0007_profiles_parental_controls")
                .fetch_one(&self.pool)
                .await?
                .get(0);
        if profiles_applied == 0 {
            let script = match self.kind {
                DatabaseKind::Sqlite => {
                    include_str!("../../migrations/sqlite/0007_profiles_parental_controls.sql")
                }
                DatabaseKind::MySql => {
                    include_str!("../../migrations/mysql/0007_profiles_parental_controls.sql")
                }
            };
            // SQLite PRAGMA foreign_keys is connection-local. Keep this migration on
            // one connection while the legacy consumption tables are rebuilt.
            let mut connection = self.pool.acquire().await?;
            for statement in script
                .split(';')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                connection.execute(statement).await?;
            }
            drop(connection);
            sqlx::query("INSERT INTO schema_migrations (version,applied_at) VALUES (?,?)")
                .bind("0007_profiles_parental_controls")
                .bind(now())
                .execute(&self.pool)
                .await?;
        }
        self.apply_versioned(
            "0008_remote_sources",
            include_str!("../../migrations/sqlite/0008_remote_sources.sql"),
            include_str!("../../migrations/mysql/0008_remote_sources.sql"),
        )
        .await?;
        self.apply_versioned(
            "0009_m3u_entries",
            include_str!("../../migrations/sqlite/0009_m3u_entries.sql"),
            include_str!("../../migrations/mysql/0009_m3u_entries.sql"),
        )
        .await?;
        self.apply_versioned(
            "0010_remote_media_sources",
            include_str!("../../migrations/sqlite/0010_remote_media_sources.sql"),
            include_str!("../../migrations/mysql/0010_remote_media_sources.sql"),
        )
        .await?;
        self.apply_versioned(
            "0011_google_drive",
            include_str!("../../migrations/sqlite/0011_google_drive.sql"),
            include_str!("../../migrations/mysql/0011_google_drive.sql"),
        )
        .await?;
        if self
            .server_config()
            .await?
            .is_some_and(|c| c.setup_completed != 0)
        {
            self.synchronize_permissions().await?;
        }
        tracing::info!(
            database_type = self.kind.as_str(),
            "database migrations complete"
        );
        Ok(())
    }

    /// Applies a numbered migration once, recording it in `schema_migrations`.
    /// Statements are separated on `;`, matching the existing loader; keep
    /// migration files free of semicolons inside string literals.
    async fn apply_versioned(
        &self,
        version: &str,
        sqlite_sql: &str,
        mysql_sql: &str,
    ) -> AppResult<()> {
        let applied: i64 = sqlx::query("SELECT COUNT(*) FROM schema_migrations WHERE version=?")
            .bind(version)
            .fetch_one(&self.pool)
            .await?
            .get(0);
        if applied != 0 {
            return Ok(());
        }
        let script = match self.kind {
            DatabaseKind::Sqlite => sqlite_sql,
            DatabaseKind::MySql => mysql_sql,
        };
        for statement in script
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.pool.execute(statement).await?;
        }
        sqlx::query("INSERT INTO schema_migrations (version,applied_at) VALUES (?,?)")
            .bind(version)
            .bind(now())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn synchronize_permissions(&self) -> AppResult<()> {
        let admin_role: Option<String> =
            sqlx::query("SELECT id FROM roles WHERE name='Administrator'")
                .fetch_optional(&self.pool)
                .await?
                .map(|r| r.get(0));
        let user_role: Option<String> = sqlx::query("SELECT id FROM roles WHERE name='User'")
            .fetch_optional(&self.pool)
            .await?
            .map(|r| r.get(0));
        for permission in PERMISSIONS {
            let permission_id: String = if let Some(row) =
                sqlx::query("SELECT id FROM permissions WHERE name=?")
                    .bind(permission)
                    .fetch_optional(&self.pool)
                    .await?
            {
                row.get(0)
            } else {
                let id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO permissions (id,name,description,created_at) VALUES (?,?,?,?)",
                )
                .bind(&id)
                .bind(permission)
                .bind(format!("Permission {permission}"))
                .bind(now())
                .execute(&self.pool)
                .await?;
                id
            };
            if let Some(role) = &admin_role {
                let count: i64 = sqlx::query(
                    "SELECT COUNT(*) FROM role_permissions WHERE role_id=? AND permission_id=?",
                )
                .bind(role)
                .bind(&permission_id)
                .fetch_one(&self.pool)
                .await?
                .get(0);
                if count == 0 {
                    sqlx::query(
                        "INSERT INTO role_permissions (role_id,permission_id) VALUES (?,?)",
                    )
                    .bind(role)
                    .bind(&permission_id)
                    .execute(&self.pool)
                    .await?;
                }
            }
            if matches!(
                *permission,
                "libraries.view"
                    | "media.view"
                    | "media.play"
                    | "playback.view_own"
                    | "playback.history.view_own"
            ) && let Some(role) = &user_role
            {
                let count: i64 = sqlx::query(
                    "SELECT COUNT(*) FROM role_permissions WHERE role_id=? AND permission_id=?",
                )
                .bind(role)
                .bind(&permission_id)
                .fetch_one(&self.pool)
                .await?
                .get(0);
                if count == 0 {
                    sqlx::query(
                        "INSERT INTO role_permissions (role_id,permission_id) VALUES (?,?)",
                    )
                    .bind(role)
                    .bind(&permission_id)
                    .execute(&self.pool)
                    .await?;
                }
            }
        }
        Ok(())
    }

    pub async fn ping(&self) -> AppResult<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
    pub async fn server_config(&self) -> AppResult<Option<ServerConfigRecord>> {
        Ok(sqlx::query_as("SELECT id, server_id, server_name, setup_completed, database_type, server_version, created_at, updated_at FROM server_config LIMIT 1").fetch_optional(&self.pool).await?)
    }
    pub async fn user_by_username(&self, username: &str) -> AppResult<Option<UserRecord>> {
        Ok(sqlx::query_as("SELECT id, username, username_normalized, display_name, email, password_hash, is_active, last_login_at, created_at, updated_at FROM users WHERE username_normalized = ?").bind(username.to_ascii_lowercase()).fetch_optional(&self.pool).await?)
    }
    pub async fn user_by_id(&self, id: &str) -> AppResult<Option<UserRecord>> {
        Ok(sqlx::query_as("SELECT id, username, username_normalized, display_name, email, password_hash, is_active, last_login_at, created_at, updated_at FROM users WHERE id = ?").bind(id).fetch_optional(&self.pool).await?)
    }
    pub async fn roles_for_user(&self, id: &str) -> AppResult<Vec<String>> {
        Ok(sqlx::query("SELECT r.name FROM roles r JOIN user_roles ur ON ur.role_id=r.id WHERE ur.user_id=? ORDER BY r.name").bind(id).fetch_all(&self.pool).await?.into_iter().map(|r| r.get(0)).collect())
    }
    pub async fn permissions_for_user(&self, id: &str) -> AppResult<Vec<String>> {
        Ok(sqlx::query("SELECT DISTINCT p.name FROM permissions p JOIN role_permissions rp ON rp.permission_id=p.id JOIN user_roles ur ON ur.role_id=rp.role_id WHERE ur.user_id=? ORDER BY p.name").bind(id).fetch_all(&self.pool).await?.into_iter().map(|r| r.get(0)).collect())
    }

    pub async fn list_users(&self) -> AppResult<Vec<UserResponse>> {
        let records: Vec<UserRecord> = sqlx::query_as("SELECT id, username, username_normalized, display_name, email, password_hash, is_active, last_login_at, created_at, updated_at FROM users ORDER BY username_normalized").fetch_all(&self.pool).await?;
        let mut users = Vec::with_capacity(records.len());
        for record in records {
            users.push(self.user_response(record).await?);
        }
        Ok(users)
    }
    pub async fn user_response(&self, user: UserRecord) -> AppResult<UserResponse> {
        let roles = self.roles_for_user(&user.id).await?;
        let library_access_count: i64 =
            sqlx::query("SELECT COUNT(*) FROM user_library_access WHERE user_id=? AND can_view=1")
                .bind(&user.id)
                .fetch_one(&self.pool)
                .await?
                .get(0);
        let is_admin = roles.iter().any(|role| role == "Administrator");
        Ok(UserResponse {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            email: user.email,
            is_active: user.is_active != 0,
            roles,
            is_admin,
            last_login_at: user.last_login_at,
            library_access_count,
            created_at: user.created_at,
            updated_at: user.updated_at,
        })
    }
    pub async fn roles(&self) -> AppResult<Vec<RoleResponse>> {
        Ok(
            sqlx::query_as("SELECT id, name, description, is_system FROM roles ORDER BY name")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn active_admin_count(&self) -> AppResult<i64> {
        Ok(sqlx::query("SELECT COUNT(*) FROM users u JOIN user_roles ur ON ur.user_id=u.id JOIN roles r ON r.id=ur.role_id WHERE u.is_active=1 AND r.name='Administrator'").fetch_one(&self.pool).await?.get(0))
    }
    pub async fn is_admin(&self, id: &str) -> AppResult<bool> {
        Ok(self
            .roles_for_user(id)
            .await?
            .iter()
            .any(|r| r == "Administrator"))
    }
    pub async fn role_ids(&self, names: &[String]) -> AppResult<Vec<String>> {
        let mut ids = Vec::new();
        for name in names {
            match sqlx::query("SELECT id FROM roles WHERE name=?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?
            {
                Some(row) => ids.push(row.get(0)),
                None => {
                    return Err(AppError::validation(
                        "INVALID_ROLE",
                        format!("Unknown role: {name}."),
                    ));
                }
            }
        }
        Ok(ids)
    }
    pub async fn audit(
        &self,
        actor: Option<&str>,
        action: &str,
        entity_type: &str,
        entity_id: Option<&str>,
        metadata: serde_json::Value,
        ip: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query("INSERT INTO audit_log (id,actor_user_id,action,entity_type,entity_id,metadata,ip_address,created_at) VALUES (?,?,?,?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string()).bind(actor).bind(action).bind(entity_type).bind(entity_id).bind(metadata.to_string()).bind(ip).bind(now()).execute(&self.pool).await?;
        Ok(())
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn create_user(
    executor: &mut AnyConnection,
    request: &crate::models::CreateUserRequest,
    password_hash: &str,
) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    sqlx::query("INSERT INTO users (id,username,username_normalized,display_name,email,password_hash,is_active,created_at,updated_at) VALUES (?,?,?,?,?,?,1,?,?)")
        .bind(&id).bind(&request.username).bind(request.username.to_ascii_lowercase()).bind(&request.display_name).bind(&request.email).bind(password_hash).bind(&timestamp).bind(&timestamp).execute(&mut *executor).await.map_err(|error| {
            if error.to_string().to_ascii_lowercase().contains("unique") || error.to_string().to_ascii_lowercase().contains("duplicate") { AppError::conflict("USERNAME_ALREADY_EXISTS", "Username is already in use.") } else { AppError::from(error) }
        })?;
    let profile_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO profiles (id,user_id,name,avatar_id,is_default,is_kids,is_active,max_age_rating,created_at,updated_at) VALUES (?,?,?,'default.png',1,0,1,18,?,?)")
        .bind(profile_id)
        .bind(&id)
        .bind(request.display_name.trim())
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *executor)
        .await?;
    Ok(id)
}

fn migration_statements(kind: DatabaseKind) -> &'static [&'static str] {
    match kind {
        DatabaseKind::Sqlite => SQLITE_MIGRATIONS,
        DatabaseKind::MySql => MYSQL_MIGRATIONS,
    }
}

const SQLITE_MIGRATIONS: &[&str] = &[
    "PRAGMA journal_mode=WAL",
    "PRAGMA foreign_keys=ON",
    "CREATE TABLE IF NOT EXISTS server_config (id TEXT PRIMARY KEY, server_id TEXT NOT NULL UNIQUE, server_name TEXT NOT NULL, setup_completed INTEGER NOT NULL DEFAULT 0, database_type TEXT NOT NULL, server_version TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, username TEXT NOT NULL, username_normalized TEXT NOT NULL UNIQUE, display_name TEXT NOT NULL, email TEXT, password_hash TEXT NOT NULL, is_active INTEGER NOT NULL DEFAULT 1, last_login_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS roles (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL, is_system INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS permissions (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL, created_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS user_roles (user_id TEXT NOT NULL REFERENCES users(id), role_id TEXT NOT NULL REFERENCES roles(id), PRIMARY KEY(user_id,role_id))",
    "CREATE TABLE IF NOT EXISTS role_permissions (role_id TEXT NOT NULL REFERENCES roles(id), permission_id TEXT NOT NULL REFERENCES permissions(id), PRIMARY KEY(role_id,permission_id))",
    "CREATE TABLE IF NOT EXISTS audit_log (id TEXT PRIMARY KEY, actor_user_id TEXT, action TEXT NOT NULL, entity_type TEXT NOT NULL, entity_id TEXT, metadata TEXT NOT NULL, ip_address TEXT, created_at TEXT NOT NULL)",
];

const MYSQL_MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS server_config (id VARCHAR(36) PRIMARY KEY, server_id VARCHAR(36) NOT NULL UNIQUE, server_name VARCHAR(128) NOT NULL, setup_completed BOOLEAN NOT NULL DEFAULT FALSE, database_type VARCHAR(16) NOT NULL, server_version VARCHAR(32) NOT NULL, created_at VARCHAR(40) NOT NULL, updated_at VARCHAR(40) NOT NULL) ENGINE=InnoDB",
    "CREATE TABLE IF NOT EXISTS users (id VARCHAR(36) PRIMARY KEY, username VARCHAR(32) NOT NULL, username_normalized VARCHAR(32) NOT NULL UNIQUE, display_name VARCHAR(128) NOT NULL, email VARCHAR(254), password_hash VARCHAR(255) NOT NULL, is_active BOOLEAN NOT NULL DEFAULT TRUE, last_login_at VARCHAR(40), created_at VARCHAR(40) NOT NULL, updated_at VARCHAR(40) NOT NULL) ENGINE=InnoDB",
    "CREATE TABLE IF NOT EXISTS roles (id VARCHAR(36) PRIMARY KEY, name VARCHAR(64) NOT NULL UNIQUE, description VARCHAR(255) NOT NULL, is_system BOOLEAN NOT NULL DEFAULT FALSE, created_at VARCHAR(40) NOT NULL, updated_at VARCHAR(40) NOT NULL) ENGINE=InnoDB",
    "CREATE TABLE IF NOT EXISTS permissions (id VARCHAR(36) PRIMARY KEY, name VARCHAR(128) NOT NULL UNIQUE, description VARCHAR(255) NOT NULL, created_at VARCHAR(40) NOT NULL) ENGINE=InnoDB",
    "CREATE TABLE IF NOT EXISTS user_roles (user_id VARCHAR(36) NOT NULL, role_id VARCHAR(36) NOT NULL, PRIMARY KEY(user_id,role_id), FOREIGN KEY(user_id) REFERENCES users(id), FOREIGN KEY(role_id) REFERENCES roles(id)) ENGINE=InnoDB",
    "CREATE TABLE IF NOT EXISTS role_permissions (role_id VARCHAR(36) NOT NULL, permission_id VARCHAR(36) NOT NULL, PRIMARY KEY(role_id,permission_id), FOREIGN KEY(role_id) REFERENCES roles(id), FOREIGN KEY(permission_id) REFERENCES permissions(id)) ENGINE=InnoDB",
    "CREATE TABLE IF NOT EXISTS audit_log (id VARCHAR(36) PRIMARY KEY, actor_user_id VARCHAR(36), action VARCHAR(64) NOT NULL, entity_type VARCHAR(64) NOT NULL, entity_id VARCHAR(36), metadata JSON NOT NULL, ip_address VARCHAR(64), created_at VARCHAR(40) NOT NULL, INDEX idx_audit_created_at(created_at)) ENGINE=InnoDB",
];

pub fn config_file(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("config/database.json")
}
