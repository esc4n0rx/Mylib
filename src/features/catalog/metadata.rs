use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{sync::Semaphore, time::sleep};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use crate::{
    errors::{AppError, AppResult},
    libraries::LibraryType,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchCandidate {
    pub provider: String,
    pub provider_id: i64,
    #[serde(rename = "type")]
    pub media_type: String,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub rating: Option<f64>,
    #[serde(skip)]
    pub popularity: f64,
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    async fn search(
        &self,
        kind: LibraryType,
        query: &str,
        year: Option<i32>,
        language: &str,
        region: Option<&str>,
    ) -> AppResult<Vec<SearchCandidate>>;
    async fn details(&self, kind: LibraryType, id: i64, language: &str) -> AppResult<Value>;
    async fn season_details(&self, show_id: i64, season: i32, language: &str) -> AppResult<Value>;
    fn configured(&self) -> bool;
}

#[derive(Clone)]
pub struct TmdbMetadataProvider {
    key: Option<String>,
    client: Client,
    slots: Arc<Semaphore>,
}

impl TmdbMetadataProvider {
    pub fn new(
        key: Option<String>,
        timeout_seconds: u64,
        slots: Arc<Semaphore>,
    ) -> AppResult<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(timeout_seconds))
            .user_agent(concat!("MyLib/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| AppError::config("Unable to initialize TMDB client."))?;
        Ok(Self { key, client, slots })
    }
    async fn get(&self, path: &str, params: &[(&str, String)]) -> AppResult<Value> {
        let key = self.key.as_ref().ok_or_else(|| {
            AppError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "TMDB_NOT_CONFIGURED",
                "TMDB is not configured.",
            )
        })?;
        let _permit = self
            .slots
            .acquire()
            .await
            .map_err(|_| AppError::config("TMDB limiter is unavailable."))?;
        for attempt in 0..3_u32 {
            let response = self
                .client
                .get(format!("https://api.themoviedb.org/3/{path}"))
                .query(&[("api_key", key)])
                .query(params)
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => {
                    return response.json().await.map_err(|_| {
                        AppError::new(
                            axum::http::StatusCode::BAD_GATEWAY,
                            "TMDB_INVALID_RESPONSE",
                            "TMDB returned an invalid response.",
                        )
                    });
                }
                Ok(response)
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    let retry = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1_u64 << attempt);
                    sleep(Duration::from_millis(
                        retry.saturating_mul(1000) + u64::from(attempt) * 137,
                    ))
                    .await;
                }
                Ok(response) => {
                    tracing::warn!(status=%response.status(), "TMDB request rejected");
                    return Err(AppError::new(
                        axum::http::StatusCode::BAD_GATEWAY,
                        "TMDB_REQUEST_FAILED",
                        "TMDB request failed.",
                    ));
                }
                Err(error) if attempt < 2 && (error.is_timeout() || error.is_connect()) => {
                    sleep(Duration::from_millis(
                        (1_u64 << attempt) * 500 + u64::from(attempt) * 137,
                    ))
                    .await
                }
                Err(error) => {
                    tracing::warn!(%error, "TMDB request failed");
                    return Err(AppError::new(
                        axum::http::StatusCode::BAD_GATEWAY,
                        "TMDB_UNAVAILABLE",
                        "TMDB is unavailable.",
                    ));
                }
            }
        }
        Err(AppError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            "TMDB_UNAVAILABLE",
            "TMDB is unavailable.",
        ))
    }
}

