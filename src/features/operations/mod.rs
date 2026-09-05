use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sysinfo::{Disks, ProcessesToUpdate, System, get_current_pid};

use crate::{
    app::AppState,
    auth::AuthUser,
    config::Config,
    errors::{AppError, AppResult},
};

const METRICS_TTL: Duration = Duration::from_secs(3);
const STORAGE_TTL: Duration = Duration::from_secs(30);
const HISTORY_CAPACITY: usize = 180;

#[derive(Clone, Debug)]
struct ResourceSnapshot {
    captured_at: String,
    cpu_usage_percent: f32,
    memory_used_bytes: u64,
    memory_total_bytes: u64,
    process_memory_bytes: u64,
    disk_total_bytes: u64,
    disk_used_bytes: u64,
    disk_free_bytes: u64,
    disk_path: String,
}

#[derive(Clone, Debug, Default)]
struct DirectorySizes {
    data: u64,
    database: u64,
    logs: u64,
    transcode_cache: u64,
}

#[derive(Clone)]
pub struct SystemMetricsService {
    system: Arc<tokio::sync::Mutex<System>>,
    cached: Arc<tokio::sync::RwLock<Option<(Instant, ResourceSnapshot)>>>,
    sizes: Arc<tokio::sync::RwLock<Option<(Instant, DirectorySizes)>>>,
    history: Arc<tokio::sync::Mutex<VecDeque<Value>>>,
    data_dir: PathBuf,
}

impl SystemMetricsService {
    pub fn new(config: &Config) -> Self {
        Self {
            system: Arc::new(tokio::sync::Mutex::new(System::new_all())),
            cached: Arc::new(tokio::sync::RwLock::new(None)),
            sizes: Arc::new(tokio::sync::RwLock::new(None)),
            history: Arc::new(tokio::sync::Mutex::new(VecDeque::with_capacity(
                HISTORY_CAPACITY,
            ))),
            data_dir: config.data_dir.clone(),
        }
    }

    async fn resources(&self) -> ResourceSnapshot {
        if let Some((captured, snapshot)) = self.cached.read().await.as_ref()
            && captured.elapsed() < METRICS_TTL
        {
            return snapshot.clone();
        }
        let mut system = self.system.lock().await;
        system.refresh_cpu_usage();
        system.refresh_memory();
        if let Ok(pid) = get_current_pid() {
            system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        }
        let process_memory = get_current_pid()
            .ok()
            .and_then(|pid| system.process(pid))
            .map(|process| process.memory())
            .unwrap_or(0);
        let (disk_total, disk_free, disk_path) = disk_for(&self.data_dir);
        let snapshot = ResourceSnapshot {
            captured_at: chrono::Utc::now().to_rfc3339(),
            cpu_usage_percent: system.global_cpu_usage(),
            memory_used_bytes: system.used_memory(),
            memory_total_bytes: system.total_memory(),
            process_memory_bytes: process_memory,
            disk_total_bytes: disk_total,
            disk_used_bytes: disk_total.saturating_sub(disk_free),
            disk_free_bytes: disk_free,
            disk_path,
        };
        *self.cached.write().await = Some((Instant::now(), snapshot.clone()));
        snapshot
    }

    async fn directory_sizes(&self) -> DirectorySizes {
        if let Some((captured, sizes)) = self.sizes.read().await.as_ref()
            && captured.elapsed() < STORAGE_TTL
        {
            return sizes.clone();
        }
        let root = self.data_dir.clone();
        let sizes = tokio::task::spawn_blocking(move || {
            let database = fs::metadata(root.join("mylib.db"))
                .map(|m| m.len())
                .unwrap_or(0);
            let logs = directory_size(&root.join("logs"));
            let transcode_cache = directory_size(&root.join("cache/transcode"));
            DirectorySizes {
                data: directory_size(&root),
                database,
                logs,
                transcode_cache,
            }
        })
        .await
        .unwrap_or_default();
        *self.sizes.write().await = Some((Instant::now(), sizes.clone()));
        sizes
    }

