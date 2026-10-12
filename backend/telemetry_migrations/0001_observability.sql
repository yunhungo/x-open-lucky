-- @created 2026-10-13
-- @description 保存有界访问明细和独立的小时访问聚合。
-- @author yunhungo
CREATE TABLE access_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  time INTEGER NOT NULL,
  service_id TEXT NOT NULL,
  status INTEGER NOT NULL,
  failed INTEGER NOT NULL,
  entry TEXT NOT NULL CHECK(length(CAST(entry AS BLOB)) <= 4096)
);
CREATE INDEX access_service ON access_logs(service_id, id DESC);
CREATE INDEX access_failed ON access_logs(failed, id DESC);
CREATE INDEX access_time ON access_logs(time);
CREATE TABLE traffic (
  hour INTEGER NOT NULL,
  service_id TEXT NOT NULL,
  requests INTEGER NOT NULL,
  client_errors INTEGER NOT NULL,
  server_errors INTEGER NOT NULL,
  stream_errors INTEGER NOT NULL,
  latency_ms REAL NOT NULL,
  PRIMARY KEY(hour, service_id)
);
