//! Shared identification pipeline: turns parsed media names into catalog
//! entries. Consumed by the local scanner (`catalog::api`) and by remote source
//! ingestion (`features::remote_sources`) so both paths share TMDB dedupe,
//! confidence scoring and persistence.

use tokio::sync::watch;

use crate::{
    db::{Database, now},
    errors::AppResult,
    libraries::LibraryType,
    metadata::{MetadataProvider, TmdbMetadataProvider},
    scanner::ParsedName,
};

use super::api::{
    associate_file, best_search_candidate, cached_details, cached_season, existing_series_match,
    persist_metadata, persist_tv_season,
};

/// Identifies `media_files` rows that are already persisted with
/// `identification_status='PENDING'`. Each tuple is `(media_file_id, parsed)`.
///
/// Returns `(matched, unmatched, failed)`. Series are deduped via
/// [`existing_series_match`]; movies via the `media_items` unique key and the
/// cached TMDB search, so a batch of episodes from one show issues a single
/// search.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn identify_pending(
    db: &Database,
    provider: &TmdbMetadataProvider,
    library_id: &str,
    kind: LibraryType,
    language: &str,
    region: Option<&str>,
    changed: &[(String, ParsedName)],
    cancel: &watch::Receiver<bool>,
) -> AppResult<(i64, i64, i64)> {
    let mut matched = 0;
    let mut unmatched = 0;
    let mut failed = 0;
    for (file_id, parsed) in changed {
        if *cancel.borrow() {
            break;
        }
        if !provider.configured() {
            sqlx::query(
                "UPDATE media_files SET identification_status='UNMATCHED',updated_at=? WHERE id=?",
            )
            .bind(now())
            .bind(file_id)
            .execute(&db.pool)
            .await?;
            unmatched += 1;
            continue;
        }
        if kind == LibraryType::TvShow
            && let Some(item_id) =
                existing_series_match(db, library_id, &parsed.title, file_id).await?
        {
            associate_file(db, file_id, &item_id, "MATCHED_AUTO", kind, parsed).await?;
            matched += 1;
            continue;
        }
        match best_search_candidate(
            db,
            provider,
            kind,
            &parsed.title,
            parsed.year,
            language,
            region,
        )
        .await
        {
            Ok(Some((score, candidate))) if score >= 0.90 => {
                match cached_details(db, provider, kind, candidate.provider_id, language).await {
                    Ok(details) => {
                        match persist_metadata(db, library_id, kind, language, &details).await {
                            Ok(item_id) => {
                                if kind == LibraryType::TvShow
                                    && let Some(season) = parsed.season
                                {
                                    let season_details = cached_season(
                                        db,
                                        provider,
                                        candidate.provider_id,
                                        season,
                                        language,
                                    )
                                    .await?;
                                    persist_tv_season(db, &item_id, &season_details).await?;
                                }
                                associate_file(db, file_id, &item_id, "MATCHED_AUTO", kind, parsed)
                                    .await?;
                                matched += 1;
                            }
                            Err(_) => failed += 1,
                        }
                    }
                    Err(_) => failed += 1,
                }
            }
            Ok(Some(_)) => {
                sqlx::query("UPDATE media_files SET identification_status='AMBIGUOUS',updated_at=? WHERE id=?")
                    .bind(now())
                    .bind(file_id)
                    .execute(&db.pool)
                    .await?;
                unmatched += 1;
            }
            Ok(None) => {
                sqlx::query("UPDATE media_files SET identification_status='UNMATCHED',updated_at=? WHERE id=?")
                    .bind(now())
                    .bind(file_id)
                    .execute(&db.pool)
                    .await?;
                unmatched += 1;
            }
            Err(_) => {
                sqlx::query(
                    "UPDATE media_files SET identification_status='ERROR',updated_at=? WHERE id=?",
                )
                .bind(now())
                .bind(file_id)
                .execute(&db.pool)
                .await?;
                failed += 1;
            }
        }
    }
    Ok((matched, unmatched, failed))
}
