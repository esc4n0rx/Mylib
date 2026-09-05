use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Any, QueryBuilder, Row};

use crate::{
    app::AppState,
    auth::AuthUser,
    db::Database,
    errors::{AppError, AppResult},
};

const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const CANDIDATE_POOL_LIMIT: i64 = 500;
const DEFAULT_LIMIT: usize = 20;

// Behaviour weights are intentionally centralized so tuning does not change the algorithm.
const FAVORITE_WEIGHT: f64 = 5.0;
const COMPLETED_WEIGHT: f64 = 4.0;
const WATCHED_HIGH_WEIGHT: f64 = 3.0;
const WATCHED_MEDIUM_WEIGHT: f64 = 1.0;
const REWATCHED_WEIGHT: f64 = 2.0;
const ABANDONED_WEIGHT: f64 = -2.0;

#[derive(Clone, Default)]
pub struct RecommendationService {
    cache: Arc<tokio::sync::RwLock<HashMap<String, (Instant, Value)>>>,
}

impl RecommendationService {
    pub fn new() -> Self {
        Self::default()
    }

    async fn get(&self, key: &str) -> Option<Value> {
        let cache = self.cache.read().await;
        cache
            .get(key)
            .filter(|(created, _)| created.elapsed() < CACHE_TTL)
            .map(|(_, value)| value.clone())
    }

    async fn put(&self, key: String, value: Value) {
        self.cache
            .write()
            .await
            .insert(key, (Instant::now(), value));
    }

    pub async fn invalidate_profile(&self, profile_id: &str) {
        let prefix = format!("{profile_id}:");
        self.cache
            .write()
            .await
            .retain(|key, _| !key.starts_with(&prefix));
    }

    pub async fn invalidate_all(&self) {
        self.cache.write().await.clear();
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/recommendations/home", get(home))
        .route("/api/v1/recommendations/for-you", get(for_you))
        .route("/api/v1/recommendations/genres", get(genres))
        .route(
            "/api/v1/recommendations/because-you-watched/{id}",
            get(because_you_watched),
        )
}

#[derive(Debug, Deserialize)]
struct RecommendationQuery {
    limit: Option<usize>,
    #[serde(rename = "type")]
    media_type: Option<String>,
}

async fn home(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    let profile_id = auth.require_profile()?;
    let key = format!("{profile_id}:home");
    if let Some(mut cached) = state.recommendations.get(&key).await {
        cached["meta"]["cacheHit"] = json!(true);
        tracing::debug!(user_id=%auth.id, "recommendation cache hit");
        return Ok(Json(cached));
    }
    let started = Instant::now();
    let db = state.database().await;
    let generated = generate_for_you(&db, &auth, DEFAULT_LIMIT, None).await?;
    let cold_start = generated.profile.interactions == 0;
    let mut sections = vec![json!({
        "key":"for_you",
        "title":if cold_start { "Descubra algo novo" } else { "Recomendado para Você" },
        "items":generated.items,
        "coldStart":cold_start
    })];
    if let Some(source) = strongest_recent_source(&db, &auth).await?
        && let Ok(items) = generate_because(&db, &auth, &source.id, 12).await
        && !items.is_empty()
    {
        sections.push(json!({"key":"because_you_watched","title":format!("Porque você assistiu {}",source.title),"sourceMediaId":source.id,"items":items}));
    }
    let value = json!({"sections":sections,"meta":{"cacheHit":false,"generatedAt":Utc::now().to_rfc3339(),"generationDurationMs":started.elapsed().as_millis(),"candidateCount":generated.candidate_count,"finalItemCount":generated.final_count}});
    tracing::info!(user_id=%auth.id,candidate_count=generated.candidate_count,final_count=generated.final_count,duration_ms=started.elapsed().as_millis(),"recommendations generated");
    state.recommendations.put(key, value.clone()).await;
    Ok(Json(value))
}

async fn for_you(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<RecommendationQuery>,
) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    let profile_id = auth.require_profile()?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 50);
    if let Some(kind) = query.media_type.as_deref()
        && !matches!(kind, "MOVIE" | "TV_SHOW")
    {
        return Err(AppError::validation(
            "INVALID_MEDIA_TYPE",
            "Media type must be MOVIE or TV_SHOW.",
        ));
    }
    let key = format!(
        "{}:for-you:{limit}:{}",
        profile_id,
        query.media_type.as_deref().unwrap_or("ALL")
    );
    if let Some(mut cached) = state.recommendations.get(&key).await {
        cached["meta"]["cacheHit"] = json!(true);
        return Ok(Json(cached));
    }
    let started = Instant::now();
    let generated = generate_for_you(
        &state.database().await,
        &auth,
        limit,
        query.media_type.as_deref(),
    )
    .await?;
    let value = json!({"items":generated.items,"affinities":generated.profile.genre_values(),"coldStart":generated.profile.interactions==0,"meta":{"cacheHit":false,"generatedAt":Utc::now().to_rfc3339(),"generationDurationMs":started.elapsed().as_millis(),"candidateCount":generated.candidate_count,"finalItemCount":generated.final_count}});
    state.recommendations.put(key, value.clone()).await;
    Ok(Json(value))
}

