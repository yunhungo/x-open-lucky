# QNAP 运维

## 运行时参数

| 环境变量 | 默认值 | 用途 |
| --- | --- | --- |
| `OPENLUCKY_DATA` | `data` | SQLite、加密密钥、初始化密钥、证书 |
| `OPENLUCKY_FRONTEND` | `frontend/dist` | 静态前端路径 |
| `OPENLUCKY_ADMIN_ADDR` | 本地开发 `0.0.0.0:56002`；QNAP `192.168.1.85:56002` | 管理监听 |
| `OPENLUCKY_HTTP_ADDR` | `[::]:58081` | HTTP 代理监听 |
| `OPENLUCKY_HTTPS_ADDR` | `[::]:58444` | HTTPS 代理监听 |
| `OPENLUCKY_PUBLIC_HTTPS_PORT` | `58444` | HTTP 跳转使用的外部 HTTPS 端口 |
| `OPENLUCKY_SECURE_COOKIE` | `false` | HTTPS 管理入口设为 `true` |
| `OPENLUCKY_LEGO_BIN` | `lego` | ACME 可执行文件 |
| `RUST_LOG` | `openlucky=info,tower_http=warn` | 标准输出结构化日志级别 |

QNAP 自定义参数保存在 `/share/Container/openlucky/runtime.env`。该文件只允许管理员维护，更新不会覆盖；健康检查使用实际管理监听地址。

## 发布与恢复

`.myself/build-and-deploy-qnap.sh` 要求干净、已提交的 Git 工作区。默认执行全部检查，再构建确定提交的 amd64 镜像，经 SSH 上传并调用 `.myself/activate-qnap.sh`。ARM 开发机使用原生 Rust 加 amd64 交叉编译器，避免在 QEMU 内运行 GCC。

发布使用目录锁。旧容器停止后，通过临时受限容器读取 `/data` 并写出快照，从而保留 UID 10001 的私密文件权限。新镜像来源和提交标签须匹配；启动后 `healthz.build_sha` 须精确等于提交。任何切换阶段失败都会恢复旧镜像与快照。

成功后，旧应用镜像会按用户要求删除。需要人工回退时检出对应旧提交重新构建镜像，或从本机 Docker 缓存恢复，然后恢复 NAS `snapshots/<commit>.tar.gz`。首次部署失败不删除配置数据。

请用管理台“设置 → 备份”做可迁移的密码加密备份。NAS 发布快照保留密钥和业务数据，只存在 NAS 的受限目录；迁移备份不包含管理员密码、会话或 ACME 账户，恢复后按当前管理员账户继续管理，后续签发可以创建新的 ACME 账户。

## 功能验收

`.myself/check.sh` 覆盖 Rust 格式与 Clippy、单元测试、真实协议集成测试、前端构建及浏览器流程。测试证书由 OpenSSL 本地生成，不冒充公开 CA 签发结果。供应商适配与正式 ACME 必须用获授权的真实域名和凭据单独验收。

管理 API 使用私网监听，现有 Lucky 服务、其配置、镜像和其他容器均不参与此部署。面板首次启动需要 `/data/setup-token`；初始化成功后密钥失效并删除。
