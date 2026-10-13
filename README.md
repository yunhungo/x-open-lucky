<div align="center">
  <img src="frontend/public/favicon.svg" alt="OpenLucky" width="72" />
  <h1>OpenLucky</h1>
  <p>面向 NAS 和家庭服务器的 DDNS、反向代理与证书管理面板。</p>

[![Verify](https://github.com/yunhungo/x-open-lucky/actions/workflows/check.yml/badge.svg)](https://github.com/yunhungo/x-open-lucky/actions/workflows/check.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

OpenLucky 把域名更新、HTTPS 证书和 Web 服务集中在一个面板中。后端使用 Rust / Axum / Tokio，前端使用 React / TypeScript / Tailwind CSS / shadcn/ui，配置保存在 SQLite。适合单管理员、单实例的自托管环境，提供 QNAP 自动部署及回滚脚本。

## 功能

| 模块 | 支持内容 |
| --- | --- |
| DDNS | Cloudflare、阿里云 DNS、腾讯云 DNSPod；IPv4 / IPv6；公网探测、网卡或指定 IP；自动同步、手动执行与冲突保护 |
| 反向代理 | 按域名及入口隔离；HTTP/1.1、HTTP/2、QUIC HTTP/3；流式上传下载；WebSocket 与 Headscale Upgrade |
| 服务配置 | HTTP / HTTPS 访问策略、TLS 最低版本、上游协议（含 h2c）、超时、HTTPS 跳转、Host 保留 |
| 自定义头 | 逐行设置或删除请求头、响应头；固定文字、变量、条件与 MiniJinja 模板；保存时校验 |
| 证书 | Let's Encrypt DNS-01 与通配符、自动续期、PEM 导入、动态 SNI、证书与私钥匹配校验 |
| 访问观测 | 访问与错误日志、服务详情、小时趋势、CPU / 内存采样、独立限容存储与丢弃计数 |
| 管理与运维 | 一次性初始化、加密凭据、会话管理、配置版本校验、加密备份、SQL 迁移、发布快照与失败回滚 |

HTTP/3 已实现实际 QUIC 服务端，按服务开启，需要有效证书和可达的 UDP 端口。既有配置升级后继续使用 HTTP/1.1 + HTTP/2。当前不支持原始 TCP/UDP 端口转发、HTTP/2/3 Extended CONNECT、多人 RBAC 或 OAuth。

## Docker 快速开始

当前 Dockerfile 构建 **Linux amd64** 镜像；可在 ARM 开发机上交叉构建，ARM NAS 镜像尚未提供。需要 Docker 与 Buildx，没有预设可拉取的公开镜像。

```sh
git clone https://github.com/yunhungo/x-open-lucky.git
cd x-open-lucky

docker buildx build --platform linux/amd64 --load \
  --build-arg BUILD_SHA="$(git rev-parse HEAD)" -t openlucky:local .
docker volume create openlucky-data
docker run -d --name openlucky --restart unless-stopped \
  -p 127.0.0.1:56002:56002 \
  -p 58081:58081 -p 58444:58444 -p 58444:58444/udp \
  -v openlucky-data:/data \
  openlucky:local
```

打开 [http://127.0.0.1:56002](http://127.0.0.1:56002)。首次启动后在自己的终端读取初始化密钥，在页面中创建管理员：

```sh
docker exec openlucky cat /data/setup-token
```

初始化后该文件自动删除，没有默认密码。管理端口默认仅发布到本机；远程服务器可使用 SSH 转发：`ssh -L 56002:127.0.0.1:56002 user@server`。`/data` 保存数据库、加密密钥和证书，更新镜像时应保留数据卷。

Linux NAS 若需要读取宿主机 IPv6、连接宿主回环服务或使用多个入口，可使用 [Compose 配置](deploy/compose.yml) 的 host 网络方案。桥接模式下，上游 `127.0.0.1` 指向容器自身；应填写容器可达的地址。

## 配置第一个服务

1. 在“DNS 凭据”中保存供应商凭据；需要动态公网地址时创建 DDNS 任务。
2. 在“SSL 证书”中申请证书，或导入已有 PEM 证书与私钥。
3. 在“Web 服务”中填写域名和上游地址，选择证书及接入协议。
4. 将域名解析到入口地址，并按实际端口映射开放 TCP；开启 HTTP/3 时还需开放对应 UDP。

“高级配置”可调整上游版本、TLS、超时和请求/响应头。例如：

| 头名称 | 值 | 用途 |
| --- | --- | --- |
| `X-Service` | `nas` | 固定标识 |
| `X-Original-Host` | `{{ request.host }}` | 原始域名 |
| `X-Client-IP` | `{{ client.ip }}` | 当前连接的客户端地址 |
| `X-Source` | `{{ request.headers["x-source"] \| default("unknown") }}` | 原始头及默认值 |

完整变量、限制、协议兼容及 HTTP/3 独立 UDP 端口配置见 [Web 服务配置](docs/web-services.md)。认证和 Cookie 头不能写入普通配置。

## QNAP 自动部署

部署脚本面向已配置 SSH 密钥和 Container Station 的 QNAP，默认 SSH 别名为 `nas`。首次发布前，在 NAS 上创建 `/share/Container/openlucky/runtime.env`，填写自己的私网地址，例如：

```sh
OPENLUCKY_ADMIN_ADDR=192.168.1.100:56002
OPENLUCKY_HTTP_ADDR='[::]:58081'
OPENLUCKY_HTTPS_ADDR='[::]:58444'
OPENLUCKY_PUBLIC_HTTPS_PORT=58444
```

该文件是 shell 环境配置，请限制为管理员可写。然后在干净、已提交的工作区执行：

```sh
./.myself/build-and-deploy-qnap.sh
```

脚本执行检查、构建和上传镜像，停止本项目服务并保存完整数据快照，切换后验证健康与 Git 版本；失败自动回滚。首次发布会创建 `admin` 账户，将随机初始密码保存在本机权限为 0600 的 `.myself/qnap-access.json`。现有账户保持不变，可用 `OPENLUCKY_INITIAL_ADMIN` 设置首次用户名。

默认安装到 `/share/Container/openlucky`。可用 `OPENLUCKY_SSH_HOST`、`OPENLUCKY_REMOTE_ROOT`、`OPENLUCKY_REMOTE_DOCKER` 覆盖目标。运行参数在升级时保留。部署仅清理带本项目来源标记的旧镜像，不执行全局 Docker prune。详见 [部署与恢复](docs/operations.md)。

## 本地开发与验证

需要 Rust 1.89+、Node.js 22、Python 3、OpenSSL。实际签发证书还需要 [lego](https://github.com/go-acme/lego)，生产镜像已内置固定版本。

```sh
# 终端一：后台
cargo run

# 终端二：前台
cd frontend
npm ci
npm run dev
```

访问 [http://127.0.0.1:5173](http://127.0.0.1:5173)，初始化密钥位于 `data/setup-token`。开发服务器代理后台 `56002`。

```sh
# 首次准备浏览器
cd frontend
npx playwright install chromium
cd ..

# 完整检查
./.myself/check.sh
```

检查包含 Rust 格式、Clippy、单元测试、HTTP/1.1 / HTTP/2 / HTTP/3 真实协议测试、流式上传下载、部署回滚、前端构建和 Playwright 浏览器流程。协议测试使用临时数据和本地证书；浏览器测试使用 `56302 / 58380 / 58343`，不会更改生产 DNS。

性能对照方法见 [性能记录](docs/performance.md)。回环压测用于发现实现回退，不代表 NAS 或公网吞吐。

## 文档与参与

- [架构与模块边界](docs/architecture.md)
- [Web 服务与动态模板](docs/web-services.md)
- [环境变量、部署与恢复](docs/operations.md)
- [API 合同](docs/api.md)
- [访问日志与性能观测](docs/observability.md)

欢迎提交 Issue 和 Pull Request。复现问题时请提供版本、部署方式、最小配置及脱敏日志；不要提交密码、DNS 凭据、私钥或完整数据备份。修改配置合同时需同步 Rust 模型、前端 Zod 合同、SQL 迁移及行为测试，并运行完整检查。

## 许可证与致谢

项目自有代码以 [MIT License](LICENSE) 开源，允许使用、修改和分发，须保留版权与许可声明。第三方依赖遵循各自许可证，详见 [第三方声明](THIRD_PARTY_NOTICES.md)。

感谢 Rust、React、shadcn/ui、lego、h3、Quinn 和 MiniJinja 等开源项目。产品功能参考了 [Lucky](https://github.com/gdy666/lucky) 的 NAS 使用场景；本项目为独立实现，未复制 Lucky 的实现代码。