    async fn record(&self, snapshot: &ResourceSnapshot, streams: i64) -> Vec<Value> {
        let mut history = self.history.lock().await;
        let should_push = history.back().and_then(|v| v["capturedAt"].as_str())
            != Some(snapshot.captured_at.as_str());
        if should_push {
            if history.len() == HISTORY_CAPACITY {
                history.pop_front();
            }
            history.push_back(json!({
                "capturedAt": snapshot.captured_at,
                "cpuUsagePercent": snapshot.cpu_usage_percent,
                "memoryUsagePercent": percent(snapshot.memory_used_bytes, snapshot.memory_total_bytes),
                "activePlaybackSessions": streams,
            }));
        }
        history.iter().cloned().collect()
    }
}

fn directory_size(root: &Path) -> u64 {
    let Ok(metadata) = fs::metadata(root) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

fn disk_for(path: &Path) -> (u64, u64, String) {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|disk| canonical.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().components().count())
        .map(|disk| {
            (
                disk.total_space(),
                disk.available_space(),
                disk.mount_point().to_string_lossy().into_owned(),
            )
        })
        .unwrap_or((0, 0, canonical.to_string_lossy().into_owned()))
}

fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 * 100.0 / total as f64
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/server/health", get(server_health))
        .route("/api/v1/server/metrics", get(server_metrics))
        .route("/api/v1/server/storage", get(server_storage))
        .route("/api/v1/server/alerts", get(server_alerts))
        .route("/api/v1/jobs", get(jobs))
        .route("/api/v1/activity", get(activity))
}

async fn server_health(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let db = state.database().await;
    let database_ok = db.ping().await.is_ok();
    let ffmpeg = state.config.ffmpeg_path.is_file();
    let ffprobe = state.config.ffprobe_path.is_file();
    let status = if !database_ok {
        "ERROR"
    } else if !ffmpeg || !ffprobe {
        "DEGRADED"
    } else {
        "HEALTHY"
    };
    Ok(Json(json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "startedAt": chrono::DateTime::<chrono::Utc>::from(state.started_at).to_rfc3339(),
        "uptimeSeconds": state.started_at.elapsed().unwrap_or_default().as_secs(),
        "databaseType": db.kind.as_str(),
        "databaseStatus": if database_ok { "AVAILABLE" } else { "UNAVAILABLE" },
        "ffmpegAvailable": ffmpeg,
        "ffprobeAvailable": ffprobe,
        "operatingSystem": System::long_os_version(),
        "architecture": std::env::consts::ARCH,
        "dataDirectory": state.config.data_dir.to_string_lossy(),
        "host": state.config.host.to_string(),
        "port": state.config.port,
    })))
}

async fn server_metrics(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let resources = state.metrics.resources().await;
    let sizes = state.metrics.directory_sizes().await;
    let db = state.database().await;
    let counts = sqlx::query("SELECT (SELECT COUNT(*) FROM playback_sessions WHERE status<>'ENDED') streams,(SELECT COUNT(*) FROM playback_sessions WHERE status<>'ENDED' AND mode='TRANSCODE') transcodes,(SELECT COUNT(*) FROM scan_jobs WHERE status IN ('SCANNING','MATCHING','PERSISTING')) scans,(SELECT COUNT(*) FROM scan_jobs WHERE status='QUEUED') queued_jobs")
        .fetch_one(&db.pool).await?;
    let streams: i64 = counts.get("streams");
    let active_transcodes: i64 = counts.get("transcodes");
    let history = state.metrics.record(&resources, streams).await;
    Ok(Json(json!({
        "capturedAt": resources.captured_at,
        "cpuUsagePercent": resources.cpu_usage_percent,
        "memoryUsedBytes": resources.memory_used_bytes,
        "memoryTotalBytes": resources.memory_total_bytes,
        "memoryUsagePercent": percent(resources.memory_used_bytes, resources.memory_total_bytes),
        "processMemoryBytes": resources.process_memory_bytes,
        "diskTotalBytes": resources.disk_total_bytes,
        "diskUsedBytes": resources.disk_used_bytes,
        "diskFreeBytes": resources.disk_free_bytes,
        "dataDirectorySizeBytes": sizes.data,
        "databaseSizeBytes": sizes.database,
        "logsSizeBytes": sizes.logs,
        "transcodeCacheSizeBytes": sizes.transcode_cache,
        "activePlaybackSessions": streams,
        "activeTranscodes": active_transcodes,
        "queuedTranscodes": state.playback.queued_count(),
        "activeScanJobs": counts.get::<i64,_>("scans"),
        "queuedJobs": counts.get::<i64,_>("queued_jobs"),
        "transcodeLimit": state.config.transcode_max_concurrent,
        "transcodeQueueLimit": state.config.transcode_max_queue,
        "history": history,
    })))
}

