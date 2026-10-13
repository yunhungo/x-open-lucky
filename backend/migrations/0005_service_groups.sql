-- @created 2026-10-13
-- @description 创建有序服务分组，并将既有 Web 服务安全迁移为未分组。
-- @author yunhungo
CREATE TABLE service_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    position INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1
);
INSERT INTO service_groups(id,name,position) VALUES
('group-network','基础网络',0),
('group-ai','AI 服务',1),
('group-media','媒体下载',2),
('group-files','文件 / WebDAV',3),
('group-operations','运维监控',4),
('group-tools','其他工具',5);
UPDATE resources SET data=json_set(data,'$.group_id','')
WHERE kind='web' AND json_extract(data,'$.group_id') IS NULL;
CREATE INDEX resources_web_group ON resources(json_extract(data,'$.group_id')) WHERE kind='web';
