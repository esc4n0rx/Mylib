use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
    time::SystemTime,
};

use regex::Regex;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::libraries::LibraryType;

const EXTENSIONS: &[&str] = &["mkv", "mp4", "m4v", "avi", "mov", "ts", "m2ts", "webm"];
const IGNORED_DIRS: &[&str] = &[
    "@eaDir",
    ".recycle",
    ".Trash",
    ".Trashes",
    "lost+found",
    "System Volume Information",
];
static EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:S(\d{1,2})[\s._-]*E(?:P)?(\d{1,3})(?:[\s._-]*E(?:P)?(\d{1,3}))?|\b(\d{1,2})x(\d{1,3})\b)")
        .expect("static episode regex")
});
static SEASON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\bS(?:EASON)?[\s._-]*0?(\d{1,2})\b|\bTEMPORADA[\s._-]*0?(\d{1,2})\b)")
        .expect("static season regex")
});
static EPISODE_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:E(?:P)?[\s._-]*)?(\d{1,3})(?:[\s._-]|$)")
        .expect("static episode-only regex")
});
static YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9])((?:19|20)\d{2})(?:[^0-9]|$)").expect("static year regex")
});
static NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(480p|720p|1080p|2160p|4k|uhd|hdr10?|dv|dolbyvision|bluray|web|webdl|webrip|hdtv|remux|x26[45]|h26[45]|hevc|av1|aac|ac3|eac3|dts|truehd|atmos|ddp|multi|dual|dubbed|subbed|repack|proper|extended|unrated|remastered)$").expect("static noise regex")
});

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub absolute_path: PathBuf,
    pub relative_path: PathBuf,
    pub filename: String,
    pub extension: String,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedName {
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episodes: Vec<i32>,
    pub noise: Vec<String>,
}

pub fn parse_filename(filename: &str, kind: LibraryType) -> ParsedName {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or(filename);
    let mut season = None;
    let mut episodes = Vec::new();
    let mut cut = stem.len();
    if kind == LibraryType::TvShow
        && let Some(caps) = EPISODE_RE.captures(stem)
    {
        cut = caps.get(0).map_or(cut, |m| m.start());
        season = caps
            .get(1)
            .or_else(|| caps.get(4))
            .and_then(|v| v.as_str().parse().ok());
        if let Some(value) = caps
            .get(2)
            .or_else(|| caps.get(5))
            .and_then(|v| v.as_str().parse().ok())
        {
            episodes.push(value);
        }
        if let Some(value) = caps.get(3).and_then(|v| v.as_str().parse().ok()) {
            episodes.push(value);
        }
    }
    let current_max = chrono::Utc::now().year() + 2;
    let year_match = YEAR_RE
        .captures(stem)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok().map(|y| (m.start(), y)))
        .filter(|(_, y)| *y <= current_max);
    if let Some((position, _)) = year_match {
        cut = cut.min(position);
    }
    let cleaned = clean_separators(&stem[..cut]);
    let mut title_words = Vec::new();
    for word in cleaned.split_whitespace() {
        if !NOISE_RE.is_match(word) {
            title_words.push(word);
        }
    }
    let normalized_all = stem
        .replace("WEB-DL", "WEBDL")
        .replace(['.', '_', '-', '[', ']', '(', ')'], " ");
    let noise = normalized_all
        .split_whitespace()
        .filter(|word| NOISE_RE.is_match(word))
        .map(String::from)
        .collect();
    ParsedName {
        title: title_words.join(" ").trim().into(),
        year: year_match.map(|(_, y)| y),
        season,
        episodes,
        noise,
    }
}

pub fn parse_media_path(filename: &str, relative_path: &Path, kind: LibraryType) -> ParsedName {
    let mut parsed = parse_filename(filename, kind);
    if kind != LibraryType::TvShow {
        return parsed;
    }

    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let episode_only = EPISODE_ONLY_RE.captures(stem);
    let mut folder_title = None;
    let mut ancestor = relative_path.parent();
    while let Some(directory) = ancestor {
        let Some(parent) = directory.file_name().and_then(|value| value.to_str()) else {
            ancestor = directory.parent();
            continue;
        };
        if parsed.year.is_none() {
            parsed.year = valid_year(parent);
        }
        if parsed.season.is_none() {
            parsed.season = SEASON_RE.captures(parent).and_then(|caps| {
                caps.get(1)
                    .or_else(|| caps.get(2))
                    .and_then(|v| v.as_str().parse().ok())
            });
        }
        let cut = SEASON_RE.find(parent).map_or(parent.len(), |m| m.start());
        let candidate = clean_separators(&parent[..cut])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !candidate.is_empty() {
            folder_title = Some(candidate);
        }
        ancestor = directory.parent();
    }

    if parsed.episodes.is_empty()
        && parsed.season.is_some()
        && let Some(episode) = episode_only
            .as_ref()
            .and_then(|caps| caps.get(1))
            .and_then(|value| value.as_str().parse().ok())
    {
        parsed.episodes.push(episode);
    }
    if (parsed.title.is_empty() || episode_only.is_some())
        && let Some(title) = folder_title
    {
        parsed.title = title;
    }
    parsed
}

