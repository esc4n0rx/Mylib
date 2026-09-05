use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Any, QueryBuilder, Row};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthUser,
    db::{Database, now},
    errors::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/media/recent", get(recent))
        .route("/api/v1/media/movies", get(movies))
        .route("/api/v1/media/tv-shows", get(tv_shows))
        .route("/api/v1/media/movies/genres", get(movie_genres))
        .route("/api/v1/media/tv-shows/genres", get(tv_genres))
        .route(
            "/api/v1/media/movies/by-genre/{genre_id}",
            get(movies_by_genre),
        )
        .route(
            "/api/v1/media/tv-shows/by-genre/{genre_id}",
            get(tv_by_genre),
        )
        .route("/api/v1/media/items/{id}", get(item_details))
        .route(
            "/api/v1/media/items/{id}/favorite",
            post(add_favorite).delete(remove_favorite),
        )
        .route("/api/v1/media/favorites", get(favorites))
        .route("/api/v1/media/items/{id}/similar", get(similar))
        .route("/api/v1/media/tv-shows/{id}", get(tv_details))
        .route("/api/v1/media/tv-shows/{id}/seasons", get(seasons))
        .route(
            "/api/v1/media/tv-shows/{id}/seasons/{season}/episodes",
            get(episodes),
        )
        .route(
            "/api/v1/media/tv-shows/{id}/seasons/{season}/episodes/{episode}",
            get(episode_details),
        )
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CatalogQuery {
    library_id: Option<String>,
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_page_size")]
    page_size: i64,
    search: Option<String>,
    genre: Option<String>,
    year: Option<i64>,
    min_rating: Option<f64>,
    sort: Option<String>,
    order: Option<String>,
    favorite: Option<bool>,
    #[serde(rename = "type")]
    media_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecentQuery {
    library_id: Option<String>,
    #[serde(rename = "type")]
    media_type: Option<String>,
    #[serde(default = "default_recent_limit")]
    limit: i64,
}

#[derive(Debug, Deserialize)]
struct SimilarQuery {
    #[serde(default = "default_similar_limit")]
    limit: i64,
}

fn default_page() -> i64 {
    1
}
fn default_page_size() -> i64 {
    24
}
fn default_recent_limit() -> i64 {
    20
}
fn default_similar_limit() -> i64 {
    12
}

async fn recent(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<RecentQuery>,
) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    let db = state.database().await;
    auth.require_profile()?;
    let mut qb = base_cards_query(&auth, None);
    if let Some(id) = query.library_id {
        qb.push(" AND mi.library_id=").push_bind(id);
    }
    if let Some(kind) = query.media_type {
        validate_type(&kind)?;
        qb.push(" AND mi.media_type=").push_bind(kind);
    }
    qb.push(" ORDER BY mi.created_at DESC LIMIT ")
        .push_bind(query.limit.clamp(1, 50));
    let rows = qb.build().fetch_all(&db.pool).await?;
    Ok(Json(json!({"items": cards_json(&db, &rows).await?})))
}

async fn movies(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<CatalogQuery>,
) -> AppResult<Json<Value>> {
    catalog(state, auth, query, "MOVIE", None).await
}
async fn tv_shows(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<CatalogQuery>,
) -> AppResult<Json<Value>> {
    catalog(state, auth, query, "TV_SHOW", None).await
}
async fn movies_by_genre(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(genre): Path<String>,
    Query(query): Query<CatalogQuery>,
) -> AppResult<Json<Value>> {
    catalog(state, auth, query, "MOVIE", Some(genre)).await
}
async fn tv_by_genre(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(genre): Path<String>,
    Query(query): Query<CatalogQuery>,
) -> AppResult<Json<Value>> {
    catalog(state, auth, query, "TV_SHOW", Some(genre)).await
}

