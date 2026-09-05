//! Streaming M3U/M3U8 parser. Lines are consumed from an async reader without
//! buffering the whole playlist; callers receive one [`RawEntry`] at a time (for
//! sync) or an aggregated [`PreviewSummary`] (for the preview screen).

use std::{collections::HashMap, sync::LazyLock};

use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use super::{
    models::EntryMediaType,
    normalize::{AnalyzedEntry, analyze_entry},
    sanitize_url,
};
use crate::errors::{AppError, AppResult};

static ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([A-Za-z0-9_-]+)="([^"]*)""#).expect("static m3u attribute regex")
});

/// One `#EXTINF` record with its stream URL, straight from the playlist.
#[derive(Debug, Clone)]
pub struct RawEntry {
    pub display_name: String,
    pub tvg_name: Option<String>,
    pub tvg_logo: Option<String>,
    pub group_title: Option<String>,
    pub stream_url: String,
}

impl RawEntry {
    pub fn best_name(&self) -> &str {
        self.tvg_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.display_name)
    }

    /// Stable identifier: normalized stream URL + normalized name. Survives
    /// re-ordering and cosmetic changes so sync can diff playlists.
    pub fn external_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(
            sanitize_url(&self.stream_url)
                .to_ascii_lowercase()
                .as_bytes(),
        );
        hasher.update(b"\n");
        hasher.update(self.best_name().trim().to_ascii_lowercase().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn entry_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.stream_url.as_bytes());
        hasher.update(b"\n");
        hasher.update(self.display_name.as_bytes());
        hasher.update(b"\n");
        hasher.update(self.group_title.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"\n");
        hasher.update(self.tvg_logo.as_deref().unwrap_or("").as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn analyze(&self) -> AnalyzedEntry {
        analyze_entry(
            self.best_name(),
            self.group_title.as_deref().unwrap_or(""),
            &self.stream_url,
        )
    }
}

struct PendingExtinf {
    display_name: String,
    tvg_name: Option<String>,
    tvg_logo: Option<String>,
    group_title: Option<String>,
}

fn parse_extinf(line: &str) -> Option<PendingExtinf> {
    let rest = line.strip_prefix("#EXTINF:")?;
    // The display name is everything after the first comma that is not inside a
    // quoted attribute value.
    let mut in_quotes = false;
    let mut split_at = None;
    for (index, ch) in rest.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                split_at = Some(index);
                break;
            }
            _ => {}
        }
    }
    let (attrs, display_name) = match split_at {
        Some(index) => (&rest[..index], rest[index + 1..].trim().to_string()),
        None => (rest, String::new()),
    };
    let mut tvg_name = None;
    let mut tvg_logo = None;
    let mut group_title = None;
    for capture in ATTR_RE.captures_iter(attrs) {
        let value = capture[2].trim().to_string();
        if value.is_empty() {
            continue;
        }
        match capture[1].to_ascii_lowercase().as_str() {
            "tvg-name" => tvg_name = Some(value),
            "tvg-logo" => tvg_logo = Some(value),
            "group-title" => group_title = Some(value),
            _ => {}
        }
    }
    Some(PendingExtinf {
        display_name,
        tvg_name,
        tvg_logo,
        group_title,
    })
}

/// Incremental line-by-line M3U state machine. Feed each trimmed, non-empty
/// line; a completed record is returned once its URL line is seen. Shared by the
/// preview aggregator and the sync engine so both parse identically.
#[derive(Default)]
pub struct M3uParser {
    seen_header: bool,
    pending: Option<PendingExtinf>,
}

