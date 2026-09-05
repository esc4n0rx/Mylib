use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    auth::hash_password,
    errors::{AppError, AppResult},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LibraryType {
    Movie,
    TvShow,
}

impl LibraryType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "MOVIE",
            Self::TvShow => "TV_SHOW",
        }
    }
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "MOVIE" => Ok(Self::Movie),
            "TV_SHOW" => Ok(Self::TvShow),
            _ => Err(AppError::validation(
                "INVALID_LIBRARY_TYPE",
                "Library type must be MOVIE or TV_SHOW.",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Privacy {
    Public,
    Private,
}

impl Privacy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "PUBLIC",
            Self::Private => "PRIVATE",
        }
    }
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "PUBLIC" => Ok(Self::Public),
            "PRIVATE" => Ok(Self::Private),
            _ => Err(AppError::validation(
                "INVALID_LIBRARY_PRIVACY",
                "Privacy must be PUBLIC or PRIVATE.",
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLibraryRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub library_type: String,
    pub privacy: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub minimum_age: i64,
    #[serde(default = "default_language")]
    pub metadata_language: String,
    #[serde(default)]
    pub metadata_region: Option<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLibraryRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub privacy: Option<String>,
    pub password: Option<String>,
    pub minimum_age: Option<i64>,
    pub metadata_language: Option<String>,
    pub metadata_region: Option<String>,
    pub is_active: Option<bool>,
    pub scan_enabled: Option<bool>,
    pub auto_sync: Option<AutoSyncRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSyncRequest {
    pub enabled: bool,
    pub mode: String,
    pub interval_minutes: Option<i64>,
    pub schedule: Option<AutoSyncScheduleRequest>,
    #[serde(default)]
    pub scan_on_startup: bool,
}

#[derive(Debug, Deserialize)]
pub struct AutoSyncScheduleRequest {
    pub hour: i64,
    pub minute: i64,
}

fn default_language() -> String {
    "en-US".into()
}

pub struct ValidatedLibrary {
    pub name: String,
    pub description: Option<String>,
    pub library_type: LibraryType,
    pub privacy: Privacy,
    pub password_hash: Option<String>,
    pub minimum_age: i64,
    pub metadata_language: String,
    pub metadata_region: Option<String>,
    pub paths: Vec<PathBuf>,
}

impl CreateLibraryRequest {
    pub fn validate(self, data_dir: &Path) -> AppResult<ValidatedLibrary> {
        validate_name(&self.name)?;
        if self
            .description
            .as_ref()
            .is_some_and(|v| v.chars().count() > 500)
        {
            return Err(AppError::validation(
                "INVALID_LIBRARY_DESCRIPTION",
                "Description may contain at most 500 characters.",
            ));
        }
        if !(0..=21).contains(&self.minimum_age) {
            return Err(AppError::validation(
                "INVALID_MINIMUM_AGE",
                "Minimum age must be between 0 and 21.",
            ));
        }
        if !valid_language(&self.metadata_language) {
            return Err(AppError::validation(
                "INVALID_METADATA_LANGUAGE",
                "Metadata language must use a language-region tag such as pt-BR.",
            ));
        }
        if self.paths.is_empty() {
            return Err(AppError::validation(
                "LIBRARY_PATH_REQUIRED",
                "At least one path is required.",
            ));
        }
        let privacy = Privacy::parse(&self.privacy)?;
        let password_hash = match (privacy, self.password) {
            (Privacy::Private, Some(password)) => Some(hash_password(&password)?),
            (Privacy::Private, None) => {
                return Err(AppError::validation(
                    "LIBRARY_PASSWORD_REQUIRED",
                    "A private library requires a password.",
                ));
            }
            (Privacy::Public, Some(_)) => {
                return Err(AppError::validation(
                    "UNEXPECTED_LIBRARY_PASSWORD",
                    "A public library may not have a password.",
                ));
            }
            (Privacy::Public, None) => None,
        };
        let mut paths: Vec<PathBuf> = Vec::with_capacity(self.paths.len());
        for value in self.paths {
            let path = validate_path(&value, data_dir)?;
            if paths.iter().any(|p| paths_overlap(p, &path)) {
                return Err(AppError::conflict(
                    "OVERLAPPING_LIBRARY_PATH",
                    "Library paths may not overlap.",
                ));
            }
            paths.push(path);
        }
        Ok(ValidatedLibrary {
            name: self.name.trim().into(),
            description: self.description.map(|v| v.trim().into()),
            library_type: LibraryType::parse(&self.library_type)?,
            privacy,
            password_hash,
            minimum_age: self.minimum_age,
            metadata_language: self.metadata_language,
            metadata_region: self.metadata_region,
            paths,
        })
    }
}

pub fn validate_name(name: &str) -> AppResult<()> {
    let length = name.trim().chars().count();
    if (3..=80).contains(&length) {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_LIBRARY_NAME",
            "Library name must contain 3-80 characters.",
        ))
    }
}
pub fn valid_language(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 5
        && bytes[2] == b'-'
        && bytes[..2].iter().all(u8::is_ascii_lowercase)
        && bytes[3..].iter().all(u8::is_ascii_uppercase)
}

pub fn validate_path(value: &str, data_dir: &Path) -> AppResult<PathBuf> {
    let raw = PathBuf::from(value);
    if raw.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::validation(
            "INVALID_LIBRARY_PATH",
            "Library path may not contain parent traversal.",
        ));
    }
    let path = fs::canonicalize(&raw).map_err(|_| {
        AppError::validation("INVALID_LIBRARY_PATH", "Library path does not exist.")
    })?;
    let internal = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    if path == internal || path.starts_with(&internal) {
        return Err(AppError::validation(
            "PROTECTED_LIBRARY_PATH",
            "MyLib internal data paths cannot be used as libraries.",
        ));
    }
    if !path.is_dir() {
        return Err(AppError::validation(
            "INVALID_LIBRARY_PATH",
            "Library path is not a directory.",
        ));
    }
    fs::read_dir(&path).map_err(|_| {
        AppError::validation("UNREADABLE_LIBRARY_PATH", "Library path is not readable.")
    })?;
    Ok(path)
}

pub fn paths_overlap(a: &Path, b: &Path) -> bool {
    a == b || a.starts_with(b) || b.starts_with(a)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathValidation {
    pub valid: bool,
    pub exists: bool,
    pub readable: bool,
    pub directory: bool,
}

pub fn inspect_path(value: &str, data_dir: &Path) -> PathValidation {
    let raw = PathBuf::from(value);
    let exists = raw.exists();
    let directory = raw.is_dir();
    let protected = fs::canonicalize(&raw)
        .ok()
        .zip(fs::canonicalize(data_dir).ok())
        .is_some_and(|(p, d)| p == d || p.starts_with(d));
    let traversal = raw.components().any(|c| matches!(c, Component::ParentDir));
    let readable = directory && fs::read_dir(&raw).is_ok();
    PathValidation {
        valid: exists && directory && readable && !protected && !traversal,
        exists,
        readable,
        directory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_detects_parents() {
        assert!(paths_overlap(
            Path::new("/media"),
            Path::new("/media/movies")
        ));
        assert!(!paths_overlap(Path::new("/movies"), Path::new("/tv")));
    }
    #[test]
    fn language_is_strict() {
        assert!(valid_language("pt-BR"));
        assert!(!valid_language("portuguese"));
    }
}