async fn catalog(
    state: AppState,
    auth: AuthUser,
    mut query: CatalogQuery,
    kind: &str,
    genre_path: Option<String>,
) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    if genre_path.is_some() {
        query.genre = genre_path;
    }
    let db = state.database().await;
    let mut count = QueryBuilder::<Any>::new(
        "SELECT COUNT(DISTINCT mi.id) FROM media_items mi JOIN libraries l ON l.id=mi.library_id WHERE l.deleted_at IS NULL AND l.is_active=1",
    );
    access_filter(&mut count, &auth);
    count.push(" AND mi.media_type=").push_bind(kind.to_owned());
    apply_filters(&mut count, &query, &auth);
    let total: i64 = count.build().fetch_one(&db.pool).await?.get(0);

    let mut qb = base_cards_query(&auth, Some(kind));
    apply_filters(&mut qb, &query, &auth);
    let order = match query.sort.as_deref() {
        Some("title") => "mi.title",
        Some("year") => "mi.year",
        Some("rating") => "mi.rating",
        Some("popularity") => "mi.popularity",
        _ => "mi.created_at",
    };
    let direction = if query.order.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    let limit = query.page_size.clamp(1, 100);
    let page = query.page.max(1);
    qb.push(" ORDER BY ")
        .push(order)
        .push(" ")
        .push(direction)
        .push(", mi.title ASC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind((page - 1) * limit);
    let rows = qb.build().fetch_all(&db.pool).await?;
    Ok(Json(
        json!({"items":cards_json(&db,&rows).await?,"page":page,"pageSize":limit,"total":total,"totalPages":if total==0 {0} else {(total+limit-1)/limit}}),
    ))
}

fn base_cards_query(auth: &AuthUser, kind: Option<&str>) -> QueryBuilder<'static, Any> {
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT mi.id,mi.library_id,mi.media_type,mi.title,mi.original_title,mi.year,mi.poster_path,mi.backdrop_path,mi.rating,mi.popularity,mi.created_at,ts.number_of_seasons,ts.number_of_episodes,CASE WHEN uf.media_item_id IS NULL THEN 0 ELSE 1 END AS is_favorite FROM media_items mi JOIN libraries l ON l.id=mi.library_id LEFT JOIN tv_shows ts ON ts.media_item_id=mi.id LEFT JOIN user_favorites uf ON uf.media_item_id=mi.id AND uf.profile_id=",
    );
    qb.push_bind(auth.profile_id.clone().unwrap_or_default());
    qb.push(" WHERE l.deleted_at IS NULL AND l.is_active=1");
    access_filter(&mut qb, auth);
    if let Some(kind) = kind {
        qb.push(" AND mi.media_type=").push_bind(kind.to_owned());
    }
    qb
}

fn access_filter(qb: &mut QueryBuilder<'_, Any>, auth: &AuthUser) {
    if !auth.is_admin() {
        qb.push(" AND (l.privacy='PUBLIC' OR EXISTS (SELECT 1 FROM user_library_access ula WHERE ula.library_id=l.id AND ula.user_id=")
            .push_bind(auth.id.clone())
            .push(" AND ula.can_view=1))");
    }
    qb.push(" AND EXISTS (SELECT 1 FROM profiles p JOIN profile_library_access pla ON pla.profile_id=p.id AND pla.library_id=l.id AND pla.is_allowed=1 WHERE p.id=")
        .push_bind(auth.profile_id.clone().unwrap_or_default())
        .push(" AND p.user_id=").push_bind(auth.id.clone())
        .push(" AND p.is_active=1 AND l.minimum_age<=p.max_age_rating AND (mi.content_age_rating IS NOT NULL AND mi.content_age_rating<=p.max_age_rating OR mi.content_age_rating IS NULL AND (p.is_kids=0 OR EXISTS(SELECT 1 FROM parental_control_settings pcs WHERE pcs.id=1 AND pcs.unknown_kids_policy='ALLOW'))))");
}

