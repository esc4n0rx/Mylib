use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
    process::Command,
    sync::{Mutex, Semaphore},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthUser,
    config::Config,
    db::now,
    errors::{AppError, AppResult},
    infrastructure::secrets,
};
use futures_util::TryStreamExt;

const QUALITY_NAMES: &[&str] = &[
    "AUTO",
    "ORIGINAL",
    "4K",
    "1080P_HIGH",
    "1080P",
    "720P",
    "480P",
];

#[derive(Clone)]
pub struct PlaybackRuntime {
    pipelines: Arc<Mutex<HashMap<String, Pipeline>>>,
    slots: Arc<Semaphore>,
    queued: Arc<AtomicUsize>,
    max_queue: usize,
    pub cache_dir: PathBuf,
    cache_ttl: Duration,
    cache_max_bytes: u64,
    ffmpeg_path: PathBuf,
}

#[derive(Clone)]
struct Pipeline {
    directory: PathBuf,
    viewers: usize,
    status: String,
}

impl PlaybackRuntime {
    pub fn new(config: &Config) -> AppResult<Self> {
        let cache_dir = config.data_dir.join("cache/transcode");
        std::fs::create_dir_all(&cache_dir)?;
        let runtime = Self {
            pipelines: Arc::new(Mutex::new(HashMap::new())),
            slots: Arc::new(Semaphore::new(config.transcode_max_concurrent)),
            queued: Arc::new(AtomicUsize::new(0)),
            max_queue: config.transcode_max_queue,
            cache_dir,
            cache_ttl: Duration::from_secs(config.transcode_cache_ttl_seconds),
            cache_max_bytes: config.transcode_cache_gb.saturating_mul(1024 * 1024 * 1024),
            ffmpeg_path: config.ffmpeg_path.clone(),
        };
        runtime.spawn_cleanup();
        Ok(runtime)
    }

    fn spawn_cleanup(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(900));
            loop {
                ticker.tick().await;
                if let Err(error) = this.cleanup_cache().await {
                    tracing::warn!(%error, "transcode cache cleanup failed");
                }
            }
        });
    }

    async fn cleanup_cache(&self) -> std::io::Result<()> {
        let active: Vec<PathBuf> = self
            .pipelines
            .lock()
            .await
            .values()
            .map(|p| p.directory.clone())
            .collect();
        let mut entries = Vec::new();
        let mut total = 0_u64;
        let mut reader = fs::read_dir(&self.cache_dir).await?;
        while let Some(entry) = reader.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() || active.contains(&path) {
                continue;
            }
            let metadata = entry.metadata().await?;
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let size = directory_size(&path).await.unwrap_or(0);
            total = total.saturating_add(size);
            entries.push((path, modified, size));
        }
        entries.sort_by_key(|(_, modified, _)| *modified);
        for (path, modified, size) in entries {
            let expired = modified.elapsed().unwrap_or_default() > self.cache_ttl;
            if expired || total > self.cache_max_bytes {
                fs::remove_dir_all(&path).await?;
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }

    async fn ensure_pipeline(
        &self,
        key: &str,
        input: &Path,
        profile: QualityProfile,
        remux: bool,
    ) -> AppResult<PathBuf> {
        {
            let mut pipelines = self.pipelines.lock().await;
            if let Some(pipeline) = pipelines.get_mut(key) {
                pipeline.viewers += 1;
                tracing::info!(pipeline_key=%key, viewers=pipeline.viewers, "TRANSCODE_SHARED");
                return Ok(pipeline.directory.clone());
            }
        }
        if self.queued.fetch_add(1, Ordering::SeqCst) >= self.max_queue {
            self.queued.fetch_sub(1, Ordering::SeqCst);
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "TRANSCODE_QUEUE_FULL",
                "A fila de transcodificação está cheia.",
            ));
        }
        let directory = self.cache_dir.join(key);
        // A deterministic key can leave an incomplete playlist after a server restart.
        // Only in-memory pipelines are shareable; a new pipeline always starts clean.
        if directory.exists() {
            fs::remove_dir_all(&directory).await?;
        }
        fs::create_dir_all(&directory).await?;
        let pipeline = Pipeline {
            directory: directory.clone(),
            viewers: 1,
            status: "QUEUED".into(),
        };
        self.pipelines.lock().await.insert(key.into(), pipeline);
        let this = self.clone();
        let key = key.to_owned();
        let input = input.to_owned();
        let output = directory.join("index.m3u8");
        let pipeline_directory = directory.clone();
        tokio::spawn(async move {
            let permit = this.slots.acquire().await;
            this.queued.fetch_sub(1, Ordering::SeqCst);
            if permit.is_err() {
                return;
            }
            if let Some(p) = this.pipelines.lock().await.get_mut(&key) {
                p.status = "RUNNING".into();
            }
            let mut command = Command::new(&this.ffmpeg_path);
            command.kill_on_drop(true);
            command
                .arg("-hide_banner")
                .arg("-loglevel")
                .arg("warning")
                .arg("-y")
                .args(["-fflags", "+genpts"])
                .arg("-i")
                .arg(&input)
                .args(["-map", "0:v:0", "-map", "0:a:0?"]);
            if remux {
                command.args(["-c", "copy"]);
            } else {
                command.args([
                    "-c:v",
                    "libx264",
                    "-preset",
                    "veryfast",
                    "-sc_threshold",
                    "0",
                    "-force_key_frames",
                    "expr:gte(t,n_forced*6)",
                    "-b:v",
                    &format!("{}k", profile.video_kbps),
                    "-maxrate",
                    &format!("{}k", profile.video_kbps),
                    "-bufsize",
                    &format!("{}k", profile.video_kbps * 2),
                    "-vf",
                    &format!("scale=w=-2:h='min({},ih)'", profile.max_height),
                    "-c:a",
                    "aac",
                    "-ac",
                    "2",
                    "-b:a",
                    &format!("{}k", profile.audio_kbps),
                ]);
            }
            command
                .args([
                    "-f",
                    "hls",
                    "-avoid_negative_ts",
                    "make_zero",
                    "-muxdelay",
                    "0",
                    "-max_muxing_queue_size",
                    "2048",
                    "-hls_time",
                    "6",
                    "-hls_playlist_type",
                    "event",
                    "-hls_flags",
                    "independent_segments+temp_file",
                    "-hls_segment_filename",
                ])
                .arg(pipeline_directory.join("segment%05d.ts"))
                .arg(&output)
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            tracing::info!(pipeline_key=%key, "TRANSCODE_STARTED");
            let result = command.output().await;
            let succeeded = result.as_ref().is_ok_and(|output| output.status.success());
            if !succeeded {
                let detail = result
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stderr).into_owned())
                    .unwrap_or_else(|error| error.to_string())
                    .replace(input.to_string_lossy().as_ref(), "<media>")
                    .replace(output.to_string_lossy().as_ref(), "<playlist>")
                    .replace(pipeline_directory.to_string_lossy().as_ref(), "<cache>");
                let detail: String = detail.chars().take(3000).collect();
                tracing::warn!(pipeline_key=%key, error=%detail, "TRANSCODE_ERROR");
            }
            let mut pipelines = this.pipelines.lock().await;
            if let Some(p) = pipelines.get_mut(&key) {
                p.status = if succeeded {
                    "READY".into()
                } else {
                    "ERROR".into()
                };
            }
            let should_release = pipelines.get(&key).is_some_and(|p| p.viewers == 0);
            tracing::info!(pipeline_key=%key, "TRANSCODE_STOPPED");
            drop(pipelines);
            if should_release {
                this.pipelines.lock().await.remove(&key);
            }
            drop(permit);
        });
        Ok(directory)
    }

    async fn release(&self, key: &str) {
        let mut pipelines = self.pipelines.lock().await;
        if let Some(pipeline) = pipelines.get_mut(key) {
            pipeline.viewers = pipeline.viewers.saturating_sub(1);
        }
        let this = self.clone();
        let key = key.to_owned();
        drop(pipelines);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let remove = this
                .pipelines
                .lock()
                .await
                .get(&key)
                .is_some_and(|p| p.viewers == 0 && p.status != "RUNNING");
            if remove {
                this.pipelines.lock().await.remove(&key);
            }
        });
    }

    pub fn queued_count(&self) -> usize {
        self.queued.load(Ordering::SeqCst)
    }

    async fn pipeline_snapshots(&self) -> Vec<(String, usize, String)> {
        self.pipelines
            .lock()
            .await
            .iter()
            .map(|(key, pipeline)| (key.clone(), pipeline.viewers, pipeline.status.clone()))
            .collect()
    }
}

