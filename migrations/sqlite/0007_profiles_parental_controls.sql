PRAGMA foreign_keys=OFF;

CREATE TABLE profiles (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  avatar_id TEXT NOT NULL DEFAULT 'default.png',
  is_default INTEGER NOT NULL DEFAULT 0,
  is_kids INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 1,
  pin_hash TEXT,
  max_age_rating INTEGER NOT NULL DEFAULT 18 CHECK(max_age_rating IN (0,10,12,14,16,18)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_used_at TEXT
);
CREATE UNIQUE INDEX uq_profiles_default_user ON profiles(user_id) WHERE is_default=1;
CREATE INDEX idx_profiles_user ON profiles(user_id,is_active);

INSERT INTO profiles (id,user_id,name,avatar_id,is_default,is_kids,is_active,max_age_rating,created_at,updated_at)
SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1,1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
       id,display_name,'default.png',1,0,1,18,created_at,updated_at
FROM users;

CREATE TABLE profile_library_access (
  profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
  library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
  is_allowed INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(profile_id,library_id)
);
CREATE INDEX idx_profile_library_access_library ON profile_library_access(library_id);
INSERT INTO profile_library_access(profile_id,library_id,is_allowed,created_at,updated_at)
SELECT p.id,l.id,1,p.created_at,p.updated_at
FROM profiles p JOIN libraries l ON l.deleted_at IS NULL AND l.is_active=1
WHERE p.is_default=1 AND (l.privacy='PUBLIC'
  OR EXISTS(SELECT 1 FROM user_library_access ula WHERE ula.user_id=p.user_id AND ula.library_id=l.id AND ula.can_view=1)
  OR EXISTS(SELECT 1 FROM user_roles ur JOIN roles r ON r.id=ur.role_id WHERE ur.user_id=p.user_id AND r.name='Administrator'));

ALTER TABLE user_favorites RENAME TO user_favorites_v6;
CREATE TABLE user_favorites (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
  media_item_id TEXT NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  UNIQUE(profile_id,media_item_id)
);
INSERT INTO user_favorites(id,user_id,profile_id,media_item_id,created_at)
SELECT f.id,f.user_id,p.id,f.media_item_id,f.created_at
FROM user_favorites_v6 f JOIN profiles p ON p.user_id=f.user_id AND p.is_default=1;
DROP TABLE user_favorites_v6;
CREATE INDEX idx_user_favorites_profile_created ON user_favorites(profile_id,created_at);

ALTER TABLE playback_sessions RENAME TO playback_sessions_v6;
CREATE TABLE playback_sessions (
  id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id), profile_id TEXT NOT NULL REFERENCES profiles(id),
  media_item_id TEXT NOT NULL REFERENCES media_items(id), media_file_id TEXT NOT NULL REFERENCES media_files(id),
  episode_id TEXT REFERENCES tv_episodes(id), mode TEXT NOT NULL, quality_profile TEXT NOT NULL, reason TEXT NOT NULL DEFAULT '[]',
  stream_token_hash TEXT NOT NULL, started_at TEXT NOT NULL, last_activity_at TEXT NOT NULL, position_ms INTEGER NOT NULL DEFAULT 0,
  duration_ms INTEGER NOT NULL DEFAULT 0, client_id TEXT, client_name TEXT, ip_address TEXT, status TEXT NOT NULL,
  pipeline_key TEXT, bytes_served INTEGER NOT NULL DEFAULT 0, buffer_events INTEGER NOT NULL DEFAULT 0, ended_at TEXT
);
INSERT INTO playback_sessions SELECT s.id,s.user_id,p.id,s.media_item_id,s.media_file_id,s.episode_id,s.mode,s.quality_profile,s.reason,s.stream_token_hash,s.started_at,s.last_activity_at,s.position_ms,s.duration_ms,s.client_id,s.client_name,s.ip_address,s.status,s.pipeline_key,s.bytes_served,s.buffer_events,s.ended_at FROM playback_sessions_v6 s JOIN profiles p ON p.user_id=s.user_id AND p.is_default=1;
DROP TABLE playback_sessions_v6;
CREATE INDEX idx_playback_sessions_profile ON playback_sessions(profile_id,last_activity_at);
CREATE INDEX idx_playback_sessions_user ON playback_sessions(user_id,last_activity_at);

ALTER TABLE playback_progress RENAME TO playback_progress_v6;
CREATE TABLE playback_progress (
  id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
  media_item_id TEXT NOT NULL REFERENCES media_items(id) ON DELETE CASCADE, episode_id TEXT REFERENCES tv_episodes(id) ON DELETE CASCADE,
  content_key TEXT NOT NULL, position_ms INTEGER NOT NULL, duration_ms INTEGER NOT NULL, percentage REAL NOT NULL,
  updated_at TEXT NOT NULL, completed_at TEXT, UNIQUE(profile_id,content_key)
);
INSERT INTO playback_progress SELECT x.id,x.user_id,p.id,x.media_item_id,x.episode_id,x.content_key,x.position_ms,x.duration_ms,x.percentage,x.updated_at,x.completed_at FROM playback_progress_v6 x JOIN profiles p ON p.user_id=x.user_id AND p.is_default=1;
DROP TABLE playback_progress_v6;
CREATE INDEX idx_playback_progress_profile ON playback_progress(profile_id,updated_at);

ALTER TABLE playback_history RENAME TO playback_history_v6;
CREATE TABLE playback_history (
  id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
  media_item_id TEXT NOT NULL REFERENCES media_items(id) ON DELETE CASCADE, episode_id TEXT REFERENCES tv_episodes(id) ON DELETE CASCADE,
  content_key TEXT NOT NULL, started_at TEXT NOT NULL, last_watched_at TEXT NOT NULL, completed INTEGER NOT NULL DEFAULT 0,
  watch_time_ms INTEGER NOT NULL DEFAULT 0, session_count INTEGER NOT NULL DEFAULT 1, UNIQUE(profile_id,content_key)
);
INSERT INTO playback_history SELECT h.id,h.user_id,p.id,h.media_item_id,h.episode_id,h.content_key,h.started_at,h.last_watched_at,h.completed,h.watch_time_ms,h.session_count FROM playback_history_v6 h JOIN profiles p ON p.user_id=h.user_id AND p.is_default=1;
DROP TABLE playback_history_v6;
CREATE INDEX idx_playback_history_profile ON playback_history(profile_id,last_watched_at);

ALTER TABLE media_items ADD COLUMN content_age_rating INTEGER;
CREATE INDEX idx_media_items_age_rating ON media_items(content_age_rating,library_id);

CREATE TABLE parental_control_settings (
  id INTEGER PRIMARY KEY CHECK(id=1),
  unknown_kids_policy TEXT NOT NULL DEFAULT 'BLOCK_FOR_KIDS' CHECK(unknown_kids_policy IN ('ALLOW','BLOCK_FOR_KIDS')),
  updated_at TEXT NOT NULL
);
INSERT INTO parental_control_settings(id,unknown_kids_policy,updated_at) VALUES(1,'BLOCK_FOR_KIDS',datetime('now'));

PRAGMA foreign_keys=ON;
