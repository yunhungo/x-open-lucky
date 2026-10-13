-- @created 2026-10-13
-- @description 补齐 Web 服务高级选项，保留已有协议及超时行为。
-- @author yunhungo
UPDATE resources SET data=json_set(data, '$.options', json('{"access_mode":"both","http1":true,"http2":true,"http3":false,"tls_min":"1.2","upstream_version":"auto","connect_timeout":10,"response_timeout":60,"request_headers":{},"response_headers":{}}'))
WHERE kind='web' AND json_extract(data, '$.options') IS NULL;