fn apply_filters(qb: &mut QueryBuilder<'_, Any>, query: &CatalogQuery, auth: &AuthUser) {
    if let Some(id) = &query.library_id {
        qb.push(" AND mi.library_id=").push_bind(id.clone());
    }
    if let Some(year) = query.year {
        qb.push(" AND mi.year=").push_bind(year);
    }
    if let Some(rating) = query.min_rating {
        qb.push(" AND mi.rating>=").push_bind(rating);
    }
    if let Some(search) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let pattern = format!("%{}%", search.to_lowercase());
        qb.push(" AND (LOWER(mi.title) LIKE ").push_bind(pattern.clone())
            .push(" OR LOWER(COALESCE(mi.original_title,'')) LIKE ").push_bind(pattern.clone())
            .push(" OR CAST(mi.year AS CHAR) LIKE ").push_bind(pattern.clone())
            .push(" OR EXISTS (SELECT 1 FROM media_genres msg JOIN genres sg ON sg.id=msg.genre_id WHERE msg.media_item_id=mi.id AND LOWER(sg.name) LIKE ").push_bind(pattern).push("))");
    }
    if let Some(genre) = &query.genre {
        qb.push(" AND EXISTS (SELECT 1 FROM media_genres mgf JOIN genres gf ON gf.id=mgf.genre_id WHERE mgf.media_item_id=mi.id AND (gf.id=")
            .push_bind(genre.clone()).push(" OR LOWER(gf.name)=LOWER(").push_bind(genre.clone()).push(")) )");
    }
    if let Some(favorite) = query.favorite {
        if favorite {
            qb.push(" AND EXISTS (SELECT 1 FROM user_favorites uff WHERE uff.media_item_id=mi.id AND uff.profile_id=").push_bind(auth.profile_id.clone().unwrap_or_default()).push(")");
        } else {
            qb.push(" AND NOT EXISTS (SELECT 1 FROM user_favorites uff WHERE uff.media_item_id=mi.id AND uff.profile_id=").push_bind(auth.profile_id.clone().unwrap_or_default()).push(")");
        }
    }
}

async fn movie_genres(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    genres(state, auth, "MOVIE").await
}
async fn tv_genres(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    genres(state, auth, "TV_SHOW").await
}
async fn genres(state: AppState, auth: AuthUser, kind: &str) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    let db = state.database().await;
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT g.id,g.name,COUNT(DISTINCT mi.id) AS item_count FROM genres g JOIN media_genres mg ON mg.genre_id=g.id JOIN media_items mi ON mi.id=mg.media_item_id JOIN libraries l ON l.id=mi.library_id WHERE l.deleted_at IS NULL AND l.is_active=1 AND mi.media_type=",
    );
    qb.push_bind(kind.to_owned());
    access_filter(&mut qb, &auth);
    qb.push(" GROUP BY g.id,g.name ORDER BY g.name");
    let rows = qb.build().fetch_all(&db.pool).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"count":r.get::<i64,_>("item_count")})).collect::<Vec<_>>())))
}

async fn item_details(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    details(&state.database().await, &auth, &id, None)
        .await
        .map(Json)
}
async fn tv_details(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    details(&state.database().await, &auth, &id, Some("TV_SHOW"))
        .await
        .map(Json)
}