async fn genres(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    auth.require_profile()?;
    let generated = generate_for_you(&state.database().await, &auth, 1, None).await?;
    Ok(Json(json!(generated.profile.genre_values())))
}

async fn because_you_watched(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<RecommendationQuery>,
) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    let profile_id = auth.require_profile()?;
    let limit = query.limit.unwrap_or(12).clamp(1, 50);
    let key = format!("{profile_id}:because:{id}:{limit}");
    if let Some(mut cached) = state.recommendations.get(&key).await {
        cached["meta"]["cacheHit"] = json!(true);
        return Ok(Json(cached));
    }
    let db = state.database().await;
    let source = source_media(&db, &auth, &id).await?;
    let items = generate_because(&db, &auth, &id, limit).await?;
    let value = json!({"sourceMediaId":id,"title":format!("Porque você assistiu {}",source.title),"items":items,"meta":{"cacheHit":false,"generatedAt":Utc::now().to_rfc3339()}});
    state.recommendations.put(key, value.clone()).await;
    Ok(Json(value))
}

#[derive(Clone, Debug)]
struct Genre {
    id: String,
    name: String,
}

#[derive(Clone, Debug)]
struct Candidate {
    id: String,
    library_id: String,
    media_type: String,
    title: String,
    original_title: Option<String>,
    year: Option<i64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    rating: f64,
    popularity: f64,
    created_at: String,
    is_favorite: bool,
    completed: bool,
    session_count: i64,
    percentage: f64,
    last_interaction: Option<String>,
    seasons: Option<i64>,
    episodes: Option<i64>,
    genres: Vec<Genre>,
}

#[derive(Default, Clone)]
struct AffinityProfile {
    genres: HashMap<String, (String, f64)>,
    favorite_genres: HashMap<String, f64>,
    media_types: HashMap<String, f64>,
    interactions: usize,
}

impl AffinityProfile {
    fn genre_values(&self) -> Vec<Value> {
        let max = self
            .genres
            .values()
            .map(|(_, score)| *score)
            .fold(0.0, f64::max)
            .max(1.0);
        let mut values = self.genres.iter().map(|(id,(name,score))| json!({"genreId":id,"name":name,"score":(*score/max).clamp(0.0,1.0)})).collect::<Vec<_>>();
        values.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a["score"].as_f64().unwrap_or(0.0))
        });
        values
    }
}

struct Generated {
    items: Vec<Value>,
    profile: AffinityProfile,
    candidate_count: usize,
    final_count: usize,
}

async fn generate_for_you(
    db: &Database,
    auth: &AuthUser,
    limit: usize,
    kind: Option<&str>,
) -> AppResult<Generated> {
    let mut candidates = load_candidates(db, auth, kind).await?;
    attach_genres(db, &mut candidates).await?;
    let profile = build_profile(&candidates);
    let mut scored = candidates
        .into_iter()
        .map(|candidate| {
            let (score, reason) = recommendation_score(&candidate, &profile);
            (score, reason, candidate)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.2.title.cmp(&b.2.title)));
    let candidate_count = scored.len();
    let selected = diversify(scored, limit);
    let items = selected
        .into_iter()
        .map(|(score, reason, candidate)| card_json(&candidate, score, &reason))
        .collect::<Vec<_>>();
    let final_count = items.len();
    Ok(Generated {
        items,
        profile,
        candidate_count,
        final_count,
    })
}

