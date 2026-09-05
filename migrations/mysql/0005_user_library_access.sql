CREATE TABLE user_library_access (
  id VARCHAR(36) PRIMARY KEY,
  user_id VARCHAR(36) NOT NULL,
  library_id VARCHAR(36) NOT NULL,
  can_view BOOLEAN NOT NULL DEFAULT TRUE,
  can_play BOOLEAN NOT NULL DEFAULT TRUE,
  granted_by VARCHAR(36),
  created_at VARCHAR(40) NOT NULL,
  updated_at VARCHAR(40) NOT NULL,
  UNIQUE KEY uq_user_library_access(user_id, library_id),
  INDEX idx_user_library_access_user(user_id),
  INDEX idx_user_library_access_library(library_id),
  FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
  FOREIGN KEY(library_id) REFERENCES libraries(id) ON DELETE CASCADE,
  FOREIGN KEY(granted_by) REFERENCES users(id)
) ENGINE=InnoDB;