async fn directory_size(root: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        let mut reader = fs::read_dir(path).await?;
        while let Some(entry) = reader.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    Ok(total)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/playback/start", post(start))
        .route("/api/v1/playback/{session_id}/direct", get(direct))
        .route("/api/v1/playback/{session_id}/remote", get(remote_stream))
        .route("/api/v1/playback/{session_id}/master.m3u8", get(master))
        .route(
            "/api/v1/playback/{session_id}/segments/{segment}",
            get(segment),
        )
        .route("/api/v1/playback/{session_id}/progress", post(progress))
        .route("/api/v1/playback/{session_id}/stop", post(stop))
        .route("/api/v1/playback/continue-watching", get(continue_watching))
        .route("/api/v1/playback/history", get(history))
        .route("/api/v1/playback/sessions", get(sessions))
        .route("/api/v1/playback/transcodes", get(transcodes))
        .route("/api/v1/playback/capabilities", get(capabilities))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(default)]
    pub containers: Vec<String>,
    #[serde(default)]
    pub video_codecs: Vec<String>,
    #[serde(default)]
    pub audio_codecs: Vec<String>,
    pub max_width: Option<i64>,
    pub max_height: Option<i64>,
    pub estimated_bandwidth_kbps: Option<i64>,
    pub max_audio_channels: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartRequest {
    media_item_id: String,
    media_file_id: Option<String>,
    episode_id: Option<String>,
    client_capabilities: ClientCapabilities,
    #[serde(default = "auto_quality")]
    quality: String,
    client_id: Option<String>,
    client_name: Option<String>,
}
fn auto_quality() -> String {
    "AUTO".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalMetadata {
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    pub overall_bitrate: Option<i64>,
    pub video_codec: Option<String>,
    pub video_profile: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub frame_rate: Option<f64>,
    pub video_bitrate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub hdr_type: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i64>,
    pub audio_sample_rate: Option<i64>,
    pub audio_bitrate: Option<i64>,
    pub audio_language: Option<String>,
    pub audio_tracks: Vec<Value>,
    pub subtitle_tracks: Vec<Value>,
}

#[derive(Debug, Clone, Copy)]
struct QualityProfile {
    name: &'static str,
    max_height: i64,
    video_kbps: i64,
    audio_kbps: i64,
}
fn quality_profile(name: &str) -> Option<QualityProfile> {
    match name.to_ascii_uppercase().as_str() {
        "AUTO" => Some(QualityProfile {
            name: "AUTO",
            max_height: 1080,
            video_kbps: 8000,
            audio_kbps: 192,
        }),
        "ORIGINAL" => Some(QualityProfile {
            name: "ORIGINAL",
            max_height: 4320,
            video_kbps: 100_000,
            audio_kbps: 1000,
        }),
        "4K" => Some(QualityProfile {
            name: "4K",
            max_height: 2160,
            video_kbps: 25_000,
            audio_kbps: 384,
        }),
        "1080P_HIGH" => Some(QualityProfile {
            name: "1080P_HIGH",
            max_height: 1080,
            video_kbps: 12_000,
            audio_kbps: 256,
        }),
        "1080P" => Some(QualityProfile {
            name: "1080P",
            max_height: 1080,
            video_kbps: 8000,
            audio_kbps: 192,
        }),
        "720P" => Some(QualityProfile {
            name: "720P",
            max_height: 720,
            video_kbps: 4000,
            audio_kbps: 160,
        }),
        "480P" => Some(QualityProfile {
            name: "480P",
            max_height: 480,
            video_kbps: 1500,
            audio_kbps: 128,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackMode {
    DirectPlay,
    DirectStream,
    Transcode,
}
impl PlaybackMode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::DirectPlay => "DIRECT_PLAY",
            Self::DirectStream => "DIRECT_STREAM",
            Self::Transcode => "TRANSCODE",
        }
    }
}

pub fn decide_playback(
    metadata: &TechnicalMetadata,
    caps: &ClientCapabilities,
    quality: &str,
) -> (PlaybackMode, Vec<String>) {
    let mut reasons = Vec::new();
    let requested_original = matches!(quality.to_ascii_uppercase().as_str(), "AUTO" | "ORIGINAL");
    let video_ok = metadata
        .video_codec
        .as_ref()
        .is_none_or(|v| caps.video_codecs.iter().any(|c| codec_eq(c, v)));
    let audio_ok = metadata
        .audio_codec
        .as_ref()
        .is_none_or(|v| caps.audio_codecs.iter().any(|c| codec_eq(c, v)));
    let audio_channels_ok = metadata
        .audio_channels
        .zip(caps.max_audio_channels)
        .is_none_or(|(source, maximum)| source <= maximum);
    let container_ok = metadata
        .container
        .as_ref()
        .is_none_or(|v| caps.containers.iter().any(|c| c.eq_ignore_ascii_case(v)));
    if !video_ok {
        reasons.push("VIDEO_CODEC_UNSUPPORTED".into());
    }
    if !audio_ok {
        reasons.push("AUDIO_CODEC_UNSUPPORTED".into());
    }
    if !audio_channels_ok {
        reasons.push("AUDIO_CHANNELS_UNSUPPORTED".into());
    }
    if !container_ok {
        reasons.push("CONTAINER_UNSUPPORTED".into());
    }
    if metadata
        .width
        .zip(caps.max_width)
        .is_some_and(|(a, b)| a > b)
        || metadata
            .height
            .zip(caps.max_height)
            .is_some_and(|(a, b)| a > b)
    {
        reasons.push("RESOLUTION_TOO_HIGH".into());
    }
    if metadata
        .overall_bitrate
        .zip(caps.estimated_bandwidth_kbps)
        .is_some_and(|(a, b)| a / 1000 > b)
    {
        reasons.push("BITRATE_TOO_HIGH".into());
    }
    if !requested_original {
        reasons.push("USER_QUALITY_LIMIT".into());
    }
    if reasons.is_empty() {
        (PlaybackMode::DirectPlay, reasons)
    } else if video_ok
        && audio_ok
        && hls_remux_compatible(metadata)
        && requested_original
        && reasons.iter().all(|r| r == "CONTAINER_UNSUPPORTED")
    {
        (PlaybackMode::DirectStream, reasons)
    } else {
        (PlaybackMode::Transcode, reasons)
    }
}
fn hls_remux_compatible(metadata: &TechnicalMetadata) -> bool {
    let video = metadata.video_codec.as_deref().unwrap_or("h264");
    let audio = metadata.audio_codec.as_deref().unwrap_or("aac");
    matches!(
        video.to_ascii_lowercase().as_str(),
        "h264" | "hevc" | "h265"
    ) && matches!(
        audio.to_ascii_lowercase().as_str(),
        "aac" | "mp3" | "ac3" | "eac3"
    )
}
fn codec_eq(a: &str, b: &str) -> bool {
    let normalize = |v: &str| match v.to_ascii_lowercase().as_str() {
        "avc" | "avc1" => "h264".to_owned(),
        "h265" | "x265" => "hevc".to_owned(),
        _ => v.to_ascii_lowercase(),
    };
    normalize(a) == normalize(b)
}

async fn start(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(payload): Json<StartRequest>,
) -> AppResult<Json<Value>> {
    auth.require("media.play")?;
    let profile_id = auth.require_profile()?.to_owned();
    let quality = quality_profile(&payload.quality)
        .ok_or_else(|| AppError::validation("INVALID_QUALITY", "Perfil de qualidade inválido."))?;
    let db = state.database().await;
    let row=sqlx::query("SELECT f.id,f.absolute_path,f.file_size,f.modified_at,f.extension,f.media_item_id,f.tv_episode_id,f.storage_kind,r.stream_ref AS remote_stream_ref,r.stream_sealed AS remote_stream_sealed,l.id AS library_id,l.privacy FROM media_files f JOIN libraries l ON l.id=f.library_id LEFT JOIN remote_media_sources r ON r.id=f.remote_media_source_id WHERE f.id=COALESCE(?,f.id) AND f.media_item_id=? AND (? IS NULL OR f.tv_episode_id=?) AND f.missing_since IS NULL ORDER BY f.storage_kind,f.filename LIMIT 1")
        .bind(payload.media_file_id.as_deref()).bind(&payload.media_item_id).bind(payload.episode_id.as_deref()).bind(payload.episode_id.as_deref()).fetch_optional(&db.pool).await?
        .ok_or_else(||AppError::not_found("MEDIA_FILE_NOT_FOUND","Arquivo de mídia indisponível."))?;
    let library_id: String = row.get("library_id");
    let privacy: String = row.get("privacy");
    if privacy == "PRIVATE" && !auth.is_admin() {
        let explicitly_allowed: i64 = sqlx::query("SELECT COUNT(*) FROM user_library_access WHERE user_id=? AND library_id=? AND can_view=1 AND can_play=1")
            .bind(&auth.id).bind(&library_id).fetch_one(&db.pool).await?.get(0);
        let unlocked = headers
            .get("x-library-unlock")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| state.tokens.verify_library_unlock(v, &auth.id, &library_id));
        if explicitly_allowed == 0 && !unlocked {
            return Err(AppError::new(
                StatusCode::LOCKED,
                "LIBRARY_LOCKED",
                "Desbloqueie a biblioteca privada antes de reproduzir.",
            ));
        }
    }
    let profile_allowed: i64 = sqlx::query("SELECT COUNT(*) FROM profiles p JOIN profile_library_access pla ON pla.profile_id=p.id AND pla.library_id=? AND pla.is_allowed=1 JOIN libraries l ON l.id=? JOIN media_items mi ON mi.id=? WHERE p.id=? AND p.user_id=? AND p.is_active=1 AND l.minimum_age<=p.max_age_rating AND (mi.content_age_rating IS NOT NULL AND mi.content_age_rating<=p.max_age_rating OR mi.content_age_rating IS NULL AND (p.is_kids=0 OR EXISTS(SELECT 1 FROM parental_control_settings pcs WHERE pcs.id=1 AND pcs.unknown_kids_policy='ALLOW')))")
        .bind(&library_id).bind(&library_id).bind(&payload.media_item_id).bind(&profile_id).bind(&auth.id).fetch_one(&db.pool).await?.get(0);
    if profile_allowed == 0 {
        return Err(AppError::forbidden());
    }
    let file_id: String = row.get("id");
    let absolute_path: String = row.get("absolute_path");
    let file_size: i64 = row.get("file_size");
    let modified_at: String = row.get("modified_at");
    let is_remote = row.get::<String, _>("storage_kind") == "REMOTE";
    // Remote sources are analyzed and transcoded from their origin URL; the raw
    // URL (with any credentials) is never returned to the client.
    let media_input = if is_remote {
        let sealed: String = row.try_get("remote_stream_ref").map_err(|_| {
            AppError::not_found(
                "REMOTE_SOURCE_REMOVED",
                "Este conteúdo pertence a uma fonte remota que foi removida.",
            )
        })?;
        if row.try_get::<i64, _>("remote_stream_sealed").unwrap_or(0) != 0 {
            secrets::open(&state.config, &sealed)?
        } else {
            sealed
        }
    } else {
        absolute_path.clone()
    };
    let metadata = technical_metadata(
        &db.pool,
        &file_id,
        &media_input,
        file_size,
        &modified_at,
        row.get::<String, _>("extension"),
        &state.config.ffprobe_path,
    )
    .await?;
    let (mode, reasons) = decide_playback(&metadata, &payload.client_capabilities, quality.name);
    let session_id = Uuid::new_v4().to_string();
    let raw_token = stream_token();
    let token_hash = hash_token(&raw_token);
    let duration = metadata.duration_ms.unwrap_or(0);
    let timestamp = now();
    let content_key = payload
        .episode_id
        .clone()
        .unwrap_or_else(|| payload.media_item_id.clone());
    let resume:i64=sqlx::query("SELECT position_ms FROM playback_progress WHERE profile_id=? AND content_key=? AND completed_at IS NULL").bind(&profile_id).bind(&content_key).fetch_optional(&db.pool).await?.map(|r|r.get(0)).unwrap_or(0);
    let content = sqlx::query("SELECT m.title,m.year,e.name AS episode_name,e.season_number,e.episode_number FROM media_items m LEFT JOIN tv_episodes e ON e.id=? WHERE m.id=?")
        .bind(payload.episode_id.as_deref()).bind(&payload.media_item_id).fetch_one(&db.pool).await?;
    let adjacent = if let Some(episode_id) = payload.episode_id.as_deref() {
        sqlx::query("SELECT e.id,e.season_number,e.episode_number,e.name,f.id AS media_file_id FROM tv_episodes current JOIN tv_episodes e ON e.tv_show_id=current.tv_show_id JOIN media_files f ON f.tv_episode_id=e.id AND f.missing_since IS NULL WHERE current.id=? AND (e.season_number*10000+e.episode_number)>(current.season_number*10000+current.episode_number) ORDER BY e.season_number,e.episode_number LIMIT 1")
            .bind(episode_id).fetch_optional(&db.pool).await?
    } else {
        None
    };
    let pipeline_key = if mode != PlaybackMode::DirectPlay {
        Some(pipeline_key(&file_id, quality.name, &mode))
    } else {
        None
    };
    sqlx::query("INSERT INTO playback_sessions (id,user_id,profile_id,media_item_id,media_file_id,episode_id,mode,quality_profile,reason,stream_token_hash,started_at,last_activity_at,position_ms,duration_ms,client_id,client_name,status,pipeline_key) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'STARTING',?)")
        .bind(&session_id).bind(&auth.id).bind(&profile_id).bind(&payload.media_item_id).bind(&file_id).bind(payload.episode_id.as_deref()).bind(mode.as_str()).bind(quality.name).bind(serde_json::to_string(&reasons).unwrap()).bind(token_hash).bind(&timestamp).bind(&timestamp).bind(resume).bind(duration).bind(payload.client_id).bind(payload.client_name).bind(pipeline_key.as_deref()).execute(&db.pool).await?;
    if let Some(key) = &pipeline_key {
        state
            .playback
            .ensure_pipeline(
                key,
                Path::new(&media_input),
                quality,
                mode == PlaybackMode::DirectStream,
            )
            .await?;
    }
    sqlx::query("UPDATE playback_sessions SET status='PLAYING' WHERE id=?")
        .bind(&session_id)
        .execute(&db.pool)
        .await?;
    upsert_history(
        &db.pool,
        &auth.id,
        &profile_id,
        &payload.media_item_id,
        payload.episode_id.as_deref(),
        &content_key,
        0,
        false,
        true,
    )
    .await?;
    db.audit(
        Some(&auth.id),
        "PLAYBACK_STARTED",
        "playback_session",
        Some(&session_id),
        json!({"mediaItemId":payload.media_item_id,"profileId":profile_id,"title":content.get::<String,_>("title"),"mode":mode.as_str()}),
        None,
    )
    .await?;
    let suffix = if mode == PlaybackMode::DirectPlay {
        if is_remote { "remote" } else { "direct" }
    } else {
        "master.m3u8"
    };
    tracing::info!(session_id=%session_id,user_id=%auth.id,mode=mode.as_str(),"PLAYBACK_STARTED");
    Ok(Json(
        json!({"sessionId":session_id,"playbackMode":mode.as_str(),"streamUrl":format!("/api/v1/playback/{session_id}/{suffix}?token={raw_token}"),"duration":duration,"resumePosition":resume,"quality":quality.name,"reason":reasons,"availableQualities":QUALITY_NAMES,"metadata":metadata,"content":{"mediaItemId":payload.media_item_id,"mediaFileId":file_id,"episodeId":payload.episode_id,"title":content.get::<String,_>("title"),"year":content.try_get::<i64,_>("year").ok(),"episodeName":content.try_get::<String,_>("episode_name").ok(),"seasonNumber":content.try_get::<i64,_>("season_number").ok(),"episodeNumber":content.try_get::<i64,_>("episode_number").ok()},"nextEpisode":adjacent.map(|r|json!({"episodeId":r.get::<String,_>("id"),"mediaFileId":r.get::<String,_>("media_file_id"),"seasonNumber":r.get::<i64,_>("season_number"),"episodeNumber":r.get::<i64,_>("episode_number"),"name":r.try_get::<String,_>("name").ok()}))}),
    ))
}

async fn technical_metadata(
    pool: &sqlx::AnyPool,
    file_id: &str,
    path: &str,
    size: i64,
    modified: &str,
    extension: String,
    ffprobe_path: &Path,
) -> AppResult<TechnicalMetadata> {
    if let Some(row)=sqlx::query("SELECT * FROM media_technical_metadata WHERE media_file_id=? AND file_size=? AND modified_at=?").bind(file_id).bind(size).bind(modified).fetch_optional(pool).await? {
        return Ok(metadata_from_row(&row));
    }
    let mut meta = probe_file(ffprobe_path, path)
        .await
        .unwrap_or_else(|_| TechnicalMetadata {
            container: Some(extension.to_ascii_lowercase()),
            ..Default::default()
        });
    if meta.container.is_none() {
        meta.container = Some(extension.to_ascii_lowercase());
    }
    sqlx::query("DELETE FROM media_technical_metadata WHERE media_file_id=?")
        .bind(file_id)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO media_technical_metadata (media_file_id,file_size,modified_at,container,duration_ms,overall_bitrate,video_codec,video_profile,width,height,frame_rate,video_bitrate,bit_depth,hdr_type,audio_codec,audio_channels,audio_sample_rate,audio_bitrate,audio_language,audio_tracks,subtitle_tracks,analyzed_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(file_id).bind(size).bind(modified).bind(&meta.container).bind(meta.duration_ms).bind(meta.overall_bitrate).bind(&meta.video_codec).bind(&meta.video_profile).bind(meta.width).bind(meta.height).bind(meta.frame_rate).bind(meta.video_bitrate).bind(meta.bit_depth).bind(&meta.hdr_type).bind(&meta.audio_codec).bind(meta.audio_channels).bind(meta.audio_sample_rate).bind(meta.audio_bitrate).bind(&meta.audio_language).bind(serde_json::to_string(&meta.audio_tracks).unwrap()).bind(serde_json::to_string(&meta.subtitle_tracks).unwrap()).bind(now()).execute(pool).await?;
    Ok(meta)
}

fn metadata_from_row(row: &sqlx::any::AnyRow) -> TechnicalMetadata {
    TechnicalMetadata {
        container: row.try_get("container").ok(),
        duration_ms: row.try_get("duration_ms").ok(),
        overall_bitrate: row.try_get("overall_bitrate").ok(),
        video_codec: row.try_get("video_codec").ok(),
        video_profile: row.try_get("video_profile").ok(),
        width: row.try_get("width").ok(),
        height: row.try_get("height").ok(),
        frame_rate: row.try_get("frame_rate").ok(),
        video_bitrate: row.try_get("video_bitrate").ok(),
        bit_depth: row.try_get("bit_depth").ok(),
        hdr_type: row.try_get("hdr_type").ok(),
        audio_codec: row.try_get("audio_codec").ok(),
        audio_channels: row.try_get("audio_channels").ok(),
        audio_sample_rate: row.try_get("audio_sample_rate").ok(),
        audio_bitrate: row.try_get("audio_bitrate").ok(),
        audio_language: row.try_get("audio_language").ok(),
        audio_tracks: row
            .try_get::<String, _>("audio_tracks")
            .ok()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default(),
        subtitle_tracks: row
            .try_get::<String, _>("subtitle_tracks")
            .ok()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default(),
    }
}

pub async fn probe_file(ffprobe_path: &Path, path: &str) -> AppResult<TechnicalMetadata> {
    let output = Command::new(ffprobe_path)
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            path,
        ])
        .output()
        .await
        .map_err(|_| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "FFPROBE_UNAVAILABLE",
                "FFprobe não está disponível.",
            )
        })?;
    if !output.status.success() {
        return Err(AppError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "FFPROBE_FAILED",
            "Não foi possível analisar o arquivo.",
        ));
    }
    parse_ffprobe(
        &serde_json::from_slice(&output.stdout)
            .map_err(|_| AppError::config("Invalid FFprobe response"))?,
    )
}
pub fn parse_ffprobe(value: &Value) -> AppResult<TechnicalMetadata> {
    let streams = value["streams"].as_array().cloned().unwrap_or_default();
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");
    let ratio = |v: Option<&str>| {
        v.and_then(|x| {
            let mut p = x.split('/');
            let a = p.next()?.parse::<f64>().ok()?;
            let b = p.next()?.parse::<f64>().ok()?;
            (b != 0.0).then_some(a / b)
        })
    };
    let tracks = |kind: &str| {
        streams
            .iter()
            .filter(|s| s["codec_type"] == kind)
            .cloned()
            .collect()
    };
    Ok(TechnicalMetadata {
        container: value["format"]["format_name"]
            .as_str()
            .and_then(|v| v.split(',').next())
            .map(String::from),
        duration_ms: value["format"]["duration"]
            .as_str()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| (v * 1000.0) as i64),
        overall_bitrate: value["format"]["bit_rate"]
            .as_str()
            .and_then(|v| v.parse().ok()),
        video_codec: video
            .and_then(|v| v["codec_name"].as_str())
            .map(String::from),
        video_profile: video.and_then(|v| v["profile"].as_str()).map(String::from),
        width: video.and_then(|v| v["width"].as_i64()),
        height: video.and_then(|v| v["height"].as_i64()),
        frame_rate: ratio(video.and_then(|v| v["avg_frame_rate"].as_str())),
        video_bitrate: video
            .and_then(|v| v["bit_rate"].as_str())
            .and_then(|v| v.parse().ok()),
        bit_depth: video
            .and_then(|v| v["bits_per_raw_sample"].as_str())
            .and_then(|v| v.parse().ok()),
        hdr_type: video
            .and_then(|v| v["color_transfer"].as_str())
            .filter(|v| matches!(*v, "smpte2084" | "arib-std-b67"))
            .map(|v| if v == "smpte2084" { "HDR10" } else { "HLG" }.into()),
        audio_codec: audio
            .and_then(|v| v["codec_name"].as_str())
            .map(String::from),
        audio_channels: audio.and_then(|v| v["channels"].as_i64()),
        audio_sample_rate: audio
            .and_then(|v| v["sample_rate"].as_str())
            .and_then(|v| v.parse().ok()),
        audio_bitrate: audio
            .and_then(|v| v["bit_rate"].as_str())
            .and_then(|v| v.parse().ok()),
        audio_language: audio
            .and_then(|v| v["tags"]["language"].as_str())
            .map(String::from),
        audio_tracks: tracks("audio"),
        subtitle_tracks: tracks("subtitle"),
    })
}

