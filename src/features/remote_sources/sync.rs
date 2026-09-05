//! M3U synchronization: fetch and stream-parse the playlist, diff against the
//! known `m3u_entries`, and catalog the newly selected records through the
//! shared resolver. Runs incrementally — only `NEW`/`UPDATED` selected entries
//! reach TMDB.

use std::{panic::AssertUnwindSafe, time::Instant};

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use uuid::Uuid;

use futures_util::{FutureExt, TryStreamExt};

use crate::{
    app::AppState,
    db::{Database, now},
    errors::{AppError, AppResult},
    features::catalog::{api::refresh_library_stats, resolver::identify_pending},
    infrastructure::secrets,
    libraries::LibraryType,
    scanner::ParsedName,
};

use super::{
    api::m3u_dir,
    m3u::{M3uParser, RawEntry, read_error},
    models::{EntryMediaType, ProviderType},
    normalize::AnalyzedEntry,
    sanitize_url,
};

const BATCH: usize = 250;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub scanned: u64,
    pub new: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub missing: u64,
    pub matched: u64,
    pub unmatched: u64,
    pub not_modified: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Clone)]
struct Selection {
    media_type: String,
    category: Option<String>,
    subcategory: Option<String>,
}

fn entry_kind(media_type: EntryMediaType) -> Option<LibraryType> {
    match media_type {
        EntryMediaType::Movie => Some(LibraryType::Movie),
        EntryMediaType::TvShow => Some(LibraryType::TvShow),
        EntryMediaType::Unknown => None,
    }
}

fn is_selected(selections: &[Selection], analyzed: &AnalyzedEntry) -> bool {
    let media_type = analyzed.media_type.as_str();
    selections.iter().any(|selection| {
        (selection.media_type == media_type || selection.media_type == "ALL")
            && selection
                .category
                .as_deref()
                .is_none_or(|category| category == analyzed.category)
            && selection
                .subcategory
                .as_deref()
                .is_none_or(|subcategory| Some(subcategory) == analyzed.subcategory.as_deref())
    })
}

/// Public entry point: guards against concurrent syncs and records the terminal
/// status/error and the next scheduled run.
pub async fn run_sync(state: &AppState, source_id: &str, trigger: &str) -> AppResult<SyncOutcome> {
    {
        let mut running = state.syncing_sources.lock().await;
        if !running.insert(source_id.to_string()) {
            return Err(AppError::conflict(
                "REMOTE_SYNC_ALREADY_RUNNING",
                "A synchronization is already running for this source.",
            ));
        }
    }
    // Catch panics so the in-progress guard is always released; otherwise a
    // single bad playlist would wedge the source in SYNCING forever.
    let result = AssertUnwindSafe(run_sync_inner(state, source_id, trigger))
        .catch_unwind()
        .await
        .unwrap_or_else(|_| {
            Err(AppError::config(
                "A sincronização foi interrompida por um erro interno.",
            ))
        });
    state.syncing_sources.lock().await.remove(source_id);

    let db = state.database().await;
    let timestamp = now();
    match &result {
        Ok(outcome) => {
            let next_sync_at = next_sync_at(&db, source_id).await.unwrap_or(None);
            let _ = sqlx::query("UPDATE remote_sources SET status='READY',last_sync_at=?,last_successful_sync_at=?,next_sync_at=?,last_error=NULL,last_error_at=NULL,updated_at=? WHERE id=?")
                .bind(&timestamp)
                .bind(&timestamp)
                .bind(&next_sync_at)
                .bind(&timestamp)
                .bind(source_id)
                .execute(&db.pool)
                .await;
            let _ = db
                .audit(
                    None,
                    "REMOTE_SOURCE_SYNCED",
                    "remote_source",
                    Some(source_id),
                    json!({
                        "trigger": trigger,
                        "scanned": outcome.scanned,
                        "new": outcome.new,
                        "updated": outcome.updated,
                        "missing": outcome.missing,
                        "matched": outcome.matched,
                        "unmatched": outcome.unmatched,
                        "notModified": outcome.not_modified,
                        "durationMs": outcome.duration_ms,
                    }),
                    None,
                )
                .await;
        }
        Err(error) => {
            let status = match error.code {
                "REMOTE_SYNC_ALREADY_RUNNING" => return result,
                "M3U_AUTH_REQUIRED" => "AUTH_REQUIRED",
                "M3U_FETCH_FAILED" | "M3U_READ_FAILED" | "PLAYLIST_UNAVAILABLE" => "UNAVAILABLE",
                _ => "ERROR",
            };
            let _ = sqlx::query("UPDATE remote_sources SET status=?,last_sync_at=?,last_error=?,last_error_at=?,updated_at=? WHERE id=?")
                .bind(status)
                .bind(&timestamp)
                .bind(error.message.clone())
                .bind(&timestamp)
                .bind(&timestamp)
                .bind(source_id)
                .execute(&db.pool)
                .await;
            let _ = db
                .audit(
                    None,
                    "REMOTE_SOURCE_SYNC_FAILED",
                    "remote_source",
                    Some(source_id),
                    json!({ "trigger": trigger, "status": status, "code": error.code }),
                    None,
                )
                .await;
        }
    }
    result
}

