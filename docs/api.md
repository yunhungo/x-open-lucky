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
| GET | `/overview` | 实际计数、版本、运行时间和监听地址 |
| GET | `/interfaces` | 宿主可用网卡及 IP |
| GET | `/resources/{kind}` | 资源列表、`revision` 及运行状态 |
| POST | `/resources/{kind}` | 添加资源，返回新 ID |
| PUT | `/resources/{kind}/{id}` | 完整配置及 `revision`；旧版本拒绝覆盖 |
| DELETE | `/resources/{kind}/{id}` | 删除未被其他资源引用的配置 |
| POST | `/resources/{kind}/{id}/run` | DDNS 或 ACME 任务；202 表示已入队 |
| POST | `/certificates/{id}/import` | `cert_pem`、`key_pem`，验证后原子激活 |
| GET | `/logs?level=&module=` | 最近 300 条事件；最多保留 2000 条 |
| POST | `/backup/export` | `password`，返回加密 `backup` 文件文本 |
| POST | `/backup/restore` | `password`、`backup`，事务替换资源 |

`kind` 为 `credentials / ddns / web / certificates`。完整类型定义见 `backend/src/models.rs` 与 `frontend/src/lib/contracts.ts`。凭据修改时可留空对应密钥字段，服务端保留已有值；服务商不可原地修改。证书域名、来源及测试/正式环境不可原地修改，避免错误复用 ACME 目录或私钥。

任务状态为 `idle / running / ready / error`。失败返回可恢复说明，状态与资源配置版本独立。入队成功不等于任务成功；UI 轮询任务结果。

`GET /healthz` 无需登录，返回 `ok / build_sha / version`，供部署时核对精确构建提交。
