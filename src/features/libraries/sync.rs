use std::time::Duration;

use chrono::{DateTime, Days, FixedOffset, TimeZone, Utc};
use sqlx::Row;

use crate::{
    AppState,
    catalog_api::enqueue_scan,
    db::now,
    errors::{AppError, AppResult},
};

pub fn start(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = run_startup_scans(&state).await {
            tracing::error!(%error, "unable to enqueue startup library scans");
        }
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(error) = run_due_scans(&state).await {
                tracing::error!(%error, "library synchronization scheduler iteration failed");
            }
        }
    })
}

async fn run_startup_scans(state: &AppState) -> AppResult<()> {
    let db = state.database().await;
    let rows = sqlx::query("SELECT id,created_by FROM libraries WHERE deleted_at IS NULL AND is_active=1 AND scan_enabled=1 AND scan_on_startup=1")
        .fetch_all(&db.pool)
        .await?;
    for row in rows {
        let id: String = row.get("id");
        let actor: String = row.get("created_by");
        if let Err(error) = enqueue_scan(state, &id, None, "STARTUP", &actor, true).await {
            tracing::warn!(library_id=%id, %error, "startup scan was not enqueued");
        }
    }
    Ok(())
}

async fn run_due_scans(state: &AppState) -> AppResult<()> {
    let db = state.database().await;
    let timestamp = now();
    let timezone = sqlx::query("SELECT server_timezone FROM server_config LIMIT 1")
        .fetch_optional(&db.pool)
        .await?
        .map_or_else(|| "America/Sao_Paulo".to_owned(), |value| value.get(0));
    let rows = sqlx::query("SELECT id,created_by,auto_sync_mode,auto_sync_interval_minutes,auto_sync_hour,auto_sync_minute,next_sync_at FROM libraries WHERE deleted_at IS NULL AND is_active=1 AND scan_enabled=1 AND auto_sync_enabled=1")
        .fetch_all(&db.pool)
        .await?;
    for row in rows {
        let id: String = row.get("id");
        let due = row
            .try_get::<Option<String>, _>("next_sync_at")
            .ok()
            .flatten()
            .is_none_or(|value| value <= timestamp);
        if !due {
            continue;
        }
        let mode: String = row.get("auto_sync_mode");
        let source = if mode == "SCHEDULE" {
            "AUTO_SCHEDULE"
        } else {
            "AUTO_INTERVAL"
        };
        let actor: String = row.get("created_by");
        let queued_job = match enqueue_scan(state, &id, None, source, &actor, true).await {
            Ok(job_id) => Some(job_id),
            Err(error) => {
                tracing::warn!(library_id=%id, trigger_source=source, %error, "automatic library scan was not enqueued");
                None
            }
        };
        let started_at = if let Some(job_id) = queued_job {
            sqlx::query("SELECT status FROM scan_jobs WHERE id=?")
                .bind(job_id)
                .fetch_optional(&db.pool)
                .await?
                .filter(|row| row.get::<String, _>("status") != "SKIPPED_ALREADY_RUNNING")
                .map(|_| now())
        } else {
            None
        };
        let next = calculate_next_sync(
            Utc::now(),
            &mode,
            row.get("auto_sync_interval_minutes"),
            row.get("auto_sync_hour"),
            row.get("auto_sync_minute"),
            &timezone,
        )?;
        sqlx::query("UPDATE libraries SET next_sync_at=?,last_auto_sync_at=COALESCE(?,last_auto_sync_at) WHERE id=?")
            .bind(next.to_rfc3339())
            .bind(started_at)
            .bind(&id)
            .execute(&db.pool)
            .await?;
    }
    Ok(())
}

pub fn calculate_next_sync(
    current: DateTime<Utc>,
    mode: &str,
    interval_minutes: i64,
    hour: i64,
    minute: i64,
    timezone: &str,
) -> AppResult<DateTime<Utc>> {
    if mode == "INTERVAL" {
        return Ok(current + chrono::Duration::minutes(interval_minutes));
    }
    if mode != "SCHEDULE" {
        return Err(AppError::validation(
            "INVALID_AUTO_SYNC_MODE",
            "Auto-sync mode must be INTERVAL or SCHEDULE.",
        ));
    }
    let offset = match timezone {
        "UTC" => FixedOffset::east_opt(0),
        "America/Sao_Paulo" => FixedOffset::west_opt(3 * 60 * 60),
        _ => None,
    }
    .ok_or_else(|| {
        AppError::validation(
            "INVALID_SERVER_TIMEZONE",
            "Supported timezones are America/Sao_Paulo and UTC.",
        )
    })?;
    let local_now = current.with_timezone(&offset);
    let local_date = local_now.date_naive();
    let time = chrono::NaiveTime::from_hms_opt(hour as u32, minute as u32, 0).ok_or_else(|| {
        AppError::validation(
            "INVALID_AUTO_SYNC_SCHEDULE",
            "Schedule hour or minute is invalid.",
        )
    })?;
    let mut candidate = offset
        .from_local_datetime(&local_date.and_time(time))
        .single()
        .ok_or_else(|| {
            AppError::validation("INVALID_AUTO_SYNC_SCHEDULE", "Schedule time is invalid.")
        })?;
    if candidate <= local_now {
        let tomorrow = local_date
            .checked_add_days(Days::new(1))
            .ok_or_else(|| AppError::config("Unable to calculate the next synchronization."))?;
        candidate = offset
            .from_local_datetime(&tomorrow.and_time(time))
            .single()
            .ok_or_else(|| AppError::config("Unable to calculate the next synchronization."))?;
    }
    Ok(candidate.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Timelike};

    use super::*;

    #[test]
    fn interval_uses_requested_minutes() {
        let now = Utc.with_ymd_and_hms(2026, 8, 30, 12, 0, 0).unwrap();
        assert_eq!(
            calculate_next_sync(now, "INTERVAL", 45, 3, 0, "UTC").unwrap(),
            now + chrono::Duration::minutes(45)
        );
    }

    #[test]
    fn daily_schedule_rolls_to_next_local_day() {
        let now = Utc.with_ymd_and_hms(2026, 8, 30, 7, 0, 0).unwrap();
        let next = calculate_next_sync(now, "SCHEDULE", 60, 3, 30, "America/Sao_Paulo").unwrap();
        assert!(next > now);
        assert_eq!(
            next.with_timezone(&FixedOffset::west_opt(10800).unwrap())
                .hour(),
            3
        );
    }
}
