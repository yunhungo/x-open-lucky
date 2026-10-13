# API v1

所有管理接口位于 `/api/v1`，使用 JSON。写请求要求 `X-OpenLucky: 1`；浏览器 Origin 必须与 Host 相同。登录成功返回 HttpOnly / SameSite=Strict cookie。受保护接口未登录返回 401，跨来源写请求返回 403，编辑冲突与重复任务返回 409。

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| GET | `/auth/status` | 是否已初始化管理员 |
| POST | `/auth/setup` | `username`、`password`、一次性 `setup_token` |
| POST | `/auth/login` | 用户名密码登录 |
| GET | `/auth/me` | 当前管理员 |
| POST | `/auth/logout` | 撤销当前会话 |
| PUT | `/auth/password` | `current_password`、`new_password`；撤销全部旧会话 |
| POST | `/auth/sessions/revoke` | 撤销其他会话 |
| GET | `/overview` | 实际计数、版本、运行时间、监听地址及 `performance` 进程/容器采样 |
| GET | `/interfaces` | 宿主可用网卡及 IP |
| GET | `/resources/{kind}` | 资源列表、`revision` 及运行状态 |
| POST | `/resources/{kind}` | 添加资源，返回新 ID |
| PUT | `/resources/{kind}/{id}` | 完整配置及 `revision`；旧版本拒绝覆盖 |
| DELETE | `/resources/{kind}/{id}` | 删除未被其他资源引用的配置 |
| POST | `/resources/{kind}/{id}/run` | DDNS 或 ACME 任务；202 表示已入队 |
| POST | `/certificates/{id}/import` | `cert_pem`、`key_pem`，验证后原子激活 |
| GET | `/groups` | 有序分组列表：`id / name / position / revision` |
| POST | `/groups` | `{name}` 创建分组，返回 ID |
| PUT | `/groups/{id}` | `{name, revision}` 重命名 |
| DELETE | `/groups/{id}` | `{revision}` 删除分组，成员原子转为未分组 |
| PUT | `/groups/order` | `{groups:[{id,revision}]}` 提交完整顺序 |
| PUT | `/groups/assign` | `{group_id,services:[{id,revision}]}` 批量归组 |
| GET | `/group-stats?hours=24` | 一次返回全部组与未分组的状态、请求、错误和加权耗时 |
| GET | `/access-logs?group_id=&service_id=&errors=false&status=&since=&until=&before=&limit=50&q=` | 访问明细游标分页，返回 `items / next_cursor`；每页 1–200 条 |
| GET | `/traffic?group_id=&service_id=&hours=24` | 当前及此前小时聚合，`hours` 为 24 / 168 / 720，未指定分组及服务时返回全站总量 |
| GET | `/telemetry` | 明细保留策略、容量、队列、最近写入与本次运行丢失计数 |
| GET | `/logs?level=&module=` | 最近 300 条事件；最多保留 2000 条 |
| POST | `/backup/export` | `password`，返回加密 `backup` 文件文本 |
| POST | `/backup/restore` | `password`、`backup`，事务替换资源 |

`kind` 为 `credentials / ddns / web / certificates`。完整类型定义见 `backend/src/models.rs` 与 `frontend/src/lib/contracts.ts`。凭据修改时可留空对应密钥字段，服务端保留已有值；服务商不可原地修改。证书域名、来源及测试/正式环境不可原地修改，避免错误复用 ACME 目录或私钥。

任务状态为 `idle / running / ready / error`。失败返回可恢复说明，状态与资源配置版本独立。入队成功不等于任务成功；UI 轮询任务结果。

`GET /healthz` 无需登录，返回 `ok / build_sha / version`，供部署时核对精确构建提交。

访问查询的 `since / until` 为 Unix 秒，`before` 是上一页返回的游标，`errors=true` 同时包含 4xx/5xx 与流/隧道错误。`q` 对路径、域名、客户端 IP、请求 ID 作字面子串匹配。`avg_duration_ms` 表示响应头耗时；无请求时返回 `null`。观测合同见 `frontend/src/lib/observability.ts` 及 [观测说明](observability.md)。

## Web 高级配置

`web.options` 默认 `{access_mode:"both", http1:true, http2:true, http3:false, tls_min:"1.2", upstream_version:"auto", connect_timeout:10, response_timeout:60, request_headers:{}, response_headers:{}}`。`access_mode` 为 `both/http/https`，上游版本为 `auto/http1/http2`；连接超时 1–60 秒，响应头超时 1–600 秒。后者不限制已开始的流式响应总时长。

头配置在 API 中为名称到字符串/null 的映射：字符串设置（含 MiniJinja 动态模板），null 删除。每组最多 32 项、总原始值最多 32 KiB；运行时每值最多 8 KiB、每组最多 32 KiB。模板语法和上下文在保存时验证；运行时缺失变量、非法字符或超限返回 502，并记录不含模板值的错误。完整变量与限制见 [Web 配置](web-services.md)。

旧配置经 `0004_web_options.sql` 补齐默认值，旧备份恢复同样应用默认值。`overview` 增加 `http3_addr`、`public_http3_port`；附加入口增加可选的 `http3_addr/http3_port`。

## 服务分组

`web.group_id` 默认空字符串，表示未分组。非空时必须引用已有分组；`0005_service_groups.sql` 为升级数据补齐默认值并建立初始分组。分组最多 64 个，名称去首尾空白后为 1–40 个字符，名称不区分大小写，同名、控制字符和保留名称“未分组”返回 400。

排序必须包含全部当前组且每个 ID 恰好一次。批量归组每次接受 1–500 个互不重复的服务；任何服务版本过期或删除，都返回 409 并回滚整批。只改写 `group_id` 及资源 `revision`，不覆盖服务其他配置。分组删除会同时增加受影响服务的版本。不存在的组返回 404，非法参数返回 400。

观测查询中省略或传空 `group_id` 表示全部；`group_id=unassigned` 表示当前未分组的服务。同时指定 `service_id` 时取交集，无成员返回空列表或零统计；不存在的组返回 404。全局未知域名及已删除服务不计入任何分组。筛选基于当前归属，移组后保留的历史随成员归属展示。

`/group-stats` 的 `hours` 为 24 / 168 / 720，返回 `{since, until, groups:[...]}`；每组包含 `id/name/position/services/enabled/errors/requests/client_errors/server_errors/stream_errors/avg_duration_ms`。`errors` 为异常服务数，其他错误字段为访问计数。平均耗时按总耗时除以总请求量计算，无请求返回 null。前端合同在 `lib/groups.ts`，统计不包含臆测的每组 CPU 或内存。

备份外层加密格式保持 `openlucky-backup-v1`，解密后的内容升级为 `version:2` 并包含 `groups`。恢复会重映射组和服务 ID、保留顺序及关联；内容版本 1 仍可恢复，其服务全部未分组，当前分组定义保留。