async fn next_sync_at(db: &Database, source_id: &str) -> AppResult<Option<String>> {
    let row = sqlx::query(
        "SELECT auto_sync_enabled,auto_sync_interval_minutes FROM remote_sources WHERE id=?",
    )
    .bind(source_id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.and_then(|row| {
        (row.get::<i64, _>("auto_sync_enabled") != 0).then(|| {
            (chrono::Utc::now()
                + chrono::Duration::minutes(row.get::<i64, _>("auto_sync_interval_minutes")))
            .to_rfc3339()
        })
    }))
}

enum Playlist {
    NotModified,
    Stream {
        reader: Box<dyn AsyncBufRead + Unpin + Send>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

async fn open_playlist(
    state: &AppState,
    db: &Database,
    source_id: &str,
    provider: ProviderType,
    config: &Value,
) -> AppResult<Playlist> {
    match provider {
        ProviderType::M3uUrl => {
            let secret =
                sqlx::query("SELECT secret_ref FROM remote_source_secrets WHERE source_id=?")
                    .bind(source_id)
                    .fetch_optional(&db.pool)
                    .await?
                    .map(|row| row.get::<String, _>("secret_ref"))
                    .ok_or_else(|| {
                        AppError::config("The M3U source is missing its stored credentials.")
                    })?;
            let sealed = secrets::open(&state.config, &secret)?;
            let url = serde_json::from_str::<Value>(&sealed)
                .ok()
                .and_then(|value| value["url"].as_str().map(str::to_owned))
                .ok_or_else(|| AppError::config("Stored M3U credentials are invalid."))?;
            let _permit = state.remote_http_slots.acquire().await.ok();
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(
                    state.config.m3u_fetch_timeout_seconds.max(1) * 6,
                ))
                .user_agent(concat!("MyLib/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| AppError::config("Unable to initialize HTTP client."))?;
            let mut request = client.get(&url);
            if let Some(etag) = config["etag"].as_str() {
                request = request.header(reqwest::header::IF_NONE_MATCH, etag);
            }
            if let Some(modified) = config["lastModified"].as_str() {
                request = request.header(reqwest::header::IF_MODIFIED_SINCE, modified);
            }
            let response = request.send().await.map_err(|error| {
                tracing::warn!(url = %sanitize_url(&url), %error, "M3U fetch failed");
                AppError::new(
                    axum::http::StatusCode::BAD_GATEWAY,
                    "M3U_FETCH_FAILED",
                    "The playlist URL could not be reached.",
                )
            })?;
            if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                return Ok(Playlist::NotModified);
            }
            if response.status() == reqwest::StatusCode::UNAUTHORIZED
                || response.status() == reqwest::StatusCode::FORBIDDEN
            {
                return Err(AppError::new(
                    axum::http::StatusCode::BAD_GATEWAY,
                    "M3U_AUTH_REQUIRED",
                    "The playlist URL rejected the stored credentials.",
                ));
            }
            if !response.status().is_success() {
                return Err(AppError::new(
                    axum::http::StatusCode::BAD_GATEWAY,
                    "M3U_FETCH_FAILED",
                    "The playlist URL returned an error.",
                ));
            }
            let etag = header_value(&response, reqwest::header::ETAG);
            let last_modified = header_value(&response, reqwest::header::LAST_MODIFIED);
            let stream = response.bytes_stream().map_err(std::io::Error::other);
            Ok(Playlist::Stream {
                reader: Box::new(BufReader::new(tokio_util::io::StreamReader::new(stream))),
                etag,
                last_modified,
            })
        }
        ProviderType::M3uFile => {
            let file_name = config["storedFile"]
                .as_str()
                .ok_or_else(|| AppError::config("The uploaded playlist is missing."))?;
            let path = m3u_dir(&state.config).join(file_name);
            let file = tokio::fs::File::open(&path).await.map_err(|_| {
                AppError::new(
                    axum::http::StatusCode::BAD_GATEWAY,
                    "PLAYLIST_UNAVAILABLE",
                    "The stored playlist file is unavailable.",
                )
            })?;
            Ok(Playlist::Stream {
                reader: Box::new(BufReader::new(file)),
                etag: None,
                last_modified: None,
            })
        }
        ProviderType::GoogleDrive => Err(AppError::validation(
            "UNSUPPORTED_PROVIDER",
            "This source type does not use M3U synchronization.",
        )),
    }
}

fn header_value(response: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

async fn load_selections(db: &Database, source_id: &str) -> AppResult<Vec<Selection>> {
    let rows = sqlx::query("SELECT media_type,category,subcategory FROM m3u_source_selections WHERE source_id=? AND is_enabled=1")
        .bind(source_id)
        .fetch_all(&db.pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| Selection {
            media_type: row.get("media_type"),
            category: row.try_get::<Option<String>, _>("category").ok().flatten(),
            subcategory: row
                .try_get::<Option<String>, _>("subcategory")
                .ok()
                .flatten(),
        })
        .collect())
}

pub(crate) async fn ensure_remote_path(
    db: &Database,
    library_id: &str,
    source_id: &str,
) -> AppResult<String> {
    let normalized = format!("remote://{source_id}");
    if let Some(row) =
        sqlx::query("SELECT id FROM library_paths WHERE library_id=? AND normalized_path=?")
            .bind(library_id)
            .bind(&normalized)
            .fetch_optional(&db.pool)
            .await?
    {
        return Ok(row.get(0));
    }
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    // is_active=0 keeps the local scanner and library statistics from touching
    // this synthetic path.
    sqlx::query("INSERT INTO library_paths (id,library_id,path,normalized_path,is_active,status,created_at,updated_at) VALUES (?,?,?,?,0,'REMOTE',?,?)")
        .bind(&id)
        .bind(library_id)
        .bind(&normalized)
        .bind(&normalized)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&db.pool)
        .await?;
    Ok(id)
}

struct SyncContext<'a> {
    db: &'a Database,
    state: &'a AppState,
    source_id: &'a str,
    library_id: &'a str,
    path_id: &'a str,
    kind: LibraryType,
    language: &'a str,
    region: Option<&'a str>,
    provider_type: ProviderType,
    selections: &'a [Selection],
}