async fn details(
    db: &Database,
    auth: &AuthUser,
    id: &str,
    required_type: Option<&str>,
) -> AppResult<Value> {
    auth.require("media.view")?;
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT mi.*,l.name AS library_name,l.privacy,m.runtime,m.status AS movie_status,m.tagline,m.production_companies,m.production_countries,m.spoken_languages,ts.last_air_date,ts.status AS tv_status,ts.number_of_seasons,ts.number_of_episodes,ts.creators,ts.production_companies AS tv_production_companies,CASE WHEN uf.media_item_id IS NULL THEN 0 ELSE 1 END AS is_favorite FROM media_items mi JOIN libraries l ON l.id=mi.library_id LEFT JOIN movies m ON m.media_item_id=mi.id LEFT JOIN tv_shows ts ON ts.media_item_id=mi.id LEFT JOIN user_favorites uf ON uf.media_item_id=mi.id AND uf.profile_id=",
    );
    qb.push_bind(auth.profile_id.clone().unwrap_or_default())
        .push(" WHERE mi.id=")
        .push_bind(id.to_owned())
        .push(" AND l.deleted_at IS NULL AND l.is_active=1");
    access_filter(&mut qb, auth);
    if let Some(kind) = required_type {
        qb.push(" AND mi.media_type=").push_bind(kind.to_owned());
    }
    let row =
        qb.build().fetch_optional(&db.pool).await?.ok_or_else(|| {
            AppError::not_found("MEDIA_ITEM_NOT_FOUND", "Media item was not found.")
        })?;
    let genre_map = genres_for(db, &[id.to_owned()]).await?;
    let credits = credits_for(db, id).await?;
    let media_type: String = row.get("media_type");
    let mut value = json!({
        "id":row.get::<String,_>("id"),"mediaType":media_type,"title":row.get::<String,_>("title"),"originalTitle":row.try_get::<String,_>("original_title").ok(),"overview":row.try_get::<String,_>("overview").ok(),"tagline":row.try_get::<String,_>("tagline").ok(),"year":row.try_get::<i64,_>("year").ok(),"releaseDate":row.try_get::<String,_>("release_date").ok(),"firstAirDate":row.try_get::<String,_>("release_date").ok(),"lastAirDate":row.try_get::<String,_>("last_air_date").ok(),"runtime":row.try_get::<i64,_>("runtime").ok(),"status":row.try_get::<String,_>(if media_type=="MOVIE"{"movie_status"}else{"tv_status"}).ok(),"posterPath":row.try_get::<String,_>("poster_path").ok(),"backdropPath":row.try_get::<String,_>("backdrop_path").ok(),"rating":row.try_get::<f64,_>("rating").ok(),"voteCount":row.try_get::<i64,_>("vote_count").ok(),"popularity":row.try_get::<f64,_>("popularity").ok(),"originalLanguage":row.try_get::<String,_>("original_language").ok(),"genres":genre_map.get(id).cloned().unwrap_or_default(),"cast":credits.0,"crew":credits.1,"productionCompanies":parse_json(row.try_get::<String,_>(if media_type=="MOVIE"{"production_companies"}else{"tv_production_companies"}).ok()),"productionCountries":parse_json(row.try_get::<String,_>("production_countries").ok()),"spokenLanguages":parse_json(row.try_get::<String,_>("spoken_languages").ok()),"tmdbId":row.get::<i64,_>("tmdb_id"),"metadataLanguage":row.get::<String,_>("metadata_language"),"metadataFetchedAt":row.get::<String,_>("metadata_fetched_at"),"library":{"id":row.get::<String,_>("library_id"),"name":row.get::<String,_>("library_name")},"isFavorite":row.get::<i64,_>("is_favorite")!=0,"numberOfSeasons":row.try_get::<i64,_>("number_of_seasons").ok(),"numberOfEpisodes":row.try_get::<i64,_>("number_of_episodes").ok(),"creators":parse_json(row.try_get::<String,_>("creators").ok())
    });
    if media_type == "MOVIE" {
        let files=sqlx::query("SELECT id,filename,relative_path,file_size,extension,modified_at,identification_status FROM media_files WHERE media_item_id=? AND missing_since IS NULL ORDER BY filename").bind(id).fetch_all(&db.pool).await?;
        value["files"] = json!(files.iter().map(file_json).collect::<Vec<_>>());
    }
    Ok(value)
}

async fn seasons(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    ensure_item(&state.database().await, &auth, &id, Some("TV_SHOW")).await?;
    let rows=sqlx::query("SELECT id,season_number,name,overview,poster_path,episode_count FROM tv_seasons WHERE tv_show_id=? ORDER BY season_number ASC").bind(id).fetch_all(&state.database().await.pool).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"seasonNumber":r.get::<i64,_>("season_number"),"name":r.try_get::<String,_>("name").ok().unwrap_or_else(||if r.get::<i64,_>("season_number")==0{"Especiais".into()}else{format!("Temporada {}",r.get::<i64,_>("season_number"))}),"overview":r.try_get::<String,_>("overview").ok(),"posterPath":r.try_get::<String,_>("poster_path").ok(),"episodeCount":r.try_get::<i64,_>("episode_count").ok().unwrap_or(0)})).collect::<Vec<_>>())))
}

async fn episodes(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, season)): Path<(String, i64)>,
) -> AppResult<Json<Value>> {
    ensure_item(&state.database().await, &auth, &id, Some("TV_SHOW")).await?;
    let rows=sqlx::query("SELECT e.id,e.episode_number,e.season_number,e.name,e.overview,e.air_date,e.still_path,e.rating,e.runtime,f.id AS media_file_id,f.filename,f.file_size FROM tv_episodes e LEFT JOIN media_files f ON f.tv_episode_id=e.id AND f.missing_since IS NULL WHERE e.tv_show_id=? AND e.season_number=? ORDER BY e.episode_number ASC").bind(id).bind(season).fetch_all(&state.database().await.pool).await?;
    Ok(Json(
        json!({"items":rows.iter().map(episode_json).collect::<Vec<_>>() }),
    ))
}