async fn load_candidates(
    db: &Database,
    auth: &AuthUser,
    kind: Option<&str>,
) -> AppResult<Vec<Candidate>> {
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT mi.id,mi.library_id,mi.media_type,mi.title,mi.original_title,mi.year,mi.poster_path,mi.backdrop_path,COALESCE(mi.rating,0) rating,COALESCE(mi.popularity,0) popularity,mi.created_at,CASE WHEN uf.media_item_id IS NULL THEN 0 ELSE 1 END is_favorite,COALESCE(h.completed,0) completed,COALESCE(h.session_count,0) session_count,COALESCE(p.percentage,0) percentage,COALESCE(p.updated_at,h.last_watched_at) last_interaction,ts.number_of_seasons,ts.number_of_episodes FROM media_items mi JOIN libraries l ON l.id=mi.library_id LEFT JOIN tv_shows ts ON ts.media_item_id=mi.id LEFT JOIN user_favorites uf ON uf.media_item_id=mi.id AND uf.profile_id=",
    );
    let profile_id = auth.require_profile()?.to_owned();
    qb.push_bind(profile_id.clone());
    qb.push(" LEFT JOIN (SELECT media_item_id,MAX(completed) completed,SUM(session_count) session_count,MAX(last_watched_at) last_watched_at FROM playback_history WHERE profile_id=").push_bind(profile_id.clone()).push(" GROUP BY media_item_id) h ON h.media_item_id=mi.id LEFT JOIN (SELECT media_item_id,MAX(percentage) percentage,MAX(updated_at) updated_at FROM playback_progress WHERE profile_id=").push_bind(profile_id.clone()).push(" GROUP BY media_item_id) p ON p.media_item_id=mi.id WHERE l.deleted_at IS NULL AND l.is_active=1 AND mi.metadata_fetched_at IS NOT NULL AND EXISTS (SELECT 1 FROM media_files mf WHERE mf.media_item_id=mi.id AND mf.missing_since IS NULL)");
    if !auth.is_admin() {
        qb.push(" AND (l.privacy='PUBLIC' OR EXISTS (SELECT 1 FROM user_library_access ula WHERE ula.library_id=l.id AND ula.user_id=").push_bind(auth.id.clone()).push(" AND ula.can_view=1))");
    }
    qb.push(" AND EXISTS(SELECT 1 FROM profiles pr JOIN profile_library_access pla ON pla.profile_id=pr.id AND pla.library_id=l.id AND pla.is_allowed=1 WHERE pr.id=").push_bind(profile_id).push(" AND pr.user_id=").push_bind(auth.id.clone()).push(" AND l.minimum_age<=pr.max_age_rating AND (mi.content_age_rating IS NOT NULL AND mi.content_age_rating<=pr.max_age_rating OR mi.content_age_rating IS NULL AND (pr.is_kids=0 OR EXISTS(SELECT 1 FROM parental_control_settings pcs WHERE pcs.id=1 AND pcs.unknown_kids_policy='ALLOW'))))");
    if let Some(kind) = kind {
        qb.push(" AND mi.media_type=").push_bind(kind.to_owned());
    }
    qb.push(" ORDER BY COALESCE(mi.popularity,0) DESC,COALESCE(mi.rating,0) DESC,mi.created_at DESC LIMIT ").push_bind(CANDIDATE_POOL_LIMIT);
    let rows = qb.build().fetch_all(&db.pool).await?;
    Ok(rows
        .iter()
        .map(|r| Candidate {
            id: r.get("id"),
            library_id: r.get("library_id"),
            media_type: r.get("media_type"),
            title: r.get("title"),
            original_title: r.try_get("original_title").ok(),
            year: r.try_get("year").ok(),
            poster_path: r.try_get("poster_path").ok(),
            backdrop_path: r.try_get("backdrop_path").ok(),
            rating: r.get("rating"),
            popularity: r.get("popularity"),
            created_at: r.get("created_at"),
            is_favorite: r.get::<i64, _>("is_favorite") != 0,
            completed: r.get::<i64, _>("completed") != 0,
            session_count: r.get("session_count"),
            percentage: numeric_f64(r, "percentage"),
            last_interaction: r.try_get("last_interaction").ok(),
            seasons: r.try_get("number_of_seasons").ok(),
            episodes: r.try_get("number_of_episodes").ok(),
            genres: Vec::new(),
        })
        .collect())
}