fn valid_year(value: &str) -> Option<i32> {
    let current_max = chrono::Utc::now().year() + 2;
    YEAR_RE
        .captures(value)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .filter(|year| *year <= current_max)
}

fn clean_separators(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect()
}

use chrono::Datelike;

pub async fn discover(
    root: PathBuf,
    sender: mpsc::Sender<DiscoveredFile>,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> std::io::Result<u64> {
    tokio::task::spawn_blocking(move || {
        let mut count = 0;
        let mut stack = vec![root.clone()];
        while let Some(directory) = stack.pop() {
            if *cancel.borrow() {
                break;
            }
            for entry in std::fs::read_dir(directory)? {
                if *cancel.borrow() {
                    break;
                }
                let entry = entry?;
                let path = entry.path();
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    if !IGNORED_DIRS.iter().any(|name| entry.file_name() == *name) {
                        stack.push(path);
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                let extension = path
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !EXTENSIONS.contains(&extension.as_str()) {
                    continue;
                }
                let metadata = entry.metadata()?;
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let modified_at: chrono::DateTime<chrono::Utc> = modified.into();
                let item = DiscoveredFile {
                    relative_path: path.strip_prefix(&root).unwrap_or(&path).to_path_buf(),
                    filename: entry.file_name().to_string_lossy().into(),
                    absolute_path: path,
                    extension,
                    size: metadata.len(),
                    modified_at: modified_at.to_rfc3339(),
                };
                if sender.blocking_send(item).is_err() {
                    return Ok(count);
                }
                count += 1;
            }
        }
        Ok(count)
    })
    .await
    .map_err(std::io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn movies() {
        let p = parse_filename("Dune.Part.Two.2024.2160p.WEB-DL.mkv", LibraryType::Movie);
        assert_eq!(p.title, "Dune Part Two");
        assert_eq!(p.year, Some(2024));
    }
    #[test]
    fn tv_patterns() {
        for (name, s, e) in [
            ("Breaking.Bad.S01E01.mkv", 1, vec![1]),
            ("Breaking.Bad.1x04.mkv", 1, vec![4]),
            ("Show.Name.S01E01E02.mkv", 1, vec![1, 2]),
            ("Show.Name.S01.E03.mkv", 1, vec![3]),
        ] {
            let p = parse_filename(name, LibraryType::TvShow);
            assert_eq!((p.season, p.episodes), (Some(s), e));
        }
    }
    #[test]
    fn real_world_series_folders_supply_year_and_unicode_separators() {
        let parsed = parse_media_path(
            "Rooster Fighter S01E01 WEB-DL 1080p x264 DUAL 2.0.mkv",
            Path::new(
                "Rooster Fighter S01 2026 WEB-DL 1080p x264 DUAL 2.0/Rooster Fighter S01E01 WEB-DL 1080p x264 DUAL 2.0.mkv",
            ),
            LibraryType::TvShow,
        );
        assert_eq!(parsed.title, "Rooster Fighter");
        assert_eq!(parsed.year, Some(2026));
        assert_eq!((parsed.season, parsed.episodes), (Some(1), vec![1]));
        let parsed = parse_filename(
            "Star Wars - Visions Apresenta — A Nona Jedi S01E08.mkv",
            LibraryType::TvShow,
        );
        assert_eq!(parsed.title, "Star Wars Visions Apresenta A Nona Jedi");
    }
    #[test]
    fn jellyfin_season_folder_supplies_series_and_episode() {
        let parsed = parse_media_path(
            "01 - Piloto.mkv",
            Path::new("Minha Serie/Season 01/01 - Piloto.mkv"),
            LibraryType::TvShow,
        );
        assert_eq!(parsed.title, "Minha Serie");
        assert_eq!((parsed.season, parsed.episodes), (Some(1), vec![1]));
    }
}
