//! Category/subcategory normalization and title cleaning for remote entries.
//! Builds on the local scanner's filename parser so M3U and Google Drive share
//! the same season/episode and year extraction as local files.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use crate::{libraries::LibraryType, scanner::parse_filename};

use super::models::EntryMediaType;

static BRACKET_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]*\]|\{[^}]*\}").expect("static bracket tag regex"));

/// Splits a `group-title` such as `"FILMES | LANÇAMENTOS 2025"` into a
/// normalized category and an optional raw subcategory. The primary separator is
/// `|`; without it the whole value is the category.
pub fn split_group_title(group_title: &str) -> (String, Option<String>) {
    let trimmed = group_title.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }
    match trimmed.split_once('|') {
        Some((category, subcategory)) => {
            let subcategory = subcategory.trim();
            (
                normalize_category(category),
                (!subcategory.is_empty()).then(|| subcategory.to_string()),
            )
        }
        None => (normalize_category(trimmed), None),
    }
}

/// Case- and accent-insensitive canonical bucket for a category label. Known
/// movie/series synonyms collapse to `FILMES` / `SERIES`; anything else is
/// upper-cased and whitespace-collapsed.
pub fn normalize_category(raw: &str) -> String {
    let folded = fold(raw);
    let compact = folded.replace(char::is_whitespace, "");
    if matches!(
        compact.as_str(),
        "serie" | "series" | "seriado" | "seriados" | "tvshow" | "tvshows" | "temporadas"
    ) || compact.starts_with("serie")
    {
        return "SERIES".to_string();
    }
    if matches!(
        compact.as_str(),
        "filme" | "filmes" | "movie" | "movies" | "lancamentos" | "cinema"
    ) || compact.starts_with("filme")
        || compact.starts_with("movie")
    {
        return "FILMES".to_string();
    }
    folded.to_uppercase()
}

fn fold(value: &str) -> String {
    value
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Debug, Clone)]
pub struct AnalyzedEntry {
    pub clean_title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub media_type: EntryMediaType,
    pub category: String,
    pub subcategory: Option<String>,
}

/// Derives clean title, year, season/episode and media type for one raw entry
/// using category, name pattern and URL hints, in that priority order.
pub fn analyze_entry(raw_name: &str, group_title: &str, stream_url: &str) -> AnalyzedEntry {
    let (category, subcategory) = split_group_title(group_title);
    let stripped = BRACKET_TAG_RE.replace_all(raw_name, " ");
    let probe = format!("{}.ts", stripped.trim());
    let parsed = parse_filename(&probe, LibraryType::TvShow);
    let has_episode = parsed.season.is_some() && !parsed.episodes.is_empty();

    let media_type = if has_episode || category == "SERIES" || url_hints_series(stream_url) {
        EntryMediaType::TvShow
    } else if category == "FILMES" || url_hints_movie(stream_url) {
        EntryMediaType::Movie
    } else {
        EntryMediaType::Unknown
    };

    let clean_title = if parsed.title.trim().is_empty() {
        BRACKET_TAG_RE
            .replace_all(raw_name, " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        parsed.title
    };

    AnalyzedEntry {
        clean_title,
        year: parsed.year,
        season: parsed.season,
        episode: parsed.episodes.first().copied(),
        media_type,
        category,
        subcategory,
    }
}

fn url_hints_series(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    ["/series/", "/serie/", "/tv/", "/tvshow", "/seriados/"]
        .iter()
        .any(|hint| lower.contains(hint))
}

fn url_hints_movie(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    ["/movie/", "/movies/", "/filme/", "/filmes/"]
        .iter()
        .any(|hint| lower.contains(hint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_synonyms_fold() {
        assert_eq!(normalize_category("Filmes"), "FILMES");
        assert_eq!(normalize_category("SÉRIES"), "SERIES");
        assert_eq!(normalize_category("series"), "SERIES");
        assert_eq!(normalize_category("Documentários"), "DOCUMENTARIOS");
    }

    #[test]
    fn group_title_splits_on_pipe() {
        let (category, subcategory) = split_group_title("FILMES | LANÇAMENTOS 2025");
        assert_eq!(category, "FILMES");
        assert_eq!(subcategory.as_deref(), Some("LANÇAMENTOS 2025"));
        let (category, subcategory) = split_group_title("NETFLIX");
        assert_eq!(category, "NETFLIX");
        assert_eq!(subcategory, None);
    }

    #[test]
    fn movie_entry_is_detected() {
        let entry = analyze_entry(
            "Uma Mulher Comum (2025)",
            "FILMES | LANÇAMENTOS 2025",
            "https://example.com/movie/stream.mp4",
        );
        assert_eq!(entry.clean_title, "Uma Mulher Comum");
        assert_eq!(entry.year, Some(2025));
        assert_eq!(entry.media_type, EntryMediaType::Movie);
        assert_eq!(entry.subcategory.as_deref(), Some("LANÇAMENTOS 2025"));
    }

    #[test]
    fn series_entry_extracts_season_and_episode() {
        let entry = analyze_entry(
            "Pablo Escobar: O Patrão do Mal (2012) S01E33 [LEG]",
            "SERIES | NETFLIX",
            "https://example.com/series/stream.mp4",
        );
        // The shared scanner normalizer strips punctuation such as the colon.
        assert_eq!(entry.clean_title, "Pablo Escobar O Patrão do Mal");
        assert_eq!(entry.year, Some(2012));
        assert_eq!(entry.season, Some(1));
        assert_eq!(entry.episode, Some(33));
        assert_eq!(entry.media_type, EntryMediaType::TvShow);
    }
}
