use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub username_normalized: String,
    pub display_name: String,
    pub email: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_active: i64,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub is_active: bool,
    pub roles: Vec<String>,
    pub is_admin: bool,
    pub last_login_at: Option<String>,
    pub library_access_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RoleResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_system: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ServerConfigRecord {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub setup_completed: i64,
    pub database_type: String,
    pub server_version: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub display_name: String,
    pub email: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub library_access: Vec<LibraryAccessEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRequest {
    pub password: Option<String>,
    pub new_password: Option<String>,
}

impl PasswordRequest {
    pub fn value(&self) -> Option<&str> {
        self.new_password.as_deref().or(self.password.as_deref())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAccessEntry {
    pub library_id: String,
    #[serde(default = "default_true")]
    pub can_view: bool,
    #[serde(default = "default_true")]
    pub can_play: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct LibraryAccessRequest {
    pub libraries: Vec<LibraryAccessEntry>,
}

#[derive(Debug, Deserialize)]
pub struct RolesRequest {
    pub roles: Vec<String>,
}
