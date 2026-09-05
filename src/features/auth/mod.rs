use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, header, request::Parts},
};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    app::AppState,
    errors::{AppError, AppResult},
};

pub fn hash_password(password: &str) -> AppResult<String> {
    validate_password(password)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|error| {
            tracing::error!(%error, "password hashing failed");
            AppError::config("Unable to secure password.")
        })
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded).ok().is_some_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

pub fn validate_username(username: &str) -> AppResult<()> {
    let valid = Regex::new(r"^[A-Za-z0-9_.-]{3,32}$")
        .map_err(|_| AppError::config("username validator failed"))?;
    if valid.is_match(username) {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_USERNAME",
            "Username must be 3-32 characters and contain only letters, numbers, _, - or .",
        ))
    }
}

pub fn validate_password(password: &str) -> AppResult<()> {
    let weak = [
        "password",
        "password123",
        "1234567890",
        "qwerty12345",
        "administrator",
    ];
    if password.chars().count() < 10 {
        return Err(AppError::validation(
            "WEAK_PASSWORD",
            "Password must contain at least 10 characters.",
        ));
    }
    if weak
        .iter()
        .any(|value| password.eq_ignore_ascii_case(value))
    {
        return Err(AppError::validation(
            "WEAK_PASSWORD",
            "Choose a less common password.",
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    username: String,
    #[serde(default)]
    profile_id: Option<String>,
    iat: usize,
    exp: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct LibraryUnlockClaims {
    sub: String,
    library_id: String,
    purpose: String,
    iat: usize,
    exp: usize,
}

#[derive(Clone)]
pub struct TokenService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    ttl: i64,
}

impl TokenService {
    pub fn new(secret: &[u8], ttl: i64) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            ttl,
        }
    }
    pub fn issue(&self, user_id: &str, username: &str) -> AppResult<String> {
        let now = Utc::now().timestamp();
        encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                sub: user_id.into(),
                username: username.into(),
                profile_id: None,
                iat: now as usize,
                exp: (now + self.ttl) as usize,
            },
            &self.encoding,
        )
        .map_err(|error| {
            tracing::error!(%error, "token issue failed");
            AppError::config("Unable to issue access token.")
        })
    }
    pub fn issue_for_profile(
        &self,
        user_id: &str,
        username: &str,
        profile_id: &str,
    ) -> AppResult<String> {
        let now = Utc::now().timestamp();
        encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                sub: user_id.into(),
                username: username.into(),
                profile_id: Some(profile_id.into()),
                iat: now as usize,
                exp: (now + self.ttl) as usize,
            },
            &self.encoding,
        )
        .map_err(|error| {
            tracing::error!(%error, "profile token issue failed");
            AppError::config("Unable to issue profile session.")
        })
    }
    pub fn verify(&self, token: &str) -> AppResult<(String, String, Option<String>)> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        let data = decode::<Claims>(token, &self.decoding, &validation)
            .map_err(|_| AppError::unauthorized())?;
        Ok((
            data.claims.sub,
            data.claims.username,
            data.claims.profile_id,
        ))
    }
    pub fn ttl(&self) -> i64 {
        self.ttl
    }
    pub fn issue_library_unlock(
        &self,
        user_id: &str,
        library_id: &str,
        ttl: i64,
    ) -> AppResult<String> {
        let now = Utc::now().timestamp();
        encode(
            &Header::new(Algorithm::HS256),
            &LibraryUnlockClaims {
                sub: user_id.into(),
                library_id: library_id.into(),
                purpose: "library_unlock".into(),
                iat: now as usize,
                exp: (now + ttl) as usize,
            },
            &self.encoding,
        )
        .map_err(|_| AppError::config("Unable to issue library unlock token."))
    }
    pub fn verify_library_unlock(&self, token: &str, user_id: &str, library_id: &str) -> bool {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        decode::<LibraryUnlockClaims>(token, &self.decoding, &validation)
            .ok()
            .is_some_and(|data| {
                data.claims.sub == user_id
                    && data.claims.library_id == library_id
                    && data.claims.purpose == "library_unlock"
            })
    }
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub profile_id: Option<String>,
    pub profile_is_kids: bool,
}

impl AuthUser {
    pub fn require(&self, permission: &str) -> AppResult<()> {
        if self.profile_is_kids
            && !matches!(
                permission,
                "media.view" | "media.play" | "playback.view_own" | "playback.history.view_own"
            )
        {
            return Err(AppError::forbidden());
        }
        if self.permissions.iter().any(|p| p == permission) {
            Ok(())
        } else {
            Err(AppError::forbidden())
        }
    }
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|r| r == "Administrator")
    }
    pub fn require_profile(&self) -> AppResult<&str> {
        self.profile_id.as_deref().ok_or_else(|| {
            AppError::new(
                StatusCode::PRECONDITION_REQUIRED,
                "PROFILE_REQUIRED",
                "Select a profile before accessing media.",
            )
        })
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(AppError::unauthorized)?;
        let token = value
            .strip_prefix("Bearer ")
            .ok_or_else(AppError::unauthorized)?;
        let (id, username, profile_id) = state.tokens.verify(token)?;
        let db = state.database().await;
        let user = db
            .user_by_id(&id)
            .await?
            .ok_or_else(AppError::unauthorized)?;
        if user.is_active == 0 {
            return Err(AppError::new(
                StatusCode::UNAUTHORIZED,
                "ACCOUNT_DISABLED",
                "This account is disabled.",
            ));
        }
        let roles = db.roles_for_user(&id).await?;
        let permissions = db.permissions_for_user(&id).await?;
        let mut profile_is_kids = false;
        if let Some(profile_id) = &profile_id {
            let profile = sqlx::query(
                "SELECT is_kids FROM profiles WHERE id=? AND user_id=? AND is_active=1",
            )
            .bind(profile_id)
            .bind(&id)
            .fetch_optional(&db.pool)
            .await?
            .ok_or_else(AppError::unauthorized)?;
            profile_is_kids = profile.get::<i64, _>("is_kids") != 0;
        }
        Ok(Self {
            id,
            username,
            roles,
            permissions,
            profile_id,
            profile_is_kids,
        })
    }
}
