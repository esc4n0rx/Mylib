CREATE TABLE user_favorites (id VARCHAR(36) PRIMARY KEY, user_id VARCHAR(36) NOT NULL, media_item_id VARCHAR(36) NOT NULL, created_at VARCHAR(40) NOT NULL, UNIQUE KEY uq_user_favorite(user_id,media_item_id), INDEX idx_user_favorites_user_created(user_id,created_at), FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE, FOREIGN KEY(media_item_id) REFERENCES media_items(id) ON DELETE CASCADE) ENGINE=InnoDB;
CREATE INDEX idx_media_items_catalog ON media_items(media_type, created_at);
CREATE INDEX idx_media_items_title ON media_items(title);
CREATE INDEX idx_media_items_year_rating ON media_items(year, rating);
CREATE INDEX idx_media_genres_genre_media ON media_genres(genre_id, media_item_id);