#[derive(Deserialize)]
struct StreamQuery {
    token: String,
}
async fn stream_session(state: &AppState, id: &str, token: &str) -> AppResult<sqlx::any::AnyRow> {
    let row=sqlx::query("SELECT s.*,f.absolute_path,f.file_size,f.storage_kind,r.stream_ref AS remote_stream_ref,r.stream_sealed AS remote_stream_sealed,r.external_key AS remote_external_key,r.provider_type AS remote_provider_type,r.updated_at AS remote_updated_at FROM playback_sessions s JOIN media_files f ON f.id=s.media_file_id LEFT JOIN remote_media_sources r ON r.id=f.remote_media_source_id WHERE s.id=? AND s.status IN ('STARTING','PLAYING','PAUSED')").bind(id).fetch_optional(&state.database().await.pool).await?.ok_or_else(||AppError::not_found("PLAYBACK_SESSION_EXPIRED","Sessão de reprodução expirada."))?;
    if row.get::<String, _>("stream_token_hash") != hash_token(token) {
        return Err(AppError::unauthorized());
    }
    let last_activity = row.get::<String, _>("last_activity_at");
    let expired = chrono::DateTime::parse_from_rfc3339(&last_activity)
        .ok()
        .is_none_or(|time| {
            chrono::Utc::now()
                .signed_duration_since(time.with_timezone(&chrono::Utc))
                .num_seconds()
                > state.config.token_ttl_seconds.max(300)
        });
    if expired {
        return Err(AppError::not_found(
            "PLAYBACK_SESSION_EXPIRED",
            "Sessão de reprodução expirada.",
        ));
    }
    Ok(row)
}