async fn server_storage(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("storage.view")?;
    let resources = state.metrics.resources().await;
    let sizes = state.metrics.directory_sizes().await;
    let db = state.database().await;
    let rows = sqlx::query("SELECT l.id,l.name,l.library_type,l.is_active,COALESCE(s.total_size_bytes,0) size_bytes,COALESCE(s.file_count,0) file_count,COALESCE(s.media_item_count,0) content_count,(SELECT COUNT(*) FROM library_paths p WHERE p.library_id=l.id AND p.status<>'AVAILABLE') unavailable_paths FROM libraries l LEFT JOIN library_stats s ON s.library_id=l.id WHERE l.deleted_at IS NULL ORDER BY l.name")
        .fetch_all(&db.pool).await?;
    let libraries = rows.iter().map(|r| json!({
        "id": r.get::<String,_>("id"), "name": r.get::<String,_>("name"), "type": r.get::<String,_>("library_type"),
        "sizeBytes": r.get::<i64,_>("size_bytes"), "fileCount": r.get::<i64,_>("file_count"), "contentCount": r.get::<i64,_>("content_count"),
        "status": if r.get::<i64,_>("is_active")==0 { "DISABLED" } else if r.get::<i64,_>("unavailable_paths")>0 { "PATH_UNAVAILABLE" } else { "READY" }
    })).collect::<Vec<_>>();
    Ok(Json(json!({
        "systemStorage": {"path":resources.disk_path,"totalBytes":resources.disk_total_bytes,"usedBytes":resources.disk_used_bytes,"freeBytes":resources.disk_free_bytes,"usagePercent":percent(resources.disk_used_bytes,resources.disk_total_bytes),"status":disk_status(percent(resources.disk_used_bytes,resources.disk_total_bytes))},
        "dataDirectory":{"sizeBytes":sizes.data},
        "libraryStorage": libraries,
        "database":{"type":db.kind.as_str(),"sizeBytes":sizes.database},
        "transcodeCache":{"sizeBytes":sizes.transcode_cache,"maxBytes":state.config.transcode_cache_gb*1024*1024*1024},
        "logs":{"sizeBytes":sizes.logs}
    })))
}

fn disk_status(usage: f64) -> &'static str {
    if usage >= 95.0 {
        "CRITICAL"
    } else if usage >= 85.0 {
        "WARNING"
    } else {
        "HEALTHY"
    }
}