impl M3uParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, line: &str) -> AppResult<Option<RawEntry>> {
        if !self.seen_header {
            if line.starts_with("#EXTM3U") {
                self.seen_header = true;
                return Ok(None);
            }
            return Err(AppError::validation(
                "INVALID_M3U",
                "The file is not a valid M3U playlist.",
            ));
        }
        if line.starts_with("#EXTINF:") {
            self.pending = parse_extinf(line);
            return Ok(None);
        }
        if line.starts_with('#') {
            return Ok(None);
        }
        let Some(extinf) = self.pending.take() else {
            return Ok(None);
        };
        Ok(Some(RawEntry {
            display_name: if extinf.display_name.is_empty() {
                extinf.tvg_name.clone().unwrap_or_else(|| line.to_string())
            } else {
                extinf.display_name
            },
            tvg_name: extinf.tvg_name,
            tvg_logo: extinf.tvg_logo,
            group_title: extinf.group_title,
            stream_url: line.to_string(),
        }))
    }
}

pub(crate) fn read_error(error: std::io::Error) -> AppError {
    tracing::warn!(%error, "failed reading M3U stream");
    AppError::new(
        axum::http::StatusCode::BAD_GATEWAY,
        "M3U_READ_FAILED",
        "The playlist could not be read.",
    )
}

/// Streams the playlist, invoking `on_entry` once per record. Returns the number
/// of entries emitted. Enforces `max_bytes` while reading.
pub async fn parse_stream<R, F>(reader: R, max_bytes: u64, mut on_entry: F) -> AppResult<u64>
where
    R: AsyncBufRead + Unpin,
    F: FnMut(RawEntry),
{
    let mut lines = reader.lines();
    let mut parser = M3uParser::new();
    let mut consumed: u64 = 0;
    let mut count = 0_u64;

    while let Some(line) = lines.next_line().await.map_err(read_error)? {
        consumed = consumed.saturating_add(line.len() as u64 + 1);
        if consumed > max_bytes {
            return Err(AppError::validation(
                "M3U_TOO_LARGE",
                "The playlist exceeds the configured maximum size.",
            ));
        }
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.is_empty() {
            continue;
        }
        if let Some(entry) = parser.feed(line)? {
            on_entry(entry);
            count += 1;
        }
    }
    Ok(count)
}

const MAX_TRACKED_CATEGORIES: usize = 4000;
const MAX_TRACKED_SUBCATEGORIES: usize = 4000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSummary {
    pub total_entries: u64,
    pub movie_candidates: u64,
    pub tv_candidates: u64,
    pub unknown_candidates: u64,
    pub categories: Vec<CategorySummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    pub name: String,
    pub media_type: String,
    pub count: u64,
    pub subcategories: Vec<SubcategorySummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubcategorySummary {
    pub name: String,
    pub count: u64,
}

#[derive(Default)]
struct CategoryAgg {
    count: u64,
    movie: u64,
    tv: u64,
    subcategories: HashMap<String, u64>,
}

/// Streams the playlist and returns aggregate counts only — memory stays bounded
/// by the number of distinct categories, never the number of entries.
pub async fn analyze_stream<R>(reader: R, max_bytes: u64) -> AppResult<PreviewSummary>
where
    R: AsyncBufRead + Unpin,
{
    let mut total = 0_u64;
    let mut movies = 0_u64;
    let mut tv = 0_u64;
    let mut unknown = 0_u64;
    let mut categories: HashMap<String, CategoryAgg> = HashMap::new();

    parse_stream(reader, max_bytes, |entry| {
        total += 1;
        let analyzed = entry.analyze();
        match analyzed.media_type {
            EntryMediaType::Movie => movies += 1,
            EntryMediaType::TvShow => tv += 1,
            EntryMediaType::Unknown => unknown += 1,
        }
        let category_name = if analyzed.category.is_empty() {
            "SEM CATEGORIA".to_string()
        } else {
            analyzed.category
        };
        if !categories.contains_key(&category_name) && categories.len() >= MAX_TRACKED_CATEGORIES {
            return;
        }
        let aggregate = categories.entry(category_name).or_default();
        aggregate.count += 1;
        match analyzed.media_type {
            EntryMediaType::Movie => aggregate.movie += 1,
            EntryMediaType::TvShow => aggregate.tv += 1,
            EntryMediaType::Unknown => {}
        }
        if let Some(subcategory) = analyzed.subcategory
            && (aggregate.subcategories.contains_key(&subcategory)
                || aggregate.subcategories.len() < MAX_TRACKED_SUBCATEGORIES)
        {
            *aggregate.subcategories.entry(subcategory).or_insert(0) += 1;
        }
    })
    .await?;

    let mut category_list: Vec<CategorySummary> = categories
        .into_iter()
        .map(|(name, aggregate)| {
            let mut subcategories: Vec<SubcategorySummary> = aggregate
                .subcategories
                .into_iter()
                .map(|(name, count)| SubcategorySummary { name, count })
                .collect();
            subcategories.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
            CategorySummary {
                media_type: if aggregate.tv > aggregate.movie {
                    "TV_SHOW"
                } else if aggregate.movie > 0 {
                    "MOVIE"
                } else {
                    "UNKNOWN"
                }
                .to_string(),
                name,
                count: aggregate.count,
                subcategories,
            }
        })
        .collect();
    category_list.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));

    Ok(PreviewSummary {
        total_entries: total,
        movie_candidates: movies,
        tv_candidates: tv,
        unknown_candidates: unknown,
        categories: category_list,
    })
}

