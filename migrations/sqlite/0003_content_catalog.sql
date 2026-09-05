CREATE TABLE user_favorites (id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, media_item_id TEXT NOT NULL REFERENCES media_items(id) ON DELETE CASCADE, created_at TEXT NOT NULL, UNIQUE(user_id, media_item_id));
CREATE INDEX idx_user_favorites_user_created ON user_favorites(user_id, created_at);
CREATE INDEX idx_media_items_catalog ON media_items(media_type, created_at);
CREATE INDEX idx_media_items_title ON media_items(title);
CREATE INDEX idx_media_items_year_rating ON media_items(year, rating);
CREATE INDEX idx_media_genres_genre_media ON media_genres(genre_id, media_item_id);