async fn direct(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let row = stream_session(&state, &id, &q.token).await?;
    let path: String = row.get("absolute_path");
    let size = row.get::<i64, _>("file_size") as u64;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|v| parse_range(v, size))
        .transpose()?;
    let (start, end, status) = range
        .map(|(a, b)| (a, b, StatusCode::PARTIAL_CONTENT))
        .unwrap_or((0, size.saturating_sub(1), StatusCode::OK));
    let length = end - start + 1;
    let mut file = fs::File::open(&path).await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let stream = ReaderStream::new(file.take(length));
    sqlx::query(
        "UPDATE playback_sessions SET bytes_served=bytes_served+?,last_activity_at=? WHERE id=?",
    )
    .bind(length as i64)
    .bind(now())
    .bind(&id)
    .execute(&state.database().await.pool)
    .await?;
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    let h = response.headers_mut();
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    if status == StatusCode::PARTIAL_CONTENT {
        h.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")).unwrap(),
        );
    }
    Ok(response)
}
/// Proxies a remote-source stream. The client's `Range` header is forwarded to
/// the origin and the origin's `Content-Range`/`Content-Length`/`Content-Type`
/// are passed straight back, so seeking never downloads the whole file. The
/// origin URL — which may embed credentials — is resolved server-side and never
/// reaches the client.
async fn remote_stream(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let row = stream_session(&state, &id, &q.token).await?;
    if row.try_get::<String, _>("storage_kind").ok().as_deref() != Some("REMOTE") {
        return Err(AppError::not_found(
            "PLAYBACK_SESSION_EXPIRED",
            "Sessão de reprodução expirada.",
        ));
    }
    let sealed: String = row.try_get("remote_stream_ref").map_err(|_| {
        AppError::not_found(
            "REMOTE_SOURCE_REMOVED",
            "Este conteúdo pertence a uma fonte remota que foi removida.",
        )
    })?;
    let reference = if row.try_get::<i64, _>("remote_stream_sealed").unwrap_or(0) != 0 {
        secrets::open(&state.config, &sealed)?
    } else {
        sealed
    };
    // Google Drive references resolve to an authenticated download URL; the
    // access token is attached server-side and never leaves the process.
    let (origin, bearer) = if reference.starts_with("drive:") {
        let db = state.database().await;
        let (url, token) =
            crate::features::remote_sources::google_drive::resolve_stream(&state, &db, &reference)
                .await?;
        (url, Some(token))
    } else {
        (reference, None)
    };

    let range_header = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let cache_key = crate::features::remote_sources::cache::RemoteCache::key(
        &row.try_get::<String, _>("remote_provider_type")
            .unwrap_or_default(),
        &row.try_get::<String, _>("remote_external_key")
            .unwrap_or_default(),
        &row.try_get::<String, _>("remote_updated_at")
            .unwrap_or_default(),
    );
    if let Some(path) = state.remote_cache.get(&cache_key).await {
        touch_session(&state, &id).await;
        return cached_file_response(&path, range_header.as_deref()).await;
    }

    let _permit = state.remote_http_slots.acquire().await.ok();
    let client = reqwest::Client::builder()
        .user_agent(concat!("MyLib/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| AppError::config("Unable to initialize HTTP client."))?;
    let mut request = client.get(&origin);
    if let Some(token) = &bearer {
        request = request.bearer_auth(token);
    }
    if let Some(range) = &range_header {
        request = request.header(reqwest::header::RANGE, range);
    }
    let upstream = request.send().await.map_err(|error| {
        tracing::warn!(session_id = %id, %error, "remote stream proxy failed");
        AppError::new(
            StatusCode::BAD_GATEWAY,
            "REMOTE_STREAM_UNAVAILABLE",
            "A origem remota está indisponível.",
        )
    })?;
    if !upstream.status().is_success() {
        return Err(AppError::new(
            StatusCode::BAD_GATEWAY,
            "REMOTE_STREAM_UNAVAILABLE",
            "A origem remota retornou um erro.",
        ));
    }
    let status = StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::OK);
    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_length = upstream
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    touch_session(&state, &id).await;

    // A full response (no client range) small enough to keep is buffered once and
    // shared with later viewers; anything else streams straight through with the
    // origin's range headers so seeking never pulls the whole file.
    if range_header.is_none()
        && status == StatusCode::OK
        && content_length.is_some_and(|length| length <= 64 * 1024 * 1024)
    {
        let bytes = upstream.bytes().await.map_err(|_| {
            AppError::new(
                StatusCode::BAD_GATEWAY,
                "REMOTE_STREAM_UNAVAILABLE",
                "A origem remota foi interrompida.",
            )
        })?;
        let _ = state.remote_cache.put(&cache_key, &bytes).await;
        let mut response = Response::new(Body::from(bytes));
        if let Some(content_type) = content_type
            && let Ok(value) = HeaderValue::from_str(&content_type)
        {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
        response
            .headers_mut()
            .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        return Ok(response);
    }

    let mut builder = Response::builder().status(status);
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            builder = builder.header(name, value);
        }
    }
    let body = Body::from_stream(upstream.bytes_stream().map_err(std::io::Error::other));
    builder
        .body(body)
        .map_err(|_| AppError::config("Unable to build the remote stream response."))
}

