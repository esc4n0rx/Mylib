CREATE TABLE remote_media_sources (id TEXT PRIMARY KEY, media_item_id TEXT REFERENCES media_items(id) ON DELETE CASCADE, episode_id TEXT REFERENCES tv_episodes(id) ON DELETE CASCADE, remote_source_id TEXT NOT NULL REFERENCES remote_sources(id) ON DELETE CASCADE, provider_type TEXT NOT NULL, external_key TEXT NOT NULL, stream_ref TEXT NOT NULL, stream_sealed INTEGER NOT NULL DEFAULT 0, quality_hint TEXT, is_active INTEGER NOT NULL DEFAULT 1, last_seen_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE(remote_source_id, external_key));
CREATE INDEX idx_remote_media_item ON remote_media_sources(media_item_id);
CREATE INDEX idx_remote_media_episode ON remote_media_sources(episode_id);
CREATE INDEX idx_remote_media_source_key ON remote_media_sources(remote_source_id, external_key);
ALTER TABLE media_files ADD COLUMN storage_kind TEXT NOT NULL DEFAULT 'LOCAL';
ALTER TABLE media_files ADD COLUMN remote_media_source_id TEXT;