async fn run_sync_inner(
    state: &AppState,
    source_id: &str,
    _trigger: &str,
) -> AppResult<SyncOutcome> {
    let started = Instant::now();
    let db = state.database().await;
    let source = super::api::source_row(&db, source_id).await?;
    let provider_type = ProviderType::parse(&source.get::<String, _>("provider_type"))?;
    if provider_type == ProviderType::GoogleDrive {
        return super::google_drive::sync_inner(state, source_id).await;
    }
    let library_id: String = source.get("library_id");
    let config: Value =
        serde_json::from_str(&source.get::<String, _>("config")).unwrap_or(json!({}));

    let library = sqlx::query(
        "SELECT library_type,metadata_language,metadata_region FROM libraries WHERE id=? AND deleted_at IS NULL",
    )
    .bind(&library_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("LIBRARY_NOT_FOUND", "Library was not found."))?;
    let kind = LibraryType::parse(&library.get::<String, _>("library_type"))?;
    let language: String = library.get("metadata_language");
    let region: Option<String> = library.try_get("metadata_region").ok();

    sqlx::query(
        "UPDATE remote_sources SET status='SYNCING',last_sync_at=?,updated_at=? WHERE id=?",
    )
    .bind(now())
    .bind(now())
    .bind(source_id)
    .execute(&db.pool)
    .await?;

    let playlist = open_playlist(state, &db, source_id, provider_type, &config).await?;
    let (reader, etag, last_modified) = match playlist {
        Playlist::NotModified => {
            return Ok(SyncOutcome {
                not_modified: true,
                duration_ms: started.elapsed().as_millis(),
                ..SyncOutcome::default()
            });
        }
        Playlist::Stream {
            reader,
            etag,
            last_modified,
        } => (reader, etag, last_modified),
    };

    let selections = load_selections(&db, source_id).await?;
    let path_id = ensure_remote_path(&db, &library_id, source_id).await?;
    let started_at = now();
    let context = SyncContext {
        db: &db,
        state,
        source_id,
        library_id: &library_id,
        path_id: &path_id,
        kind,
        language: &language,
        region: region.as_deref(),
        provider_type,
        selections: &selections,
    };

    let mut parser = M3uParser::new();
    let mut lines = reader.lines();
    let mut consumed = 0_u64;
    let mut outcome = SyncOutcome::default();
    let mut batch: Vec<RawEntry> = Vec::with_capacity(BATCH);

    while let Some(line) = lines.next_line().await.map_err(read_error)? {
        consumed = consumed.saturating_add(line.len() as u64 + 1);
        if consumed > state.config.m3u_max_bytes {
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
            batch.push(entry);
            if batch.len() >= BATCH {
                process_batch(&context, std::mem::take(&mut batch), &mut outcome).await?;
            }
        }
    }
    if !batch.is_empty() {
        process_batch(&context, batch, &mut outcome).await?;
    }

    // Missing pass: entries not seen this run keep their catalog rows but are
    // flagged so playback and the UI can react without deleting anything.
    let missing = sqlx::query("UPDATE m3u_entries SET sync_status='MISSING',missing_since=COALESCE(missing_since,?),updated_at=? WHERE source_id=? AND last_seen_at<? AND sync_status<>'MISSING'")
        .bind(&started_at)
        .bind(now())
        .bind(source_id)
        .bind(&started_at)
        .execute(&db.pool)
        .await?
        .rows_affected();
    outcome.missing = missing;
    sqlx::query("UPDATE media_files SET scan_status='MISSING',missing_since=COALESCE(missing_since,?),updated_at=? WHERE remote_media_source_id IN (SELECT id FROM remote_media_sources WHERE remote_source_id=? AND last_seen_at<?)")
        .bind(&started_at)
        .bind(now())
        .bind(source_id)
        .bind(&started_at)
        .execute(&db.pool)
        .await?;
    sqlx::query("UPDATE remote_media_sources SET is_active=0,updated_at=? WHERE remote_source_id=? AND last_seen_at<?")
        .bind(now())
        .bind(source_id)
        .bind(&started_at)
        .execute(&db.pool)
        .await?;

    if provider_type == ProviderType::M3uUrl && (etag.is_some() || last_modified.is_some()) {
        let mut stored = config.clone();
        if let Some(object) = stored.as_object_mut() {
            object.insert("etag".into(), etag.map(Value::from).unwrap_or(Value::Null));
            object.insert(
                "lastModified".into(),
                last_modified.map(Value::from).unwrap_or(Value::Null),
            );
        }
        sqlx::query("UPDATE remote_sources SET config=?,updated_at=? WHERE id=?")
            .bind(stored.to_string())
            .bind(now())
            .bind(source_id)
            .execute(&db.pool)
            .await?;
    }

    refresh_library_stats(&db, &library_id).await?;
    state.recommendations.invalidate_all().await;
    outcome.duration_ms = started.elapsed().as_millis();
    Ok(outcome)
}

