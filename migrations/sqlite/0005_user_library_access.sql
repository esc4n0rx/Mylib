CREATE TABLE user_library_access (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
  can_view INTEGER NOT NULL DEFAULT 1,
  can_play INTEGER NOT NULL DEFAULT 1,
  granted_by TEXT REFERENCES users(id),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(user_id, library_id)
);
CREATE INDEX idx_user_library_access_user ON user_library_access(user_id);
CREATE INDEX idx_user_library_access_library ON user_library_access(library_id);
