use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use mylib_server::{AppState, Config, build_app, db::now};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

struct TestApp {
    _dir: TempDir,
    media: TempDir,
    app: Router,
    state: AppState,
}

impl TestApp {
    async fn new() -> Self {
        Self::with_ttl(3600).await
    }
    async fn with_ttl(token_ttl_seconds: i64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        for child in ["config", "logs", "secrets"] {
            std::fs::create_dir_all(dir.path().join(child)).unwrap();
        }
        let config = Config {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            data_dir: PathBuf::from(dir.path()),
            log_level: "error".into(),
            allowed_origins: vec!["http://localhost:3000".into()],
            database_type: None,
            database_url: None,
            jwt_secret: "test-secret-with-at-least-thirty-two-characters".into(),
            token_ttl_seconds,
            tmdb_api_key: None,
            tmdb_timeout_seconds: 10,
            tmdb_max_concurrency: 4,
            scan_max_concurrent_libraries: 2,
            scan_discovery_workers: 4,
            scan_parse_workers: 8,
            scan_metadata_workers: 4,
            scan_batch_size: 250,
            transcode_max_concurrent: 2,
            transcode_max_queue: 10,
            transcode_cache_gb: 1,
            transcode_cache_ttl_seconds: 3600,
            playback_completion_percent: 92,
            ffmpeg_path: PathBuf::from("./tools/ffmpeg/ffmpeg.exe"),
            ffprobe_path: PathBuf::from("./tools/ffmpeg/ffprobe.exe"),
            remote_cache_gb: 1,
            remote_cache_ttl_seconds: 3600,
            m3u_max_bytes: 8 * 1024 * 1024,
            m3u_fetch_timeout_seconds: 5,
            remote_http_max_concurrency: 4,
            remote_sync_interval_seconds: 60,
            google_oauth_client_id: None,
            google_oauth_client_secret: None,
            google_oauth_redirect_url: None,
        };
        let state = AppState::initialize(config).await.unwrap();
        let app = build_app(state.clone()).unwrap();
        Self {
            _dir: dir,
            media,
            app,
            state,
        }
    }
    async fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let mut request = builder
            .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 42100))));
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}))
        };
        (status, value)
    }
    async fn setup_and_login(&self) -> String {
        let setup = json!({"serverName":"Test Server","database":{"type":"sqlite"},"administrator":{"username":"Admin","password":"Correct Horse Battery Staple!","displayName":"Administrator"}});
        assert_eq!(
            self.request("POST", "/api/v1/setup", Some(setup), None)
                .await
                .0,
            StatusCode::CREATED
        );
        let (status, body) = self
            .request(
                "POST",
                "/api/v1/auth/login",
                Some(json!({"username":"admin","password":"Correct Horse Battery Staple!"})),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        body["accessToken"].as_str().unwrap().into()
    }
    fn media_dir(&self, name: &str) -> PathBuf {
        let path = self.media.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    async fn request_bytes(
        &self,
        method: &str,
        uri: &str,
        body: &[u8],
        token: &str,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(body.to_vec()))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 42100))));
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value)
    }
}

async fn seed_catalog(app: &TestApp, owner: &str) -> (String, String) {
    let db = app.state.database().await;
    let timestamp = now();
    sqlx::query("INSERT INTO libraries(id,name,library_type,privacy,minimum_age,metadata_language,is_active,scan_enabled,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,1,1,?,?,?)")
        .bind("catalog-library").bind("Catálogo").bind("MOVIE").bind("PUBLIC").bind(0_i64).bind("pt-BR").bind(owner).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO profile_library_access(profile_id,library_id,is_allowed,created_at,updated_at) SELECT id,'catalog-library',1,?,? FROM profiles WHERE user_id=? AND is_active=1")
        .bind(&timestamp).bind(&timestamp).bind(owner).execute(&db.pool).await.unwrap();
    for (id, kind, tmdb, title, year, rating, popularity) in [
        (
            "movie-one",
            "MOVIE",
            101_i64,
            "Aventura Espacial",
            2024_i64,
            8.4_f64,
            90.0_f64,
        ),
        (
            "movie-two",
            "MOVIE",
            102_i64,
            "Drama Lunar",
            2022_i64,
            7.8_f64,
            70.0_f64,
        ),
        (
            "show-one",
            "TV_SHOW",
            201_i64,
            "Estação Final",
            2023_i64,
            8.1_f64,
            80.0_f64,
        ),
    ] {
        sqlx::query("INSERT INTO media_items(id,library_id,media_type,tmdb_id,title,original_title,overview,release_date,year,rating,popularity,adult,metadata_language,metadata_source,metadata_fetched_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,0,?,?,?,?,?)")
            .bind(id).bind("catalog-library").bind(kind).bind(tmdb).bind(title).bind(title).bind("Sinopse").bind(format!("{year}-01-01")).bind(year).bind(rating).bind(popularity).bind("pt-BR").bind("TMDB").bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO movies(media_item_id,runtime,status,tagline,production_companies,production_countries,spoken_languages) VALUES(?,?,'Released','Além das estrelas','[]','[]','[]'),(?,?,'Released',NULL,'[]','[]','[]')")
        .bind("movie-one").bind(126_i64).bind("movie-two").bind(110_i64).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO tv_shows(media_item_id,status,number_of_seasons,number_of_episodes,creators,production_companies) VALUES(?,'Returning Series',1,2,'[]','[]')").bind("show-one").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO genres(id,tmdb_id,name) VALUES('genre-scifi',878,'Ficção científica'),('genre-drama',18,'Drama')").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO media_genres(media_item_id,genre_id) VALUES('movie-one','genre-scifi'),('movie-two','genre-drama'),('show-one','genre-scifi')").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO tv_seasons(id,tv_show_id,tmdb_id,season_number,name,episode_count) VALUES('season-one','show-one',301,1,'Temporada 1',2)").execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO tv_episodes(id,tv_show_id,season_id,tmdb_id,season_number,episode_number,name,overview,rating,runtime) VALUES('episode-one','show-one','season-one',401,1,1,'Partida','Começo',8.0,45),('episode-two','show-one','season-one',402,1,2,'Chegada','Fim',8.2,47)").execute(&db.pool).await.unwrap();
    ("movie-one".into(), "show-one".into())
}