fn numeric_f64(row: &sqlx::any::AnyRow, column: &str) -> f64 {
    row.try_get::<f64, _>(column)
        .or_else(|_| row.try_get::<i64, _>(column).map(|value| value as f64))
        .unwrap_or(0.0)
}

async fn attach_genres(db: &Database, candidates: &mut [Candidate]) -> AppResult<()> {
    if candidates.is_empty() {
        return Ok(());
    }
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT mg.media_item_id,g.id,g.name FROM media_genres mg JOIN genres g ON g.id=mg.genre_id WHERE mg.media_item_id IN (",
    );
    let mut separated = qb.separated(",");
    for c in candidates.iter() {
        separated.push_bind(c.id.clone());
    }
    separated.push_unseparated(")");
    let rows = qb.build().fetch_all(&db.pool).await?;
    let mut map: HashMap<String, Vec<Genre>> = HashMap::new();
    for r in rows {
        map.entry(r.get("media_item_id")).or_default().push(Genre {
            id: r.get("id"),
            name: r.get("name"),
        });
    }
    for candidate in candidates {
        candidate.genres = map.remove(&candidate.id).unwrap_or_default();
    }
    Ok(())
}

fn build_profile(candidates: &[Candidate]) -> AffinityProfile {
    let mut profile = AffinityProfile::default();
    for c in candidates {
        if !c.is_favorite && c.session_count == 0 && c.percentage == 0.0 {
            continue;
        }
        profile.interactions += 1;
        let mut weight = 0.0;
        if c.is_favorite {
            weight += FAVORITE_WEIGHT;
        }
        if c.completed {
            weight += COMPLETED_WEIGHT;
        } else if c.percentage >= 75.0 {
            weight += WATCHED_HIGH_WEIGHT;
        } else if c.percentage >= 25.0 {
            weight += WATCHED_MEDIUM_WEIGHT;
        }
        if c.session_count > 1 {
            weight += REWATCHED_WEIGHT * ((c.session_count - 1).min(2) as f64);
        }
        if c.session_count > 1 && c.percentage < 25.0 && !c.is_favorite {
            weight += ABANDONED_WEIGHT;
        }
        weight *= recency_multiplier(c.last_interaction.as_deref());
        *profile.media_types.entry(c.media_type.clone()).or_default() += weight;
        for genre in &c.genres {
            let entry = profile
                .genres
                .entry(genre.id.clone())
                .or_insert((genre.name.clone(), 0.0));
            entry.1 += weight;
            if c.is_favorite {
                *profile.favorite_genres.entry(genre.id.clone()).or_default() +=
                    FAVORITE_WEIGHT * recency_multiplier(c.last_interaction.as_deref());
            }
        }
    }
    profile
}

fn recency_multiplier(value: Option<&str>) -> f64 {
    let days = value
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|date| (Utc::now() - date.with_timezone(&Utc)).num_days())
        .unwrap_or(365);
    match days {
        d if d <= 7 => 1.0,
        d if d <= 30 => 0.8,
        d if d <= 90 => 0.6,
        _ => 0.4,
    }
}