async fn process_batch(
    context: &SyncContext<'_>,
    batch: Vec<RawEntry>,
    outcome: &mut SyncOutcome,
) -> AppResult<()> {
    let db = context.db;
    let mut pending: Vec<(String, ParsedName)> = Vec::new();

    for entry in batch {
        outcome.scanned += 1;
        let external_key = entry.external_key();
        let entry_hash = entry.entry_hash();
        let analyzed = entry.analyze();
        let selected = is_selected(context.selections, &analyzed);
        let catalogable = selected && entry_kind(analyzed.media_type) == Some(context.kind);
        let timestamp = now();
        let sealed_stream = secrets::seal(&context.state.config, &entry.stream_url)?;

        let existing = sqlx::query(
            "SELECT id,entry_hash FROM m3u_entries WHERE source_id=? AND external_key=?",
        )
        .bind(context.source_id)
        .bind(&external_key)
        .fetch_optional(&db.pool)
        .await?;

        let (entry_id, changed) = match existing {
            Some(row) => {
                let changed = row.get::<String, _>("entry_hash") != entry_hash;
                let status = if changed { "UPDATED" } else { "UNCHANGED" };
                sqlx::query("UPDATE m3u_entries SET raw_name=?,clean_title=?,year=?,media_type=?,category=?,subcategory=?,season_number=?,episode_number=?,tvg_logo=?,stream_ref=?,stream_sealed=1,is_selected=?,entry_hash=?,sync_status=?,missing_since=NULL,last_seen_at=?,updated_at=? WHERE id=?")
                    .bind(entry.best_name())
                    .bind(&analyzed.clean_title)
                    .bind(analyzed.year)
                    .bind(analyzed.media_type.as_str())
                    .bind(nullable(&analyzed.category))
                    .bind(&analyzed.subcategory)
                    .bind(analyzed.season)
                    .bind(analyzed.episode)
                    .bind(&entry.tvg_logo)
                    .bind(&sealed_stream)
                    .bind(i64::from(selected))
                    .bind(&entry_hash)
                    .bind(status)
                    .bind(&timestamp)
                    .bind(&timestamp)
                    .bind(row.get::<String, _>("id"))
                    .execute(&db.pool)
                    .await?;
                if changed {
                    outcome.updated += 1;
                } else {
                    outcome.unchanged += 1;
                }
                (row.get::<String, _>("id"), changed)
            }
            None => {
                let id = Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO m3u_entries (id,source_id,external_key,entry_hash,raw_name,clean_title,year,media_type,category,subcategory,season_number,episode_number,tvg_logo,stream_ref,stream_sealed,is_selected,sync_status,last_seen_at,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,1,?,'NEW',?,?,?)")
                    .bind(&id)
                    .bind(context.source_id)
                    .bind(&external_key)
                    .bind(&entry_hash)
                    .bind(entry.best_name())
                    .bind(&analyzed.clean_title)
                    .bind(analyzed.year)
                    .bind(analyzed.media_type.as_str())
                    .bind(nullable(&analyzed.category))
                    .bind(&analyzed.subcategory)
                    .bind(analyzed.season)
                    .bind(analyzed.episode)
                    .bind(&entry.tvg_logo)
                    .bind(&sealed_stream)
                    .bind(i64::from(selected))
                    .bind(&timestamp)
                    .bind(&timestamp)
                    .bind(&timestamp)
                    .execute(&db.pool)
                    .await?;
                outcome.new += 1;
                (id, true)
            }
        };

        if !catalogable {
            continue;
        }

        let parsed = ParsedName {
            title: analyzed.clean_title.clone(),
            year: analyzed.year,
            season: analyzed.season,
            episodes: analyzed.episode.into_iter().collect(),
            noise: vec![],
        };
        let file_id = upsert_remote_media(
            context,
            &external_key,
            &sealed_stream,
            &entry,
            &parsed,
            &timestamp,
        )
        .await?;
        // Link the entry to its media file for the admin detail view.
        sqlx::query("UPDATE m3u_entries SET media_file_id=? WHERE id=?")
            .bind(&file_id)
            .bind(&entry_id)
            .execute(&db.pool)
            .await?;
        if changed {
            pending.push((file_id, parsed));
        }
    }

    if !pending.is_empty() {
        let provider = crate::features::catalog::api::provider(context.state)?;
        let (_never_cancel_tx, cancel) = tokio::sync::watch::channel(false);
        let (matched, unmatched, _failed) = identify_pending(
            db,
            &provider,
            context.library_id,
            context.kind,
            context.language,
            context.region,
            &pending,
            &cancel,
        )
        .await?;
        outcome.matched += matched as u64;
        outcome.unmatched += unmatched as u64;
        // Propagate the identification result onto the remote media source rows.
        sqlx::query("UPDATE remote_media_sources SET media_item_id=(SELECT media_item_id FROM media_files WHERE media_files.remote_media_source_id=remote_media_sources.id),episode_id=(SELECT tv_episode_id FROM media_files WHERE media_files.remote_media_source_id=remote_media_sources.id),updated_at=? WHERE remote_source_id=?")
            .bind(now())
            .bind(context.source_id)
            .execute(&db.pool)
            .await?;
    }
    Ok(())
}