async fn episode_details(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, season, episode)): Path<(String, i64, i64)>,
) -> AppResult<Json<Value>> {
    ensure_item(&state.database().await, &auth, &id, Some("TV_SHOW")).await?;
    let row=sqlx::query("SELECT e.id,e.episode_number,e.season_number,e.name,e.overview,e.air_date,e.still_path,e.rating,e.runtime,f.id AS media_file_id,f.filename,f.relative_path,f.file_size,f.extension,f.modified_at,f.identification_status FROM tv_episodes e LEFT JOIN media_files f ON f.tv_episode_id=e.id AND f.missing_since IS NULL WHERE e.tv_show_id=? AND e.season_number=? AND e.episode_number=?").bind(id).bind(season).bind(episode).fetch_optional(&state.database().await.pool).await?.ok_or_else(||AppError::not_found("EPISODE_NOT_FOUND","Episode was not found."))?;
    let mut value = episode_json(&row);
    if row.try_get::<String, _>("media_file_id").is_ok() {
        value["file"] = file_json(&row);
    }
    Ok(Json(value))
}

async fn add_favorite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let db = state.database().await;
    ensure_item(&db, &auth, &id, None).await?;
    let exists: i64 =
        sqlx::query("SELECT COUNT(*) FROM user_favorites WHERE profile_id=? AND media_item_id=?")
            .bind(auth.require_profile()?)
            .bind(&id)
            .fetch_one(&db.pool)
            .await?
            .get(0);
    if exists == 0 {
        sqlx::query(
            "INSERT INTO user_favorites(id,user_id,profile_id,media_item_id,created_at) VALUES(?,?,?,?,?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&auth.id)
        .bind(auth.require_profile()?)
        .bind(&id)
        .bind(now())
        .execute(&db.pool)
        .await?;
    }
    state
        .recommendations
        .invalidate_profile(auth.require_profile()?)
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id":id,"isFavorite":true})),
    ))
}
async fn remove_favorite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let profile_id = auth.require_profile()?.to_owned();
    sqlx::query("DELETE FROM user_favorites WHERE profile_id=? AND media_item_id=?")
        .bind(&profile_id)
        .bind(id)
        .execute(&state.database().await.pool)
        .await?;
    state.recommendations.invalidate_profile(&profile_id).await;
    Ok(StatusCode::NO_CONTENT)
}
async fn favorites(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(mut query): Query<CatalogQuery>,
) -> AppResult<Json<Value>> {
    query.favorite = Some(true);
    let media_type = query.media_type.clone();
    if let Some(kind) = media_type {
        validate_type(&kind)?;
        catalog(state, auth, query, &kind, None).await
    } else {
        catalog_all(state, auth, query).await
    }
}