/// Cheap structural check used before accepting an uploaded file.
pub fn looks_like_m3u(head: &[u8]) -> bool {
    let text = String::from_utf8_lossy(head);
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with("#EXTM3U")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#EXTM3U\n#EXTINF:-1 tvg-name=\"Uma Mulher Comum (2025)\" tvg-logo=\"logo.png\" group-title=\"FILMES | LANÇAMENTOS 2025\",Uma Mulher Comum (2025)\nhttps://example.com/movie/a.mp4\n#EXTINF:-1 tvg-name=\"Breaking Bad (2008) S01E01\" group-title=\"SERIES | NETFLIX\",Breaking Bad S01E01\nhttps://example.com/series/b.mp4\n#EXTINF:-1 group-title=\"SERIES | NETFLIX\",Breaking Bad S01E02\nhttps://example.com/series/c.mp4\n";

    #[tokio::test]
    async fn parses_entries_and_attributes() {
        let mut entries = Vec::new();
        let count = parse_stream(SAMPLE.as_bytes(), 1_000_000, |entry| entries.push(entry))
            .await
            .unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            entries[0].group_title.as_deref(),
            Some("FILMES | LANÇAMENTOS 2025")
        );
        assert_eq!(entries[0].tvg_logo.as_deref(), Some("logo.png"));
        assert_eq!(entries[0].stream_url, "https://example.com/movie/a.mp4");
    }

    #[tokio::test]
    async fn summary_aggregates_by_category() {
        let summary = analyze_stream(SAMPLE.as_bytes(), 1_000_000).await.unwrap();
        assert_eq!(summary.total_entries, 3);
        assert_eq!(summary.movie_candidates, 1);
        assert_eq!(summary.tv_candidates, 2);
        let series = summary
            .categories
            .iter()
            .find(|category| category.name == "SERIES")
            .unwrap();
        assert_eq!(series.count, 2);
        assert_eq!(series.subcategories[0].name, "NETFLIX");
        assert_eq!(series.subcategories[0].count, 2);
    }

    #[tokio::test]
    async fn rejects_non_m3u() {
        let error = parse_stream("not a playlist\n".as_bytes(), 1000, |_| {})
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_M3U");
    }

    #[tokio::test]
    async fn enforces_size_limit() {
        let error = analyze_stream(SAMPLE.as_bytes(), 10).await.unwrap_err();
        assert_eq!(error.code, "M3U_TOO_LARGE");
    }

    #[test]
    fn stable_external_key_ignores_credentials() {
        let a = RawEntry {
            display_name: "Film".into(),
            tvg_name: None,
            tvg_logo: None,
            group_title: None,
            stream_url: "https://user:pass@host/movie/1.mp4".into(),
        };
        let b = RawEntry {
            stream_url: "https://host/movie/1.mp4".into(),
            ..a.clone()
        };
        assert_eq!(a.external_key(), b.external_key());
    }
}