/// Creates or refreshes the `remote_media_sources` + `media_files` pair for one
/// selected entry. Returns the media file id.
async fn upsert_remote_media(
    context: &SyncContext<'_>,
    external_key: &str,
    sealed_stream: &str,
    entry: &RawEntry,
    parsed: &ParsedName,
    timestamp: &str,
) -> AppResult<String> {
    let db = context.db;
    let quality = quality_hint(&entry.stream_url);
    let rms_id = match sqlx::query(
        "SELECT id FROM remote_media_sources WHERE remote_source_id=? AND external_key=?",
    )
    .bind(context.source_id)
    .bind(external_key)
    .fetch_optional(&db.pool)
    .await?
    {
        Some(row) => {
            let id: String = row.get("id");
            sqlx::query("UPDATE remote_media_sources SET stream_ref=?,stream_sealed=1,quality_hint=?,is_active=1,last_seen_at=?,updated_at=? WHERE id=?")
                .bind(sealed_stream)
                .bind(&quality)
                .bind(timestamp)
                .bind(timestamp)
                .bind(&id)
                .execute(&db.pool)
                .await?;
            id
        }
        None => {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO remote_media_sources (id,remote_source_id,provider_type,external_key,stream_ref,stream_sealed,quality_hint,is_active,last_seen_at,created_at,updated_at) VALUES (?,?,?,?,?,1,?,1,?,?,?)")
                .bind(&id)
                .bind(context.source_id)
                .bind(context.provider_type.as_str())
                .bind(external_key)
                .bind(sealed_stream)
                .bind(&quality)
                .bind(timestamp)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&db.pool)
                .await?;
            id
        }
    };

    let absolute_path = format!("remote://{}/{}", context.source_id, external_key);
    let extension = stream_extension(&entry.stream_url);
    let existing = sqlx::query(
        "SELECT id,identification_status FROM media_files WHERE library_id=? AND absolute_path=?",
    )
    .bind(context.library_id)
    .bind(&absolute_path)
    .fetch_optional(&db.pool)
    .await?;
    match existing {
        Some(row) => {
            let id: String = row.get("id");
            let status: String = row.get("identification_status");
            // Re-queue identification only when it has not succeeded before.
            let next_status = if matches!(status.as_str(), "MATCHED_AUTO" | "MATCHED_MANUAL") {
                status
            } else {
                "PENDING".to_string()
            };
            sqlx::query("UPDATE media_files SET library_path_id=?,filename=?,extension=?,content_type=?,scan_status='PRESENT',identification_status=?,normalized_title=?,parsed_year=?,parsed_season=?,parsed_episode=?,storage_kind='REMOTE',remote_media_source_id=?,missing_since=NULL,last_seen_at=?,updated_at=? WHERE id=?")
                .bind(context.path_id)
                .bind(entry.best_name())
                .bind(&extension)
                .bind(context.kind.as_str())
                .bind(&next_status)
                .bind(&parsed.title)
                .bind(parsed.year)
                .bind(parsed.season)
                .bind(parsed.episodes.first().copied())
                .bind(&rms_id)
                .bind(timestamp)
                .bind(timestamp)
                .bind(&id)
                .execute(&db.pool)
                .await?;
            Ok(id)
        }
        None => {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO media_files (id,library_id,library_path_id,absolute_path,relative_path,filename,extension,file_size,modified_at,content_type,scan_status,identification_status,normalized_title,parsed_year,parsed_season,parsed_episode,storage_kind,remote_media_source_id,created_at,updated_at,last_seen_at) VALUES (?,?,?,?,?,?,?,0,?,?,'PRESENT','PENDING',?,?,?,?,'REMOTE',?,?,?,?)")
                .bind(&id)
                .bind(context.library_id)
                .bind(context.path_id)
                .bind(&absolute_path)
                .bind(external_key)
                .bind(entry.best_name())
                .bind(&extension)
                .bind(timestamp)
                .bind(context.kind.as_str())
                .bind(&parsed.title)
                .bind(parsed.year)
                .bind(parsed.season)
                .bind(parsed.episodes.first().copied())
                .bind(&rms_id)
                .bind(timestamp)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&db.pool)
                .await?;
            Ok(id)
        }
    }
}

fn nullable(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn stream_extension(url: &str) -> String {
    sanitize_url(url)
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_default()
}

fn quality_hint(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    for hint in ["2160p", "4k", "1080p", "720p", "480p", "hdr"] {
        if lower.contains(hint) {
            return Some(hint.to_ascii_uppercase());
        }
    }
    None
}
