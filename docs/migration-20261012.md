# Lucky 配置迁移验收（2026-10-12）

作者：yunhungo

已部署 `openlucky:full-migration-20261012`，运行标识 `5e0b7a0-full-migration-20261012`（工作区构建，非新 Git 提交）。

- 私网入口：`100.64.0.5:443`，22 条启用规则；保持 Tailnet 访问范围。
- 公网入口：IPv6 `[::]:41003`，同端口 HTTP 跳转与 HTTPS。20 条公网配置中仅 `vw.s`、`headscale.s`、`plane.s` 启用，沿用旧 Lucky 状态。
- 合计 43 条 Web 规则，25 条启用；按要求不迁移 ailedger。
- DDNS：OpenLucky 自行维护 `*.s.yunhung.com` AAAA，36 秒检测周期，TTL 自动、DNS only。沿用 Lucky 的四个 IP 查询接口，已确认手动与定时执行均成功，地址与旧配置一致。
- 两张正式 Let's Encrypt 证书覆盖 `*.t.yunhung.com` 与 `*.s.yunhung.com`，由 OpenLucky 自动续期。
- `lucky.t.yunhung.com` 已转到 OpenLucky 管理面板。旧 Lucky 容器已停止，重启策略从 `always` 改为 `no`，防止重启后抢占端口或重复维护 DDNS。
- Lucky 的端口转发、STUN、Cloudflared、Rclone、网络唤醒、计划任务无任务；DLNA、WebDAV、FTP、FileBrowser 未启用；全局黑名单与子规则使用的黑名单无条目、Coraza 无实例。

## 验证

11 个 Rust 测试、13 组协议集成测试、3 组部署/回滚场景、5 个真实浏览器流程通过。协议测试涵盖 HTTP/2、WebSocket、Headscale control upgrade、同端口 HTTP/TLS、公私入口的 SNI 与 Host 隔离。测试不更新生产 DNS。

25 个已启用域名通过实际 DNS 访问时均完成可信 TLS 握手。`webdav.t` 返回预期 401，Jellyfin/Uptime Kuma 返回 302，其余除 Plane 外返回 200。公网入口使用私网 Host 返回 404。

`plane.s` 在迁移前与迁移后均返回 404；最终检查 `plane.t` 也返回 404。直接从 NAS 请求其上游 `127.0.0.1:59013`，使用对应 Host，同样返回 404，属于 Plane 上游响应，未更改其应用配置。本次没有通过用户手机的实际 5G 网络验收。

## 恢复资料

NAS 的 `/share/Container/openlucky/migration-20261012/` 保存权限 0600 的 Lucky 配置归档、升级前 OpenLucky 数据快照、切换前环境及临时入口环境。旧 OpenLucky 镜像保留用于回滚；不删除 Lucky 配置或其他 NAS 应用。

回滚必须一并恢复升级前的数据快照和监听环境（不能直接用旧二进制读取新增 SQL 迁移后的数据库），切回旧 OpenLucky 发布目录，再启动 Lucky；若恢复长期旧方案，还需恢复 Lucky 的 `always` 重启策略。旧 OpenLucky 继续负责私网 443，Lucky 接回公网 41003 和 DDNS。执行恢复前应另备份当前配置。