#[tokio::test]
async fn first_run_health_setup_login_and_me() {
    let app = TestApp::new().await;
    let (status, body) = app.request("GET", "/api/v1/setup/status", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["setupRequired"], true);
    let token = app.setup_and_login().await;
    let (_, body) = app
        .request("GET", "/api/v1/auth/me", None, Some(&token))
        .await;
    assert_eq!(body["username"], "Admin");
    assert_eq!(body["isAdmin"], true);
    assert_eq!(body["permissions"].as_array().unwrap().len(), 22);
    assert!(
        body["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "media.play")
    );
    let (status, body) = app.request("GET", "/api/v1/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["databaseType"], "sqlite");
}

#[tokio::test]
async fn operational_dashboard_is_cached_paginated_and_admin_only() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;

    let (status, health) = app
        .request("GET", "/api/v1/server/health", None, Some(&admin))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["databaseStatus"], "AVAILABLE");
    assert!(health["uptimeSeconds"].is_number());

    let (_, first) = app
        .request("GET", "/api/v1/server/metrics", None, Some(&admin))
        .await;
    let (_, second) = app
        .request("GET", "/api/v1/server/metrics", None, Some(&admin))
        .await;
    assert_eq!(first["capturedAt"], second["capturedAt"]);
    assert!(first["memoryTotalBytes"].as_u64().unwrap_or(0) > 0);

    for path in [
        "/api/v1/server/storage",
        "/api/v1/server/alerts",
        "/api/v1/jobs?page=1&pageSize=5",
        "/api/v1/activity?page=1&pageSize=5",
        "/api/v1/playback/sessions",
        "/api/v1/playback/transcodes",
        "/api/v1/playback/capabilities",
    ] {
        let (status, body) = app.request("GET", path, None, Some(&admin)).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }

    let (_, created) = app.request("POST", "/api/v1/users", Some(json!({"username":"viewer","displayName":"Viewer","password":"A strong viewer password!","libraryAccess":[]})), Some(&admin)).await;
    assert_eq!(created["username"], "viewer");
    let (_, login) = app
        .request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"username":"viewer","password":"A strong viewer password!"})),
            None,
        )
        .await;
    let viewer = login["accessToken"].as_str().unwrap();
    for path in [
        "/api/v1/server/metrics",
        "/api/v1/server/storage",
        "/api/v1/jobs",
        "/api/v1/playback/sessions",
        "/api/v1/playback/transcodes",
        "/api/v1/playback/capabilities",
    ] {
        let (status, _) = app.request("GET", path, None, Some(viewer)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
}

#[tokio::test]
async fn recommendations_are_personalized_cached_and_library_safe() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let (_, me) = app
        .request("GET", "/api/v1/auth/me", None, Some(&admin))
        .await;
    let owner = me["id"].as_str().unwrap();
    let _ = seed_catalog(&app, owner).await;
    let db = app.state.database().await;
    let timestamp = now();
    let media_root = app.media_dir("recommendations");
    sqlx::query("INSERT INTO library_paths(id,library_id,path,normalized_path,is_active,status,created_at,updated_at) VALUES('recommendation-path','catalog-library',?,?,1,'AVAILABLE',?,?)")
        .bind(media_root.to_string_lossy().to_string()).bind(media_root.to_string_lossy().to_string()).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    for (index, id) in ["movie-one", "movie-two", "show-one"].iter().enumerate() {
        let file = media_root.join(format!("{id}.mp4"));
        std::fs::write(&file, b"media").unwrap();
        sqlx::query("INSERT INTO media_files(id,library_id,library_path_id,media_item_id,absolute_path,relative_path,filename,extension,file_size,modified_at,content_type,scan_status,identification_status,created_at,updated_at,last_seen_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(format!("recommendation-file-{index}")).bind("catalog-library").bind("recommendation-path").bind(*id).bind(file.to_string_lossy().to_string()).bind(format!("{id}.mp4")).bind(format!("{id}.mp4")).bind("mp4").bind(5_i64).bind(&timestamp).bind("VIDEO").bind("PRESENT").bind("MATCHED").bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    }

    let (status, cold) = app
        .request("GET", "/api/v1/recommendations/home", None, Some(&admin))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cold["sections"][0]["coldStart"], true);
    assert_eq!(cold["sections"][0]["title"], "Descubra algo novo");
    assert_eq!(cold["sections"][0]["items"].as_array().unwrap().len(), 3);
    assert_eq!(cold["meta"]["cacheHit"], false);
    let (_, cached) = app
        .request("GET", "/api/v1/recommendations/home", None, Some(&admin))
        .await;
    assert_eq!(cached["meta"]["cacheHit"], true);

    app.request(
        "POST",
        "/api/v1/media/items/movie-one/favorite",
        None,
        Some(&admin),
    )
    .await;
    let (_, personalized) = app
        .request("GET", "/api/v1/recommendations/home", None, Some(&admin))
        .await;
    assert_eq!(personalized["meta"]["cacheHit"], false);
    assert_eq!(
        personalized["sections"][0]["title"],
        "Recomendado para Você"
    );
    let (_, affinities) = app
        .request("GET", "/api/v1/recommendations/genres", None, Some(&admin))
        .await;
    assert_eq!(affinities[0]["name"], "Ficção científica");

    let (_, because) = app
        .request(
            "GET",
            "/api/v1/recommendations/because-you-watched/movie-one?limit=10",
            None,
            Some(&admin),
        )
        .await;
    assert!(
        because["title"]
            .as_str()
            .unwrap()
            .contains("Aventura Espacial")
    );
    assert_eq!(because["items"][0]["id"], "show-one");
    assert!(
        because["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["id"] != "movie-one")
    );

    let (_, created) = app.request("POST", "/api/v1/users", Some(json!({"username":"reco-viewer","displayName":"Reco Viewer","password":"A strong recommendation password!","libraryAccess":[]})), Some(&admin)).await;
    let viewer_id = created["id"].as_str().unwrap();
    let (_, login) = app
        .request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"username":"reco-viewer","password":"A strong recommendation password!"})),
            None,
        )
        .await;
    let viewer = login["accessToken"].as_str().unwrap();
    sqlx::query("INSERT INTO libraries(id,name,library_type,privacy,minimum_age,metadata_language,is_active,scan_enabled,created_by,created_at,updated_at) VALUES('private-reco','Segredos','MOVIE','PRIVATE',0,'pt-BR',1,1,?,?,?)").bind(owner).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO library_paths(id,library_id,path,normalized_path,is_active,status,created_at,updated_at) VALUES('private-reco-path','private-reco','private','private',1,'AVAILABLE',?,?)").bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO media_items(id,library_id,media_type,tmdb_id,title,year,rating,popularity,adult,metadata_language,metadata_source,metadata_fetched_at,created_at,updated_at) VALUES('private-movie','private-reco','MOVIE',999,'Filme secreto',2026,9.9,100,0,'pt-BR','TMDB',?,?,?)").bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO media_files(id,library_id,library_path_id,media_item_id,absolute_path,relative_path,filename,extension,file_size,modified_at,content_type,scan_status,identification_status,created_at,updated_at,last_seen_at) VALUES('private-file','private-reco','private-reco-path','private-movie','private.mp4','private.mp4','private.mp4','mp4',5,?,'VIDEO','PRESENT','MATCHED',?,?,?)").bind(&timestamp).bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    app.state.recommendations.invalidate_all().await;
    let (_, hidden) = app
        .request(
            "GET",
            "/api/v1/recommendations/for-you?limit=50",
            None,
            Some(viewer),
        )
        .await;
    assert!(
        hidden["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["id"] != "private-movie")
    );
    app.request(
        "PUT",
        &format!("/api/v1/users/{viewer_id}/library-access"),
        Some(json!({"libraries":[{"libraryId":"private-reco","canView":true,"canPlay":true}]})),
        Some(&admin),
    )
    .await;
    let (_, visible) = app
        .request(
            "GET",
            "/api/v1/recommendations/for-you?limit=50",
            None,
            Some(viewer),
        )
        .await;
    assert!(
        visible["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == "private-movie")
    );
}

#[tokio::test]
async fn setup_is_single_use_and_login_does_not_enumerate_users() {
    let app = TestApp::new().await;
    app.setup_and_login().await;
    let setup = json!({"serverName":"Again","database":{"type":"sqlite"},"administrator":{"username":"other","password":"Another Secure Password!","displayName":"Other"}});
    let (status, body) = app
        .request("POST", "/api/v1/setup", Some(setup), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "SETUP_ALREADY_COMPLETED");
    for username in ["missing", "Admin"] {
        let (_, body) = app
            .request(
                "POST",
                "/api/v1/auth/login",
                Some(json!({"username":username,"password":"wrong password"})),
                None,
            )
            .await;
        assert_eq!(body["error"]["code"], "INVALID_CREDENTIALS");
    }
}

#[tokio::test]
async fn permissions_and_last_admin_are_enforced() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let user =
        json!({"username":"viewer","password":"Viewer Password 123!","displayName":"Viewer"});
    let (status, created) = app
        .request("POST", "/api/v1/users", Some(user), Some(&admin))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let viewer_id = created["id"].as_str().unwrap();
    let duplicate = json!({"username":"VIEWER","password":"Another Viewer Password!","displayName":"Duplicate"});
    let (status, body) = app
        .request("POST", "/api/v1/users", Some(duplicate), Some(&admin))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "USERNAME_ALREADY_EXISTS");
    let (_, login) = app
        .request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"username":"viewer","password":"Viewer Password 123!"})),
            None,
        )
        .await;
    let viewer = login["accessToken"].as_str().unwrap();
    let (status,_)=app.request("POST","/api/v1/users",Some(json!({"username":"denied","password":"Denied Password 123!","displayName":"Denied"})),Some(viewer)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .request(
            "POST",
            &format!("/api/v1/users/{viewer_id}/disable"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = app
        .request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"username":"viewer","password":"Viewer Password 123!"})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "INVALID_CREDENTIALS");
    assert_eq!(
        app.request(
            "POST",
            &format!("/api/v1/users/{viewer_id}/enable"),
            None,
            Some(&admin)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.request(
            "PUT",
            &format!("/api/v1/users/{viewer_id}/password"),
            Some(json!({"password":"A New Viewer Password 456!"})),
            Some(&admin)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, me) = app
        .request("GET", "/api/v1/auth/me", None, Some(&admin))
        .await;
    let admin_id = me["id"].as_str().unwrap();
    let (status, body) = app
        .request(
            "POST",
            &format!("/api/v1/users/{admin_id}/disable"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "LAST_ADMIN_PROTECTION");
    let (status, body) = app
        .request(
            "PUT",
            &format!("/api/v1/users/{admin_id}/roles"),
            Some(json!({"roles":["User"]})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "LAST_ADMIN_PROTECTION");
}

#[tokio::test]
async fn invalid_and_expired_tokens_are_rejected() {
    let app = TestApp::new().await;
    let (status, _) = app
        .request("GET", "/api/v1/auth/me", None, Some("not-a-token"))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let expired_app = TestApp::with_ttl(-1).await;
    let expired = expired_app.setup_and_login().await;
    let (status, _) = expired_app
        .request("GET", "/api/v1/auth/me", None, Some(&expired))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn libraries_validate_create_update_and_private_unlock() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let movies = app.media_dir("movies");
    let (status, validation) = app
        .request(
            "POST",
            "/api/v1/libraries/paths/validate",
            Some(json!({"path":movies})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(validation["valid"], true);

    let payload = json!({"name":"Private Movies","type":"MOVIE","privacy":"PRIVATE","password":"A strong library password!","minimumAge":18,"metadataLanguage":"pt-BR","metadataRegion":"BR","paths":[movies]});
    let (status, library) = app
        .request("POST", "/api/v1/libraries", Some(payload), Some(&admin))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{library}");
    assert!(library.get("passwordHash").is_none());
    let id = library["id"].as_str().unwrap();

    let (status, _) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{id}/unlock"),
            Some(json!({"password":"wrong password"})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, unlocked) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{id}/unlock"),
            Some(json!({"password":"A strong library password!"})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unlocked["unlocked"], true);
    assert!(unlocked["unlockToken"].as_str().is_some());

    let (status, updated) = app
        .request(
            "PATCH",
            &format!("/api/v1/libraries/{id}"),
            Some(json!({"name":"Private Cinema","minimumAge":16})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["minimumAge"], 16);
    let (status, body) = app
        .request(
            "DELETE",
            &format!("/api/v1/libraries/{id}"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["code"],
        "LIBRARY_DELETE_CONFIRMATION_REQUIRED"
    );
}

#[tokio::test]
async fn scan_is_async_incremental_and_marks_removed_files_missing() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let movies = app.media_dir("scan-movies");
    std::fs::write(movies.join("Interstellar.2014.1080p.mkv"), []).unwrap();
    std::fs::write(movies.join("poster.jpg"), []).unwrap();
    let (_, library) = app.request("POST", "/api/v1/libraries", Some(json!({"name":"Movies","type":"MOVIE","privacy":"PUBLIC","minimumAge":0,"metadataLanguage":"en-US","paths":[movies]})), Some(&admin)).await;
    let id = library["id"].as_str().unwrap();
    let (status, scan) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{id}/scan"),
            Some(json!({"scanType":"FULL"})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{scan}");
    let scan_id = scan["jobId"].as_str().unwrap();
    let (status, conflict) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{id}/scan"),
            Some(json!({"scanType":"FULL"})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "LIBRARY_SCAN_ALREADY_RUNNING");
    let mut completed = Value::Null;
    for _ in 0..100 {
        let (_, body) = app
            .request(
                "GET",
                &format!("/api/v1/libraries/{id}/scans/{scan_id}"),
                None,
                Some(&admin),
            )
            .await;
        if matches!(
            body["status"].as_str(),
            Some("COMPLETED") | Some("COMPLETED_WITH_WARNINGS") | Some("FAILED")
        ) {
            completed = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_ne!(completed, Value::Null);
    assert_ne!(completed["status"], "FAILED", "{completed}");
    assert_eq!(completed["discoveredFiles"], 1);
    assert_eq!(completed["unmatchedFiles"], 1);

    let offline = app.media.path().join("temporarily-offline");
    std::fs::rename(&movies, &offline).unwrap();
    let (_, scan) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{id}/scan"),
            Some(json!({"scanType":"INCREMENTAL"})),
            Some(&admin),
        )
        .await;
    let offline_scan = scan["jobId"].as_str().unwrap();
    for _ in 0..100 {
        let (_, body) = app
            .request(
                "GET",
                &format!("/api/v1/libraries/{id}/scans/{offline_scan}"),
                None,
                Some(&admin),
            )
            .await;
        if body["status"]
            .as_str()
            .is_some_and(|v| v.starts_with("COMPLETED"))
        {
            completed = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(completed["status"], "COMPLETED_WITH_WARNINGS");
    assert_eq!(completed["removedFiles"], 0);
    std::fs::rename(&offline, &movies).unwrap();

    std::fs::remove_file(movies.join("Interstellar.2014.1080p.mkv")).unwrap();
    let (_, scan) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{id}/scan"),
            Some(json!({"scanType":"INCREMENTAL"})),
            Some(&admin),
        )
        .await;
    let second = scan["jobId"].as_str().unwrap();
    for _ in 0..100 {
        let (_, body) = app
            .request(
                "GET",
                &format!("/api/v1/libraries/{id}/scans/{second}"),
                None,
                Some(&admin),
            )
            .await;
        if body["status"]
            .as_str()
            .is_some_and(|v| v.starts_with("COMPLETED"))
        {
            completed = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(completed["removedFiles"], 1, "{completed}");
}

#[tokio::test]
async fn content_catalog_details_tv_favorites_and_similar_work() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let (_, me) = app
        .request("GET", "/api/v1/auth/me", None, Some(&admin))
        .await;
    let (movie, show) = seed_catalog(&app, me["id"].as_str().unwrap()).await;

    let (status, recent) = app
        .request("GET", "/api/v1/media/recent?limit=2", None, Some(&admin))
        .await;
    assert_eq!(status, StatusCode::OK, "{recent}");
    assert_eq!(recent["items"].as_array().unwrap().len(), 2);
    let (_, filtered) = app
        .request(
            "GET",
            "/api/v1/media/movies?search=espacial&genre=genre-scifi&minRating=8&pageSize=1",
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["items"][0]["title"], "Aventura Espacial");
    let (_, genres) = app
        .request("GET", "/api/v1/media/movies/genres", None, Some(&admin))
        .await;
    assert!(
        genres
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["name"] == "Ficção científica")
    );

    let (_, details) = app
        .request(
            "GET",
            &format!("/api/v1/media/items/{movie}"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(details["runtime"], 126);
    assert_eq!(details["genres"][0]["name"], "Ficção científica");
    let (_, seasons) = app
        .request(
            "GET",
            &format!("/api/v1/media/tv-shows/{show}/seasons"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(seasons[0]["episodeCount"], 2);
    let (_, episodes) = app
        .request(
            "GET",
            &format!("/api/v1/media/tv-shows/{show}/seasons/1/episodes"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(episodes["items"][1]["name"], "Chegada");

    assert_eq!(
        app.request(
            "POST",
            &format!("/api/v1/media/items/{movie}/favorite"),
            None,
            Some(&admin)
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (_, favorites) = app
        .request("GET", "/api/v1/media/favorites", None, Some(&admin))
        .await;
    assert_eq!(favorites["total"], 1);
    assert_eq!(favorites["items"][0]["isFavorite"], true);
    let (_, similar) = app
        .request(
            "GET",
            &format!("/api/v1/media/items/{movie}/similar"),
            None,
            Some(&admin),
        )
        .await;
    assert!(similar["items"].as_array().is_some());
    assert_eq!(
        app.request(
            "DELETE",
            &format!("/api/v1/media/items/{movie}/favorite"),
            None,
            Some(&admin)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, favorites) = app
        .request("GET", "/api/v1/media/favorites", None, Some(&admin))
        .await;
    assert_eq!(favorites["total"], 0);
}

#[tokio::test]
async fn playback_direct_range_progress_resume_and_history_work() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let (_, me) = app
        .request("GET", "/api/v1/auth/me", None, Some(&admin))
        .await;
    let (movie, _) = seed_catalog(&app, me["id"].as_str().unwrap()).await;
    let root = app.media_dir("playback");
    let file = root.join("sample.mp4");
    std::fs::write(&file, (0_u8..=255).collect::<Vec<_>>()).unwrap();
    let timestamp = now();
    let db = app.state.database().await;
    sqlx::query("INSERT INTO library_paths(id,library_id,path,normalized_path,is_active,status,created_at,updated_at) VALUES('playback-path','catalog-library',?,? ,1,'AVAILABLE',?,?)")
        .bind(root.to_string_lossy().to_string()).bind(root.to_string_lossy().to_string()).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO media_files(id,library_id,library_path_id,media_item_id,absolute_path,relative_path,filename,extension,file_size,modified_at,content_type,scan_status,identification_status,created_at,updated_at,last_seen_at) VALUES('playback-file','catalog-library','playback-path',?,?,?,'sample.mp4','mp4',256,?,'VIDEO','PRESENT','MATCHED',?,?,?)")
        .bind(&movie).bind(file.to_string_lossy().to_string()).bind("sample.mp4").bind(&timestamp).bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    let (status, started) = app.request("POST", "/api/v1/playback/start", Some(json!({"mediaItemId":movie,"mediaFileId":"playback-file","clientCapabilities":{"containers":["mp4"],"videoCodecs":["h264"],"audioCodecs":["aac"],"maxWidth":1920,"maxHeight":1080},"quality":"AUTO"})), Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["playbackMode"], "DIRECT_PLAY");
    let stream_url = started["streamUrl"].as_str().unwrap();
    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(stream_url)
                .header(header::RANGE, "bytes=10-19")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 10-19/256");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), &(10_u8..20).collect::<Vec<_>>());
    let session = started["sessionId"].as_str().unwrap();
    let (status, saved) = app
        .request(
            "POST",
            &format!("/api/v1/playback/{session}/progress"),
            Some(json!({"positionMs":50_000,"durationMs":100_000,"state":"PAUSED"})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (_, watching) = app
        .request(
            "GET",
            "/api/v1/playback/continue-watching",
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(watching["items"][0]["positionMs"], 50_000);
    let (_, history) = app
        .request("GET", "/api/v1/playback/history", None, Some(&admin))
        .await;
    assert_eq!(history["items"][0]["sessionCount"], 1);
}

#[tokio::test]
async fn user_library_access_controls_private_visibility_and_is_revocable() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let public_path = app.media_dir("shared-public");
    let private_path = app.media_dir("shared-private");
    let (_, public_library) = app.request("POST", "/api/v1/libraries", Some(json!({"name":"Pública","type":"MOVIE","privacy":"PUBLIC","minimumAge":0,"metadataLanguage":"pt-BR","paths":[public_path]})), Some(&admin)).await;
    let (_, private_library) = app.request("POST", "/api/v1/libraries", Some(json!({"name":"Privada","type":"TV_SHOW","privacy":"PRIVATE","password":"Private Library Password!","minimumAge":0,"metadataLanguage":"pt-BR","paths":[private_path]})), Some(&admin)).await;
    let private_id = private_library["id"].as_str().unwrap();
    let (status, created) = app.request("POST", "/api/v1/users", Some(json!({
        "username":"guest","displayName":"Convidado","email":"guest@example.com","password":"Guest Password 123!",
        "libraryAccess":[{"libraryId":private_id,"canView":true,"canPlay":true}]
    })), Some(&admin)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["libraryAccessCount"], 1);
    let user_id = created["id"].as_str().unwrap();
    let (_, login) = app
        .request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"username":"guest","password":"Guest Password 123!"})),
            None,
        )
        .await;
    let guest = login["accessToken"].as_str().unwrap();
    let (status, visible) = app
        .request("GET", "/api/v1/libraries", None, Some(guest))
        .await;
    assert_eq!(status, StatusCode::OK, "{visible}");
    assert_eq!(visible["items"].as_array().unwrap().len(), 2);
    let (_, access) = app
        .request(
            "GET",
            &format!("/api/v1/users/{user_id}/library-access"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(access["libraries"][0]["libraryId"], private_id);
    assert_eq!(
        app.request(
            "PUT",
            &format!("/api/v1/users/{user_id}/library-access"),
            Some(json!({"libraries":[]})),
            Some(&admin)
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, visible) = app
        .request("GET", "/api/v1/libraries", None, Some(guest))
        .await;
    assert_eq!(visible["items"].as_array().unwrap().len(), 1);
    assert_eq!(visible["items"][0]["id"], public_library["id"]);
    assert_eq!(
        app.request(
            "GET",
            &format!("/api/v1/libraries/{private_id}"),
            None,
            Some(guest)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, users) = app
        .request(
            "GET",
            "/api/v1/users?page=1&pageSize=10&search=guest",
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(users["total"], 1);
    assert!(users["items"][0]["lastLoginAt"].is_string());
}

#[tokio::test]
async fn profiles_support_crud_selection_pin_rate_limit_and_avatar_pagination() {
    let app = TestApp::new().await;
    let default_token = app.setup_and_login().await;
    let (_, initial) = app
        .request("GET", "/api/v1/profiles", None, Some(&default_token))
        .await;
    assert_eq!(initial["items"].as_array().unwrap().len(), 1);
    assert_eq!(initial["items"][0]["isDefault"], true);
    assert_eq!(initial["items"][0]["name"], "Administrator");

    let (status, kids) = app
        .request(
            "POST",
            "/api/v1/profiles",
            Some(json!({"name":"Kids","avatarId":"kids.png","isKids":true,"maxAgeRating":10})),
            Some(&default_token),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{kids}");
    let kids_id = kids["id"].as_str().unwrap();
    assert_eq!(kids["isKids"], true);
    assert_eq!(kids["maxAgeRating"], 10);

    assert_eq!(
        app.request(
            "PUT",
            &format!("/api/v1/profiles/{kids_id}/pin"),
            Some(json!({"pin":"1234"})),
            Some(&default_token),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    for _ in 0..5 {
        assert_eq!(
            app.request(
                "POST",
                &format!("/api/v1/profiles/{kids_id}/unlock"),
                Some(json!({"pin":"9999"})),
                Some(&default_token),
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        app.request(
            "POST",
            &format!("/api/v1/profiles/{kids_id}/unlock"),
            Some(json!({"pin":"1234"})),
            Some(&default_token),
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );

    let (status, categories) = app
        .request("GET", "/api/v1/avatars/categories", None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(categories.as_array().unwrap().len(), 5);
    let (status, avatars) = app
        .request("GET", "/api/v1/avatars?page=1&pageSize=1", None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(avatars["items"].as_array().unwrap().len(), 1);
    assert_eq!(avatars["total"], 2);
    assert_eq!(
        app.request("GET", "/api/v1/avatars?category=invalid", None, None)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn favorites_are_isolated_by_profile() {
    let app = TestApp::new().await;
    let default_token = app.setup_and_login().await;
    let (_, me) = app
        .request("GET", "/api/v1/auth/me", None, Some(&default_token))
        .await;
    let owner = me["id"].as_str().unwrap();
    let (movie, _) = seed_catalog(&app, owner).await;
    let (_, second) = app
        .request(
            "POST",
            "/api/v1/profiles",
            Some(json!({"name":"Second","isKids":false,"maxAgeRating":18})),
            Some(&default_token),
        )
        .await;
    let second_id = second["id"].as_str().unwrap();
    let (status, selected) = app
        .request(
            "POST",
            &format!("/api/v1/profiles/{second_id}/select"),
            Some(json!({})),
            Some(&default_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    let second_token = selected["accessToken"].as_str().unwrap();
    assert_eq!(
        app.request(
            "POST",
            &format!("/api/v1/media/items/{movie}/favorite"),
            None,
            Some(second_token),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (_, default_favorites) = app
        .request("GET", "/api/v1/media/favorites", None, Some(&default_token))
        .await;
    let (_, second_favorites) = app
        .request("GET", "/api/v1/media/favorites", None, Some(second_token))
        .await;
    assert_eq!(default_favorites["items"].as_array().unwrap().len(), 0);
    assert_eq!(second_favorites["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn kids_profile_filters_age_unknown_content_and_administration() {
    let app = TestApp::new().await;
    let adult = app.setup_and_login().await;
    let (_, me) = app
        .request("GET", "/api/v1/auth/me", None, Some(&adult))
        .await;
    let owner = me["id"].as_str().unwrap();
    seed_catalog(&app, owner).await;
    let db = app.state.database().await;
    sqlx::query("UPDATE media_items SET content_age_rating=10 WHERE id IN('movie-one','show-one')")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE media_items SET content_age_rating=16 WHERE id='movie-two'")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO media_items(id,library_id,media_type,tmdb_id,title,adult,metadata_language,metadata_source,metadata_fetched_at,created_at,updated_at) VALUES('movie-unknown','catalog-library','MOVIE',999,'Sem classificação',0,'pt-BR','TMDB',?,?,?)")
        .bind(now()).bind(now()).bind(now()).execute(&db.pool).await.unwrap();

    let (_, kids) = app
        .request(
            "POST",
            "/api/v1/profiles",
            Some(json!({"name":"Kids","isKids":true,"maxAgeRating":10})),
            Some(&adult),
        )
        .await;
    let (_, selected) = app
        .request(
            "POST",
            &format!("/api/v1/profiles/{}/select", kids["id"].as_str().unwrap()),
            Some(json!({})),
            Some(&adult),
        )
        .await;
    let kids_token = selected["accessToken"].as_str().unwrap();
    let (status, movies) = app
        .request("GET", "/api/v1/media/movies", None, Some(kids_token))
        .await;
    assert_eq!(status, StatusCode::OK, "{movies}");
    assert_eq!(movies["items"].as_array().unwrap().len(), 1);
    assert_eq!(movies["items"][0]["id"], "movie-one");
    assert_eq!(
        app.request("GET", "/api/v1/server", None, Some(kids_token))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    assert_eq!(
        app.request(
            "PUT",
            "/api/v1/parental-controls/settings",
            Some(json!({"unknownKidsPolicy":"ALLOW"})),
            Some(&adult),
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, movies) = app
        .request("GET", "/api/v1/media/movies", None, Some(kids_token))
        .await;
    assert_eq!(movies["items"].as_array().unwrap().len(), 2);
    assert!(
        movies["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == "movie-unknown")
    );
}

#[tokio::test]
async fn remote_sources_m3u_crud_upload_and_preview_work() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let (status, library) = app
        .request(
            "POST",
            "/api/v1/libraries",
            Some(json!({"name":"Remota","type":"MOVIE","privacy":"PUBLIC","minimumAge":0,"metadataLanguage":"pt-BR","paths":[app.media_dir("remote").to_string_lossy()]})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let library_id = library["id"].as_str().unwrap().to_string();

    // Create an M3U-by-URL source; the stored config must never expose credentials.
    let (status, source) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{library_id}/remote-sources"),
            Some(json!({"name":"Lista","providerType":"M3U_URL","config":{"url":"https://user:secret@host.example/list.m3u?token=abc"}})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let source_id = source["id"].as_str().unwrap().to_string();
    assert_eq!(source["config"]["url"], "https://host.example/list.m3u");
    assert_eq!(source["providerType"], "M3U_URL");
    assert_eq!(source["autoSync"]["enabled"], true);

    let (status, listed) = app
        .request(
            "GET",
            &format!("/api/v1/libraries/{library_id}/remote-sources"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);

    let (status, updated) = app
        .request(
            "PATCH",
            &format!("/api/v1/remote-sources/{source_id}"),
            Some(json!({"name":"Lista Renomeada","isActive":false})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Lista Renomeada");
    assert_eq!(updated["status"], "DISABLED");

    // Upload + preview an M3U file.
    let playlist = "#EXTM3U\n#EXTINF:-1 group-title=\"FILMES | LANCAMENTOS\",Filme Um (2024)\nhttps://host.example/movie/1.mp4\n#EXTINF:-1 group-title=\"SERIES | NETFLIX\",Serie X S01E01\nhttps://host.example/series/2.mp4\n";
    let (status, upload) = app
        .request_bytes(
            "POST",
            "/api/v1/remote-sources/m3u/upload",
            playlist.as_bytes(),
            &admin,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let upload_id = upload["uploadId"].as_str().unwrap().to_string();

    let (status, preview) = app
        .request(
            "POST",
            "/api/v1/remote-sources/m3u/preview",
            Some(json!({"type":"upload","uploadId":upload_id})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["totalEntries"], 2);
    assert_eq!(preview["movieCandidates"], 1);
    assert_eq!(preview["tvCandidates"], 1);

    // A garbage upload is rejected.
    let (status, _) = app
        .request_bytes(
            "POST",
            "/api/v1/remote-sources/m3u/upload",
            b"not a playlist",
            &admin,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = app
        .request(
            "DELETE",
            &format!("/api/v1/remote-sources/{source_id}"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn m3u_file_sync_is_incremental_and_tracks_missing_entries() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let (_, library) = app
        .request(
            "POST",
            "/api/v1/libraries",
            Some(json!({"name":"Filmes Remotos","type":"MOVIE","privacy":"PUBLIC","minimumAge":0,"metadataLanguage":"pt-BR","paths":[app.media_dir("m3u").to_string_lossy()]})),
            Some(&admin),
        )
        .await;
    let library_id = library["id"].as_str().unwrap().to_string();

    let playlist_v1 = "#EXTM3U\n#EXTINF:-1 group-title=\"FILMES | ACAO\",Filme Alpha (2021)\nhttps://host.example/movie/alpha.mp4\n#EXTINF:-1 group-title=\"FILMES | ACAO\",Filme Beta (2022)\nhttps://host.example/movie/beta.mp4\n#EXTINF:-1 group-title=\"SERIES | X\",Serie Gama S01E01\nhttps://host.example/series/gama.mp4\n";
    let (_, upload) = app
        .request_bytes(
            "POST",
            "/api/v1/remote-sources/m3u/upload",
            playlist_v1.as_bytes(),
            &admin,
        )
        .await;
    let upload_id = upload["uploadId"].as_str().unwrap().to_string();
    let (status, source) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{library_id}/remote-sources"),
            Some(json!({"name":"Lista Local","providerType":"M3U_FILE","config":{"uploadId":upload_id}})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let source_id = source["id"].as_str().unwrap().to_string();

    // Select every movie category.
    let (status, _) = app
        .request(
            "PUT",
            &format!("/api/v1/remote-sources/{source_id}/selections"),
            Some(json!({"selections":[{"mediaType":"MOVIE","category":null,"subcategory":null,"includeAll":true}]})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // First sync — discovers 3 entries, catalogs the 2 movies (TMDB not configured → unmatched).
    let (status, outcome) = app
        .request(
            "POST",
            &format!("/api/v1/remote-sources/{source_id}/sync?wait=true"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{outcome}");
    assert_eq!(outcome["scanned"], 3);
    assert_eq!(outcome["new"], 3);
    assert_eq!(outcome["unmatched"], 2);
    assert_eq!(outcome["missing"], 0);

    let (_, entries) = app
        .request(
            "GET",
            &format!("/api/v1/remote-sources/{source_id}/entries"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(entries["total"], 3);

    // Media files were created for the two selected movies only.
    let db = app.state.database().await;
    let media_files: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM media_files WHERE library_id=? AND storage_kind='REMOTE'",
    )
    .bind(&library_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(media_files, 2);

    // Second sync of the same file — everything unchanged, nothing re-cataloged.
    let (_, outcome) = app
        .request(
            "POST",
            &format!("/api/v1/remote-sources/{source_id}/sync?wait=true"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(outcome["new"], 0);
    assert_eq!(outcome["unchanged"], 3);
    assert_eq!(outcome["matched"], 0);
    assert_eq!(outcome["unmatched"], 0);

    // Replace the stored playlist: drop Beta, keep Alpha, add Delta.
    let playlist_v2 = "#EXTM3U\n#EXTINF:-1 group-title=\"FILMES | ACAO\",Filme Alpha (2021)\nhttps://host.example/movie/alpha.mp4\n#EXTINF:-1 group-title=\"FILMES | ACAO\",Filme Delta (2024)\nhttps://host.example/movie/delta.mp4\n#EXTINF:-1 group-title=\"SERIES | X\",Serie Gama S01E01\nhttps://host.example/series/gama.mp4\n";
    std::fs::write(
        app._dir
            .path()
            .join("remote/m3u")
            .join(format!("{source_id}.m3u")),
        playlist_v2,
    )
    .unwrap();

    let (_, outcome) = app
        .request(
            "POST",
            &format!("/api/v1/remote-sources/{source_id}/sync?wait=true"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(outcome["new"], 1, "{outcome}");
    assert_eq!(outcome["missing"], 1);

    let (_, missing) = app
        .request(
            "GET",
            &format!("/api/v1/remote-sources/{source_id}/entries?syncStatus=MISSING"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(missing["total"], 1);
    assert_eq!(missing["items"][0]["cleanTitle"], "Filme Beta");

    // Deleting the source must remove its catalog footprint so playback never
    // 404s on an orphaned media file.
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM media_files WHERE library_id=? AND storage_kind='REMOTE'",
    )
    .bind(&library_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(before > 0);
    let (status, _) = app
        .request(
            "DELETE",
            &format!("/api/v1/remote-sources/{source_id}"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    for (table, sql) in [
        (
            "media_files",
            "SELECT COUNT(*) FROM media_files WHERE library_id=? AND storage_kind='REMOTE'",
        ),
        (
            "library_paths",
            "SELECT COUNT(*) FROM library_paths WHERE library_id=? AND status='REMOTE'",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(sql)
            .bind(&library_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table} not cleaned");
    }
    let entries_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM m3u_entries WHERE source_id=?")
            .bind(&source_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(entries_left, 0);
    let orphan_items: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM media_items mi WHERE mi.library_id=? AND NOT EXISTS (SELECT 1 FROM media_files f WHERE f.media_item_id=mi.id)",
    )
    .bind(&library_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(orphan_items, 0);
}

#[tokio::test]
async fn remote_playback_proxies_origin_without_leaking_the_url() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;
    let (_, me) = app
        .request("GET", "/api/v1/auth/me", None, Some(&admin))
        .await;
    let (movie, _) = seed_catalog(&app, me["id"].as_str().unwrap()).await;
    let db = app.state.database().await;
    let timestamp = now();
    let origin = "http://127.0.0.1:59371/private/stream.mp4?token=supersecret";
    sqlx::query("INSERT INTO remote_sources(id,library_id,provider_type,name,is_active,config,status,auto_sync_enabled,auto_sync_interval_minutes,created_by,created_at,updated_at) VALUES('rs-1','catalog-library','M3U_URL','Lista',1,'{}','READY',1,720,?,?,?)")
        .bind(me["id"].as_str().unwrap()).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO library_paths(id,library_id,path,normalized_path,is_active,status,created_at,updated_at) VALUES('rs-1-path','catalog-library','remote://rs-1','remote://rs-1',0,'REMOTE',?,?)")
        .bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO remote_media_sources(id,media_item_id,remote_source_id,provider_type,external_key,stream_ref,stream_sealed,is_active,last_seen_at,created_at,updated_at) VALUES('rms-1',?,'rs-1','M3U_URL','key-1',?,0,1,?,?,?)")
        .bind(&movie).bind(origin).bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO media_files(id,library_id,library_path_id,media_item_id,absolute_path,relative_path,filename,extension,file_size,modified_at,content_type,scan_status,identification_status,storage_kind,remote_media_source_id,created_at,updated_at,last_seen_at) VALUES('rf-1','catalog-library','rs-1-path',?,'remote://rs-1/key-1','key-1','Aventura','mp4',0,?,'MOVIE','PRESENT','MATCHED_AUTO','REMOTE','rms-1',?,?,?)")
        .bind(&movie).bind(&timestamp).bind(&timestamp).bind(&timestamp).bind(&timestamp).execute(&db.pool).await.unwrap();

    let (status, started) = app
        .request(
            "POST",
            "/api/v1/playback/start",
            Some(json!({"mediaItemId":movie,"mediaFileId":"rf-1","clientCapabilities":{"containers":["mp4"],"videoCodecs":["h264"],"audioCodecs":["aac"],"maxWidth":1920,"maxHeight":1080},"quality":"AUTO"})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["playbackMode"], "DIRECT_PLAY");
    let stream_url = started["streamUrl"].as_str().unwrap();
    assert!(stream_url.contains("/remote?token="), "{stream_url}");
    assert!(!started.to_string().contains("supersecret"));
    assert!(!started.to_string().contains("59371"));

    // The proxy runs but the fake origin is unreachable → 502, never a panic and
    // never the origin URL in the response.
    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(stream_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&body).contains("supersecret"));
}

#[tokio::test]
async fn google_drive_endpoints_guard_configuration_and_state() {
    let app = TestApp::new().await;
    let admin = app.setup_and_login().await;

    // OAuth is not configured in the test harness.
    let (status, body) = app
        .request(
            "POST",
            "/api/v1/remote-sources/google-drive/connect",
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "GOOGLE_OAUTH_NOT_CONFIGURED");

    // An unknown callback state is rejected.
    let (status, body) = app
        .request(
            "GET",
            "/api/v1/remote-sources/google-drive/callback?code=abc&state=nope",
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "INVALID_OAUTH_STATE");

    let (status, list) = app
        .request(
            "GET",
            "/api/v1/remote-sources/google-drive/connections",
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["items"].as_array().unwrap().len(), 0);

    // Creating a Drive source requires a real connection.
    let (_, library) = app
        .request(
            "POST",
            "/api/v1/libraries",
            Some(json!({"name":"Drive","type":"MOVIE","privacy":"PUBLIC","minimumAge":0,"metadataLanguage":"pt-BR","paths":[app.media_dir("gd").to_string_lossy()]})),
            Some(&admin),
        )
        .await;
    let library_id = library["id"].as_str().unwrap();
    let (status, body) = app
        .request(
            "POST",
            &format!("/api/v1/libraries/{library_id}/remote-sources"),
            Some(json!({"name":"Drive","providerType":"GOOGLE_DRIVE","config":{"connectionId":"missing","folders":[{"folderId":"root","displayName":"Meu Drive"}]}})),
            Some(&admin),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "GOOGLE_DRIVE_CONNECTION_NOT_FOUND");
}