async fn server_alerts(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let resources = state.metrics.resources().await;
    let sizes = state.metrics.directory_sizes().await;
    let db = state.database().await;
    let created_at = chrono::Utc::now().to_rfc3339();
    let mut alerts = Vec::new();
    let mut add = |id: &str, severity: &str, kind: &str, title: &str, message: String| {
        alerts.push(json!({"id":id,"severity":severity,"type":kind,"title":title,"message":message,"createdAt":created_at,"resolved":false}))
    };
    if !state.config.ffmpeg_path.is_file() {
        add(
            "ffmpeg-unavailable",
            "WARNING",
            "FFMPEG_UNAVAILABLE",
            "FFmpeg indisponível",
            "O motor de transcodificação não foi encontrado.".into(),
        );
    }
    if !state.config.ffprobe_path.is_file() {
        add(
            "ffprobe-unavailable",
            "WARNING",
            "FFPROBE_UNAVAILABLE",
            "FFprobe indisponível",
            "A análise técnica de mídia está limitada.".into(),
        );
    }
    let usage = percent(resources.disk_used_bytes, resources.disk_total_bytes);
    if usage >= 85.0 {
        add(
            "disk-usage",
            if usage >= 95.0 { "CRITICAL" } else { "WARNING" },
            "DISK_USAGE",
            "Armazenamento quase cheio",
            format!("O volume de dados está com {usage:.1}% de uso."),
        );
    }
    let cache_limit = state.config.transcode_cache_gb * 1024 * 1024 * 1024;
    if cache_limit > 0 && percent(sizes.transcode_cache, cache_limit) >= 95.0 {
        add(
            "transcode-cache-full",
            "WARNING",
            "TRANSCODE_CACHE_FULL",
            "Cache de transcode cheio",
            "O cache atingiu pelo menos 95% do limite configurado.".into(),
        );
    }
    if state.playback.queued_count() >= state.config.transcode_max_queue {
        add(
            "transcode-queue",
            "CRITICAL",
            "TRANSCODE_QUEUE_SATURATED",
            "Fila de transcode saturada",
            "A fila atingiu o limite configurado.".into(),
        );
    }
    let paths =
        sqlx::query("SELECT id,path FROM library_paths WHERE is_active=1 AND status<>'AVAILABLE'")
            .fetch_all(&db.pool)
            .await?;
    for row in paths {
        let id: String = row.get("id");
        add(
            &format!("path-{id}"),
            "CRITICAL",
            "PATH_UNAVAILABLE",
            "Path indisponível",
            row.get("path"),
        );
    }
    Ok(Json(json!({"items":alerts})))
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobsQuery {
    status: Option<String>,
    r#type: Option<String>,
    library_id: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
}

async fn jobs(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<JobsQuery>,
) -> AppResult<Json<Value>> {
    auth.require("jobs.view")?;
    let rows = sqlx::query("SELECT j.*,l.name library_name FROM scan_jobs j JOIN libraries l ON l.id=j.library_id ORDER BY j.created_at DESC LIMIT 500").fetch_all(&state.database().await.pool).await?;
    let mut items = rows
        .iter()
        .map(job_json)
        .filter(|v| {
            q.status.as_ref().is_none_or(|x| v["status"] == *x)
                && q.r#type.as_ref().is_none_or(|x| v["type"] == *x)
                && q.library_id
                    .as_ref()
                    .is_none_or(|x| v["library"]["id"] == *x)
        })
        .collect::<Vec<_>>();
    let total = items.len();
    let page = q.page.unwrap_or(1).max(1);
    let size = q.page_size.unwrap_or(20).clamp(1, 100);
    let start = (page - 1) * size;
    items = items.into_iter().skip(start).take(size).collect();
    Ok(Json(
        json!({"items":items,"page":page,"pageSize":size,"total":total,"totalPages":total.div_ceil(size)}),
    ))
}

fn job_json(r: &sqlx::any::AnyRow) -> Value {
    let started = r.try_get::<String, _>("started_at").ok();
    let finished = r.try_get::<String, _>("finished_at").ok();
    let duration = started
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .zip(
            finished
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()),
        )
        .map(|(a, b)| (b - a).num_milliseconds());
    let source: String = r.get("trigger_source");
    json!({"id":r.get::<String,_>("id"),"type":if source=="MANUAL"{"LIBRARY_SCAN"}else{"LIBRARY_AUTO_SYNC"},"status":normalize_job_status(r.get::<String,_>("status")),"progress":r.get::<f64,_>("progress"),"createdAt":r.get::<String,_>("created_at"),"startedAt":started,"finishedAt":finished,"duration":duration,"source":source,"library":{"id":r.get::<String,_>("library_id"),"name":r.get::<String,_>("library_name")},"message":r.try_get::<String,_>("error_message").ok(),"errorCode":if r.get::<String,_>("status")=="FAILED"{Some("LIBRARY_SCAN_FAILED")}else{None::<&str>},"actions":{"cancellable":matches!(r.get::<String,_>("status").as_str(),"QUEUED"|"SCANNING"|"MATCHING"|"PERSISTING"),"retryable":matches!(r.get::<String,_>("status").as_str(),"FAILED"|"CANCELLED")}})
}
fn normalize_job_status(status: String) -> String {
    match status.as_str() {
        "SCANNING" | "MATCHING" | "PERSISTING" => "RUNNING".into(),
        other => other.into(),
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivityQuery {
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn activity(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ActivityQuery>,
) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let page = q.page.unwrap_or(1).max(1);
    let size = q.page_size.unwrap_or(20).clamp(1, 100);
    let db = state.database().await;
    let total: i64 = sqlx::query("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&db.pool)
        .await?
        .get(0);
    let rows=sqlx::query("SELECT a.id,a.action,a.entity_type,a.entity_id,a.metadata,a.created_at,u.display_name FROM audit_log a LEFT JOIN users u ON u.id=a.actor_user_id ORDER BY a.created_at DESC LIMIT ? OFFSET ?").bind(size).bind((page-1)*size).fetch_all(&db.pool).await?;
    let items = rows.iter().map(activity_json).collect::<Vec<_>>();
    Ok(Json(
        json!({"items":items,"page":page,"pageSize":size,"total":total,"totalPages":((total+size-1)/size)}),
    ))
}

fn activity_json(r: &sqlx::any::AnyRow) -> Value {
    let action: String = r.get("action");
    let actor = r
        .try_get::<String, _>("display_name")
        .unwrap_or_else(|_| "Sistema".into());
    let metadata: Value = r
        .try_get::<String, _>("metadata")
        .ok()
        .and_then(|m| serde_json::from_str(&m).ok())
        .unwrap_or(json!({}));
    let (kind, title, message) = match action.as_str() {
        "LOGIN_SUCCESS" => (
            "LOGIN",
            format!("{actor} entrou no MyLib"),
            "Login realizado com sucesso".into(),
        ),
        "USER_CREATED" => (
            "USER_CREATED",
            format!("{actor} criou um usuário"),
            "Novo acesso ao servidor".into(),
        ),
        "LIBRARY_CREATED" => (
            "LIBRARY_CREATED",
            format!("{actor} criou uma biblioteca"),
            metadata["name"]
                .as_str()
                .unwrap_or("Biblioteca adicionada")
                .into(),
        ),
        "PLAYBACK_STARTED" => (
            "PLAYBACK_STARTED",
            format!(
                "{actor} iniciou {}",
                metadata["title"].as_str().unwrap_or("uma reprodução")
            ),
            format!(
                "Modo {}",
                metadata["mode"].as_str().unwrap_or("desconhecido")
            ),
        ),
        "PLAYBACK_COMPLETED" => (
            "PLAYBACK_COMPLETED",
            format!("{actor} concluiu uma reprodução"),
            "Conteúdo assistido até o fim".into(),
        ),
        "PLAYBACK_SESSION_TERMINATED_BY_ADMIN" => (
            "PLAYBACK_STOPPED",
            format!("{actor} encerrou uma reprodução"),
            "Sessão finalizada pelo administrador".into(),
        ),
        "REMOTE_SOURCE_CREATED" => (
            "REMOTE_SOURCE_CREATED",
            format!("{actor} adicionou uma fonte remota"),
            metadata["name"].as_str().unwrap_or("Fonte remota").into(),
        ),
        "REMOTE_SOURCE_SYNCED" => {
            let new_items = metadata["new"].as_u64().unwrap_or(0);
            let message = if metadata["notModified"].as_bool().unwrap_or(false) {
                "Lista sem alterações".to_string()
            } else if new_items > 0 {
                format!("{new_items} novos conteúdos encontrados")
            } else {
                "Sincronização concluída".to_string()
            };
            (
                "REMOTE_SOURCE_SYNCED",
                "Fonte remota sincronizada".into(),
                message,
            )
        }
        "REMOTE_SOURCE_SYNC_FAILED" => (
            "REMOTE_SOURCE_ERROR",
            "Falha ao sincronizar fonte remota".into(),
            match metadata["status"].as_str() {
                Some("AUTH_REQUIRED") => "Autenticação da fonte expirou".into(),
                Some("UNAVAILABLE") => "Fonte remota indisponível".into(),
                _ => "Erro durante a sincronização".into(),
            },
        ),
        "GOOGLE_DRIVE_CONNECTED" => (
            "GOOGLE_DRIVE_CONNECTED",
            format!("{actor} conectou uma conta Google Drive"),
            metadata["accountEmail"]
                .as_str()
                .unwrap_or("Conta conectada")
                .into(),
        ),
        _ => (
            "SERVER",
            friendly_action(&action),
            format!("Ação realizada por {actor}"),
        ),
    };
    json!({"id":r.get::<String,_>("id"),"type":kind,"title":title,"message":message,"createdAt":r.get::<String,_>("created_at"),"entityType":r.get::<String,_>("entity_type"),"entityId":r.try_get::<String,_>("entity_id").ok()})
}
fn friendly_action(action: &str) -> String {
    action
        .to_ascii_lowercase()
        .replace('_', " ")
        .split_whitespace()
        .map(|s| {
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(dead_code)]
fn _internal_error(message: &str) -> AppError {
    AppError::config(message)
}
