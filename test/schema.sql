-- Schema for Atuin history and store tables
-- Based on Atuin's database structure

-- Legacy history table
CREATE TABLE IF NOT EXISTS history (
    id UUID PRIMARY KEY,
    client_id UUID NOT NULL,
    user_id UUID NOT NULL,
    hostname VARCHAR(255) NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL,
    duration BIGINT NOT NULL,
    exit INTEGER NOT NULL,
    session TEXT NOT NULL,
    deleted_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history(timestamp);
CREATE INDEX IF NOT EXISTS idx_history_user_id ON history(user_id);

-- New store table (record sync)
CREATE TABLE IF NOT EXISTS store (
    id UUID PRIMARY KEY,
    client_id UUID NOT NULL,
    host_id UUID NOT NULL,
    tag VARCHAR(255) NOT NULL,
    timestamp BIGINT NOT NULL,
    version VARCHAR(255) NOT NULL,
    data BYTEA NOT NULL,
    crc BIGINT NOT NULL,
    idx BIGINT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_store_tag ON store(tag);
CREATE INDEX IF NOT EXISTS idx_store_timestamp ON store(timestamp);
CREATE INDEX IF NOT EXISTS idx_store_host_id ON store(host_id);
