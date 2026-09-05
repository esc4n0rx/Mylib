//! Background scheduler that drives due remote-source synchronizations. Mirrors
//! `features::libraries::sync` for local libraries.

use std::time::Duration;

use sqlx::Row;

use crate::{app::AppState, db::now, errors::AppResult};

use super::sync::run_sync;

pub fn start(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let pool = state.database().await.pool;
        // A SYNCING status left in the database belongs to a previous process;
        // the in-memory guard is authoritative, so clear stale rows on boot.
        let _ = sqlx::query("UPDATE remote_sources SET status='READY' WHERE status='SYNCING'")
            .execute(&pool)
            .await;
        // Reclaim catalog rows orphaned by a source deleted before the cleanup
        // in `delete_source` existed (or by a failed transaction).
        let _ = sqlx::query("DELETE FROM media_files WHERE storage_kind='REMOTE' AND remote_media_source_id IS NOT NULL AND remote_media_source_id NOT IN (SELECT id FROM remote_media_sources)")
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM library_paths WHERE status='REMOTE' AND NOT EXISTS (SELECT 1 FROM media_files WHERE media_files.library_path_id=library_paths.id)")
            .execute(&pool)
            .await;
        // Catalog items left with no file after the orphan sweep. Scans recreate
        // any that a source still provides (idempotent upsert on the tmdb key).
        let _ = sqlx::query("DELETE FROM media_items WHERE NOT EXISTS (SELECT 1 FROM media_files WHERE media_files.media_item_id=media_items.id)")
            .execute(&pool)
            .await;
        let period = Duration::from_secs(state.config.remote_sync_interval_seconds.max(15));
        let mut interval = tokio::time::interval(period);
        loop {
            interval.tick().await;
            if let Err(error) = run_due(&state).await {
                tracing::warn!(%error, "remote source scheduler iteration failed");
            }
        }
    })
}

async fn run_due(state: &AppState) -> AppResult<()> {
    let db = state.database().await;
    let timestamp = now();
    let rows = sqlx::query(
        "SELECT id,auto_sync_interval_minutes FROM remote_sources WHERE is_active=1 AND auto_sync_enabled=1 AND (next_sync_at IS NULL OR next_sync_at<=?)",
    )
    .bind(&timestamp)
    .fetch_all(&db.pool)
    .await?;
    for row in rows {
        let id: String = row.get("id");
        let interval_minutes: i64 = row.get("auto_sync_interval_minutes");
        // Claim the slot before spawning so a failing sync backs off to its
        // interval instead of retrying every tick. A successful run overwrites
        // this with its own freshly computed schedule.
        let next =
            (chrono::Utc::now() + chrono::Duration::minutes(interval_minutes.max(5))).to_rfc3339();
        sqlx::query("UPDATE remote_sources SET next_sync_at=? WHERE id=?")
            .bind(&next)
            .bind(&id)
            .execute(&db.pool)
            .await?;
        let worker = state.clone();
        tokio::spawn(async move {
            if let Err(error) = run_sync(&worker, &id, "AUTO_INTERVAL").await
                && error.code != "REMOTE_SYNC_ALREADY_RUNNING"
            {
                tracing::warn!(source_id = %id, code = error.code, "scheduled remote sync failed");
            }
        });
    }
    Ok(())
}
