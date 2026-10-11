-- @created 2026-10-12
-- @description 为已有代理规则补齐默认监听入口，保持升级前访问范围。
-- @author yunhungo
UPDATE resources SET data=json_set(data, '$.listener', 'default')
WHERE kind='web' AND json_extract(data, '$.listener') IS NULL;