#[async_trait]
impl MetadataProvider for TmdbMetadataProvider {
    fn configured(&self) -> bool {
        self.key.is_some()
    }
    async fn search(
        &self,
        kind: LibraryType,
        query: &str,
        year: Option<i32>,
        language: &str,
        region: Option<&str>,
    ) -> AppResult<Vec<SearchCandidate>> {
        let endpoint = match kind {
            LibraryType::Movie => "search/movie",
            LibraryType::TvShow => "search/tv",
        };
        let mut params = vec![
            ("query", query.into()),
            ("language", language.into()),
            ("include_adult", "false".into()),
        ];
        if let Some(year) = year {
            params.push((
                if kind == LibraryType::Movie {
                    "year"
                } else {
                    "first_air_date_year"
                },
                year.to_string(),
            ));
        }
        if kind == LibraryType::Movie
            && let Some(region) = region
        {
            params.push(("region", region.into()));
        }
        let value = self.get(endpoint, &params).await?;
        Ok(value["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let id = item["id"].as_i64()?;
                let title = item[if kind == LibraryType::Movie {
                    "title"
                } else {
                    "name"
                }]
                .as_str()?
                .to_owned();
                let original_title = item[if kind == LibraryType::Movie {
                    "original_title"
                } else {
                    "original_name"
                }]
                .as_str()
                .map(str::to_owned);
                let date = item[if kind == LibraryType::Movie {
                    "release_date"
                } else {
                    "first_air_date"
                }]
                .as_str();
                Some(SearchCandidate {
                    provider: "TMDB".into(),
                    provider_id: id,
                    media_type: kind.as_str().into(),
                    title,
                    original_title,
                    year: date.and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
                    overview: item["overview"].as_str().map(str::to_owned),
                    poster_path: item["poster_path"].as_str().map(str::to_owned),
                    rating: item["vote_average"].as_f64(),
                    popularity: item["popularity"].as_f64().unwrap_or_default(),
                })
            })
            .collect())
    }
    async fn details(&self, kind: LibraryType, id: i64, language: &str) -> AppResult<Value> {
        let entity = if kind == LibraryType::Movie {
            "movie"
        } else {
            "tv"
        };
        self.get(
            &format!("{entity}/{id}"),
            &[
                ("language", language.into()),
                (
                    "append_to_response",
                    if kind == LibraryType::Movie {
                        "credits,external_ids,keywords,release_dates"
                    } else {
                        "credits,external_ids,keywords,content_ratings"
                    }
                    .into(),
                ),
            ],
        )
        .await
    }
    async fn season_details(&self, show_id: i64, season: i32, language: &str) -> AppResult<Value> {
        self.get(
            &format!("tv/{show_id}/season/{season}"),
            &[("language", language.into())],
        )
        .await
    }
}

pub fn confidence(
    parsed_title: &str,
    parsed_year: Option<i32>,
    candidate: &SearchCandidate,
) -> f64 {
    let normalize = |v: &str| {
        v.nfd()
            .filter(|c| !is_combining_mark(*c))
            .flat_map(char::to_lowercase)
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let title = normalize(parsed_title);
    let compare = |candidate_title: &str| {
        let candidate_title = normalize(candidate_title);
        if title == candidate_title {
            0.84
        } else {
            let a: std::collections::HashSet<_> = title.split_whitespace().collect();
            let b: std::collections::HashSet<_> = candidate_title.split_whitespace().collect();
            let union = a.union(&b).count();
            if union == 0 {
                0.0
            } else {
                0.76 * a.intersection(&b).count() as f64 / union as f64
            }
        }
    };
    let title_score = candidate
        .original_title
        .as_deref()
        .map(|original| compare(original).max(compare(&candidate.title)))
        .unwrap_or_else(|| compare(&candidate.title));
    let year_score = match (parsed_year, candidate.year) {
        (Some(a), Some(b)) if a == b => 0.18,
        (Some(_), Some(_)) => -0.2,
        _ => 0.10,
    };
    (title_score + year_score).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_title_and_year_auto_matches() {
        let c = SearchCandidate {
            provider: "TMDB".into(),
            provider_id: 1,
            media_type: "MOVIE".into(),
            title: "The Matrix".into(),
            original_title: None,
            year: Some(1999),
            overview: None,
            poster_path: None,
            rating: None,
            popularity: 1.0,
        };
        assert_eq!(confidence("The Matrix", Some(1999), &c), 1.0);
        assert!(confidence("The Matrix", Some(2020), &c) < 0.9);
        let mut localized = c.clone();
        localized.title = "O Último de Nós".into();
        localized.original_title = Some("The Last of Us".into());
        localized.year = None;
        assert!(confidence("The Last of Us", None, &localized) >= 0.9);
        localized.title = "Coracao Valente".into();
        localized.original_title = None;
        assert!(confidence("Coração Valente", None, &localized) >= 0.9);
    }
}