async fn touch_session(state: &AppState, session_id: &str) {
    let _ = sqlx::query("UPDATE playback_sessions SET last_activity_at=? WHERE id=?")
        .bind(now())
        .bind(session_id)
        .execute(&state.database().await.pool)
        .await;
}

/// Serves a byte range out of a locally cached remote payload.
async fn cached_file_response(path: &Path, range: Option<&str>) -> AppResult<Response> {
    let size = fs::metadata(path).await?.len();
    let parsed = range.map(|value| parse_range(value, size)).transpose()?;
    let (start, end, status) = parsed
        .map(|(a, b)| (a, b, StatusCode::PARTIAL_CONTENT))
        .unwrap_or((0, size.saturating_sub(1), StatusCode::OK));
    let length = end - start + 1;
    let mut file = fs::File::open(path).await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut response = Response::new(Body::from_stream(ReaderStream::new(file.take(length))));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(value) = HeaderValue::from_str(&length.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    }
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    if let Ok(value) = HeaderValue::from_str(&mime) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    if status == StatusCode::PARTIAL_CONTENT
        && let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{size}"))
    {
        headers.insert(header::CONTENT_RANGE, value);
    }
    Ok(response)
}

pub fn parse_range(value: &str, size: u64) -> AppResult<(u64, u64)> {
    let spec = value
        .strip_prefix("bytes=")
        .ok_or_else(|| AppError::validation("INVALID_RANGE", "Range inválido."))?;
    if spec.contains(',') {
        return Err(AppError::new(
            StatusCode::RANGE_NOT_SATISFIABLE,
            "INVALID_RANGE",
            "Múltiplos ranges não são suportados.",
        ));
    }
    let (a, b) = spec
        .split_once('-')
        .ok_or_else(|| AppError::validation("INVALID_RANGE", "Range inválido."))?;
    let (start, end) = if a.is_empty() {
        let suffix = b
            .parse::<u64>()
            .map_err(|_| AppError::validation("INVALID_RANGE", "Range inválido."))?
            .min(size);
        (size - suffix, size - 1)
    } else {
        let start = a
            .parse::<u64>()
            .map_err(|_| AppError::validation("INVALID_RANGE", "Range inválido."))?;
        let end = if b.is_empty() {
            size - 1
        } else {
            b.parse::<u64>()
                .map_err(|_| AppError::validation("INVALID_RANGE", "Range inválido."))?
                .min(size - 1)
        };
        (start, end)
    };
    if size == 0 || start > end || start >= size {
        return Err(AppError::new(
            StatusCode::RANGE_NOT_SATISFIABLE,
            "INVALID_RANGE",
            "Range fora do arquivo.",
        ));
    }
    Ok((start, end))
}