fn recommendation_score(c: &Candidate, p: &AffinityProfile) -> (f64, String) {
    let genre_max = p
        .genres
        .values()
        .map(|(_, v)| *v)
        .fold(0.0, f64::max)
        .max(1.0);
    let favorite_max = p
        .favorite_genres
        .values()
        .copied()
        .fold(0.0, f64::max)
        .max(1.0);
    let type_max = p.media_types.values().copied().fold(0.0, f64::max).max(1.0);
    let (genre_affinity, best_genre) = c
        .genres
        .iter()
        .map(|g| {
            (
                p.genres
                    .get(&g.id)
                    .map(|(_, v)| (*v / genre_max).clamp(0.0, 1.0))
                    .unwrap_or(0.0),
                g.name.clone(),
            )
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((0.0, String::new()));
    let favorite_similarity = c
        .genres
        .iter()
        .map(|g| p.favorite_genres.get(&g.id).copied().unwrap_or(0.0) / favorite_max)
        .fold(0.0, f64::max)
        .clamp(0.0, 1.0);
    let rating = (c.rating / 10.0).clamp(0.0, 1.0);
    let popularity = (c.popularity / 100.0).clamp(0.0, 1.0);
    let freshness = recency_multiplier(Some(&c.created_at));
    let type_affinity =
        (p.media_types.get(&c.media_type).copied().unwrap_or(0.0) / type_max).clamp(0.0, 1.0);
    let mut score = genre_affinity * 0.35
        + favorite_similarity * 0.20
        + rating * 0.15
        + popularity * 0.10
        + freshness * 0.10
        + type_affinity * 0.10;
    if p.interactions == 0 {
        score = rating * 0.45 + popularity * 0.35 + freshness * 0.20;
    }
    if c.completed {
        score -= 0.25 + (c.session_count.saturating_sub(1).min(3) as f64 * 0.08);
    }
    if c.last_interaction
        .as_deref()
        .is_some_and(|v| recency_multiplier(Some(v)) >= 1.0)
    {
        score -= 0.15;
    }
    if c.session_count > 1 && c.percentage < 25.0 && !c.is_favorite {
        score -= 0.20;
    }
    let reason = if p.interactions == 0 {
        "Baseado em avaliações e popularidade".into()
    } else if !best_genre.is_empty() && genre_affinity > 0.0 {
        format!("Porque você gosta de {best_genre}")
    } else {
        "Baseado no seu histórico".into()
    };
    (score.clamp(0.0, 1.0), reason)
}

fn diversify(scored: Vec<(f64, String, Candidate)>, limit: usize) -> Vec<(f64, String, Candidate)> {
    let genre_cap = (limit * 3).div_ceil(5).max(2);
    let type_cap = (limit * 4).div_ceil(5).max(2);
    let decade_cap = (limit * 3).div_ceil(5).max(2);
    let mut selected = Vec::new();
    let mut deferred = Vec::new();
    let mut genres: HashMap<String, usize> = HashMap::new();
    let mut types: HashMap<String, usize> = HashMap::new();
    let mut decades: HashMap<i64, usize> = HashMap::new();
    for item in scored {
        let primary = item
            .2
            .genres
            .first()
            .map(|g| g.id.clone())
            .unwrap_or_default();
        let decade = item.2.year.unwrap_or(0) / 10;
        let genre_seen = genres.contains_key(&primary);
        if *genres.get(&primary).unwrap_or(&0) >= genre_cap
            || (genre_seen && *types.get(&item.2.media_type).unwrap_or(&0) >= type_cap)
            || (genre_seen && *decades.get(&decade).unwrap_or(&0) >= decade_cap)
        {
            deferred.push(item);
            continue;
        }
        *genres.entry(primary).or_default() += 1;
        *types.entry(item.2.media_type.clone()).or_default() += 1;
        *decades.entry(decade).or_default() += 1;
        selected.push(item);
        if selected.len() == limit {
            break;
        }
    }
    if selected.len() < limit {
        selected.extend(deferred.into_iter().take(limit - selected.len()));
    }
    selected
}

fn card_json(c: &Candidate, score: f64, reason: &str) -> Value {
    json!({"id":c.id,"title":c.title,"originalTitle":c.original_title,"year":c.year,"posterPath":c.poster_path,"backdropPath":c.backdrop_path,"rating":c.rating,"popularity":c.popularity,"genres":c.genres.iter().map(|g|json!({"id":g.id,"name":g.name})).collect::<Vec<_>>(),"mediaType":c.media_type,"libraryId":c.library_id,"addedAt":c.created_at,"isFavorite":c.is_favorite,"numberOfSeasons":c.seasons,"numberOfEpisodes":c.episodes,"recommendationScore":score,"recommendationReason":reason})
}

#[derive(Clone)]
struct SourceMedia {
    id: String,
    title: String,
}
async fn strongest_recent_source(db: &Database, auth: &AuthUser) -> AppResult<Option<SourceMedia>> {
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT mi.id,mi.title FROM playback_history h JOIN media_items mi ON mi.id=h.media_item_id JOIN libraries l ON l.id=mi.library_id WHERE h.profile_id=",
    );
    qb.push_bind(auth.profile_id.clone().unwrap_or_default())
        .push(
            " AND l.deleted_at IS NULL AND l.is_active=1 AND (h.completed=1 OR h.watch_time_ms>0)",
        );
    if !auth.is_admin() {
        qb.push(" AND (l.privacy='PUBLIC' OR EXISTS (SELECT 1 FROM user_library_access ula WHERE ula.library_id=l.id AND ula.user_id=").push_bind(auth.id.clone()).push(" AND ula.can_view=1))");
    }
    recommendation_access_filter(&mut qb, auth);
    qb.push(" ORDER BY h.completed DESC,h.last_watched_at DESC LIMIT 1");
    Ok(qb
        .build()
        .fetch_optional(&db.pool)
        .await?
        .map(|r| SourceMedia {
            id: r.get("id"),
            title: r.get("title"),
        }))
}

async fn source_media(db: &Database, auth: &AuthUser, id: &str) -> AppResult<SourceMedia> {
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT mi.id,mi.title FROM media_items mi JOIN libraries l ON l.id=mi.library_id WHERE mi.id=",
    );
    qb.push_bind(id.to_owned())
        .push(" AND l.deleted_at IS NULL AND l.is_active=1");
    if !auth.is_admin() {
        qb.push(" AND (l.privacy='PUBLIC' OR EXISTS (SELECT 1 FROM user_library_access ula WHERE ula.library_id=l.id AND ula.user_id=").push_bind(auth.id.clone()).push(" AND ula.can_view=1))");
    }
    recommendation_access_filter(&mut qb, auth);
    qb.build()
        .fetch_optional(&db.pool)
        .await?
        .map(|r| SourceMedia {
            id: r.get("id"),
            title: r.get("title"),
        })
        .ok_or_else(|| AppError::not_found("MEDIA_ITEM_NOT_FOUND", "Media item was not found."))
}

