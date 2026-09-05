use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderType {
    M3uUrl,
    M3uFile,
    GoogleDrive,
}

impl ProviderType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::M3uUrl => "M3U_URL",
            Self::M3uFile => "M3U_FILE",
            Self::GoogleDrive => "GOOGLE_DRIVE",
        }
    }
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "M3U_URL" => Ok(Self::M3uUrl),
            "M3U_FILE" => Ok(Self::M3uFile),
            "GOOGLE_DRIVE" => Ok(Self::GoogleDrive),
            _ => Err(AppError::validation(
                "INVALID_PROVIDER_TYPE",
                "Provider type must be M3U_URL, M3U_FILE or GOOGLE_DRIVE.",
            )),
        }
    }
    pub fn is_m3u(self) -> bool {
        matches!(self, Self::M3uUrl | Self::M3uFile)
    }
}

/// Operational state of a remote source. Mirrors the values documented in the
/// Task 12 specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Ready,
    Syncing,
    Warning,
    AuthRequired,
    Unavailable,
    Error,
    Disabled,
}

impl SourceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Syncing => "SYNCING",
            Self::Warning => "WARNING",
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::Unavailable => "UNAVAILABLE",
            Self::Error => "ERROR",
            Self::Disabled => "DISABLED",
        }
    }
}

/// Detected media type for a normalized remote entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryMediaType {
    Movie,
    TvShow,
    Unknown,
}

impl EntryMediaType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "MOVIE",
            Self::TvShow => "TV_SHOW",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteSourceRequest {
    pub name: String,
    pub provider_type: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub auto_sync: Option<RemoteAutoSyncRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRemoteSourceRequest {
    pub name: Option<String>,
    pub is_active: Option<bool>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub auto_sync: Option<RemoteAutoSyncRequest>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAutoSyncRequest {
    pub enabled: bool,
    #[serde(default)]
    pub interval_minutes: Option<i64>,
}

pub fn validate_name(name: &str) -> AppResult<()> {
    let length = name.trim().chars().count();
    if (1..=128).contains(&length) {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_REMOTE_SOURCE_NAME",
            "Source name must contain 1-128 characters.",
        ))
    }
}