async fn master(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<StreamQuery>,
) -> AppResult<Response> {
    let row = stream_session(&state, &id, &q.token).await?;
    let key: String = row
        .try_get("pipeline_key")
        .map_err(|_| AppError::not_found("STREAM_NOT_FOUND", "Stream indisponível."))?;
    let directory = state.playback.cache_dir.join(&key);
    let path = directory.join("index.m3u8");
    let mut playlist = None;
    for _ in 0..150 {
        if let Ok(candidate) = fs::read_to_string(&path).await
            && let Some(segment) = first_playlist_segment(&candidate)
            && directory.join(segment).exists()
        {
            playlist = Some(candidate);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let playlist = playlist.ok_or_else(|| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "TRANSCODE_STARTING",
            "Transcodificação ainda está iniciando.",
        )
    })?;
    let rendered = playlist
        .lines()
        .map(|line| {
            if line.ends_with(".ts") {
                format!("segments/{line}?token={}", q.token)
            } else {
                line.into()
            }
        })
        .collect::<Vec<String>>()
        .join("\n");
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.apple.mpegurl"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        rendered,
    )
        .into_response())
}

fn first_playlist_segment(playlist: &str) -> Option<&str> {
    playlist
        .lines()
        .map(str::trim)
        .find(|line| line.ends_with(".ts") && !line.contains('/') && !line.contains('\\'))
}

