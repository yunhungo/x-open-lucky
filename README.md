# OpenLucky

用于单节点 NAS 的域名、反向代理和证书管理服务。Rust / Axum / Tokio 后台，React / TypeScript / Tailwind CSS / shadcn 前台，SQLite WAL 持久化。

## 功能

- 动态域名：Cloudflare、阿里云 DNS、腾讯云 DNSPod；IPv4 / IPv6；公网、网卡或指定 IP；定时更新、手动同步、启停、失败记录、同名记录冲突保护。
- Web 服务：按域名路由，流式 HTTP/HTTPS 上游，WebSocket 双向隧道，正确处理逐跳请求头，原始 Host 选项，可信转发头，HTTPS 跳转，循环代理保护。
- SSL：通过固定版本 lego 申请 Let's Encrypt DNS-01 证书，包括通配符；测试/正式环境；到期前 30 天续期；PEM 导入；SAN、有效期和私钥匹配校验；原子发布证书；动态 SNI。
- 管理：一次性管理员初始化，Argon2id 密码，HttpOnly / SameSite 会话，CSRF 校验，登录限流，修改密码撤销旧会话，配置乐观并发版本，依赖资源删除保护，审计事件。
- 运维：版本化 SQLx 迁移，后台任务并发上限，重启恢复，优雅停止，密码加密配置备份和事务恢复，独立数据目录，镜像版本核验、健康检查、失败回滚、限定旧镜像清理。

## 本地开发

需要 Rust 1.89+、Node.js 22、OpenSSL（集成测试）及 lego（实际签发）。

```sh
cd frontend
npm ci
npm run dev
```

另一终端在仓库根目录运行 `cargo run`。Vite 默认代理后台 `56002`，访问 `http://127.0.0.1:5173`。首次初始化密钥位于 `data/setup-token`，初始化成功即删除。未设置任何默认密码。

```sh
cd frontend
npm run build
npx playwright install chromium
cd ..
./.myself/check.sh
```

协议测试使用临时数据目录和动态端口；浏览器测试使用 `56302 / 58380 / 58343`，自动清理测试数据。均不接触生产 DNS。

## QNAP 部署

```sh
git add .
git commit -m 'Update OpenLucky'
./.myself/build-and-deploy-qnap.sh
```

默认通过 SSH `nas` 将 `linux/amd64` 镜像上传到 QNAP，并安装至 `/share/Container/openlucky`。管理面板绑定 NAS 私网地址 `192.168.1.85:56002`，Web 监听 `58080 / 58443`。使用 host 网络以读取宿主 IPv6 地址和连接 NAS 服务；应用以 UID 10001 运行、只读根文件系统、无 Docker socket，独立 `/data`。

首次成功发布后，脚本经 SSH 初始化管理员 `yunhungo`，随机生成密码并以 0600 权限保存在本机 `.myself/qnap-access.json`。该文件被 Git 忽略，不输出密码到日志；后续部署保留账户。可用 `OPENLUCKY_INITIAL_ADMIN` 更改初始用户名。

`OPENLUCKY_SSH_HOST`、`OPENLUCKY_REMOTE_ROOT` 和 `OPENLUCKY_REMOTE_DOCKER` 可以修改部署目标。运行时环境可写在 NAS 的 `runtime.env` 中，更新保留该文件。配置项参见 [部署说明](docs/operations.md)。

更新前停止本应用并创建包含数据库、密钥和证书的完整快照；新服务版本必须与 Git 提交完全一致。失败会恢复此前镜像和快照。成功后只删除带有本项目来源标记的旧 `openlucky:*` 镜像与悬空镜像，不使用全局 prune。快照和旧发布目录保留供恢复，不自动删除业务数据。

## 开发边界

[架构说明](docs/architecture.md) 定义模块边界、持久化和扩展约定。[API 合同](docs/api.md) 定义 `/api/v1` 接口。供应商适配器是少量、可测试的 HTTP 集成代码；HTTP/TLS、密码、数据库、ACME 交给成熟库和客户端。

当前针对单管理员、单实例 NAS 部署。端口转发、STUN、WOL、OAuth、多人 RBAC 和通知通道尚未实现；不显示无作用的入口。实际供应商 API 更新和公开 CA 签发需要用户域名及有效 DNS 凭据，集成测试不会凭空声明生产验证完成。
