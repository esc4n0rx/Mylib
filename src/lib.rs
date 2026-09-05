pub mod app;
pub mod core;
pub mod features;
pub mod http;
pub mod infrastructure;

// Compatibility exports keep the crate's existing public API stable while the
// implementation is organized into layers and feature boundaries.
pub use core::{config, errors, models};
pub use features::{
    auth,
    catalog::{api as catalog_api, media as media_api, metadata, scanner},
    libraries::{api as libraries, sync as library_sync},
    operations as operational, playback, profiles, recommendations,
};
pub use http::api;
pub use infrastructure::{database as db, web_assets};

pub use app::{AppState, build_app};
pub use config::Config;