async fn segment(
    State(state): State<AppState>,
    AxumPath((id, segment)): AxumPath<(String, String)>,
    Query(q): Query<StreamQuery>,
) -> AppResult<Response> {
    if !segment.starts_with("segment")
        || !segment.ends_with(".ts")
        || segment.contains('/')
        || segment.contains('\\')
    {
        return Err(AppError::not_found(
            "SEGMENT_NOT_FOUND",
            "Segmento não encontrado.",
        ));
    }
    let row = stream_session(&state, &id, &q.token).await?;
    let key: String = row
        .try_get("pipeline_key")
        .map_err(|_| AppError::not_found("SEGMENT_NOT_FOUND", "Segmento não encontrado."))?;
    let bytes = fs::read(state.playback.cache_dir.join(key).join(segment))
        .await
        .map_err(|_| {
            AppError::not_found("SEGMENT_NOT_FOUND", "Segmento ainda não está disponível.")
        })?;
    Ok((
        [
            (header::CONTENT_TYPE, "video/mp2t"),
            (header::CACHE_CONTROL, "public, max-age=21600"),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgressRequest {
    position_ms: i64,
    duration_ms: i64,
    state: String,
    #[serde(default)]
    buffer_events: i64,
}
async fn progress(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
    Json(p): Json<ProgressRequest>,
) -> AppResult<Json<Value>> {
    auth.require("playback.view_own")?;
    if p.position_ms < 0
        || p.duration_ms < 0
        || p.position_ms > p.duration_ms.saturating_add(30_000)
    {
        return Err(AppError::validation(
            "INVALID_PROGRESS",
            "Posição de reprodução inválida.",
        ));
    }
    let db = state.database().await;
    let profile_id = auth.require_profile()?.to_owned();
    let row=sqlx::query("SELECT media_item_id,episode_id,position_ms FROM playback_sessions WHERE id=? AND user_id=? AND profile_id=? AND status<>'ENDED'").bind(&id).bind(&auth.id).bind(&profile_id).fetch_optional(&db.pool).await?.ok_or_else(||AppError::not_found("PLAYBACK_SESSION_NOT_FOUND","Sessão não encontrada."))?;
    let media: String = row.get("media_item_id");
    let episode: Option<String> = row.try_get("episode_id").ok();
    let previous: i64 = row.get("position_ms");
    let key = episode.clone().unwrap_or_else(|| media.clone());
    let percentage = if p.duration_ms > 0 {
        p.position_ms as f64 * 100.0 / p.duration_ms as f64
    } else {
        0.0
    };
    let complete =
        percentage >= state.config.playback_completion_percent as f64 || p.state == "ENDED";
    let previous_percentage = if p.duration_ms > 0 {
        previous as f64 * 100.0 / p.duration_ms as f64
    } else {
        0.0
    };
    upsert_progress(
        &db.pool,
        &auth.id,
        &profile_id,
        &media,
        episode.as_deref(),
        &key,
        p.position_ms,
        p.duration_ms,
        percentage,
        complete,
    )
    .await?;
    upsert_history(
        &db.pool,
        &auth.id,
        &profile_id,
        &media,
        episode.as_deref(),
        &key,
        p.position_ms.saturating_sub(previous).clamp(0, 30_000),
        complete,
        false,
    )
    .await?;
    let status = match p.state.as_str() {
        "PAUSED" => "PAUSED",
        "ENDED" => "ENDED",
        _ => "PLAYING",
    };
    sqlx::query("UPDATE playback_sessions SET position_ms=?,duration_ms=?,last_activity_at=?,status=?,buffer_events=buffer_events+? WHERE id=?").bind(p.position_ms).bind(p.duration_ms).bind(now()).bind(status).bind(p.buffer_events).bind(&id).execute(&db.pool).await?;
    if complete {
        tracing::info!(session_id=%id,"PLAYBACK_COMPLETED");
        if p.state == "ENDED" {
            db.audit(
                Some(&auth.id),
                "PLAYBACK_COMPLETED",
                "playback_session",
                Some(&id),
                json!({"mediaItemId":media}),
                None,
            )
            .await?;
        }
    }
    let previous_milestone = (previous_percentage / 25.0).floor() as i64;
    let current_milestone = (percentage / 25.0).floor() as i64;
    let recommendations_invalidated =
        complete || (current_milestone >= 1 && current_milestone != previous_milestone);
    if recommendations_invalidated {
        state.recommendations.invalidate_profile(&profile_id).await;
    }
    Ok(Json(
        json!({"saved":true,"completed":complete,"percentage":percentage,"recommendationsInvalidated":recommendations_invalidated}),
    ))
}
async fn stop(
    State(state): State<AppState>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Json<Value>> {
    let db = state.database().await;
    let profile_id = auth.require_profile()?.to_owned();
    let can_manage = auth.is_admin() && !auth.profile_is_kids;
    let row = sqlx::query(
        "SELECT pipeline_key,user_id,profile_id FROM playback_sessions WHERE id=? AND (profile_id=? OR ?=1)",
    )
    .bind(&id)
    .bind(&profile_id)
    .bind(if can_manage { 1 } else { 0 })
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::not_found("PLAYBACK_SESSION_NOT_FOUND", "Sessão não encontrada."))?;
    let owner: String = row.get("user_id");
    let owner_profile: String = row.get("profile_id");
    if owner_profile != profile_id {
        auth.require("playback.sessions.manage")?;
    }
    sqlx::query(
        "UPDATE playback_sessions SET status='ENDED',ended_at=?,last_activity_at=? WHERE id=?",
    )
    .bind(now())
    .bind(now())
    .bind(&id)
    .execute(&db.pool)
    .await?;
    if let Ok(key) = row.try_get::<String, _>("pipeline_key") {
        state.playback.release(&key).await;
    }
    if owner_profile != profile_id {
        db.audit(
            Some(&auth.id),
            "PLAYBACK_SESSION_TERMINATED_BY_ADMIN",
            "playback_session",
            Some(&id),
            json!({"ownerUserId":owner}),
            None,
        )
        .await?;
    }
    tracing::info!(session_id=%id,"PLAYBACK_STOPPED");
    Ok(Json(json!({"stopped":true})))
}

#[allow(clippy::too_many_arguments)]
async fn upsert_progress(
    pool: &sqlx::AnyPool,
    user: &str,
    profile: &str,
    media: &str,
    episode: Option<&str>,
    key: &str,
    position: i64,
    duration: i64,
    percentage: f64,
    complete: bool,
) -> AppResult<()> {
    let completed = complete.then(now);
    let exists =
        sqlx::query("SELECT id FROM playback_progress WHERE profile_id=? AND content_key=?")
            .bind(profile)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    if exists.is_some() {
        sqlx::query("UPDATE playback_progress SET position_ms=?,duration_ms=?,percentage=?,updated_at=?,completed_at=? WHERE profile_id=? AND content_key=?").bind(position).bind(duration).bind(percentage).bind(now()).bind(completed).bind(profile).bind(key).execute(pool).await?;
    } else {
        sqlx::query("INSERT INTO playback_progress (id,user_id,profile_id,media_item_id,episode_id,content_key,position_ms,duration_ms,percentage,updated_at,completed_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)").bind(Uuid::new_v4().to_string()).bind(user).bind(profile).bind(media).bind(episode).bind(key).bind(position).bind(duration).bind(percentage).bind(now()).bind(completed).execute(pool).await?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn upsert_history(
    pool: &sqlx::AnyPool,
    user: &str,
    profile: &str,
    media: &str,
    episode: Option<&str>,
    key: &str,
    watch: i64,
    complete: bool,
    new_session: bool,
) -> AppResult<()> {
    let exists =
        sqlx::query("SELECT id FROM playback_history WHERE profile_id=? AND content_key=?")
            .bind(profile)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    if exists.is_some() {
        sqlx::query("UPDATE playback_history SET last_watched_at=?,completed=?,watch_time_ms=watch_time_ms+?,session_count=session_count+? WHERE profile_id=? AND content_key=?").bind(now()).bind(if complete{1}else{0}).bind(watch.max(0)).bind(if new_session{1}else{0}).bind(profile).bind(key).execute(pool).await?;
    } else {
        let timestamp = now();
        sqlx::query("INSERT INTO playback_history (id,user_id,profile_id,media_item_id,episode_id,content_key,started_at,last_watched_at,completed,watch_time_ms,session_count) VALUES (?,?,?,?,?,?,?,?,?,?,1)").bind(Uuid::new_v4().to_string()).bind(user).bind(profile).bind(media).bind(episode).bind(key).bind(&timestamp).bind(&timestamp).bind(if complete{1}else{0}).bind(watch.max(0)).execute(pool).await?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageQuery {
    page: Option<i64>,
    page_size: Option<i64>,
    completed: Option<bool>,
}
async fn continue_watching(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    auth.require("playback.view_own")?;
    let profile_id = auth.require_profile()?;
    let rows=sqlx::query("SELECT p.media_item_id,p.episode_id,p.position_ms,p.duration_ms,p.percentage,p.updated_at,m.title,m.poster_path,m.backdrop_path,m.media_type,e.name AS episode_name,e.season_number,e.episode_number,e.still_path FROM playback_progress p JOIN media_items m ON m.id=p.media_item_id JOIN libraries l ON l.id=m.library_id JOIN profiles pr ON pr.id=p.profile_id JOIN profile_library_access pla ON pla.profile_id=pr.id AND pla.library_id=l.id AND pla.is_allowed=1 LEFT JOIN tv_episodes e ON e.id=p.episode_id WHERE p.profile_id=? AND p.position_ms>0 AND p.completed_at IS NULL AND l.minimum_age<=pr.max_age_rating AND (m.content_age_rating IS NOT NULL AND m.content_age_rating<=pr.max_age_rating OR m.content_age_rating IS NULL AND (pr.is_kids=0 OR EXISTS(SELECT 1 FROM parental_control_settings pcs WHERE pcs.id=1 AND pcs.unknown_kids_policy='ALLOW'))) ORDER BY p.updated_at DESC LIMIT 30").bind(profile_id).fetch_all(&state.database().await.pool).await?;
    Ok(Json(
        json!({"items":rows.iter().map(progress_json).collect::<Vec<_>>() }),
    ))
}
fn progress_json(r: &sqlx::any::AnyRow) -> Value {
    json!({"mediaItemId":r.get::<String,_>("media_item_id"),"episodeId":r.try_get::<String,_>("episode_id").ok(),"positionMs":r.get::<i64,_>("position_ms"),"durationMs":r.get::<i64,_>("duration_ms"),"percentage":r.get::<f64,_>("percentage"),"updatedAt":r.get::<String,_>("updated_at"),"title":r.get::<String,_>("title"),"posterPath":r.try_get::<String,_>("poster_path").ok(),"backdropPath":r.try_get::<String,_>("backdrop_path").ok(),"mediaType":r.get::<String,_>("media_type"),"episodeName":r.try_get::<String,_>("episode_name").ok(),"seasonNumber":r.try_get::<i64,_>("season_number").ok(),"episodeNumber":r.try_get::<i64,_>("episode_number").ok(),"stillPath":r.try_get::<String,_>("still_path").ok()})
}
async fn history(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    auth.require("playback.history.view_own")?;
    let profile_id = auth.require_profile()?.to_owned();
    let page = q.page.unwrap_or(1).max(1);
    let size = q.page_size.unwrap_or(20).clamp(1, 100);
    let mut sql="SELECT h.*,m.title,m.poster_path,m.media_type,e.name AS episode_name,e.season_number,e.episode_number FROM playback_history h JOIN media_items m ON m.id=h.media_item_id JOIN libraries l ON l.id=m.library_id JOIN profiles pr ON pr.id=h.profile_id JOIN profile_library_access pla ON pla.profile_id=pr.id AND pla.library_id=l.id AND pla.is_allowed=1 LEFT JOIN tv_episodes e ON e.id=h.episode_id WHERE h.profile_id=? AND l.minimum_age<=pr.max_age_rating AND (m.content_age_rating IS NOT NULL AND m.content_age_rating<=pr.max_age_rating OR m.content_age_rating IS NULL AND (pr.is_kids=0 OR EXISTS(SELECT 1 FROM parental_control_settings pcs WHERE pcs.id=1 AND pcs.unknown_kids_policy='ALLOW')))".to_string();
    if q.completed.is_some() {
        sql.push_str(" AND h.completed=?");
    }
    sql.push_str(" ORDER BY h.last_watched_at DESC LIMIT ? OFFSET ?");
    let mut query = sqlx::query(&sql).bind(&profile_id);
    if let Some(v) = q.completed {
        query = query.bind(if v { 1 } else { 0 });
    }
    let rows = query
        .bind(size)
        .bind((page - 1) * size)
        .fetch_all(&state.database().await.pool)
        .await?;
    Ok(Json(
        json!({"items":rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"mediaItemId":r.get::<String,_>("media_item_id"),"episodeId":r.try_get::<String,_>("episode_id").ok(),"title":r.get::<String,_>("title"),"completed":r.get::<i64,_>("completed")!=0,"watchTimeMs":r.get::<i64,_>("watch_time_ms"),"sessionCount":r.get::<i64,_>("session_count"),"lastWatchedAt":r.get::<String,_>("last_watched_at")})).collect::<Vec<_>>(),"page":page,"pageSize":size}),
    ))
}
async fn sessions(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("playback.sessions.view")?;
    let rows=sqlx::query("SELECT s.id,s.user_id,s.profile_id,p.name profile_name,u.username,u.display_name,m.title,m.media_type,e.name episode_name,e.season_number,e.episode_number,s.mode,s.quality_profile,s.client_name,s.ip_address,s.started_at,s.last_activity_at,s.position_ms,s.duration_ms,s.status,s.bytes_served,t.overall_bitrate,t.width,t.height FROM playback_sessions s JOIN users u ON u.id=s.user_id JOIN profiles p ON p.id=s.profile_id JOIN media_items m ON m.id=s.media_item_id LEFT JOIN tv_episodes e ON e.id=s.episode_id LEFT JOIN media_technical_metadata t ON t.media_file_id=s.media_file_id WHERE s.status<>'ENDED' ORDER BY s.started_at DESC").fetch_all(&state.database().await.pool).await?;
    Ok(Json(
        json!({"items":rows.iter().map(|r|json!({"sessionId":r.get::<String,_>("id"),"user":{"id":r.get::<String,_>("user_id"),"username":r.get::<String,_>("username"),"displayName":r.get::<String,_>("display_name")},"profile":{"id":r.get::<String,_>("profile_id"),"name":r.get::<String,_>("profile_name")},"media":{"title":r.get::<String,_>("title"),"mediaType":r.get::<String,_>("media_type"),"episode":r.try_get::<String,_>("episode_name").ok(),"seasonNumber":r.try_get::<i64,_>("season_number").ok(),"episodeNumber":r.try_get::<i64,_>("episode_number").ok()},"clientName":r.try_get::<String,_>("client_name").ok(),"ipAddress":r.try_get::<String,_>("ip_address").ok(),"playbackMode":r.get::<String,_>("mode"),"quality":r.get::<String,_>("quality_profile"),"bitrate":r.try_get::<i64,_>("overall_bitrate").ok(),"width":r.try_get::<i64,_>("width").ok(),"height":r.try_get::<i64,_>("height").ok(),"position":r.get::<i64,_>("position_ms"),"duration":r.get::<i64,_>("duration_ms"),"startedAt":r.get::<String,_>("started_at"),"lastActivityAt":r.get::<String,_>("last_activity_at"),"status":r.get::<String,_>("status")})).collect::<Vec<_>>() }),
    ))
}

async fn transcodes(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("playback.sessions.view")?;
    let snapshots = state.playback.pipeline_snapshots().await;
    let db = state.database().await;
    let mut items = Vec::new();
    for (key, viewers, runtime_status) in snapshots {
        let row = sqlx::query("SELECT m.title,s.mode,s.quality_profile,s.started_at,t.video_codec,t.width,t.height,t.overall_bitrate FROM playback_sessions s JOIN media_items m ON m.id=s.media_item_id LEFT JOIN media_technical_metadata t ON t.media_file_id=s.media_file_id WHERE s.pipeline_key=? AND s.status<>'ENDED' ORDER BY s.started_at LIMIT 1")
            .bind(&key).fetch_optional(&db.pool).await?;
        if let Some(r) = row {
            let mode: String = r.get("mode");
            items.push(json!({"pipelineId":key,"media":r.get::<String,_>("title"),"sourceCodec":r.try_get::<String,_>("video_codec").unwrap_or_else(|_|"unknown".into()),"sourceResolution":resolution(r.try_get::<i64,_>("width").ok(),r.try_get::<i64,_>("height").ok()),"targetCodec":if mode=="DIRECT_STREAM"{"copy"}else{"h264"},"targetResolution":r.get::<String,_>("quality_profile"),"qualityProfile":r.get::<String,_>("quality_profile"),"hardwareAccelerator":"CPU","speed":Value::Null,"fps":Value::Null,"bitrate":r.try_get::<i64,_>("overall_bitrate").ok(),"activeViewers":viewers,"cacheHitRate":Value::Null,"startedAt":r.get::<String,_>("started_at"),"status":runtime_status}));
        }
    }
    Ok(Json(
        json!({"items":items,"active":items.len(),"queued":state.playback.queued_count(),"limit":state.config.transcode_max_concurrent,"queueLimit":state.config.transcode_max_queue}),
    ))
}

fn resolution(width: Option<i64>, height: Option<i64>) -> String {
    match (width, height) {
        (Some(w), Some(h)) => format!("{w}x{h}"),
        _ => "—".into(),
    }
}
async fn capabilities(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("server.view")?;
    let ffmpeg = command_output(&state.config.ffmpeg_path, &["-version"]).await;
    let ffprobe = command_output(&state.config.ffprobe_path, &["-version"]).await;
    let encoders = command_output(&state.config.ffmpeg_path, &["-hide_banner", "-encoders"])
        .await
        .unwrap_or_default();
    let hardware = [
        ("NVENC", "h264_nvenc"),
        ("QUICK_SYNC", "h264_qsv"),
        ("VAAPI", "h264_vaapi"),
        ("VIDEOTOOLBOX", "h264_videotoolbox"),
    ]
    .into_iter()
    .filter(|(_, needle)| encoders.contains(needle))
    .map(|(name, _)| name)
    .collect::<Vec<_>>();
    Ok(Json(
        json!({"ffmpegAvailable":ffmpeg.is_some(),"ffprobeAvailable":ffprobe.is_some(),"ffmpegPath":state.config.ffmpeg_path.to_string_lossy(),"ffprobePath":state.config.ffprobe_path.to_string_lossy(),"hardwareAcceleration":hardware,"softwareFallback":true,"qualityProfiles":QUALITY_NAMES,"maxConcurrentTranscodes":state.config.transcode_max_concurrent}),
    ))
}
async fn command_output(command: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().await.ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
fn pipeline_key(file: &str, quality: &str, mode: &PlaybackMode) -> String {
    let mut h = Sha256::new();
    h.update(format!(
        "{file}|{quality}|{}|h264|aac|stereo|none",
        mode.as_str()
    ));
    format!("{:x}", h.finalize())
}
fn stream_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=10-19", 100).unwrap(), (10, 19));
        assert_eq!(parse_range("bytes=90-", 100).unwrap(), (90, 99));
        assert_eq!(parse_range("bytes=-10", 100).unwrap(), (90, 99));
        assert!(parse_range("bytes=100-101", 100).is_err());
    }
    #[test]
    fn decisions() {
        let m = TechnicalMetadata {
            container: Some("mp4".into()),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            width: Some(1920),
            height: Some(1080),
            ..Default::default()
        };
        let c = ClientCapabilities {
            containers: vec!["mp4".into()],
            video_codecs: vec!["h264".into()],
            audio_codecs: vec!["aac".into()],
            max_width: Some(1920),
            max_height: Some(1080),
            estimated_bandwidth_kbps: None,
            max_audio_channels: Some(2),
        };
        assert_eq!(decide_playback(&m, &c, "AUTO").0, PlaybackMode::DirectPlay);
        let mut c2 = c.clone();
        c2.containers = vec!["webm".into()];
        assert_eq!(
            decide_playback(&m, &c2, "AUTO").0,
            PlaybackMode::DirectStream
        );
        let mut opus = m.clone();
        opus.audio_codec = Some("opus".into());
        c2.audio_codecs.push("opus".into());
        assert_eq!(
            decide_playback(&opus, &c2, "AUTO").0,
            PlaybackMode::Transcode
        );
        assert_eq!(decide_playback(&m, &c, "720P").0, PlaybackMode::Transcode);
        let mut surround = m.clone();
        surround.audio_channels = Some(6);
        assert_eq!(
            decide_playback(&surround, &c2, "AUTO").0,
            PlaybackMode::Transcode
        );
    }
    #[test]
    fn parses_ffprobe_payload() {
        let value = json!({"format":{"format_name":"matroska,webm","duration":"120.5","bit_rate":"8000000"},"streams":[{"codec_type":"video","codec_name":"h264","profile":"High","width":1920,"height":1080,"avg_frame_rate":"24000/1001","bit_rate":"7600000"},{"codec_type":"audio","codec_name":"aac","channels":6,"sample_rate":"48000","bit_rate":"384000","tags":{"language":"por"}},{"codec_type":"subtitle","codec_name":"subrip","tags":{"language":"por"}}]});
        let metadata = parse_ffprobe(&value).unwrap();
        assert_eq!(metadata.container.as_deref(), Some("matroska"));
        assert_eq!(metadata.duration_ms, Some(120_500));
        assert_eq!(metadata.audio_channels, Some(6));
        assert_eq!(metadata.subtitle_tracks.len(), 1);
    }
    #[test]
    fn playlist_must_reference_a_safe_segment() {
        assert_eq!(
            first_playlist_segment("#EXTM3U\n#EXTINF:6,\nsegment00000.ts\n"),
            Some("segment00000.ts")
        );
        assert_eq!(first_playlist_segment("#EXTM3U\n#EXTINF:6,\n"), None);
        assert_eq!(first_playlist_segment("../segment00000.ts"), None);
    }
}
