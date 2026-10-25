CREATE UNIQUE INDEX IF NOT EXISTS sessions_hash_string_idx ON sessions(hash_string);
CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON sessions(user_id);
CREATE INDEX IF NOT EXISTS sessions_expires_at_idx ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS user_roles_role_id_idx ON user_roles(role_id);
CREATE INDEX IF NOT EXISTS redirects_created_by_idx ON redirects(created_by);