async fn catalog_all(
    state: AppState,
    auth: AuthUser,
    query: CatalogQuery,
) -> AppResult<Json<Value>> {
    auth.require("media.view")?;
    let db = state.database().await;
    let mut qb = base_cards_query(&auth, None);
    apply_filters(&mut qb, &query, &auth);
    let limit = query.page_size.clamp(1, 100);
    let page = query.page.max(1);
    qb.push(" ORDER BY uf.created_at DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind((page - 1) * limit);
    let rows = qb.build().fetch_all(&db.pool).await?;
    let mut count = QueryBuilder::<Any>::new(
        "SELECT COUNT(*) FROM user_favorites uf JOIN media_items mi ON mi.id=uf.media_item_id JOIN libraries l ON l.id=mi.library_id WHERE uf.profile_id=",
    );
    count
        .push_bind(auth.profile_id.clone().unwrap_or_default())
        .push(" AND l.deleted_at IS NULL AND l.is_active=1");
    access_filter(&mut count, &auth);
    let total: i64 = count.build().fetch_one(&db.pool).await?.get(0);
    Ok(Json(
        json!({"items":cards_json(&db,&rows).await?,"page":page,"pageSize":limit,"total":total,"totalPages":if total==0{0}else{(total+limit-1)/limit}}),
    ))
}

async fn similar(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<SimilarQuery>,
) -> AppResult<Json<Value>> {
    let db = state.database().await;
    ensure_item(&db, &auth, &id, None).await?;
    let current =
        sqlx::query("SELECT library_id,media_type,year,rating FROM media_items WHERE id=?")
            .bind(&id)
            .fetch_one(&db.pool)
            .await?;
    let mut qb = base_cards_query(&auth, Some(current.get::<String, _>("media_type").as_str()));
    qb.push(" AND mi.library_id=")
        .push_bind(current.get::<String, _>("library_id"))
        .push(" AND mi.id<>")
        .push_bind(id.clone())
        .push(" LIMIT 200");
    let rows = qb.build().fetch_all(&db.pool).await?;
    let mut ids = vec![id.clone()];
    ids.extend(rows.iter().map(|r| r.get::<String, _>("id")));
    let genres = genres_for(&db, &ids).await?;
    let own: HashSet<String> = genres
        .get(&id)
        .into_iter()
        .flatten()
        .filter_map(|v| v["id"].as_str().map(str::to_owned))
        .collect();
    let year = current.try_get::<i64, _>("year").ok();
    let rating = current.try_get::<f64, _>("rating").ok();
    let mut scored: Vec<(f64, &sqlx::any::AnyRow)> = rows
        .iter()
        .map(|r| {
            let rid = r.get::<String, _>("id");
            let other: HashSet<String> = genres
                .get(&rid)
                .into_iter()
                .flatten()
                .filter_map(|v| v["id"].as_str().map(str::to_owned))
                .collect();
            let overlap = if own.is_empty() {
                0.0
            } else {
                own.intersection(&other).count() as f64 / own.len() as f64
            };
            let rating_score = match (rating, r.try_get::<f64, _>("rating").ok()) {
                (Some(a), Some(b)) => (1.0 - (a - b).abs() / 10.0).max(0.0),
                _ => 0.0,
            };
            let year_score = match (year, r.try_get::<i64, _>("year").ok()) {
                (Some(a), Some(b)) => (1.0 - (a - b).abs() as f64 / 30.0).max(0.0),
                _ => 0.0,
            };
            let popularity =
                (r.try_get::<f64, _>("popularity").ok().unwrap_or(0.0) / 1000.0).min(1.0);
            (
                overlap * 0.5 + rating_score * 0.2 + year_score * 0.15 + popularity * 0.15,
                r,
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let selected: Vec<&sqlx::any::AnyRow> = scored
        .into_iter()
        .take(query.limit.clamp(1, 50) as usize)
        .map(|v| v.1)
        .collect();
    Ok(Json(json!({"items":cards_json_refs(&db,&selected).await?})))
}

async fn ensure_item(
    db: &Database,
    auth: &AuthUser,
    id: &str,
    kind: Option<&str>,
) -> AppResult<()> {
    auth.require("media.view")?;
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT COUNT(*) FROM media_items mi JOIN libraries l ON l.id=mi.library_id WHERE mi.id=",
    );
    qb.push_bind(id.to_owned())
        .push(" AND l.deleted_at IS NULL AND l.is_active=1");
    access_filter(&mut qb, auth);
    if let Some(kind) = kind {
        qb.push(" AND mi.media_type=").push_bind(kind.to_owned());
    }
    if qb.build().fetch_one(&db.pool).await?.get::<i64, _>(0) == 0 {
        Err(AppError::not_found(
            "MEDIA_ITEM_NOT_FOUND",
            "Media item was not found.",
        ))
    } else {
        Ok(())
    }
}

async fn cards_json(db: &Database, rows: &[sqlx::any::AnyRow]) -> AppResult<Vec<Value>> {
    let refs = rows.iter().collect::<Vec<_>>();
    cards_json_refs(db, &refs).await
}
async fn cards_json_refs(db: &Database, rows: &[&sqlx::any::AnyRow]) -> AppResult<Vec<Value>> {
    let ids = rows
        .iter()
        .map(|r| r.get::<String, _>("id"))
        .collect::<Vec<_>>();
    let genres = genres_for(db, &ids).await?;
    Ok(rows.iter().map(|r|{let id=r.get::<String,_>("id");json!({"id":id,"title":r.get::<String,_>("title"),"originalTitle":r.try_get::<String,_>("original_title").ok(),"year":r.try_get::<i64,_>("year").ok(),"posterPath":r.try_get::<String,_>("poster_path").ok(),"backdropPath":r.try_get::<String,_>("backdrop_path").ok(),"rating":r.try_get::<f64,_>("rating").ok(),"popularity":r.try_get::<f64,_>("popularity").ok(),"genres":genres.get(&id).cloned().unwrap_or_default(),"mediaType":r.get::<String,_>("media_type"),"libraryId":r.get::<String,_>("library_id"),"addedAt":r.get::<String,_>("created_at"),"isFavorite":r.get::<i64,_>("is_favorite")!=0,"numberOfSeasons":r.try_get::<i64,_>("number_of_seasons").ok(),"numberOfEpisodes":r.try_get::<i64,_>("number_of_episodes").ok()})}).collect())
}
async fn genres_for(db: &Database, ids: &[String]) -> AppResult<HashMap<String, Vec<Value>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut qb = QueryBuilder::<Any>::new(
        "SELECT mg.media_item_id,g.id,g.name FROM media_genres mg JOIN genres g ON g.id=mg.genre_id WHERE mg.media_item_id IN (",
    );
    let mut sep = qb.separated(",");
    for id in ids {
        sep.push_bind(id.clone());
    }
    sep.push_unseparated(") ORDER BY g.name");
    let rows = qb.build().fetch_all(&db.pool).await?;
    let mut map: HashMap<String, Vec<Value>> = HashMap::new();
    for r in rows {
        map.entry(r.get("media_item_id"))
            .or_default()
            .push(json!({"id":r.get::<String,_>("id"),"name":r.get::<String,_>("name")}));
    }
    Ok(map)
}
async fn credits_for(db: &Database, id: &str) -> AppResult<(Vec<Value>, Vec<Value>)> {
    let rows=sqlx::query("SELECT c.credit_type,c.character_name,c.job,c.department,c.credit_order,p.id,p.name,p.profile_path FROM credits c JOIN people p ON p.id=c.person_id WHERE c.media_item_id=? ORDER BY c.credit_type,c.credit_order").bind(id).fetch_all(&db.pool).await?;
    let mut cast = Vec::new();
    let mut crew = Vec::new();
    for r in rows {
        let v = json!({"id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"profilePath":r.try_get::<String,_>("profile_path").ok(),"character":r.try_get::<String,_>("character_name").ok(),"job":r.try_get::<String,_>("job").ok(),"department":r.try_get::<String,_>("department").ok()});
        if r.get::<String, _>("credit_type") == "CAST" {
            cast.push(v)
        } else {
            crew.push(v)
        }
    }
    Ok((cast, crew))
}
fn episode_json(r: &sqlx::any::AnyRow) -> Value {
    json!({"id":r.get::<String,_>("id"),"episodeNumber":r.get::<i64,_>("episode_number"),"seasonNumber":r.get::<i64,_>("season_number"),"name":r.try_get::<String,_>("name").ok(),"overview":r.try_get::<String,_>("overview").ok(),"airDate":r.try_get::<String,_>("air_date").ok(),"stillPath":r.try_get::<String,_>("still_path").ok(),"rating":r.try_get::<f64,_>("rating").ok(),"runtime":r.try_get::<i64,_>("runtime").ok(),"mediaFileId":r.try_get::<String,_>("media_file_id").ok(),"filename":r.try_get::<String,_>("filename").ok(),"fileSize":r.try_get::<i64,_>("file_size").ok()})
}
fn file_json(r: &sqlx::any::AnyRow) -> Value {
    json!({"id":r.try_get::<String,_>("media_file_id").or_else(|_|r.try_get::<String,_>("id")).ok(),"filename":r.get::<String,_>("filename"),"relativePath":r.try_get::<String,_>("relative_path").ok(),"fileSize":r.get::<i64,_>("file_size"),"extension":r.try_get::<String,_>("extension").ok(),"modifiedAt":r.try_get::<String,_>("modified_at").ok(),"identificationStatus":r.try_get::<String,_>("identification_status").ok()})
}
fn parse_json(value: Option<String>) -> Value {
    value
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_else(|| json!([]))
}
fn validate_type(value: &str) -> AppResult<()> {
    if matches!(value, "MOVIE" | "TV_SHOW") {
        Ok(())
    } else {
        Err(AppError::validation(
            "INVALID_MEDIA_TYPE",
            "Media type must be MOVIE or TV_SHOW.",
        ))
    }
}