fn recommendation_access_filter(qb: &mut QueryBuilder<'_, Any>, auth: &AuthUser) {
    qb.push(" AND EXISTS(SELECT 1 FROM profiles pr JOIN profile_library_access pla ON pla.profile_id=pr.id AND pla.library_id=l.id AND pla.is_allowed=1 WHERE pr.id=")
        .push_bind(auth.profile_id.clone().unwrap_or_default())
        .push(" AND pr.user_id=").push_bind(auth.id.clone())
        .push(" AND l.minimum_age<=pr.max_age_rating AND (mi.content_age_rating IS NOT NULL AND mi.content_age_rating<=pr.max_age_rating OR mi.content_age_rating IS NULL AND (pr.is_kids=0 OR EXISTS(SELECT 1 FROM parental_control_settings pcs WHERE pcs.id=1 AND pcs.unknown_kids_policy='ALLOW'))))");
}

async fn generate_because(
    db: &Database,
    auth: &AuthUser,
    id: &str,
    limit: usize,
) -> AppResult<Vec<Value>> {
    source_media(db, auth, id).await?;
    let mut candidates = load_candidates(db, auth, None).await?;
    attach_genres(db, &mut candidates).await?;
    let source = candidates
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| {
            AppError::not_found("MEDIA_ITEM_NOT_AVAILABLE", "Media item is not available.")
        })?;
    let own: HashSet<&str> = source.genres.iter().map(|g| g.id.as_str()).collect();
    let mut scored = candidates
        .into_iter()
        .filter(|c| c.id != id)
        .map(|c| {
            let overlap = if own.is_empty() {
                0.0
            } else {
                c.genres
                    .iter()
                    .filter(|g| own.contains(g.id.as_str()))
                    .count() as f64
                    / own.len() as f64
            };
            let rating = (1.0 - (source.rating - c.rating).abs() / 10.0).max(0.0);
            let year = match (source.year, c.year) {
                (Some(a), Some(b)) => (1.0 - (a - b).abs() as f64 / 30.0).max(0.0),
                _ => 0.0,
            };
            let popularity = (c.popularity / 100.0).clamp(0.0, 1.0);
            (
                overlap * 0.55 + rating * 0.20 + year * 0.10 + popularity * 0.15,
                c,
            )
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(score, c)| card_json(&c, score, &format!("Semelhante a {}", source.title)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, genre: &str) -> Candidate {
        Candidate {
            id: id.into(),
            library_id: "library".into(),
            media_type: "MOVIE".into(),
            title: id.into(),
            original_title: None,
            year: Some(2025),
            poster_path: None,
            backdrop_path: None,
            rating: 8.0,
            popularity: 60.0,
            created_at: Utc::now().to_rfc3339(),
            is_favorite: false,
            completed: false,
            session_count: 0,
            percentage: 0.0,
            last_interaction: None,
            seasons: None,
            episodes: None,
            genres: vec![Genre {
                id: genre.into(),
                name: genre.into(),
            }],
        }
    }

    #[test]
    fn favorite_completion_and_abandonment_shape_affinity() {
        let mut favorite = candidate("favorite", "sci-fi");
        favorite.is_favorite = true;
        favorite.last_interaction = Some(Utc::now().to_rfc3339());
        let mut completed = candidate("completed", "drama");
        completed.completed = true;
        completed.percentage = 100.0;
        completed.session_count = 1;
        let mut abandoned = candidate("abandoned", "horror");
        abandoned.session_count = 3;
        abandoned.percentage = 8.0;
        let profile = build_profile(&[favorite, completed, abandoned]);
        assert!(profile.genres["sci-fi"].1 > profile.genres["drama"].1);
        assert!(profile.genres["horror"].1 < profile.genres["drama"].1);
    }

    #[test]
    fn completed_and_repeatedly_abandoned_items_are_penalized() {
        let signal = {
            let mut value = candidate("signal", "sci-fi");
            value.is_favorite = true;
            value
        };
        let profile = build_profile(&[signal]);
        let fresh = candidate("fresh", "sci-fi");
        let mut completed = fresh.clone();
        completed.completed = true;
        completed.session_count = 2;
        let mut abandoned = fresh.clone();
        abandoned.session_count = 3;
        abandoned.percentage = 5.0;
        assert!(
            recommendation_score(&fresh, &profile).0 > recommendation_score(&completed, &profile).0
        );
        assert!(
            recommendation_score(&fresh, &profile).0 > recommendation_score(&abandoned, &profile).0
        );
    }

    #[test]
    fn recency_never_erases_old_interactions() {
        let recent = Utc::now().to_rfc3339();
        let old = (Utc::now() - chrono::Duration::days(200)).to_rfc3339();
        assert_eq!(recency_multiplier(Some(&recent)), 1.0);
        assert_eq!(recency_multiplier(Some(&old)), 0.4);
    }

    #[test]
    fn ranking_keeps_minimum_genre_diversity() {
        let mut scored = Vec::new();
        for index in 0..6 {
            scored.push((
                1.0 - index as f64 * 0.01,
                "reason".into(),
                candidate(&format!("same-{index}"), "sci-fi"),
            ));
        }
        for index in 0..3 {
            scored.push((
                0.8 - index as f64 * 0.01,
                "reason".into(),
                candidate(&format!("other-{index}"), "drama"),
            ));
        }
        let selected = diversify(scored, 5);
        let genres = selected
            .iter()
            .map(|item| item.2.genres[0].id.as_str())
            .collect::<HashSet<_>>();
        assert!(genres.len() > 1);
    }
}
