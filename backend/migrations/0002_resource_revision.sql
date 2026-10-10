-- @created 2026-10-11
-- @description 为资源配置加入乐观并发版本，防止覆盖其他会话的编辑。
-- @author yunhungo
ALTER TABLE resources ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
